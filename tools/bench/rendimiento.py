#!/usr/bin/env python3
"""Mide el rendimiento de pcode y lo compara con una base o con un presupuesto.

Uso:
    rendimiento.py medir --pcode BIN [--proyecto DIR] [--repeticiones N] [--salida actual.json]
    rendimiento.py comparar --base base.json --actual actual.json [--umbral 0.2]
    rendimiento.py presupuesto --actual actual.json
    rendimiento.py medir-ide --pcode BIN [--proyecto DIR] [--espera S]
    rendimiento.py procesos PATRON [PATRON ...]

`medir` saca, en Linux:
  - arranque_ms: mediana de `pcode --version` (arranque en frío del binario).
  - studio_listo_ms: desde lanzar `pcode studio --no-abrir` hasta que `/` responde 200.
  - reposo_mb: memoria (PSS, o RSS si no hay smaps) del demonio y sus hijos tras unas
    peticiones típicas (árbol de archivos, búsqueda, git) y un segundo en reposo.
  - pico_mb: pico de memoria residente del demonio (VmHWM).
  - hilos: hilos del demonio.

`comparar` sale con 1 si alguna métrica empeora más del umbral (20 % por defecto) respecto a la base.
`presupuesto` sale con 1 si se pasa de los límites absolutos (PRESUPUESTO).
`procesos` suma la memoria de los procesos cuya orden contiene PATRON (para comparar con otros
editores: `rendimiento.py procesos /usr/share/code opencode`).
"""
import argparse
import json
import os
import re
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request

# Límites absolutos (docs/rendimiento.md): lo que se compromete para un equipo modesto.
PRESUPUESTO = {"arranque_ms": 50.0, "studio_listo_ms": 300.0, "reposo_mb": 60.0, "ide_reposo_mb": 120.0}

# Métricas en las que menos es mejor (todas las que se comparan).
METRICAS = ["arranque_ms", "studio_listo_ms", "reposo_mb", "pico_mb", "ide_reposo_mb"]

ESPERA_MAX_S = 20.0  # nada espera más que esto


def memoria_kb(pid):
    """(pss_kb, rss_kb) de un proceso; pss es None si no se puede leer smaps_rollup."""
    rss = pss = None
    try:
        with open(f"/proc/{pid}/status", encoding="utf-8") as f:
            for l in f:
                if l.startswith("VmRSS:"):
                    rss = int(l.split()[1])
    except OSError:
        return None, None
    try:
        with open(f"/proc/{pid}/smaps_rollup", encoding="utf-8") as f:
            for l in f:
                if l.startswith("Pss:"):
                    pss = int(l.split()[1])
    except OSError:
        pass
    return pss, rss


def campo_status(pid, campo):
    """Valor numérico de un campo de /proc/<pid>/status (VmHWM, Threads…) o None."""
    try:
        with open(f"/proc/{pid}/status", encoding="utf-8") as f:
            for l in f:
                if l.startswith(campo + ":"):
                    return int(l.split()[1])
    except OSError:
        return None
    return None


def hijos(pid):
    """PIDs de todos los descendientes de `pid` (leyendo el ppid de /proc/*/stat)."""
    padres = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/stat", encoding="utf-8") as f:
                # El nombre va entre paréntesis y puede tener espacios: se corta tras el último ')'.
                resto = f.read().rsplit(")", 1)[1].split()
            padres.setdefault(int(resto[1]), []).append(int(d))
        except (OSError, IndexError, ValueError):
            continue
    out, pend = [], [pid]
    while pend and len(out) < 10000:
        p = pend.pop()
        for h in padres.get(p, []):
            out.append(h)
            pend.append(h)
    return out


def memoria_arbol_mb(pid):
    """Memoria del proceso y sus descendientes en MB (PSS si se puede; si no, RSS)."""
    total = 0
    for p in [pid] + hijos(pid):
        pss, rss = memoria_kb(p)
        total += pss if pss is not None else (rss or 0)
    return round(total / 1024, 1)


def mediana_arranque_ms(pcode, n):
    """Mediana en ms de `pcode --version`."""
    tiempos = []
    for _ in range(n):
        t = time.perf_counter()
        subprocess.run([pcode, "--version"], check=True, capture_output=True, timeout=ESPERA_MAX_S)
        tiempos.append((time.perf_counter() - t) * 1000)
    return round(statistics.median(tiempos), 1)


def rpc(base, token, metodo, params):
    """Llamada JSON-RPC al demonio; devuelve el `result` o None."""
    cuerpo = json.dumps({"jsonrpc": "2.0", "id": 1, "method": metodo, "params": params}).encode()
    req = urllib.request.Request(
        base + "/rpc", data=cuerpo,
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
    )
    try:
        with urllib.request.urlopen(req, timeout=ESPERA_MAX_S) as r:
            return json.loads(r.read()).get("result")
    except (urllib.error.URLError, OSError, ValueError):
        return None


def medir_studio(pcode, proyecto):
    """Arranca Studio sin navegador y mide el tiempo hasta servir `/`, la memoria y los hilos."""
    t0 = time.perf_counter()
    p = subprocess.Popen(
        [pcode, "studio", "--no-abrir", "--puerto", "0"], cwd=proyecto,
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True,
    )
    try:
        url = None
        limite = time.monotonic() + ESPERA_MAX_S
        while url is None and time.monotonic() < limite:
            linea = p.stderr.readline()
            if not linea:
                break
            m = re.search(r"(http://[\d.:]+)/\?t=([0-9a-f]+)", linea)
            if m:
                url = m.groups()
        if url is None:
            raise RuntimeError("pcode studio no imprimió su dirección")
        base, token = url
        listo = None
        while time.monotonic() < limite:
            try:
                with urllib.request.urlopen(f"{base}/?t={token}", timeout=2) as r:
                    if r.status == 200:
                        r.read()
                        listo = (time.perf_counter() - t0) * 1000
                        break
            except (urllib.error.URLError, OSError):
                time.sleep(0.01)
        if listo is None:
            raise RuntimeError("Studio no respondió a tiempo")
        # Lo que hace la página al abrirse: info, árbol, archivos, git y una búsqueda.
        rpc(base, token, "server.info", {})
        rpc(base, token, "fs.list", {"path": ""})
        rpc(base, token, "fs.files", {})
        rpc(base, token, "git.status", {})
        rpc(base, token, "search.text", {"query": "fn main"})
        time.sleep(1.0)
        return {
            "studio_listo_ms": round(listo, 1),
            "reposo_mb": memoria_arbol_mb(p.pid),
            "pico_mb": round((campo_status(p.pid, "VmHWM") or 0) / 1024, 1),
            "hilos": campo_status(p.pid, "Threads") or 0,
        }
    finally:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait(timeout=5)


def medir(pcode, proyecto, repeticiones=7):
    """Todas las métricas de `pcode` en `proyecto`."""
    r = {"arranque_ms": mediana_arranque_ms(pcode, repeticiones)}
    r.update(medir_studio(pcode, proyecto))
    return r


def medir_ide(pcode, proyecto, espera=5.0):
    """Memoria del demonio + IDE nativo (`pcode ide`) tras `espera` s; None si no hay pantalla."""
    import shutil
    orden = [pcode, "ide"]
    if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
        if not shutil.which("xvfb-run"):
            return None
        orden = ["xvfb-run", "-a"] + orden
    p = subprocess.Popen(orden, cwd=proyecto, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(espera)
        return {"ide_reposo_mb": memoria_arbol_mb(p.pid)}
    finally:
        p.terminate()
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait(timeout=5)


def comparar(base, actual, umbral=0.2):
    """Lista de textos con lo que empeora más de `umbral` (fracción) respecto a la base."""
    malos = []
    for m in METRICAS:
        b, a = base.get(m), actual.get(m)
        if b is None or a is None or b <= 0:
            continue
        if a > b * (1 + umbral):
            malos.append(f"{m}: {a} > {b} (+{(a / b - 1) * 100:.0f} %, máximo +{umbral * 100:.0f} %)")
    return malos


def fuera_de_presupuesto(actual, presupuesto=None):
    """Lista de textos con lo que pasa del presupuesto absoluto."""
    pres = presupuesto or PRESUPUESTO
    return [f"{m}: {actual[m]} > {lim}" for m, lim in pres.items() if m in actual and actual[m] > lim]


def procesos(patrones):
    """{patrón: (n_procesos, mb)} con la memoria (PSS si se puede) de los procesos que coinciden."""
    out = {pat: [0, 0] for pat in patrones}
    for d in os.listdir("/proc"):
        if not d.isdigit() or int(d) == os.getpid():
            continue
        try:
            with open(f"/proc/{d}/cmdline", "rb") as f:
                cmd = f.read().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            continue
        for pat in patrones:
            if pat in cmd and "rendimiento.py" not in cmd:
                pss, rss = memoria_kb(d)
                out[pat][0] += 1
                out[pat][1] += pss if pss is not None else (rss or 0)
                break
    return {k: (n, round(kb / 1024, 1)) for k, (n, kb) in out.items()}


def leer_json(ruta):
    with open(ruta, encoding="utf-8") as f:
        return json.load(f)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="orden", required=True)
    m = sub.add_parser("medir")
    m.add_argument("--pcode", default="pcode")
    m.add_argument("--proyecto", default=".")
    m.add_argument("--repeticiones", type=int, default=7)
    m.add_argument("--salida")
    c = sub.add_parser("comparar")
    c.add_argument("--base", required=True)
    c.add_argument("--actual", required=True)
    c.add_argument("--umbral", type=float, default=0.2)
    mi = sub.add_parser("medir-ide")
    mi.add_argument("--pcode", default="pcode")
    mi.add_argument("--proyecto", default=".")
    mi.add_argument("--espera", type=float, default=5.0)
    p = sub.add_parser("presupuesto")
    p.add_argument("--actual", required=True)
    pr = sub.add_parser("procesos")
    pr.add_argument("patrones", nargs="+")
    a = ap.parse_args(argv)

    if a.orden == "medir":
        r = medir(a.pcode, a.proyecto, a.repeticiones)
        texto = json.dumps(r, indent=2, ensure_ascii=False)
        if a.salida:
            with open(a.salida, "w", encoding="utf-8") as f:
                f.write(texto + "\n")
        print(texto)
        return 0
    if a.orden == "medir-ide":
        r = medir_ide(a.pcode, a.proyecto, a.espera)
        print("sin pantalla: se salta" if r is None else json.dumps(r, indent=2))
        return 0
    if a.orden == "procesos":
        for pat, (n, mb) in procesos(a.patrones).items():
            print(f"{pat}: {n} procesos · {mb} MB")
        return 0
    actual = leer_json(a.actual)
    malos = comparar(leer_json(a.base), actual, a.umbral) if a.orden == "comparar" else fuera_de_presupuesto(actual)
    for l in malos:
        print("✗ " + l)
    if not malos:
        print("✓ dentro de lo previsto")
    return 1 if malos else 0


if __name__ == "__main__":
    sys.exit(main())
