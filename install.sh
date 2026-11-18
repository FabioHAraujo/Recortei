#!/usr/bin/env bash
# Instala o Recortei no Linux, só para o seu usuário (sem sudo).
#
# Funciona de dois jeitos:
#   - no tarball da release (recortei-<versão>-linux-x86_64.tar.gz): usa o
#     binário `recortei` que vem ao lado deste script;
#   - num checkout do código: compila com o cargo (se preciso) e usa
#     target/release/recortei.
#
# Copia o binário para ~/.local/bin, instala o ícone e o atalho do menu e roda
# `recortei --setup`, que configura o início com a sessão, o atalho de captura
# no GNOME (padrão PrintScreen; troque em Configurações, no ícone da bandeja) e
# a permissão do portal de captura de tela.
#
#   ./install.sh               instala (ou atualiza)
#   ./install.sh --uninstall   remove (seus recortes e configurações ficam)
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
APP="$HOME/.local/bin/recortei"
APP_ID="io.github.recortei"
ICONS="$HOME/.local/share/icons/hicolor"
DESKTOP_FILE="$HOME/.local/share/applications/$APP_ID.desktop"
AUTOSTART_FILE="$HOME/.config/autostart/$APP_ID.desktop"
KB=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/
MEDIA_KEYS=org.gnome.settings-daemon.plugins.media-keys

stop_running() {
  # Encerra a versão anterior (Rust ou a antiga em Python) antes de substituir.
  if [ -x "$APP" ]; then "$APP" --quit >/dev/null 2>&1 || true; fi
  pkill -f "python3 .*recortei.py" 2>/dev/null || true
  sleep 0.3
}

has_gnome_keys() {
  # Sem -q no grep: com pipefail, o grep saindo cedo faria o gsettings falhar.
  command -v gsettings >/dev/null && gsettings list-schemas 2>/dev/null | grep -x "$MEDIA_KEYS" >/dev/null
}

uninstall() {
  stop_running
  # O próprio app desfaz o --setup: tira o atalho do GNOME e devolve os atalhos
  # de captura do GNOME (Print) que ele tinha liberado. Binários antigos sem
  # --unsetup caem no caminho manual abaixo.
  if [ -x "$APP" ] && [[ "$("$APP" --help 2>/dev/null || true)" == *--unsetup* ]]; then
    "$APP" --unsetup || echo ">> Aviso: o 'recortei --unsetup' não conseguiu desfazer tudo (veja as mensagens acima)."
  elif has_gnome_keys; then
    # Tira o atalho do Recortei da lista de atalhos personalizados do GNOME.
    CUR=$(gsettings get "$MEDIA_KEYS" custom-keybindings)
    if [[ "$CUR" == *"'$KB'"* ]]; then
      NEW=$(printf '%s' "$CUR" | sed -e "s#'$KB'##" -e 's/\[, /[/' -e 's/, ,/,/' -e 's/, \]/]/')
      [[ "$NEW" == "[]" ]] && NEW="@as []"
      gsettings set "$MEDIA_KEYS" custom-keybindings "$NEW"
    fi
    for key in name command binding; do
      gsettings reset "$MEDIA_KEYS.custom-keybinding:$KB" "$key" 2>/dev/null || true
    done
  fi
  rm -f "$APP" "$DESKTOP_FILE" "$AUTOSTART_FILE" \
    "$ICONS/scalable/apps/$APP_ID.svg" "$ICONS/256x256/apps/$APP_ID.png"
  echo ">> Recortei removido. Seus recortes (Imagens/Recortei) e configurações foram mantidos."
}

if [ "${1:-}" = "--uninstall" ]; then
  uninstall
  exit 0
elif [ -n "${1:-}" ]; then
  echo "Uso: $0 [--uninstall]" >&2
  exit 2
fi

if [ -f "$DIR/recortei" ] && [ -x "$DIR/recortei" ]; then
  # Tarball da release: binário pronto ao lado do script.
  BIN="$DIR/recortei"
elif [ -f "$DIR/Cargo.toml" ]; then
  # Checkout do código: compila se não houver binário ou se o código for mais
  # novo que ele.
  BIN="$DIR/target/release/recortei"
  if [ ! -x "$BIN" ] || [ -n "$(find "$DIR/src" "$DIR/Cargo.toml" -newer "$BIN" 2>/dev/null | head -1)" ]; then
    if ! command -v cargo >/dev/null; then
      echo ">> É preciso o Rust para compilar: https://rustup.rs"
      echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
      exit 1
    fi
    if ! command -v cc >/dev/null; then
      echo ">> É preciso um compilador C para linkar (Debian/Ubuntu: sudo apt install build-essential)"
      exit 1
    fi
    echo ">> Compilando (cargo build --release)..."
    (cd "$DIR" && cargo build --release --locked)
  fi
else
  echo ">> Não achei o binário 'recortei' nem o código-fonte ao lado deste script." >&2
  exit 1
fi

# Ícone: o da marca (packaging/icons no código, recortei.svg no tarball).
ICON_SVG="$DIR/packaging/icons/recortei.svg"
[ -f "$ICON_SVG" ] || ICON_SVG="$DIR/recortei.svg"
ICON_PNG="$DIR/packaging/icons/recortei-256.png"

stop_running
install -Dm755 "$BIN" "$APP"
install -Dm644 "$ICON_SVG" "$ICONS/scalable/apps/$APP_ID.svg"
if [ -f "$ICON_PNG" ]; then install -Dm644 "$ICON_PNG" "$ICONS/256x256/apps/$APP_ID.png"; fi

# Atalho no menu de aplicativos. O Exec usa o caminho completo porque o
# ~/.local/bin nem sempre está no PATH da sessão gráfica.
mkdir -p "$(dirname "$DESKTOP_FILE")"
cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Recortei
GenericName=Captura de tela
Comment=Captura de área de tela com anotações
Exec="$APP"
Icon=$APP_ID
Terminal=false
Categories=Utility;
Keywords=screenshot;captura;print;recorte;anotação;
StartupNotify=false
EOF
command -v update-desktop-database >/dev/null && update-desktop-database -q "$(dirname "$DESKTOP_FILE")" 2>/dev/null || true

# Início com a sessão, atalho de captura no GNOME e permissão do portal: tudo
# fica no `recortei --setup`, o mesmo que roda sozinho na primeira execução.
HELP_TEXT="$("$APP" --help 2>/dev/null || true)"
if [[ "$HELP_TEXT" == *--setup* ]]; then
  "$APP" --setup || echo ">> Aviso: o 'recortei --setup' não conseguiu configurar tudo (veja as mensagens acima)."
else
  echo ">> Aviso: este binário não tem 'recortei --setup'; configure um atalho global (ex.: PrintScreen) para: $APP --capture"
fi

case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo ">> Dica: adicione ~/.local/bin ao PATH para usar o comando 'recortei' no terminal." ;;
esac

if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
  echo ">> Recortei instalado em $APP. Iniciando... (para trocar o atalho: ícone da bandeja → Configurações, ou $APP --settings)"
  nohup "$APP" >/dev/null 2>&1 &
else
  echo ">> Recortei instalado em $APP. Ele inicia sozinho no próximo login (ou rode: $APP)."
fi
