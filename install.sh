#!/usr/bin/env bash
# Instala o Recortei: autostart, .desktop e atalho Insert no GNOME.
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"
APP="$DIR/recortei.py"
chmod +x "$APP"

DEPS="gir1.2-ayatanaappindicator3-0.1 python3-gi-cairo"
if ! python3 -c "import gi; gi.require_version('AyatanaAppIndicator3','0.1'); gi.require_foreign('cairo')" 2>/dev/null; then
  echo ">> Instalando dependências (pede senha):"
  if [ -t 0 ]; then sudo apt install -y $DEPS; else pkexec apt install -y $DEPS; fi
fi

DESKTOP="[Desktop Entry]
Type=Application
Name=Recortei
Comment=Captura de área de tela
Exec=/usr/bin/python3 $APP
Icon=$DIR/recortei.svg
Terminal=false
X-GNOME-Autostart-enabled=true"
mkdir -p ~/.local/share/applications ~/.config/autostart
echo "$DESKTOP" > ~/.local/share/applications/io.github.recortei.desktop
echo "$DESKTOP" > ~/.config/autostart/io.github.recortei.desktop

# Atalho global Insert (Wayland não permite o app capturar teclas sozinho,
# então registramos um atalho personalizado do GNOME que chama a ação via D-Bus).
KB=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/
SCHEMA=org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$KB
CMD="gdbus call --session --dest io.github.recortei --object-path /io/github/recortei --method org.freedesktop.Application.ActivateAction capture [] {}"
gsettings set $SCHEMA name 'Recortei'
gsettings set $SCHEMA command "sh -c '$CMD || /usr/bin/python3 $APP --capture'"
gsettings set $SCHEMA binding 'Insert'
CUR=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
if [[ "$CUR" != *"$KB"* ]]; then
  if [[ "$CUR" == "@as []" || "$CUR" == "[]" ]]; then NEW="['$KB']"; else NEW="${CUR%]}, '$KB']"; fi
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW"
fi

# Permissão do portal de screenshot para o app (sem isso o GNOME tenta abrir
# um diálogo de permissão, que falha porque o app não tem janela focada).
gdbus call --session --dest org.freedesktop.impl.portal.PermissionStore \
  --object-path /org/freedesktop/impl/portal/PermissionStore \
  --method org.freedesktop.impl.portal.PermissionStore.SetPermission \
  screenshot true screenshot io.github.recortei "['yes']" >/dev/null

pkill -f "python3 $APP" 2>/dev/null || true
echo ">> Pronto. Iniciando o Recortei..."
nohup /usr/bin/python3 "$APP" >/dev/null 2>&1 &
