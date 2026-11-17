#!/usr/bin/env bash
# Instala o Recortei no Linux: compila (se preciso), copia o binário para
# ~/.local/bin, registra autostart, .desktop, atalho Insert no GNOME e a
# permissão do portal de screenshot.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$DIR/target/release/recortei"
APP="$HOME/.local/bin/recortei"
APP_ID="io.github.recortei"

# Compila se não houver binário ou se o código for mais novo que ele.
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
  (cd "$DIR" && cargo build --release)
fi

# Encerra a versão anterior (Rust ou a antiga em Python) antes de substituir.
[ -x "$APP" ] && "$APP" --quit 2>/dev/null || true
pkill -f "python3 .*recortei.py" 2>/dev/null || true
sleep 0.3

install -Dm755 "$BIN" "$APP"
install -Dm644 "$DIR/recortei.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/$APP_ID.svg"

DESKTOP="[Desktop Entry]
Type=Application
Name=Recortei
Comment=Captura de área de tela com anotações
Exec=$APP
Icon=$APP_ID
Terminal=false
Categories=Utility;Graphics;
StartupNotify=false
X-GNOME-Autostart-enabled=true"
mkdir -p ~/.local/share/applications ~/.config/autostart
echo "$DESKTOP" > ~/.local/share/applications/$APP_ID.desktop
echo "$DESKTOP" > ~/.config/autostart/$APP_ID.desktop

if command -v gsettings >/dev/null && gsettings list-schemas | grep -q org.gnome.settings-daemon.plugins.media-keys; then
  # Atalho global Insert: o Wayland não permite o app capturar teclas sozinho,
  # então registramos um atalho personalizado do GNOME que chama o app.
  KB=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/
  SCHEMA=org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KB
  gsettings set $SCHEMA name 'Recortei'
  gsettings set $SCHEMA command "$APP --capture"
  gsettings set $SCHEMA binding 'Insert'
  CUR=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
  if [[ "$CUR" != *"$KB"* ]]; then
    if [[ "$CUR" == "@as []" || "$CUR" == "[]" ]]; then NEW="['$KB']"; else NEW="${CUR%]}, '$KB']"; fi
    gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW"
  fi
else
  echo ">> GNOME não encontrado: configure no seu ambiente um atalho Insert para: $APP --capture"
fi

# Permissão do portal de screenshot para o app (sem isso o GNOME tenta abrir
# um diálogo de permissão, que falha porque o app não tem janela focada).
gdbus call --session --dest org.freedesktop.impl.portal.PermissionStore \
  --object-path /org/freedesktop/impl/portal/PermissionStore \
  --method org.freedesktop.impl.portal.PermissionStore.SetPermission \
  screenshot true screenshot $APP_ID "['yes']" >/dev/null 2>&1 \
  || echo ">> Aviso: não foi possível gravar a permissão do portal de screenshot."

if [[ "${XDG_CURRENT_DESKTOP,,}" == *gnome* ]] && command -v gnome-extensions >/dev/null; then
  if ! gnome-extensions list --enabled 2>/dev/null | grep -qi appindicator; then
    echo ">> Dica: ative a extensão 'AppIndicator' do GNOME para ver o ícone na bandeja."
  fi
fi

echo ">> Pronto. Iniciando o Recortei..."
nohup "$APP" >/dev/null 2>&1 &
