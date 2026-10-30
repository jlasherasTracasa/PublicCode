#!/usr/bin/env sh
# Instala pcode en ~/.local/bin (Linux). Ejecútalo desde la carpeta descomprimida de la release.
set -eu
dest="${HOME}/.local/bin"
mkdir -p "$dest"
cp "$(dirname "$0")/pcode" "$dest/pcode"
chmod +x "$dest/pcode"
case ":$PATH:" in *":$dest:"*) ;; *) echo "Añade $dest a tu PATH";; esac
"$dest/pcode" --version
