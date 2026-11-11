#!/usr/bin/env python3
"""Plan de implementación de PublicCode: valida, genera y consulta.

Fuente de verdad (lo único que se edita a mano):
  tareas/plan.toml      índice: fases ([[fase]]) e hitos ([[hito]])
  tareas/plan/F<n>.toml tareas de cada fase ([[tarea]])

De ahí salen tareas/<id>-<nombre>.md y las secciones entre marcadores de docs/implementacion.md.

Campos de una [[tarea]]:
  obligatorios  id, nombre, titulo, fase, ejecutor (haiku | coordinador), depende, toca, acepta,
                objetivo, tests
  opcionales    contexto, firmas, lenguaje (de las firmas; por defecto rust), toca_extra, notas,
                papers, hito, talla (S | M | L), ejemplo, no_hagas, sin_firmas (motivo),
                motivo_coordinador, estado (sustituida | descartada | esbozo), por, motivo

Ids: F<fase>.<NN> para una tarea y F<fase>.<NN><letra> para una subtarea (F8.14a). La tarea madre
depende sola de sus subtareas y queda como tarea de cierre.

Estados (calculados):
  hecha       existe informes/<id>-<nombre>.md (y no declara `Estado: bloqueada`)
  sustituida  otra tarea cubre lo mismo (campos `por` y `motivo`); cuenta como cerrada
  descartada  ya no se hará (`motivo`); cuenta como cerrada
  esbozo      falta desgranarla: ningún agente debe empezarla
  lista       pendiente y con todas sus dependencias cerradas
  bloqueada   pendiente y con alguna dependencia abierta

Uso:
  tools/tareas/generar.py                  valida y regenera lo generado
  tools/tareas/generar.py --comprobar      valida y comprueba que lo generado está al día (CI)
  tools/tareas/generar.py --siguiente [N]  las N tareas listas más prioritarias (por defecto 5)
  tools/tareas/generar.py --estado         resumen por fase, hito y estado
  tools/tareas/generar.py --tarea F8.14    estado de una tarea y qué la bloquea
  tools/tareas/generar.py --lint a.toml    valida las tareas de un archivo suelto (borradores)
Con --siguiente, `--ejecutor haiku|coordinador` filtra y `--ids` imprime solo los ids.
"""
import argparse
import glob
import os
import re
import sys
import tomllib

RAIZ = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INDICE = os.path.join("tareas", "plan.toml")
DIR_PLAN = os.path.join("tareas", "plan")
DOC = os.path.join("docs", "implementacion.md")
MARCAS = {
    "tabla": ("<!-- tabla:inicio (generado por tools/tareas/generar.py; no editar a mano) -->", "<!-- tabla:fin -->"),
    "resumen": ("<!-- resumen:inicio (generado por tools/tareas/generar.py; no editar a mano) -->",
                "<!-- resumen:fin -->"),
}
OBLIGATORIOS = ("id", "nombre", "titulo", "fase", "ejecutor", "depende", "toca", "acepta", "objetivo", "tests")
CONOCIDOS = set(OBLIGATORIOS) | {
    "contexto", "firmas", "lenguaje", "toca_extra", "notas", "papers", "hito", "talla", "ejemplo", "no_hagas",
    "sin_firmas", "motivo_coordinador", "estado", "por", "motivo"}
RE_ID = re.compile(r"F(\d+)\.(\d\d)([a-z]?)")
ESTADOS_EXPLICITOS = ("sustituida", "descartada", "esbozo")
CERRADOS = ("hecha", "sustituida", "descartada")
EXT_CODIGO = (".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".kt", ".sh", ".ps1")
RAICES_REPO = ("crates/", "apps/", "tools/", "docs/", "tareas/", "informes/", "bench/", "packaging/", ".github/",
               ".claude/")
ARCHIVOS_RAIZ = ("AGENTS.md", "CLAUDE.md", "README.md", "CHANGELOG.md", "Cargo.toml", "Cargo.lock", "install.sh",
                 "install.ps1", "rust-toolchain.toml")
# Límites de una tarea para un modelo pequeño.
MAX_TOCA, MIN_OBJETIVO, MAX_TEXTO, MIN_DESC_TEST = 4, 300, 7000, 12


def fid(t):
    return f"{t['id']}-{t['nombre']}"


def clave(i):
    """Orden natural de un id: F9.02 < F9.02a < F9.10 < F10.01."""
    m = RE_ID.fullmatch(i or "")
    return (int(m.group(1)), int(m.group(2)), m.group(3)) if m else (10 ** 6, 0, i or "")


def madre(i):
    """Id de la tarea madre de una subtarea (F8.14a → F8.14) o None."""
    m = RE_ID.fullmatch(i or "")
    return f"F{m.group(1)}.{m.group(2)}" if m and m.group(3) else None


def cargar(raiz=RAIZ):
    """Devuelve (índice, tareas ordenadas). Las madres ganan como dependencia a sus subtareas."""
    with open(os.path.join(raiz, INDICE), "rb") as f:
        indice = tomllib.load(f)
    tareas = list(indice.pop("tarea", []))
    for ruta in sorted(glob.glob(os.path.join(raiz, DIR_PLAN, "*.toml"))):
        with open(ruta, "rb") as f:
            for t in tomllib.load(f).get("tarea", []):
                t["_archivo"] = os.path.relpath(ruta, raiz)
                tareas.append(t)
    return indice, completar(tareas)


def completar(tareas):
    """Ordena y añade a cada madre la dependencia de sus subtareas."""
    tareas.sort(key=lambda t: clave(t.get("id")))
    por_id = {t.get("id"): t for t in tareas}
    for t in tareas:
        m = madre(t.get("id"))
        if m in por_id and t["id"] not in por_id[m].setdefault("depende", []):
            por_id[m]["depende"].append(t["id"])
    return tareas


def informe_cierra(ruta):
    """Un informe cierra la tarea salvo que declare `Estado: bloqueada` (el agente paró sin terminarla)."""
    if not os.path.exists(ruta):
        return False
    with open(ruta, encoding="utf-8") as f:
        return not re.search(r"(?mi)^\s*estado:\s*bloquead[ao]\b", f.read())


def estados(tareas, raiz=RAIZ):
    """Estado calculado de cada tarea: {id: estado}."""
    ids = {t["id"]: t for t in tareas}
    est = {}
    for t in tareas:
        if informe_cierra(os.path.join(raiz, "informes", f"{fid(t)}.md")):
            est[t["id"]] = "hecha"
        elif t.get("estado") in ESTADOS_EXPLICITOS:
            est[t["id"]] = t["estado"]
    for t in tareas:
        if t["id"] not in est:
            abiertas = [d for d in t["depende"] if d in ids and est.get(d) not in CERRADOS]
            est[t["id"]] = "bloqueada" if abiertas else "lista"
    return est


def validar(tareas, indice):
    """Errores de estructura: valen para todas las tareas, hechas o no."""
    errores, ids = [], {}
    fases = {f["id"] for f in indice.get("fase", [])}
    hitos = {h["id"] for h in indice.get("hito", [])}
    for t in tareas:
        i = t.get("id", "?")
        for k in OBLIGATORIOS:
            if k not in t or t[k] in ("", None):
                if not (k == "toca" and t.get("toca") == []):
                    errores.append(f"{i}: falta `{k}`")
        for k in t:
            if k not in CONOCIDOS and not k.startswith("_"):
                errores.append(f"{i}: campo desconocido `{k}`")
        if i in ids:
            errores.append(f"{i}: id repetido")
        ids[i] = t
        m = RE_ID.fullmatch(i)
        if not m:
            errores.append(f"{i}: id con formato F<fase>.<NN> o F<fase>.<NN><letra>")
        elif t.get("fase") != f"F{m.group(1)}":
            errores.append(f"{i}: la fase debe ser F{m.group(1)}")
        if fases and t.get("fase") not in fases:
            errores.append(f"{i}: la fase {t.get('fase')} no está en tareas/plan.toml")
        if t.get("ejecutor") not in ("haiku", "coordinador"):
            errores.append(f"{i}: ejecutor debe ser haiku o coordinador")
        if t.get("ejecutor") == "haiku" and len(t.get("toca", [])) > MAX_TOCA:
            errores.append(f"{i}: una tarea para haiku toca como mucho {MAX_TOCA} archivos ({len(t['toca'])})")
        if "estado" in t and t["estado"] not in ESTADOS_EXPLICITOS:
            errores.append(f"{i}: estado debe ser uno de {', '.join(ESTADOS_EXPLICITOS)} (hecha se deduce del informe)")
        if t.get("estado") == "sustituida" and not (t.get("por") and t.get("motivo")):
            errores.append(f"{i}: una tarea sustituida necesita `por` (ids) y `motivo`")
        if t.get("estado") in ("descartada", "esbozo") and not t.get("motivo"):
            errores.append(f"{i}: una tarea {t['estado']} necesita `motivo`")
        if "hito" in t and hitos and t["hito"] not in hitos:
            errores.append(f"{i}: el hito {t['hito']} no está en tareas/plan.toml")
        if "talla" in t and t["talla"] not in ("S", "M", "L"):
            errores.append(f"{i}: talla debe ser S, M o L")
        if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", t.get("nombre", "")):
            errores.append(f"{i}: nombre en minúsculas, cifras y guiones")
    for t in tareas:
        m = madre(t["id"])
        if m and m not in ids:
            errores.append(f"{t['id']}: es subtarea de {m}, que no existe")
        for d in list(t.get("depende", [])) + list(t.get("por", [])):
            if d not in ids:
                errores.append(f"{t['id']}: menciona {d}, que no existe")
        for d in t.get("depende", []):
            if d in ids and ids[d]["fase"] == t["fase"] and clave(d) >= clave(t["id"]) and madre(d) != t["id"]:
                errores.append(f"{t['id']}: depende de {d}, que va después (orden de la tabla)")
    # Ciclos (DFS).
    marca = {}

    def visita(i, camino):
        if marca.get(i) == 1:
            errores.append("ciclo: " + " → ".join(camino + [i]))
            return
        if marca.get(i) == 2 or i not in ids:
            return
        marca[i] = 1
        for d in ids[i].get("depende", []):
            visita(d, camino + [i])
        marca[i] = 2

    for i in ids:
        visita(i, [])
    return errores


def rutas_citadas(texto):
    """Rutas del repo citadas en un texto (para comprobar que existen)."""
    encontradas = []
    for tok in re.findall(r"[\w./\-]+", texto):
        tok = tok.rstrip(".,:;")
        if "*" in tok or "<" in tok:
            continue
        if tok.startswith(RAICES_REPO) or tok in ARCHIVOS_RAIZ:
            encontradas.append(tok)
    return encontradas


def crea_antes(t, ids):
    """Archivos que tocan la tarea y sus dependencias (transitivas): pueden no existir todavía."""
    vistos, pila, archivos = set(), [t["id"]], set()
    while pila:
        i = pila.pop()
        if i in vistos or i not in ids:
            continue
        vistos.add(i)
        archivos.update(ids[i].get("toca", []), ids[i].get("toca_extra", []))
        pila.extend(ids[i].get("depende", []))
    return archivos


def calidad(tareas, est, indice, raiz=RAIZ, comprobar_rutas=True):
    """Errores de «apta para un modelo pequeño». Solo se exige a las tareas abiertas y desgranadas."""
    errores = []
    ids = {t["id"]: t for t in tareas}
    con_hijas = {madre(t["id"]) for t in tareas if madre(t["id"])}
    for t in tareas:
        i = t["id"]
        if est.get(i) in CERRADOS or est.get(i) == "esbozo":
            continue

        def e(msg):
            errores.append(f"{i}: {msg}")

        for d in t["depende"]:
            if est.get(d) in ("esbozo", "descartada"):
                e(f"depende de {d}, que está en estado {est[d]}")
        if not t.get("hito"):
            e("falta `hito` (en qué entrega entra)")
        if "\n" in t["acepta"].strip():
            e("`acepta` debe ser una sola línea")
        if re.search(r"\|\|\s*true\b|;\s*true\s*$", t["acepta"]):
            e("`acepta` no puede acabar siempre en éxito (`|| true`)")
        if t["ejecutor"] == "coordinador":
            if not t.get("motivo_coordinador"):
                e("una tarea de coordinador abierta explica en `motivo_coordinador` por qué no puede hacerla "
                  "un modelo pequeño (credenciales, servicios externos, decisión del propietario)")
            continue
        # A partir de aquí, tareas para el modelo pequeño.
        if t.get("talla") not in ("S", "M"):
            e("falta `talla` S (≤ 30 min) o M (30–90 min); si es L, desgránala en subtareas")
        cierre = i in con_hijas and not t["toca"]
        if not t["toca"] and not cierre:
            e("`toca` vacío: solo vale en una tarea de cierre con subtareas")
        if len(set(t["toca"])) != len(t["toca"]):
            e("`toca` repite archivos")
        if not t.get("contexto"):
            e("falta `contexto`: qué leer (archivo y función) antes de empezar")
        obj = t["objetivo"]
        pasos = re.findall(r"(?m)^\s*\d+\.\s", obj)
        if not cierre and (len(obj.strip()) < MIN_OBJETIVO or len(pasos) < 2):
            e(f"`objetivo` demasiado abierto: pasos numerados (≥ 2) y al menos {MIN_OBJETIVO} caracteres "
              f"(tiene {len(pasos)} pasos y {len(obj.strip())})")
        total = len(obj) + len(t.get("firmas", "")) + len(t.get("ejemplo", "")) + len(t.get("notas", ""))
        if total > MAX_TEXTO:
            e(f"tarea demasiado larga ({total} caracteres > {MAX_TEXTO}): desgránala")
        codigo = [p for p in t["toca"] if p.endswith(EXT_CODIGO)]
        if codigo and not t.get("firmas") and not t.get("sin_firmas"):
            e("toca código y no tiene `firmas` (o `sin_firmas = \"motivo\"` si no añade API)")
        texto = obj + "\n" + t.get("firmas", "") + "\n" + t.get("notas", "")
        for p in t["toca"]:
            if os.path.basename(p) not in texto:
                e(f"`{p}` está en `toca` pero ni el objetivo ni las firmas dicen qué hacer en él")
        for x in t["tests"]:
            if x.strip().startswith("("):
                continue
            nombre, _, desc = x.partition(":")
            if not re.fullmatch(r"[A-Za-z_][\w.:\- ]*", nombre.strip()):
                e(f"test con nombre no válido: `{nombre.strip()}`")
            if len(desc.strip()) < MIN_DESC_TEST:
                e(f"el test `{nombre.strip()}` no dice qué comprueba (`nombre: qué entra y qué se espera`)")
        if comprobar_rutas:
            previos = crea_antes(t, ids)
            for r in rutas_citadas(" ".join(t.get("contexto", []))):
                if not os.path.exists(os.path.join(raiz, r)) and r not in previos:
                    e(f"`contexto` cita `{r}`, que no existe ni lo crea una dependencia")
    return errores


# ── Generación ────────────────────────────────────────────────────────────────────────────────

def lista(xs):
    return "\n".join(f"- {x}" for x in xs) if xs else "- (nada)"


def comprobaciones(t):
    """Órdenes de comprobación previas a `acepta`, según lo que toca la tarea (AGENTS.md §4)."""
    rutas = t["toca"] + t.get("toca_extra", [])
    pasos = []
    if any(p.endswith(".rs") or p.endswith("Cargo.toml") for p in rutas):
        pasos += ["`cargo fmt --all`", "`cargo clippy --workspace --all-targets -- -D warnings`"]
    if any(p.startswith("apps/ui/") for p in rutas):
        pasos += ["`npm run -w apps/ui lint`", "`npm run -w apps/ui typecheck`", "`npm run -w apps/ui test`"]
    if any(p.startswith("tools/") and p.endswith(".py") for p in rutas):
        pasos += ["`python3 -m unittest <tu test> -v`"]
    if any(p.startswith("tareas/plan") for p in rutas):
        pasos += ["`tools/tareas/generar.py --comprobar`"]
    return pasos


def render(t, ids):
    deps = [fid(ids[d]) for d in t["depende"]]
    hijas = [fid(x) for x in ids.values() if madre(x["id"]) == t["id"]]
    modo = "aislado" if t["ejecutor"] == "haiku" else "coordinador"
    ficha = f"**Fase** {t['fase']} · **Ejecutor** {t['ejecutor']}"
    if t.get("hito"):
        ficha += f" · **Hito** {t['hito']}"
    if t.get("talla"):
        ficha += f" · **Talla** {t['talla']}"
    ficha += f" · **Depende de** {', '.join(deps) or '—'}"
    partes = [
        f"<!-- acepta: {t['acepta']} -->",
        f"<!-- modo: {modo} -->",
        f"<!-- depende: {', '.join(deps)} -->",
        f"# {fid(t)} — {t['titulo']}",
        "",
        ficha,
        "",
        "> Generado desde `tareas/plan/`. Sigue `AGENTS.md` y la skill `tarea-haiku`.",
        "",
    ]
    if t.get("estado") in ("sustituida", "descartada"):
        por = ", ".join(fid(ids[p]) for p in t.get("por", []))
        partes += [f"> **Tarea {t['estado']}{' por ' + por if por else ''}: no la ejecutes.** {t['motivo'].strip()}", ""]
    if t.get("estado") == "esbozo":
        partes += [f"> **Esbozo: no la ejecutes todavía.** Falta desgranarla. {t['motivo'].strip()}", ""]
    if t.get("motivo_coordinador"):
        partes += [f"> **Por qué es del coordinador:** {t['motivo_coordinador'].strip()}", ""]
    if hijas:
        partes += ["## Subtareas (hazlas antes; esta tarea solo cierra)", "", lista(f"`{h}`" for h in hijas), ""]
    partes += [
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
    if t.get("ejemplo"):
        partes += ["## Ejemplo (entrada y resultado esperado)", "", "```", t["ejemplo"].strip(), "```", ""]
    partes += ["## Tests (con estos nombres)", "", lista([f"`{x.split(':', 1)[0].strip()}`:"
                                                        f"{x.split(':', 1)[1] if ':' in x else ''}"
                                                        for x in t["tests"]]), ""]
    if t.get("no_hagas"):
        partes += ["## No hagas", "", lista(t["no_hagas"]), ""]
    if t.get("papers"):
        partes += ["## Papers (docs/papers.md)", "", lista(t["papers"]), ""]
    if t.get("notas"):
        partes += ["## Notas", "", t["notas"].strip(), ""]
    antes = comprobaciones(t)
    partes += [
        "## Aceptación (ejecútala una vez al final)",
        "",
        "```bash",
        t["acepta"],
        "```",
        "",
        (f"Antes: {', '.join(antes)}. " if antes else "")
        + f"Después: informe en `informes/{fid(t)}.md`, commit "
        f"`git add <toca> informes/{fid(t)}.md && git commit -m \"{fid(t)}: …\"` y `git push origin HEAD`.",
        "",
    ]
    return "\n".join(partes)


def tabla(tareas, est, indice):
    titulos = {f["id"]: f.get("titulo", "") for f in indice.get("fase", [])}
    filas, fase = [], None
    for t in tareas:
        if t["fase"] != fase:
            fase = t["fase"]
            nombre = f"{fase} — {titulos[fase]}" if titulos.get(fase) else fase
            filas += ["", f"### {nombre}", "", "| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |",
                      "|---|---|---|---|---|---|---|"]
        filas.append(f"| [{fid(t)}](../tareas/{fid(t)}.md) | {t['titulo']} | {t['ejecutor']} | "
                     f"{t.get('talla', '—')} | {t.get('hito', '—')} | {', '.join(t['depende']) or '—'} | "
                     f"{est[t['id']]} |")
    return "\n".join(filas).strip()


def cuenta(tareas, est, filtro):
    c = {}
    for t in tareas:
        if filtro(t):
            c[est[t["id"]]] = c.get(est[t["id"]], 0) + 1
    return c


def resumen(tareas, est, indice):
    cols = ("hecha", "sustituida", "descartada", "lista", "bloqueada", "esbozo")
    cab = "| " + " | ".join(c.capitalize() for c in cols) + " | Total |"
    sep = "|---" * (len(cols) + 2) + "|"

    def fila(nombre, c):
        return f"| {nombre} | " + " | ".join(str(c.get(k, 0)) for k in cols) + f" | {sum(c.values())} |"

    out = ["### Por fase", "", "| Fase " + cab, sep]
    for f in indice.get("fase", []):
        out.append(fila(f"{f['id']} {f.get('titulo', '')}".strip(), cuenta(tareas, est, lambda t: t["fase"] == f["id"])))
    out.append(fila("**Total**", cuenta(tareas, est, lambda t: True)))
    out += ["", "### Por hito (solo tareas abiertas)", "", "| Hito | Qué entrega | Listas | Bloqueadas | Esbozos | "
            "Para haiku | Para coordinador |", "|---|---|---|---|---|---|---|"]
    for h in indice.get("hito", []):
        ab = [t for t in tareas if t.get("hito") == h["id"] and est[t["id"]] not in CERRADOS]
        n = lambda k: sum(1 for t in ab if est[t["id"]] == k)  # noqa: E731
        out.append(f"| **{h['id']}** | {h.get('titulo', '')} | {n('lista')} | {n('bloqueada')} | {n('esbozo')} | "
                   f"{sum(1 for t in ab if t['ejecutor'] == 'haiku')} | "
                   f"{sum(1 for t in ab if t['ejecutor'] == 'coordinador')} |")
    sig = siguientes(tareas, est, indice)
    out += ["", "### Qué se puede empezar ahora", "",
            "Tareas **listas** (dependencias cerradas), por orden de hito y de cuántas desbloquean. La lista al día "
            "sale de `tools/tareas/generar.py --siguiente`.", "", "| Tarea | Qué | Ejecutor | Talla | Hito | Desbloquea |",
            "|---|---|---|---|---|---|"]
    for t, n in sig[:25]:
        out.append(f"| [{fid(t)}](../tareas/{fid(t)}.md) | {t['titulo']} | {t['ejecutor']} | {t.get('talla', '—')} | "
                   f"{t.get('hito', '—')} | {n} |")
    if len(sig) > 25:
        out.append(f"\n…y {len(sig) - 25} más.")
    return "\n".join(out)


def siguientes(tareas, est, indice, ejecutor=None):
    """Tareas listas, ordenadas por hito, por cuántas tareas abiertas desbloquean y por id."""
    orden_hito = {h["id"]: n for n, h in enumerate(indice.get("hito", []))}
    dependientes = {}
    for t in tareas:
        if est[t["id"]] not in CERRADOS:
            for d in t["depende"]:
                dependientes.setdefault(d, set()).add(t["id"])

    def alcance(i):
        vistos, pila = set(), [i]
        while pila:
            for x in dependientes.get(pila.pop(), ()):
                if x not in vistos:
                    vistos.add(x)
                    pila.append(x)
        return len(vistos)

    listas = [(t, alcance(t["id"])) for t in tareas
              if est[t["id"]] == "lista" and (ejecutor is None or t["ejecutor"] == ejecutor)]
    listas.sort(key=lambda par: (orden_hito.get(par[0].get("hito"), 10 ** 6), -par[1], clave(par[0]["id"])))
    return listas


def leer(ruta):
    with open(ruta, encoding="utf-8") as f:
        return f.read()


def escribir(ruta, texto):
    with open(ruta, "w", encoding="utf-8") as f:
        f.write(texto)


def sustituir(doc, marca, cuerpo):
    ini, fin = MARCAS[marca]
    if ini not in doc or fin not in doc:
        raise SystemExit(f"[tareas] ERROR falta el marcador «{marca}» en {DOC}")
    i, j = doc.index(ini) + len(ini), doc.index(fin)
    return doc[:i] + "\n" + cuerpo + "\n" + doc[j:]


def generar(tareas, est, indice, raiz, comprobar):
    """Escribe (o solo compara) lo generado. Devuelve la lista de archivos desfasados."""
    ids = {t["id"]: t for t in tareas}
    desfasados = []
    esperados = {f"{fid(t)}.md": render(t, ids) for t in tareas}
    for nombre, txt in esperados.items():
        ruta = os.path.join(raiz, "tareas", nombre)
        if not os.path.exists(ruta) or leer(ruta) != txt:
            desfasados.append(nombre)
            if not comprobar:
                escribir(ruta, txt)
    for viejo in sorted(os.listdir(os.path.join(raiz, "tareas"))):
        if viejo.endswith(".md") and viejo not in esperados:
            desfasados.append(f"{viejo} (sobra)")
            if not comprobar:
                os.remove(os.path.join(raiz, "tareas", viejo))
    doc = leer(os.path.join(raiz, DOC))
    nuevo = sustituir(sustituir(doc, "tabla", tabla(tareas, est, indice)), "resumen", resumen(tareas, est, indice))
    if nuevo != doc:
        desfasados.append(DOC)
        if not comprobar:
            escribir(os.path.join(raiz, DOC), nuevo)
    return desfasados


def explicar(t, tareas, est):
    ids = {x["id"]: x for x in tareas}
    print(f"{fid(t)} — {t['titulo']}")
    print(f"  estado: {est[t['id']]} · ejecutor: {t['ejecutor']} · hito: {t.get('hito', '—')} · "
          f"talla: {t.get('talla', '—')}")
    for d in t["depende"]:
        print(f"  depende de {fid(ids[d])}: {est[d]}")
    usan = [fid(x) for x in tareas if t["id"] in x["depende"]]
    if usan:
        print("  la esperan: " + ", ".join(usan))
    print(f"  archivo: tareas/{fid(t)}.md")


def main(argv=None, raiz=RAIZ):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--comprobar", action="store_true", help="no escribe; falla si lo generado está desfasado")
    ap.add_argument("--siguiente", nargs="?", const=5, type=int, metavar="N", help="tareas listas más prioritarias")
    ap.add_argument("--ejecutor", choices=("haiku", "coordinador"), help="filtro de --siguiente")
    ap.add_argument("--ids", action="store_true", help="con --siguiente, imprime solo los ids")
    ap.add_argument("--estado", action="store_true", help="resumen por fase e hito")
    ap.add_argument("--tarea", metavar="ID", help="estado de una tarea y qué la bloquea")
    ap.add_argument("--lint", metavar="ARCHIVO", help="valida las tareas de un archivo .toml suelto")
    a = ap.parse_args(argv)

    indice, tareas = cargar(raiz)
    if a.lint:
        with open(a.lint, "rb") as f:
            nuevas = tomllib.load(f).get("tarea", [])
        # El borrador manda sobre el plan: sustituye las tareas con el mismo id.
        propios = {t.get("id") for t in nuevas}
        tareas = completar([t for t in tareas if t["id"] not in propios] + nuevas)
    errores = validar(tareas, indice)
    est = {} if errores else estados(tareas, raiz)
    if not errores:
        errores = calidad(tareas, est, indice, raiz)
        if a.lint:
            errores = [e for e in errores if e.split(":")[0] in propios]
    if errores:
        print("\n".join(f"[tareas] ERROR {e}" for e in errores), file=sys.stderr)
        print(f"[tareas] {len(errores)} errores", file=sys.stderr)
        return 1
    if a.lint:
        print(f"[tareas] {len(propios)} tareas de {a.lint}: correctas", file=sys.stderr)
        return 0
    if a.tarea:
        t = next((x for x in tareas if a.tarea in (x["id"], fid(x))), None)
        if not t:
            print(f"[tareas] ERROR no existe la tarea {a.tarea}", file=sys.stderr)
            return 1
        explicar(t, tareas, est)
        return 0
    if a.siguiente is not None:
        for t, n in siguientes(tareas, est, indice, a.ejecutor)[:a.siguiente]:
            print(t["id"] if a.ids else f"{fid(t)}\t{t['ejecutor']}\t{t.get('talla', '—')}\thito {t.get('hito', '—')}"
                                        f"\tdesbloquea {n}\ttareas/{fid(t)}.md")
        return 0
    if a.estado:
        print(resumen(tareas, est, indice))
        return 0
    desfasados = generar(tareas, est, indice, raiz, a.comprobar)
    n_h = sum(1 for t in tareas if t["ejecutor"] == "haiku")
    c = cuenta(tareas, est, lambda t: True)
    print(f"[tareas] {len(tareas)} tareas ({n_h} para haiku, {len(tareas) - n_h} para coordinador): "
          + ", ".join(f"{v} {k}" for k, v in sorted(c.items())), file=sys.stderr)
    if a.comprobar and desfasados:
        print("[tareas] desfasado: " + ", ".join(desfasados) + " → ejecuta tools/tareas/generar.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
