#!/usr/bin/env python3
"""Genera tareas/<id>.md y la tabla de docs/implementacion.md a partir de tareas/plan.toml.

plan.toml es la ÚNICA fuente de verdad del plan de implementación. Cada [[tarea]] tiene:
  id, nombre, titulo, fase, ejecutor (haiku | coordinador), depende, toca, acepta,
  contexto, objetivo, firmas (opcional), lenguaje (de las firmas; por defecto rust),
  toca_extra (opcional: configuración estándar que no cuenta para el límite de 4), tests,
  notas (opcional), papers (opcional).

Uso:
  tools/tareas/generar.py              # valida y escribe tareas/*.md + tabla de implementacion.md
  tools/tareas/generar.py --comprobar  # solo valida y comprueba que lo generado está al día (CI)
"""
import argparse, os, re, sys, tomllib

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PLAN = os.path.join(RAIZ, "tareas", "plan.toml")
DOC = os.path.join(RAIZ, "docs", "implementacion.md")
INI, FIN = "<!-- tabla:inicio (generado por tools/tareas/generar.py; no editar a mano) -->", "<!-- tabla:fin -->"
OBLIGATORIOS = ("id", "nombre", "titulo", "fase", "ejecutor", "depende", "toca", "acepta", "objetivo", "tests")


def fid(t):
    return f"{t['id']}-{t['nombre']}"


def validar(tareas):
    errores, ids = [], {}
    for t in tareas:
        for k in OBLIGATORIOS:
            if k not in t or t[k] in ("", None):
                errores.append(f"{t.get('id', '?')}: falta `{k}`")
        if t.get("id") in ids:
            errores.append(f"{t['id']}: id repetido")
        ids[t.get("id")] = t
        if t.get("ejecutor") not in ("haiku", "coordinador"):
            errores.append(f"{t.get('id')}: ejecutor debe ser haiku o coordinador")
        if t.get("ejecutor") == "haiku" and len(t.get("toca", [])) > 4:
            errores.append(f"{t['id']}: una tarea para haiku toca como mucho 4 archivos ({len(t['toca'])})")
        if not re.fullmatch(r"F\d\.\d\d", t.get("id", "")):
            errores.append(f"{t.get('id')}: id con formato F<fase>.<NN>")
    for t in tareas:
        for d in t.get("depende", []):
            if d not in ids:
                errores.append(f"{t['id']}: depende de {d}, que no existe")
            elif ids[d]["id"] >= t["id"] and ids[d]["fase"] == t["fase"]:
                errores.append(f"{t['id']}: depende de {d}, que va después (orden de la tabla)")
    # Ciclos (DFS).
    estado = {}

    def visita(i, camino):
        if estado.get(i) == 1:
            errores.append("ciclo: " + " → ".join(camino + [i]))
            return
        if estado.get(i) == 2 or i not in ids:
            return
        estado[i] = 1
        for d in ids[i].get("depende", []):
            visita(d, camino + [i])
        estado[i] = 2

    for i in ids:
        visita(i, [])
    return errores


def lista(xs):
    return "\n".join(f"- {x}" for x in xs) if xs else "- (nada)"


def render(t, ids):
    deps = [fid(ids[d]) for d in t["depende"]]
    modo = "aislado" if t["ejecutor"] == "haiku" else "coordinador"
    partes = [
        f"<!-- acepta: {t['acepta']} -->",
        f"<!-- modo: {modo} -->",
        f"<!-- depende: {', '.join(deps)} -->",
        f"# {fid(t)} — {t['titulo']}",
        "",
        f"**Fase** {t['fase']} · **Ejecutor** {t['ejecutor']} · **Depende de** {', '.join(deps) or '—'}",
        "",
        "> Generado desde `tareas/plan.toml`. Sigue `AGENTS.md` y la skill `tarea-haiku`.",
        "",
        "## Toca (los únicos archivos que puedes crear o modificar, más tu informe)",
        "",
        lista([f"`{p}`" for p in t["toca"]] + [f"`{p}` (configuración)" for p in t.get("toca_extra", [])]
              + [f"`informes/{fid(t)}.md`"]),
        "",
        "## Contexto (lee esto y nada más)",
        "",
        lista(t.get("contexto", [])),
        "",
        "## Objetivo",
        "",
        t["objetivo"].strip(),
        "",
    ]
    if t.get("firmas"):
        partes += ["## Firmas (cópialas tal cual)", "", f"```{t.get('lenguaje', 'rust')}", t["firmas"].strip(), "```", ""]
    partes += ["## Tests (con estos nombres)", "", lista([f"`{x.split(':', 1)[0].strip()}`:"
                                                        f"{x.split(':', 1)[1] if ':' in x else ''}"
                                                        for x in t["tests"]]), ""]
    if t.get("papers"):
        partes += ["## Papers (docs/papers.md)", "", lista(t["papers"]), ""]
    if t.get("notas"):
        partes += ["## Notas", "", t["notas"].strip(), ""]
    partes += [
        "## Aceptación (ejecútala una vez al final)",
        "",
        "```bash",
        t["acepta"],
        "```",
        "",
        "Después: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, informe en "
        f"`informes/{fid(t)}.md` y commit `git add <toca> informes/{fid(t)}.md && git commit -m \"{fid(t)}: …\"`.",
        "",
    ]
    return "\n".join(partes)


def tabla(tareas, ids):
    filas, fase = [], None
    for t in tareas:
        if t["fase"] != fase:
            fase = t["fase"]
            filas += ["", f"### {fase}", "", "| Tarea | Qué | Ejecutor | Depende | Estado |", "|---|---|---|---|---|"]
        hecha = os.path.exists(os.path.join(RAIZ, "informes", f"{fid(t)}.md"))
        filas.append(f"| [{fid(t)}](../tareas/{fid(t)}.md) | {t['titulo']} | {t['ejecutor']} | "
                     f"{', '.join(t['depende']) or '—'} | {'hecha' if hecha else 'pendiente'} |")
    return "\n".join(filas).strip()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--comprobar", action="store_true")
    a = ap.parse_args()
    tareas = tomllib.load(open(PLAN, "rb"))["tarea"]
    errores = validar(tareas)
    if errores:
        print("\n".join(f"[tareas] ERROR {e}" for e in errores), file=sys.stderr)
        return 1
    ids = {t["id"]: t for t in tareas}
    desfasados = []
    esperados = {f"{fid(t)}.md": render(t, ids) for t in tareas}
    for nombre, txt in esperados.items():
        ruta = os.path.join(RAIZ, "tareas", nombre)
        if not os.path.exists(ruta) or open(ruta, encoding="utf-8").read() != txt:
            desfasados.append(nombre)
            if not a.comprobar:
                open(ruta, "w", encoding="utf-8").write(txt)
    for viejo in os.listdir(os.path.join(RAIZ, "tareas")):
        if viejo.endswith(".md") and viejo not in esperados:
            desfasados.append(f"{viejo} (sobra)")
    doc = open(DOC, encoding="utf-8").read()
    i, j = doc.index(INI) + len(INI), doc.index(FIN)
    nuevo = doc[:i] + "\n" + tabla(tareas, ids) + "\n" + doc[j:]
    if nuevo != doc:
        desfasados.append("docs/implementacion.md")
        if not a.comprobar:
            open(DOC, "w", encoding="utf-8").write(nuevo)
    n_h = sum(1 for t in tareas if t["ejecutor"] == "haiku")
    print(f"[tareas] {len(tareas)} tareas ({n_h} para haiku, {len(tareas) - n_h} para coordinador)", file=sys.stderr)
    if a.comprobar and desfasados:
        print("[tareas] desfasado: " + ", ".join(desfasados) + " → ejecuta tools/tareas/generar.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
