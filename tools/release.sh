#!/usr/bin/env sh
# Publica una release: tools/release.sh X.Y.Z   ·   Solo valida: tools/release.sh --comprobar
# Comprueba árbol limpio, tests y CHANGELOG; sube la versión, hace commit, etiqueta y empuja (el workflow publica).
set -eu
raiz="$(cd "$(dirname "$0")/.." && pwd)"
cd "$raiz"
actual="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' Cargo.toml)"
if [ "${1:-}" = "--comprobar" ]; then
  grep -q "^## \[$actual\]" CHANGELOG.md || { echo "CHANGELOG.md no tiene la sección ## [$actual]" >&2; exit 1; }
  echo "✓ versión $actual con su sección en el CHANGELOG"
  exit 0
fi
v="${1:?uso: tools/release.sh X.Y.Z}"
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "versión no válida: $v" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "hay cambios sin commit" >&2; exit 1; }
grep -q "^## \[$v\]" CHANGELOG.md || { echo "falta ## [$v] en CHANGELOG.md" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$v" >/dev/null && { echo "la etiqueta v$v ya existe" >&2; exit 1; }
cargo test --workspace
sed -i "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$v\"/" Cargo.toml
cargo update -w --offline >/dev/null 2>&1 || true
git add Cargo.toml Cargo.lock
git commit -m "release: v$v"
git tag -a "v$v" -m "PublicCode v$v"
git push origin HEAD "v$v"
echo "Etiqueta v$v empujada: el workflow «release» publica los binarios."
