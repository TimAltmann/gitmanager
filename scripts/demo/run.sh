#!/bin/bash
# Erzeugt Screenshots und Videos für das README (Linux-Build in Docker,
# ohne Netz, nur erfundene Demo-Daten). Ergebnis in assets/media/.
# Aufruf aus dem Repo-Wurzelordner: scripts/demo/run.sh [szene …]
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT=assets/media
mkdir -p "$OUT"
docker compose build -q dev
docker build -q -t gm-capture scripts/demo >/dev/null
docker compose run --rm dev cargo build --release
for scene in "${@:-shots repos mrs}"; do
  for s in $scene; do
    echo "== $s"
    docker run --rm --network none -v "$PWD:/app:ro" \
      -v gitmanager_target-cache:/app/target:ro -v "$PWD/$OUT:/out" \
      gm-capture bash /app/scripts/demo/record.sh "$s"
  done
done
# Kurze Schleifen fürs README (spielen auf GitHub direkt ab)
for v in "$OUT"/*.mp4; do
  docker run --rm --network none -v "$PWD/$OUT:/out" gm-capture ffmpeg -y -loglevel error \
    -i "/out/$(basename "$v")" -vf "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer:bayer_scale=4" \
    "/out/$(basename "${v%.mp4}").gif"
done
# Der Container schreibt als root: Dateien dem aufrufenden Benutzer geben
docker run --rm -v "$PWD/$OUT:/out" gm-capture chown -R "$(id -u):$(id -g)" /out
ls -lh "$OUT"
