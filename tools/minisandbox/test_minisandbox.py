#!/usr/bin/env python3
"""Tests de minisandbox. Ejecutar: python3 -m unittest tools/minisandbox/test_minisandbox.py -v

Se saltan si el kernel no permite namespaces de usuario sin privilegios.
"""
import json, os, subprocess, sys, tempfile, time, unittest

MS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "minisandbox.py")


def userns_ok():
    try:
        return subprocess.run(["unshare", "--user", "--map-root-user", "--mount", "true"],
                              capture_output=True, timeout=10).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def ms(*args, **kw):
    return subprocess.run([sys.executable, MS, *args], capture_output=True, text=True, timeout=60, **kw)


def py(codigo):
    """Orden para ejecutar Python dentro de la sandbox (evita depender de utilidades del shell)."""
    return [sys.executable, "-c", codigo]


@unittest.skipUnless(userns_ok(), "sin namespaces de usuario sin privilegios")
class TestMiniSandbox(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ms-test-")
        base = self.tmp.name
        self.repo, self.fuera, self.estado = (os.path.join(base, n) for n in ("repo", "fuera", "estado"))
        os.makedirs(os.path.join(self.repo, "d"))
        os.makedirs(self.fuera)
        for ruta, txt in (("a.txt", "hola\n"), ("b.txt", "b\n"), ("d/x", "x\n"), (".gitignore", "target/\n")):
            with open(os.path.join(self.repo, ruta), "w") as f:
                f.write(txt)
        g = ["git", "-C", self.repo, "-c", "user.name=t", "-c", "user.email=t@t"]
        subprocess.run(g + ["init", "-q"], check=True)
        subprocess.run(g + ["add", "-A"], check=True)
        subprocess.run(g + ["commit", "-qm", "i"], check=True)

    def tearDown(self):
        if os.path.exists(self.estado):
            ms("descartar", self.estado)
        self.tmp.cleanup()

    def ejecutar(self, codigo, *extra):
        return ms("ejecutar", "--dir", self.repo, "--estado", self.estado, *extra, "--", *py(codigo))

    def test_escrituras_van_a_la_capa_y_el_repo_no_cambia(self):
        r = self.ejecutar("import os\n"
                          "open('a.txt','a').write('cambio\\n'); os.remove('b.txt')\n"
                          "open('c.txt','w').write('nuevo\\n'); os.makedirs('target'); open('target/o','w').write('bin')")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(open(os.path.join(self.repo, "a.txt")).read(), "hola\n")
        self.assertTrue(os.path.exists(os.path.join(self.repo, "b.txt")))
        self.assertFalse(os.path.exists(os.path.join(self.repo, "c.txt")))
        cs = json.loads(ms("cambios", self.estado, "--json").stdout)
        self.assertEqual({(c["ruta"], c["tipo"]) for c in cs},
                         {("a.txt", "modificado"), ("b.txt", "borrado"), ("c.txt", "añadido")})  # target/ ignorado

    def test_aplicar_lleva_los_cambios_al_repo(self):
        self.ejecutar("import os; open('a.txt','w').write('nuevo\\n'); os.remove('d/x'); open('e.txt','w').write('e')")
        r = ms("aplicar", self.estado)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(open(os.path.join(self.repo, "a.txt")).read(), "nuevo\n")
        self.assertFalse(os.path.exists(os.path.join(self.repo, "d/x")))
        self.assertTrue(os.path.exists(os.path.join(self.repo, "e.txt")))
        self.assertFalse(os.path.exists(self.estado))

    def test_descartar_no_toca_el_repo(self):
        self.ejecutar("open('a.txt','w').write('nada\\n')")
        self.assertEqual(ms("descartar", self.estado).returncode, 0)
        self.assertEqual(open(os.path.join(self.repo, "a.txt")).read(), "hola\n")
        self.assertFalse(os.path.exists(self.estado))

    def test_fuera_del_repo_es_solo_lectura(self):
        r = self.ejecutar(f"open({os.path.join(self.fuera, 'z')!r},'w')")
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(os.path.exists(os.path.join(self.fuera, "z")))

    def test_escribible_explicito(self):
        r = self.ejecutar(f"open({os.path.join(self.fuera, 'z')!r},'w').write('ok')", "--escribible", self.fuera)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(os.path.exists(os.path.join(self.fuera, "z")))

    def test_sin_red_por_defecto(self):
        r = self.ejecutar("import socket,sys\n"
                          "s=socket.socket(); s.settimeout(2)\n"
                          "sys.exit(0 if s.connect_ex(('1.1.1.1',53)) else 7)")
        self.assertEqual(r.returncode, 0, "la sandbox no debería tener red")

    def test_tiempo_maximo(self):
        t0 = time.monotonic()
        r = self.ejecutar("import time; time.sleep(30)", "--tiempo", "1")
        self.assertEqual(r.returncode, 124)
        self.assertLess(time.monotonic() - t0, 10)

    def test_pids_propios(self):
        r = self.ejecutar("import os; print(os.getpid())")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertLess(int(r.stdout.strip()), 10)  # init del namespace es el 1; la orden, de los primeros

    def test_git_funciona_dentro(self):
        r = ms("ejecutar", "--dir", self.repo, "--estado", self.estado, "--", "git", "status", "--short")
        self.assertEqual(r.returncode, 0, r.stderr)


if __name__ == "__main__":
    unittest.main()
