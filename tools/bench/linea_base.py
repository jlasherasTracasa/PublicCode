#!/usr/bin/env python3
"""Línea base del flujo de agentes de SO_IA (opencode + GLM + ci/orquestador.py).

Lee `ci/estado.json` y `ci/orquestador.log` de un clon de SO_IA e imprime las métricas contra las
que se compara PublicCode (docs/evaluacion.md).

Uso: tools/bench/linea_base.py --repo ../SO_IA [--json]
"""
import argparse, collections, json, os, re, subprocess, sys


def medir(repo):
    estado = json.load(open(os.path.join(repo, "ci/estado.json")))
    tareas = {t: v for t, v in estado.items() if isinstance(v, dict)}
    intentos = collections.Counter(v.get("intentos", 1) for v in tareas.values())
    estados = collections.Counter(v.get("estado") for v in tareas.values())
    log = open(os.path.join(repo, "ci/orquestador.log"), encoding="utf-8", errors="replace").read()
    cuenta = lambda patron: len(re.findall(patron, log))
    total = len(tareas)
    primera = sum(1 for v in tareas.values() if v.get("estado") == "hecha" and v.get("intentos", 1) == 1)
    try:
        commit = subprocess.run(["git", "-C", repo, "rev-parse", "--short", "HEAD"], capture_output=True,
                                text=True).stdout.strip()
    except OSError:
        commit = "?"
    return {
        "commit": commit,
        "tareas": total,
        "hechas": estados.get("hecha", 0),
        "bloqueadas": estados.get("bloqueada", 0),
        "aceptadas_a_la_primera": primera,
        "pct_primera": round(100 * primera / total, 1) if total else 0.0,
        "intentos": dict(sorted(intentos.items())),
        "tareas_con_3_o_mas_intentos": sum(n for k, n in intentos.items() if k >= 3),
        "tareas_ci_arreglo": sum(1 for t in tareas if t.startswith("CI-arreglo")),
        "aceptacion_ok_en_log": cuenta(r"aceptación OK"),
        "aceptacion_fallo_en_log": cuenta(r"aceptación FALLO"),
        "integraciones_fallidas_en_log": cuenta(r"integrar \S+ falló"),
        "reinicios_orquestador_en_log": cuenta(r"murió"),
        "lineas_de_pegamento": sum(sum(1 for _ in open(os.path.join(repo, "ci", f)))
                                   for f in ("orquestador.py", "orquestador.sh", "agente.sh", "integrar.sh",
                                             "sync.sh") if os.path.exists(os.path.join(repo, "ci", f))),
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", required=True, help="ruta a un clon de SO_IA")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    m = medir(a.repo)
    if a.json:
        print(json.dumps(m, ensure_ascii=False, indent=1))
        return
    print(f"Línea base SO_IA @ {m['commit']}\n")
    print("| Métrica | Valor |\n|---|---|")
    for k, v in m.items():
        if k != "commit":
            print(f"| {k.replace('_', ' ')} | {v} |")


if __name__ == "__main__":
    sys.exit(main())
