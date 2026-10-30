#!/usr/bin/env python3
"""minisandbox — sandbox de milisegundos para una orden o un subagente, sin root ni Docker.

Ejecuta una orden con:
  * el directorio de trabajo (el repo) cubierto por un overlay copy-on-write: las escrituras van a
    una capa propia; el repo real NO cambia hasta `aplicar`;
  * el resto del sistema de archivos en solo lectura (salvo /tmp nuevo, HOME nuevo y --escribible);
  * sin red (namespace de red vacío, solo loopback) salvo --red;
  * su propio espacio de PIDs (todo muere al acabar o al vencer --tiempo).

Usa namespaces de usuario del kernel Linux (`unshare`), así que no necesita privilegios.

Uso:
  minisandbox.py ejecutar [--dir D] [--estado E] [--red] [--tiempo S] [--escribible P]... -- ORDEN...
  minisandbox.py cambios E [--json] [--diff]
  minisandbox.py aplicar E          # copia los cambios al repo real y borra E
  minisandbox.py descartar E        # borra E sin tocar el repo

`ejecutar` imprime en stderr la ruta del estado E y termina con el código de la orden.
"""
import argparse, difflib, json, os, shlex, shutil, stat, subprocess, sys, tempfile, time

PREPARAR = r"""
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
"""


def ejecutar(a):
    d = os.path.realpath(a.dir)
    e = a.estado or tempfile.mkdtemp(prefix="minisandbox-")
    e = os.path.realpath(e)
    if e == d or e.startswith(d + os.sep):
        sys.exit("minisandbox: el estado no puede estar dentro del directorio de trabajo")
    for sub in ("upper", "work"):
        os.makedirs(os.path.join(e, sub), exist_ok=True)
    escribibles = [os.path.realpath(p) for p in a.escribible]
    tmp_nuevo = not any(p == "/tmp" or p.startswith("/tmp/") for p in [d, e] + escribibles)
    if not tmp_nuevo:
        os.makedirs(os.path.join(e, "tmp"), exist_ok=True)
        escribibles.append(os.path.join(e, "tmp"))
    for p in escribibles:
        if " " in p:
            sys.exit(f"minisandbox: rutas con espacios no soportadas: {p}")
    ns = ["unshare", "--user", "--map-root-user", "--mount", "--pid", "--fork", "--kill-child",
          "--mount-proc"] + ([] if a.red else ["--net"])
    env = dict(os.environ, MS_DIR=d, MS_UP=f"{e}/upper", MS_WORK=f"{e}/work",
               ESCRIBIBLES=" ".join(escribibles),
               MS_TMP_NUEVO="1" if tmp_nuevo else "0", MS_TMP=os.path.join(e, "tmp"),
               # Dentro somos «root» mapeado: que git no se queje de la propiedad del repo.
               GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="safe.directory", GIT_CONFIG_VALUE_0="*")
    meta = {"dir": d, "orden": a.orden, "red": a.red, "inicio": time.time()}
    t0 = time.monotonic()
    try:
        rc = subprocess.run(ns + ["sh", "-c", PREPARAR, "minisandbox"] + a.orden, env=env,
                            timeout=a.tiempo or None).returncode
    except subprocess.TimeoutExpired:
        rc = 124
    meta.update(rc=rc, ms=round((time.monotonic() - t0) * 1000, 1))
    json.dump(meta, open(os.path.join(e, "meta.json"), "w"), indent=1)
    print(f"[minisandbox] estado={e} rc={rc} ms={meta['ms']}", file=sys.stderr)
    return rc


def _opaco(ruta):
    for k in ("trusted.overlay.opaque", "user.overlay.opaque"):
        try:
            if os.getxattr(ruta, k, follow_symlinks=False) == b"y":
                return True
        except OSError:
            pass
    return False


def leer_cambios(e, incluir_ignorados=False):
    """Lista de {ruta, tipo} con tipo en añadido | modificado | borrado | reemplazado (dir opaco)."""
    meta = json.load(open(os.path.join(e, "meta.json")))
    d, up, res = meta["dir"], os.path.join(e, "upper"), []
    for raiz, dirs, archivos in os.walk(up):
        rel_raiz = os.path.relpath(raiz, up)
        for nombre in sorted(dirs + archivos):
            p = os.path.join(raiz, nombre)
            rel = os.path.normpath(os.path.join(rel_raiz, nombre))
            st = os.lstat(p)
            if stat.S_ISCHR(st.st_mode) and st.st_rdev == 0:
                res.append({"ruta": rel, "tipo": "borrado"})
            elif stat.S_ISDIR(st.st_mode):
                if _opaco(p) and os.path.lexists(os.path.join(d, rel)):
                    res.append({"ruta": rel, "tipo": "reemplazado"})
                elif not os.path.lexists(os.path.join(d, rel)) and not os.listdir(p):
                    res.append({"ruta": rel, "tipo": "añadido"})
            else:
                res.append({"ruta": rel, "tipo": "modificado" if os.path.lexists(os.path.join(d, rel)) else "añadido"})
    if not incluir_ignorados and res and os.path.isdir(os.path.join(d, ".git")):
        r = subprocess.run(["git", "-C", d, "check-ignore", "--stdin"], input="\n".join(c["ruta"] for c in res),
                           capture_output=True, text=True)
        ign = set(r.stdout.split("\n"))
        res = [c for c in res if c["ruta"] not in ign]
    return sorted(res, key=lambda c: c["ruta"])


def cambios(a):
    cs = leer_cambios(a.estado, a.incluir_ignorados)
    if a.json:
        print(json.dumps(cs, ensure_ascii=False, indent=1))
        return 0
    meta = json.load(open(os.path.join(a.estado, "meta.json")))
    for c in cs:
        print(f"{c['tipo']:12} {c['ruta']}")
        if a.diff and c["tipo"] in ("modificado", "añadido"):
            nuevo = os.path.join(a.estado, "upper", c["ruta"])
            viejo = os.path.join(meta["dir"], c["ruta"])
            if os.path.isfile(nuevo) and os.path.getsize(nuevo) < 1 << 20:
                try:
                    va = open(viejo).readlines() if os.path.exists(viejo) else []
                    vb = open(nuevo).readlines()
                    sys.stdout.writelines(difflib.unified_diff(va, vb, f"a/{c['ruta']}", f"b/{c['ruta']}"))
                except UnicodeDecodeError:
                    print("  (binario)")
    return 0


def aplicar(a):
    meta = json.load(open(os.path.join(a.estado, "meta.json")))
    d, up = meta["dir"], os.path.join(a.estado, "upper")
    for c in leer_cambios(a.estado, a.incluir_ignorados):
        dst, src = os.path.join(d, c["ruta"]), os.path.join(up, c["ruta"])
        if c["tipo"] in ("borrado", "reemplazado") and os.path.lexists(dst):
            shutil.rmtree(dst) if os.path.isdir(dst) and not os.path.islink(dst) else os.remove(dst)
        if c["tipo"] == "reemplazado":
            shutil.copytree(src, dst, symlinks=True)
        elif c["tipo"] in ("añadido", "modificado"):
            os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
            if os.path.isdir(src) and not os.path.islink(src):
                os.makedirs(dst, exist_ok=True)
            else:
                if os.path.lexists(dst) and (os.path.islink(dst) or not os.path.isdir(dst)):
                    os.remove(dst)
                shutil.copy2(src, dst, follow_symlinks=False)
        print(f"{c['tipo']:12} {c['ruta']}")
    return descartar(a)


def descartar(a):
    # overlayfs deja `work/` con permisos 000 dentro del namespace: devolverlos antes de borrar.
    for raiz, dirs, _ in os.walk(a.estado):
        for n in dirs:
            try:
                os.chmod(os.path.join(raiz, n), 0o700)
            except OSError:
                pass
    shutil.rmtree(a.estado)
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("ejecutar")
    p.add_argument("--dir", default=".", help="directorio de trabajo a cubrir con el overlay (por defecto .)")
    p.add_argument("--estado", help="dónde guardar la capa de cambios (por defecto un tmp nuevo)")
    p.add_argument("--red", action="store_true", help="permitir red (por defecto: sin red)")
    p.add_argument("--tiempo", type=float, default=0, help="segundos máximos (0 = sin límite)")
    p.add_argument("--escribible", action="append", default=[], help="ruta extra escribible (repetible)")
    p.add_argument("orden", nargs=argparse.REMAINDER)
    for nombre in ("cambios", "aplicar", "descartar"):
        q = sub.add_parser(nombre)
        q.add_argument("estado")
        q.add_argument("--incluir-ignorados", action="store_true", help="no filtrar lo que ignora git")
        if nombre == "cambios":
            q.add_argument("--json", action="store_true")
            q.add_argument("--diff", action="store_true")
    a = ap.parse_args()
    if a.cmd == "ejecutar":
        a.orden = a.orden[1:] if a.orden[:1] == ["--"] else a.orden
        if not a.orden:
            ap.error("falta la orden tras --")
    sys.exit({"ejecutar": ejecutar, "cambios": cambios, "aplicar": aplicar, "descartar": descartar}[a.cmd](a))


if __name__ == "__main__":
    main()
