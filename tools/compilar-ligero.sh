#!/usr/bin/env sh
# Compila pcode y pcode-ide con el perfil `ligero` (menos RAM y tiempo que release con LTO).
set -eu
libre=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 4096)
if [ "$libre" -lt 1500 ]; then jobs=1; else jobs=2; fi
cargo build --profile ligero -p pcode-cli -p pcode-ide -j "$jobs"
echo "Binarios en target/ligero/: pcode y pcode-ide"
