#!/usr/bin/env python3
"""Tests del hook pre-commit: valida el plan solo cuando el commit toca tareas/ o generar.py.

Uso: python3 -m unittest tools/hooks/test_pre_commit.py -v
"""
import os
import shutil
import stat
import subprocess
import tempfile
import unittest

HOOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pre-commit")
TIMEOUT = 30

VALIDADOR_FALSO = """#!/usr/bin/env sh
touch llamado
exit {codigo}
"""


class PreCommitTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        self._git("init", "-q")
        destino = os.path.join(self.dir, ".git", "hooks", "pre-commit")
        shutil.copy(HOOK, destino)
        os.chmod(destino, 0o755)

    def _git(self, *args):
        return subprocess.run(
            ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
            cwd=self.dir, capture_output=True, text=True, timeout=TIMEOUT, check=False,
        )

    def _validador(self, codigo):
        ruta = os.path.join(self.dir, "validador_falso.sh")
        with open(ruta, "w", encoding="utf-8") as f:
            f.write(VALIDADOR_FALSO.format(codigo=codigo))
        os.chmod(ruta, os.stat(ruta).st_mode | stat.S_IXUSR)
        return ruta

    def _escribir(self, ruta, contenido="x\n"):
        completa = os.path.join(self.dir, ruta)
        os.makedirs(os.path.dirname(completa), exist_ok=True)
        with open(completa, "w", encoding="utf-8") as f:
            f.write(contenido)
        self._git("add", ruta)

    def _commit(self, codigo):
        env = dict(os.environ, PCODE_GENERAR=self._validador(codigo))
        return subprocess.run(
            ["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "prueba"],
            cwd=self.dir, env=env, capture_output=True, text=True, timeout=TIMEOUT, check=False,
        )

    def _hay_commits(self):
        r = self._git("rev-list", "--all", "--count")
        return r.returncode == 0 and r.stdout.strip() not in ("", "0")

    def test_plan_invalido_bloquea_el_commit(self):
        self._escribir("tareas/plan/F1.toml")
        r = self._commit(codigo=1)
        self.assertNotEqual(r.returncode, 0, r.stderr)
        self.assertFalse(self._hay_commits())
        self.assertIn("El plan no valida", r.stderr)

    def test_plan_valido_deja_hacer_el_commit(self):
        self._escribir("tareas/plan/F1.toml")
        r = self._commit(codigo=0)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(self._hay_commits())
        self.assertTrue(os.path.exists(os.path.join(self.dir, "llamado")))

    def test_otros_archivos_no_validan(self):
        self._escribir("README.md", "hola\n")
        r = self._commit(codigo=1)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(self._hay_commits())
        self.assertFalse(os.path.exists(os.path.join(self.dir, "llamado")))


if __name__ == "__main__":
    unittest.main()
