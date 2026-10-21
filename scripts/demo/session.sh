#!/bin/bash
# Startet im Aufnahme-Container: Demo-Repos, Demo-Config, Fake-glab,
# virtuelles Display und GitManager (Linux-Release). Läuft weiter, bis der
# Container endet; Bedienung per `docker exec … xdotool`.
set -euo pipefail
export DEMO_HOME=/home/demo HOME=/home/demo XDG_CONFIG_HOME=/home/demo/.config
export GLAB_CONFIG_DIR=/home/demo/.config/glab-cli FAKE_GLAB_LOG=/tmp/fake-glab.log
export DISPLAY=:99 LANG=C.UTF-8
unset WAYLAND_DISPLAY
WIDTH=${WIDTH:-1280} HEIGHT=${HEIGHT:-800}
DIR=$(dirname "$0")
mkdir -p "$XDG_CONFIG_HOME/gitmanager" "$GLAB_CONFIG_DIR"
# Fake-glab als `glab` im PATH (so zeigen die Einstellungen kein Demo-Skript)
ln -sf "$(realpath "$DIR/fake-glab.py")" /usr/local/bin/glab
bash "$DIR/make-demo-repos.sh"
cp "$DIR/config.json" "$XDG_CONFIG_HOME/gitmanager/config.json"
printf 'hosts:\n    gitlab.example.com:\n        api_protocol: https\n        git_protocol: ssh\n        user: aberger\n' \
  > "$GLAB_CONFIG_DIR/config.yml"
Xvfb :99 -screen 0 "${WIDTH}x${HEIGHT}x24" -nolisten tcp >/tmp/xvfb.log 2>&1 &
sleep 1
/app/target/release/gitmanager >/tmp/app.log 2>&1 &
for _ in $(seq 50); do
  WID=$(xdotool search --name '^GitManager' 2>/dev/null | head -1 || true)
  [ -n "$WID" ] && break; sleep 0.2
done
xdotool windowmove "$WID" 0 0 windowsize "$WID" "$WIDTH" "$HEIGHT"
echo "$WID" > /tmp/wid
# Ohne Fenstermanager bekommt das Fenster sonst keinen Fokus (keine
# Texteingabe); record.sh setzt ihn vor jeder Eingabe erneut
xdotool windowfocus "$WID" || true
echo "GitManager läuft (Fenster $WID)"
wait
