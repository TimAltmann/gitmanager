#!/usr/bin/env bash
set -euo pipefail

# Build-Skript für Linux/macOS (WSL)
# Baut die Windows .exe via Docker (mingw-gnu)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

echo "==> Docker Build (Windows GNU)..."
docker compose run --rm dev cargo build --target x86_64-pc-windows-gnu --release

echo ""
echo "==> Kopiere EXE aus Docker-Volume auf Host..."
# Target liegt im named volume gitmanager_target-cache, nicht direkt auf Host (WSL Performance)
# Daher via Hilfscontainer kopieren
if docker volume inspect gitmanager_target-cache >/dev/null 2>&1; then
    VOLUME_NAME="gitmanager_target-cache"
else
    # Fallback: suche Volume mit Suffix
    VOLUME_NAME=$(docker volume ls -q | grep -E "gitmanager.*target|repomanager.*target" | head -n1)
fi

if [ -n "${VOLUME_NAME:-}" ]; then
    echo "   Volume: $VOLUME_NAME"
    docker run --rm -v "${VOLUME_NAME}:/vol" -v "${ROOT_DIR}:/out" alpine sh -c "cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/gitmanager.exe 2>/dev/null && cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/target/x86_64-pc-windows-gnu/release/gitmanager.exe 2>/dev/null || cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/gitmanager.exe; ls -lh /out/gitmanager.exe"
    # Rechte korrigieren (Container lief als root)
    if [ -f "${ROOT_DIR}/gitmanager.exe" ]; then
        sudo chown "$(id -u):$(id -g)" "${ROOT_DIR}/gitmanager.exe" 2>/dev/null || true
        chmod +x "${ROOT_DIR}/gitmanager.exe" 2>/dev/null || true
    fi
    mkdir -p "${ROOT_DIR}/target/x86_64-pc-windows-gnu/release" 2>/dev/null || sudo mkdir -p "${ROOT_DIR}/target/x86_64-pc-windows-gnu/release" && sudo chown -R "$(id -u):$(id -g)" "${ROOT_DIR}/target" 2>/dev/null || true
    docker run --rm -v "${VOLUME_NAME}:/vol" -v "${ROOT_DIR}:/out" alpine sh -c "mkdir -p /out/target/x86_64-pc-windows-gnu/release && cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/target/x86_64-pc-windows-gnu/release/gitmanager.exe && ls -lh /out/target/x86_64-pc-windows-gnu/release/gitmanager.exe" 2>/dev/null || true
    sudo chown -R "$(id -u):$(id -g)" "${ROOT_DIR}/target" 2>/dev/null || true
else
    echo "Warnung: Konnte Volume nicht finden, versuche direkten Host-Pfad..."
fi

echo ""
echo "==> Artefakte:"
ls -lh gitmanager.exe 2>/dev/null || true
ls -lh target/x86_64-pc-windows-gnu/release/gitmanager.exe 2>/dev/null || true
file gitmanager.exe 2>/dev/null | head -n 5 || true
echo ""
echo "Fertig. EXE liegt in ./gitmanager.exe und target/x86_64-pc-windows-gnu/release/gitmanager.exe"
echo "Für MSVC (kleiner, optional):"
echo "  docker compose run --rm dev cargo xwin build --target x86_64-pc-windows-msvc --release"
# MSVC ebenfalls kopieren falls vorhanden
if docker run --rm -v "${VOLUME_NAME}:/vol" alpine test -f /vol/x86_64-pc-windows-msvc/release/gitmanager.exe 2>/dev/null; then
    echo "  MSVC Artefakt gefunden, kopiere..."
    docker run --rm -v "${VOLUME_NAME}:/vol" -v "${ROOT_DIR}:/out" alpine sh -c "cp /vol/x86_64-pc-windows-msvc/release/gitmanager.exe /out/repomanager-msvc.exe && ls -lh /out/repomanager-msvc.exe"
fi
