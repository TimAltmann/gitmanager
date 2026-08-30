# Build-Skript für Windows PowerShell
# Baut die Windows .exe via Docker

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Resolve-Path "$ScriptDir\.."
Set-Location $RootDir

Write-Host "==> Docker Build (Windows GNU)..." -ForegroundColor Cyan
docker compose run --rm dev cargo build --target x86_64-pc-windows-gnu --release

Write-Host ""
Write-Host "==> Kopiere EXE aus Docker-Volume auf Host..." -ForegroundColor Cyan
$VolumeName = "gitmanager_target-cache"
# Fallback: suche Volume
if (-not (docker volume inspect $VolumeName 2>$null)) {
    $VolumeName = (docker volume ls -q | Select-String -Pattern "gitmanager.*target|repomanager.*target" | Select-Object -First 1).ToString().Trim()
}
Write-Host "   Volume: $VolumeName" -ForegroundColor Gray

# Kopiere via Hilfscontainer (alpine)
docker run --rm -v "${VolumeName}:/vol" -v "${RootDir}:/out" alpine sh -c "mkdir -p /out/target/x86_64-pc-windows-gnu/release && cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/gitmanager.exe && cp /vol/x86_64-pc-windows-gnu/release/gitmanager.exe /out/target/x86_64-pc-windows-gnu/release/gitmanager.exe && ls -lh /out/gitmanager.exe /out/target/x86_64-pc-windows-gnu/release/gitmanager.exe"

Write-Host ""
Write-Host "==> Artefakte:" -ForegroundColor Cyan
$exe = "gitmanager.exe"
$exe2 = "target\x86_64-pc-windows-gnu\release\gitmanager.exe"
if (Test-Path $exe) {
    Get-Item $exe | Format-List Name, Length, LastWriteTime
    Write-Host "Fertig. EXE: $exe" -ForegroundColor Green
}
if (Test-Path $exe2) {
    Get-Item $exe2 | Format-List Name, Length, LastWriteTime
}
if (Test-Path $exe) {
    & file $exe 2>$null | Out-Host
}

Write-Host ""
Write-Host "Für MSVC (kleiner, optional):"
Write-Host "  docker compose run --rm dev cargo xwin build --target x86_64-pc-windows-msvc --release"
