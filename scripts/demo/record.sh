#!/bin/bash
# Nimmt im Aufnahme-Container eine Szene auf (Screenshots nach /out/*.png,
# Videos nach /out/*.mp4). Aufruf: record.sh <szene>; Szenen siehe unten.
set -euo pipefail
DIR=$(dirname "$0")
export DISPLAY=:99 WIDTH=${WIDTH:-1280} HEIGHT=${HEIGHT:-800}
bash "$DIR/session.sh" >/tmp/session.log 2>&1 &
for _ in $(seq 100); do [ -s /tmp/wid ] && break; sleep 0.2; done
[ -s /tmp/wid ] || { echo "GitManager-Fenster fehlt:" >&2; cat /tmp/session.log >&2; exit 1; }
sleep 3

# --- Bausteine ---------------------------------------------------------------
pos() { xdotool getmouselocation --shell | sed -n 's/^\([XY]\)=//p' | tr '\n' ' '; }
move() { # move <x> <y> [schritte]: gleitend, damit Videos ruhig wirken
  read -r cx cy <<<"$(pos)"
  local steps=${3:-24} i
  for i in $(seq 1 "$steps"); do
    xdotool mousemove $((cx + ($1 - cx) * i / steps)) $((cy + ($2 - cy) * i / steps))
    sleep 0.012
  done
}
click() { move "$1" "$2"; sleep 0.2; xdotool click 1; }
drag() { # drag <x1> <y1> <x2> <y2>
  # egui braucht je einen Frame mit Zeiger über dem Griff vor Druck und Loslassen
  move "$1" "$2"; sleep 0.4; xdotool mousedown 1; sleep 0.2
  move "$3" "$4" 40; sleep 0.2; xdotool mouseup 1
}
# Fokus vor jeder Tastatureingabe neu setzen (ohne Fenstermanager geht er verloren)
focus() { xdotool windowfocus --sync "$(cat /tmp/wid)" || true; sleep 0.1; }
type_slow() { focus; xdotool type --delay 110 "$1"; }
key() { focus; xdotool key "$@"; }
shot() { sleep 0.6; import -window root "/out/$1.png"; }
FFPID=
rec_start() {
  ffmpeg -y -loglevel error -f x11grab -draw_mouse 1 -framerate 30 \
    -video_size "${WIDTH}x${HEIGHT}" -i :99 -c:v libx264 -preset slow -crf 24 \
    -pix_fmt yuv420p -movflags +faststart "/out/$1.mp4" &
  FFPID=$!
  sleep 0.8
}
rec_stop() { sleep 0.8; kill -INT "$FFPID"; wait "$FFPID" || true; }

# --- Szenen --------------------------------------------------------------------
TAB_REPOS=(160 21) TAB_MRS=(260 21)

scene_shots() {
  move 900 760 1
  shot repo-list
  click 255 427; sleep 0.8; click 315 455; type_slow "fe"; shot repo-branch-search
  key Escape; sleep 0.5
  click "${TAB_MRS[@]}"; sleep 3; move 640 760; shot merge-requests
  click 1217 310; sleep 1; move 1000 420; shot mr-checkout
  key Escape; sleep 0.5
  click 223 104; sleep 2.5; move 640 760; shot mr-merged
  click 1240 21; sleep 1.5
  drag 734 573 1244 784; sleep 0.5
  click 636 66; sleep 3; move 1270 795; shot settings-merge-requests
}

scene_repos() {
  move 640 400 1
  rec_start demo-repos
  move 255 427; sleep 0.4; click 255 427; sleep 1
  click 315 455; type_slow "fix"; sleep 1
  click 245 511; sleep 2
  move 265 587; sleep 0.3; click 265 587; sleep 1
  click 315 615; type_slow "date"; sleep 0.8
  click 255 671; sleep 1.5
  move 657 145; sleep 1.4
  click 572 145; sleep 1.2
  move 640 400; sleep 0.6
  rec_stop
}

scene_mrs() {
  move 640 400 1
  rec_start demo-merge-requests
  click "${TAB_MRS[@]}"; sleep 2.5
  move 85 334; sleep 2
  click 1148 101; sleep 1.2; click 1148 101; sleep 1
  click 955 134; type_slow "refund"; sleep 1.5
  key ctrl+a BackSpace; sleep 1
  click 1217 310; sleep 1.2; move 1000 420; sleep 1.5; move 1000 480; sleep 1
  key Escape; sleep 0.6
  click 223 104; sleep 2.5
  move 300 300; sleep 1
  click 166 104; sleep 1.5
  rec_stop
}

"scene_${1:?Szene fehlt (shots|repos|mrs)}"
