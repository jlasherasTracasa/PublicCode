#!/usr/bin/env python3
"""Busca en la ACL Anthology los papers de ACL/EMNLP (u otro evento) relevantes para PublicCode.

Descarga los .bib de cada volumen del evento (con caché), extrae título, autores, resumen y URL,
y puntúa cada paper por temas del plan (agentes de código, subagentes, tools, skills, contexto,
memoria, verificación, sandbox/seguridad, evaluación, modelos pequeños).

Uso:
  tools/papers/buscar_papers.py                          # ACL 2026 + EMNLP 2026, top 60 en Markdown
  tools/papers/buscar_papers.py --evento acl-2026 --top 0 --formato jsonl > candidatos.jsonl
  tools/papers/buscar_papers.py --tema sandbox --tema skills --min 3

Solo biblioteca estándar. Nunca inventa nada: todo lo que imprime sale del .bib oficial.
"""
import argparse, json, os, re, sys, urllib.error, urllib.request

ANTHOLOGY = "https://aclanthology.org/volumes/{vol}.bib"  # sin resúmenes, pero pequeño
ANTHOLOGY_RESUMENES = "https://aclanthology.org/anthology+abstracts.bib.gz"
CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".cache")

# Volúmenes que puede tener un evento. Los que no existan (404) se ignoran.
VOLUMENES = {
    "acl": ["acl-long", "acl-short", "findings-acl", "acl-demo", "acl-demos", "acl-industry", "acl-srw"],
    "emnlp": ["emnlp-main", "findings-emnlp", "emnlp-demos", "emnlp-industry"],
    "naacl": ["naacl-long", "naacl-short", "findings-naacl", "naacl-demos", "naacl-industry"],
    "eacl": ["eacl-long", "eacl-short", "findings-eacl", "eacl-demo"],
}

# Tema -> (peso, patrones). Los patrones son regex sin distinguir mayúsculas sobre título + resumen.
TEMAS = {
    "agente-codigo": (3, [r"\bcoding agents?\b", r"\bcode agents?\b", r"software engineering agents?",
                          r"\bSWE-?bench", r"issue resolution", r"repository-level", r"\bSWE agents?\b",
                          r"agentic coding", r"program repair"]),
    "subagentes": (3, [r"multi-agent", r"\bsub-?agents?\b", r"orchestrat\w*", r"delegat\w*",
                       r"hierarchical agents?", r"planner[- ]executor", r"agent teams?"]),
    "tools": (2, [r"tool[- ]use", r"tool[- ]calling", r"function[- ]calling", r"tool learning",
                  r"\bMCP\b", r"model context protocol", r"tool selection", r"tool retrieval", r"\bAPI calls?\b"]),
    "skills": (3, [r"\bskill librar\w*", r"\bagent skills?\b", r"procedural memory", r"workflow memory",
                   r"reusable (skills|workflows|procedures)", r"skill (discovery|acquisition|induction)"]),
    "contexto": (2, [r"context compression", r"prompt compression", r"context management",
                     r"context (window )?engineering", r"KV[- ]cache", r"long[- ]horizon", r"context pruning",
                     r"observation (pruning|compression)"]),
    "memoria": (2, [r"agent memory", r"episodic memory", r"long-term memory", r"memory (bank|management)",
                    r"experience (replay|library|reuse)", r"self-evolving", r"self-improving"]),
    "verificacion": (2, [r"self-debug\w*", r"self-repair", r"execution feedback", r"unit tests?",
                         r"test generation", r"\bverifier\b", r"process reward", r"self-verification",
                         r"reflection"]),
    "sandbox-seguridad": (3, [r"\bsandbox\w*", r"prompt injection", r"agent (safety|security)",
                              r"guardrails?", r"permission\w*", r"least privilege", r"red[- ]team\w*",
                              r"jailbreak"]),
    "evaluacion": (2, [r"agent benchmark", r"benchmark\w* (for|of) (llm )?agents", r"trajector(y|ies)",
                       r"LLM-as-a-judge", r"terminal-bench", r"\bagentic benchmark"]),
    "codigo-rag": (2, [r"code retrieval", r"code search", r"repository context", r"retrieval-augmented code",
                       r"code (graph|knowledge graph)", r"repo(sitory)? map"]),
    "modelos-pequenos": (2, [r"small language models?", r"\bSLMs?\b", r"distill\w*", r"efficient agents?",
                             r"cost-efficient", r"model routing", r"\brouter\b", r"cascade"]),
}
# Para entrar en la lista, el paper debe tocar agentes, herramientas o código.
ANCLA = re.compile(r"\bagent|\btool|\bcode\b|coding|software|program", re.I)


def con_resumenes(eventos, refrescar=False):
    """Entradas de los eventos pedidos sacadas de anthology+abstracts.bib.gz (único bib con resúmenes).

    Se descarga una vez (~47 MB) y se guarda filtrado por evento en la caché."""
    os.makedirs(CACHE, exist_ok=True)
    salida, faltan = {}, []
    for ev in eventos:
        ruta = os.path.join(CACHE, f"{ev}+resumenes.bib")
        if os.path.exists(ruta) and not refrescar:
            salida[ev] = open(ruta, encoding="utf-8").read()
        else:
            faltan.append(ev)
    if faltan:
        gz = os.path.join(CACHE, "anthology+abstracts.bib.gz")
        if not os.path.exists(gz) or refrescar:
            print(f"[papers] descargando {ANTHOLOGY_RESUMENES}", file=sys.stderr)
            urllib.request.urlretrieve(ANTHOLOGY_RESUMENES, gz)
        import gzip
        trozos = {ev: [] for ev in faltan}
        prefijos = {ev: tuple(f"aclanthology.org/{ev.rsplit('-', 1)[1]}.{suf}."
                              for suf in VOLUMENES.get(ev.rsplit("-", 1)[0], []))
                    for ev in faltan}
        with gzip.open(gz, "rt", encoding="utf-8") as f:
            for trozo in f.read().split("\n@"):
                m = re.search(r'url = "https?://(aclanthology\.org/[^"]+)"', trozo)
                if not m:
                    continue
                for ev, pref in prefijos.items():
                    if m.group(1).startswith(pref):
                        trozos[ev].append("@" + trozo.lstrip("@"))
        for ev in faltan:
            salida[ev] = "\n".join(trozos[ev])
            open(os.path.join(CACHE, f"{ev}+resumenes.bib"), "w", encoding="utf-8").write(salida[ev])
    return salida


def descargar(vol, refrescar=False):
    os.makedirs(CACHE, exist_ok=True)
    ruta = os.path.join(CACHE, f"{vol}.bib")
    if os.path.exists(ruta) and not refrescar:
        return open(ruta, encoding="utf-8").read()
    try:
        with urllib.request.urlopen(ANTHOLOGY.format(vol=vol), timeout=60) as r:
            txt = r.read().decode("utf-8")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise
    open(ruta, "w", encoding="utf-8").write(txt)
    return txt


def entradas(bib):
    """Parser mínimo de BibTeX: devuelve dicts con la clave y los campos (valores sin llaves)."""
    i, n = 0, len(bib)
    while True:
        i = bib.find("@", i)
        if i < 0:
            return
        abre = bib.find("{", i)
        if abre < 0:
            return
        tipo = bib[i + 1:abre].strip().lower()
        coma = bib.find(",", abre)
        e = {"_tipo": tipo, "_clave": bib[abre + 1:coma].strip()}
        j = coma + 1
        while j < n:
            m = re.compile(r"\s*([A-Za-z_]+)\s*=\s*").match(bib, j)
            if not m:
                break
            campo, j = m.group(1).lower(), m.end()
            if bib[j] in "{\"":
                cierre, prof, k = "}" if bib[j] == "{" else "\"", 0, j + 1
                while k < n:
                    c = bib[k]
                    if c == "\\":
                        k += 2
                        continue
                    if c == "{":
                        prof += 1
                    elif c == "}" and prof > 0:
                        prof -= 1
                    elif c == cierre and prof == 0:
                        break
                    k += 1
                valor, j = bib[j + 1:k], k + 1
            else:
                m2 = re.compile(r"[^,}\s]+").match(bib, j)
                valor, j = (m2.group(0), m2.end()) if m2 else ("", j)
            e[campo] = re.sub(r"\s+", " ", valor.replace("{", "").replace("}", "")).strip()
            m3 = re.compile(r"\s*,").match(bib, j)
            if m3:
                j = m3.end()
            if re.compile(r"\s*}").match(bib, j):
                break
        yield e
        i = j


def puntuar(e, temas):
    texto = f"{e.get('title', '')} {e.get('abstract', '')}"
    if not ANCLA.search(texto):
        return 0, []
    total, tocados = 0, []
    for tema, (peso, patrones) in TEMAS.items():
        if temas and tema not in temas:
            continue
        hits = sum(1 for p in patrones if re.search(p, texto, re.I))
        if hits:
            # El título pesa el doble: es la señal más fuerte de que el tema es central.
            en_titulo = any(re.search(p, e.get("title", ""), re.I) for p in patrones)
            total += peso * min(hits, 3) + (peso * 2 if en_titulo else 0)
            tocados.append(tema)
    return total, tocados


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--evento", action="append", help="p. ej. acl-2026, emnlp-2026 (repetible)")
    ap.add_argument("--tema", action="append", choices=sorted(TEMAS), help="limitar a estos temas")
    ap.add_argument("--min", type=int, default=6, help="puntuación mínima (por defecto 6)")
    ap.add_argument("--top", type=int, default=60, help="cuántos mostrar (0 = todos)")
    ap.add_argument("--formato", choices=["md", "jsonl"], default="md")
    ap.add_argument("--refrescar", action="store_true", help="ignorar la caché")
    ap.add_argument("--solo-titulos", action="store_true",
                    help="usar los .bib por volumen (rápido, sin resúmenes: puntúa solo con el título)")
    a = ap.parse_args()
    eventos = a.evento or ["acl-2026", "emnlp-2026"]
    if a.solo_titulos:
        bibs = []
        for ev in eventos:
            conf, anio = ev.rsplit("-", 1)
            for suf in VOLUMENES.get(conf, [f"{conf}-long", f"{conf}-main"]):
                bib = descargar(f"{anio}.{suf}", a.refrescar)
                if bib is not None:
                    bibs.append(bib)
    else:
        bibs = list(con_resumenes(eventos, a.refrescar).values())
    res, vistos, n = [], set(), 0
    for bib in bibs:
        for e in entradas(bib):
            if e["_tipo"] != "inproceedings" or e["_clave"] in vistos:
                continue
            vistos.add(e["_clave"])
            n += 1
            p, tocados = puntuar(e, set(a.tema or []))
            if p >= a.min:
                url = e.get("url", "")
                aid = url.rstrip("/").rsplit("/", 1)[-1] if url else e["_clave"]
                res.append({"id": aid, "clave": e["_clave"], "volumen": aid.rsplit(".", 1)[0],
                            "puntos": p, "temas": tocados, "titulo": e.get("title", ""),
                            "autores": e.get("author", ""), "url": url or f"https://aclanthology.org/{aid}/",
                            "resumen": e.get("abstract", "")})
    print(f"[papers] {n} papers leídos de {', '.join(eventos)}; {len(res)} con puntuación ≥ {a.min}",
          file=sys.stderr)
    res.sort(key=lambda r: (-r["puntos"], r["id"]))
    if a.top:
        res = res[:a.top]
    if a.formato == "jsonl":
        for r in res:
            print(json.dumps(r, ensure_ascii=False))
        return
    print("| # | Paper | Volumen | Temas | Puntos |\n|---|---|---|---|---|")
    for k, r in enumerate(res, 1):
        print(f"| {k} | [{r['titulo']}]({r['url']}) | {r['volumen']} | {', '.join(r['temas'])} | {r['puntos']} |")


if __name__ == "__main__":
    main()
