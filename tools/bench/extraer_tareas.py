#!/usr/bin/env python3
"""Extrae de SO_IA las tareas de SinapsisBench: una línea JSON por tarea con su criterio `acepta`,
su commit base (desde dónde se re-ejecuta) y el commit en que el orquestador la aceptó.

Commit base = el más reciente entre el commit que creó `ci/tareas/<id>.md` y los commits
`orquestador: <dep>: …` de cada dependencia (el estado del repo cuando la tarea pudo lanzarse).

Uso: tools/bench/extraer_tareas.py --repo ../SO_IA [--solo-aceptadas] > bench/tareas.jsonl
Necesita la historia completa (`git fetch --unshallow` si el clon es superficial).
"""
import argparse, glob, json, os, re, subprocess, sys


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True).stdout.strip()


def cabecera(txt):
    g = lambda k, d="": (re.search(rf"<!--\s*{k}:\s*(.*?)\s*-->", txt) or [None, d])[1]
    return {"acepta": g("acepta"), "modo": g("modo", "kernel"),
            "depende": [d.strip() for d in g("depende").split(",") if d.strip()]}


def commit_orquestador(repo, tid):
    """Primer commit `orquestador: <tid>: …` (registro de aceptación o bloqueo)."""
    salida = git(repo, "log", "--reverse", "--format=%H %ct %s", "--fixed-strings", f"--grep=orquestador: {tid}:")
    for linea in salida.splitlines():
        h, t, s = linea.split(" ", 2)
        return h, int(t), s
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", required=True)
    ap.add_argument("--solo-aceptadas", action="store_true")
    a = ap.parse_args()
    if git(a.repo, "rev-parse", "--is-shallow-repository") == "true":
        print("[bench] AVISO: clon superficial; faltarán commits base. Ejecuta: git -C REPO fetch --unshallow",
              file=sys.stderr)
    estado = json.load(open(os.path.join(a.repo, "ci/estado.json")))
    n = 0
    for ruta in sorted(glob.glob(os.path.join(a.repo, "ci/tareas/*.md"))):
        tid = os.path.basename(ruta)[:-3]
        cab = cabecera(open(ruta, encoding="utf-8").read())
        if not cab["acepta"]:
            continue
        creado = git(a.repo, "log", "--diff-filter=A", "--format=%H %ct", "--", f"ci/tareas/{tid}.md").split()
        candidatos = [(int(creado[1]), creado[0])] if len(creado) >= 2 else []
        for dep in cab["depende"]:
            c = commit_orquestador(a.repo, dep)
            if c:
                candidatos.append((c[1], c[0]))
        fin = commit_orquestador(a.repo, tid)
        st = estado.get(tid, {})
        if a.solo_aceptadas and st.get("estado") != "hecha":
            continue
        print(json.dumps({"id": tid, **cab, "base": max(candidatos)[1] if candidatos else None,
                          "aceptada_en": fin[0] if fin else None, "estado": st.get("estado"),
                          "intentos": st.get("intentos"), "tarea": f"ci/tareas/{tid}.md"}, ensure_ascii=False))
        n += 1
    print(f"[bench] {n} tareas", file=sys.stderr)


if __name__ == "__main__":
    main()
