#!/usr/bin/env sh
# Trae los cambios, compila con el perfil ligero e instala pcode en una sola orden.
# uso: tools/actualizar-local.sh [--sin-pull] [--en-seco]
set -eu
cd "$(dirname "$0")/.."

sin_pull=0
seco=0
for arg in "$@"; do
  case "$arg" in
    --sin-pull) sin_pull=1 ;;
    --en-seco) seco=1 ;;
    *) echo "uso: tools/actualizar-local.sh [--sin-pull] [--en-seco]" >&2; exit 2 ;;
  esac
done

paso() { if [ "$seco" = 1 ]; then echo "+ $*"; else "$@"; fi; }

# Pull solo fast-forward y solo con el árbol limpio: nunca stash, reset ni checkout.
if [ "$sin_pull" = 0 ]; then
  if [ -n "$(git status --porcelain)" ]; then
    echo "Hay cambios sin commit: se compila lo que hay sin hacer pull" >&2
  else
    paso git pull --ff-only
  fi
fi

paso sh tools/compilar-ligero.sh
paso sh install.sh --local target/ligero
paso "${PCODE_DIR:-$HOME/.local/bin}/pcode" --version
