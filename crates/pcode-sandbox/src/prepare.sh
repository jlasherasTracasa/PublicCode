set -e
mount --make-rprivate /
# 1) Puntos que deben seguir escribibles: bind sobre sí mismos para que sean montajes propios.
for p in $ESCRIBIBLES; do mount --bind "$p" "$p"; done
# 2) Overlay CoW sobre el directorio de trabajo (antes de poner la raíz en solo lectura).
mount -t overlay overlay -o "lowerdir=$MS_DIR,upperdir=$MS_UP,workdir=$MS_WORK" "$MS_DIR"
# 3) Todo lo demás en solo lectura (mejor esfuerzo: algunos montajes del sistema no se dejan).
awk '{print $5}' /proc/self/mountinfo | while read -r m; do
  case " $MS_DIR $ESCRIBIBLES /proc /dev " in *" $m "*) continue ;; esac
  case "$m" in /proc/*|/dev/*|/sys/*) continue ;; esac
  mount -o remount,bind,ro "$m" 2>/dev/null || true
done
# 4) /tmp y HOME nuevos y vacíos (si el repo vive bajo /tmp, el tmp privado va dentro del estado).
if [ "$MS_TMP_NUEVO" = 1 ]; then mount -t tmpfs -o size=1g tmpfs /tmp; MS_TMP=/tmp; fi
mkdir -p "$MS_TMP/home" && export HOME="$MS_TMP/home" TMPDIR="$MS_TMP"
ip link set lo up 2>/dev/null || true
cd "$MS_DIR"
exec "$@"
