#!/usr/bin/env python3
"""Tests de install.sh --local (python3 -m unittest tools/instalar/test_install.py -v).

Cada test usa un directorio temporal: no toca ningún .bashrc real ni usa la red.
"""
import os
import stat
import subprocess
import tempfile
import unittest

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
INSTALL = os.path.join(RAIZ, "install.sh")

FALSO_PCODE = """#!/bin/sh
if [ "$1" = "--version" ]; then echo "pcode 9.9.9"; exit 0; fi
exit 1
"""

FALSO_IDE = """#!/bin/sh
exit 0
"""


def escribir_ejecutable(ruta, contenido):
    with open(ruta, "w", encoding="utf-8") as f:
        f.write(contenido)
    os.chmod(ruta, 0o755)


class InstalarLocal(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name
        self.src = os.path.join(self.tmp, "src")
        self.bin = os.path.join(self.tmp, "bin")
        self.home = os.path.join(self.tmp, "home")
        os.makedirs(self.src)
        os.makedirs(self.bin)
        os.makedirs(self.home)

    def tearDown(self):
        self._tmp.cleanup()

    def ejecutar(self, *args):
        env = {
            "PCODE_DIR": self.bin,
            "HOME": self.home,
            "PATH": os.path.join(self.tmp, "bin") + ":/usr/bin:/bin",
        }
        return subprocess.run(
            ["sh", INSTALL, *args],
            env=env,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_local_copia_pcode_y_pcode_ide(self):
        escribir_ejecutable(os.path.join(self.src, "pcode"), FALSO_PCODE)
        escribir_ejecutable(os.path.join(self.src, "pcode-ide"), FALSO_IDE)
        r = self.ejecutar("--local", self.src)
        self.assertEqual(r.returncode, 0, r.stderr)
        for nombre in ("pcode", "pcode-ide"):
            ruta = os.path.join(self.bin, nombre)
            self.assertTrue(os.path.isfile(ruta), nombre)
            self.assertEqual(stat.S_IMODE(os.stat(ruta).st_mode), 0o755, nombre)
        self.assertIn("pcode 9.9.9", r.stdout)

    def test_local_sin_ide_instala_solo_pcode(self):
        escribir_ejecutable(os.path.join(self.src, "pcode"), FALSO_PCODE)
        r = self.ejecutar("--local", self.src)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.path.isfile(os.path.join(self.bin, "pcode")))
        self.assertFalse(os.path.exists(os.path.join(self.bin, "pcode-ide")))

    def test_local_carpeta_sin_pcode_falla(self):
        r = self.ejecutar("--local", self.src)
        self.assertEqual(r.returncode, 1)
        self.assertIn("No hay pcode compilado", r.stderr)
        self.assertEqual(os.listdir(self.bin), [])

    def test_argumento_desconocido_sale_con_2(self):
        r = self.ejecutar("--otro")
        self.assertEqual(r.returncode, 2)
        self.assertIn("uso: install.sh", r.stderr)


if __name__ == "__main__":
    unittest.main()
