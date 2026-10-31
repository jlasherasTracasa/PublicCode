#!/usr/bin/env sh
# Instala o actualiza pcode en Linux (x86_64), por usuario y sin sudo.
#
#   curl -fsSL https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.sh | sh
#
# Desde la carpeta descomprimida de una release instala el `pcode` que hay al lado.
# Variables: PCODE_VERSION (p. ej. v0.4.0; por defecto, la última), PCODE_DIR (por defecto
# ~/.local/bin), PCODE_REPO (para un fork).
set -eu
repo="${PCODE_REPO:-jlasherasTracasa/PublicCode}"
dest="${PCODE_DIR:-$HOME/.local/bin}"
mkdir -p "$dest"

dl() {
  if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then wget -qO "$2" "$1"
  else echo "Hace falta curl o wget" >&2; exit 1; fi
}

here=""
case "$0" in */install.sh|install.sh) here="$(cd "$(dirname "$0")" && pwd)";; esac
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if [ -n "$here" ] && [ -f "$here/pcode" ]; then
  src="$here/pcode"
else
  [ "$(uname -s)" = Linux ] || { echo "Este instalador es para Linux; en Windows usa install.ps1" >&2; exit 1; }
  case "$(uname -m)" in
    x86_64|amd64) ;;
    *) echo "No hay binarios para $(uname -m): compila con cargo install --path crates/pcode-cli" >&2; exit 1;;
  esac
  tag="${PCODE_VERSION:-}"
  if [ -z "$tag" ]; then
    dl "https://api.github.com/repos/$repo/releases/latest" "$tmp/latest.json"
    tag="$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -n 1)"
    [ -n "$tag" ] || { echo "No se pudo averiguar la última versión" >&2; exit 1; }
  fi
  name="pcode-$tag-x86_64-unknown-linux-gnu.tar.gz"
  base="https://github.com/$repo/releases/download/$tag"
  echo "Descargando $name…"
  dl "$base/$name" "$tmp/$name"
  dl "$base/SHA256SUMS" "$tmp/SHA256SUMS"
  expected="$(grep " \*\{0,1\}$name\$" "$tmp/SHA256SUMS" | cut -d ' ' -f 1)"
  actual="$(sha256sum "$tmp/$name" | cut -d ' ' -f 1)"
  [ -n "$expected" ] && [ "$expected" = "$actual" ] || { echo "La suma SHA-256 de $name no coincide" >&2; exit 1; }
  tar -xzf "$tmp/$name" -C "$tmp"
  src="$tmp/${name%.tar.gz}/pcode"
fi

# Se copia y se renombra: así se puede actualizar aunque pcode esté abierto.
cp "$src" "$dest/.pcode.nuevo"
chmod 755 "$dest/.pcode.nuevo"
mv -f "$dest/.pcode.nuevo" "$dest/pcode"

case ":$PATH:" in
  *":$dest:"*) ;;
  *)
    line="export PATH=\"$dest:\$PATH\""
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
      if [ -f "$rc" ] && ! grep -qF "$line" "$rc"; then
        printf '\n# PublicCode\n%s\n' "$line" >> "$rc"
        echo "Añadido $dest al PATH en $rc"
      fi
    done
    echo "Abre una terminal nueva (o ejecuta: $line)"
    ;;
esac
"$dest/pcode" --version
echo "Siguiente: define ANTHROPIC_API_KEY (o GEMINI_API_KEY u OPENAI_API_KEY) y ejecuta: pcode doctor"
