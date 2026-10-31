"""Tests de reglas.py (python3 -m unittest tools/bench/test_reglas.py -v)."""
import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import reglas  # noqa: E402

TAREA = """# F1.02 · ejemplo

## Toca

- `crates/a/src/lib.rs`
- `crates/a/Cargo.toml`
- `crates/a/tests/`

## Acepta
`cargo test`
"""


class Reglas(unittest.TestCase):
    def test_toca_de_tarea(self):
        self.assertEqual(reglas.toca_de_tarea(TAREA), ["crates/a/src/lib.rs", "crates/a/Cargo.toml", "crates/a/tests/"])

    def test_fuera_de_toca_ignora_informe(self):
        toca = reglas.toca_de_tarea(TAREA)
        tocados = ["crates/a/src/lib.rs", "crates/a/tests/x.rs", "informes/F1.02-ejemplo.md", "README.md", "informes/F9.99-otro.md"]
        self.assertEqual(reglas.fuera_de_toca(tocados, toca, "F1.02"), ["README.md", "informes/F9.99-otro.md"])

    def test_ordenes_prohibidas_detecta_stash_y_add_all(self):
        log = "$ cargo test\n$ git stash\n$ git add -A && git commit -m x\n$ git add src/lib.rs\n"
        self.assertEqual(sorted(reglas.ordenes_prohibidas(log)), ["git add -A", "git stash"])

    def test_toca_de_una_tarea_real(self):
        md = open(os.path.join(os.path.dirname(__file__), "..", "..", "tareas", "F0.01-workspace.md"), encoding="utf-8").read()
        toca = reglas.toca_de_tarea(md)
        self.assertIn("Cargo.toml", toca)
        self.assertIn("crates/pcode-cli/src/main.rs", toca)

    def test_ordenes_prohibidas_ignora_comentarios(self):
        log = "# nunca uses git push ni git reset --hard\n$ git status  # git clean no\n"
        self.assertEqual(reglas.ordenes_prohibidas(log), [])

    def test_archivos_tocados_en_repo_temporal(self):
        with tempfile.TemporaryDirectory() as d:
            g = lambda *a: subprocess.run(["git", "-C", d, "-c", "user.name=t", "-c", "user.email=t@t", *a], check=True, capture_output=True)
            g("init", "-q")
            open(os.path.join(d, "a.txt"), "w").write("a")
            g("add", "a.txt")
            g("commit", "-qm", "uno")
            open(os.path.join(d, "b.txt"), "w").write("b")
            open(os.path.join(d, "a.txt"), "w").write("a2")
            g("add", "a.txt", "b.txt")
            g("commit", "-qm", "dos")
            self.assertEqual(sorted(reglas.archivos_tocados(d, "HEAD~1", "HEAD")), ["a.txt", "b.txt"])


if __name__ == "__main__":
    unittest.main()
