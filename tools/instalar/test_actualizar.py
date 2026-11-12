#!/usr/bin/env python3
"""Tests de tools/actualizar-local.sh (python3 -m unittest tools/instalar/test_actualizar.py -v).

Solo usan --en-seco: no compilan, no instalan y no usan la red.
"""
import os
import subprocess
import unittest

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SCRIPT = os.path.join(RAIZ, "tools", "actualizar-local.sh")


def ejecutar(*args):
    return subprocess.run(
        ["sh", SCRIPT, *args],
        cwd=RAIZ,
        capture_output=True,
        text=True,
        timeout=30,
    )


class ActualizarLocal(unittest.TestCase):
    def test_en_seco_lista_los_pasos_en_orden(self):
        r = ejecutar("--en-seco")
        self.assertEqual(r.returncode, 0, r.stderr)
        lineas = [l for l in r.stdout.splitlines() if l.startswith("+ ")]
        pasos = [
            "compilar-ligero.sh",
            "install.sh --local target/ligero",
            "pcode --version",
        ]
        posiciones = []
        for paso in pasos:
            encontrados = [i for i, l in enumerate(lineas) if paso in l]
            self.assertTrue(encontrados, f"falta el paso: {paso}\n{r.stdout}")
            posiciones.append(encontrados[0])
        self.assertEqual(posiciones, sorted(posiciones), r.stdout)

    def test_sin_pull_no_hace_pull(self):
        r = ejecutar("--sin-pull", "--en-seco")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("git pull", r.stdout)

    def test_argumento_desconocido_sale_con_2(self):
        r = ejecutar("--otro")
        self.assertEqual(r.returncode, 2)
        self.assertIn("uso:", r.stderr)


if __name__ == "__main__":
    unittest.main()
