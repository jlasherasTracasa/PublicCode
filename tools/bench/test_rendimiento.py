"""Tests de rendimiento.py (python3 -m unittest tools/bench/test_rendimiento.py -v)."""
import json
import os
import stat
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import rendimiento  # noqa: E402

# `pcode` falso: --version, `studio` (sirve / y /rpc en loopback) e `ide` (duerme).
PCODE_FALSO = textwrap.dedent('''\
    #!/usr/bin/env python3
    import http.server, json, sys, time
    if sys.argv[1:] == ["--version"]:
        print("pcode 0.0.0"); sys.exit(0)
    if sys.argv[1] == "chat":
        import os
        ruta = os.environ.get("PCODE_MOCK_SCRIPT")
        if not ruta or not os.path.exists(ruta):
            sys.exit(2)
        print("hola desde el mock"); sys.exit(0)
    if sys.argv[1] == "ide":
        time.sleep(30); sys.exit(0)
    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *a): pass
        def _ok(self, body):
            self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
        def do_GET(self): self._ok(b"<html></html>")
        def do_POST(self):
            self.rfile.read(int(self.headers.get("Content-Length", 0)))
            self._ok(json.dumps({"jsonrpc": "2.0", "id": 1, "result": {}}).encode())
    s = http.server.HTTPServer(("127.0.0.1", 0), H)
    print(f"PublicCode Studio: http://127.0.0.1:{s.server_port}/?t={'a' * 64}", file=sys.stderr, flush=True)
    s.serve_forever()
''')


class Rendimiento(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.pcode = os.path.join(self.d.name, "pcode")
        with open(self.pcode, "w") as f:
            f.write(PCODE_FALSO)
        os.chmod(self.pcode, os.stat(self.pcode).st_mode | stat.S_IEXEC)

    def tearDown(self):
        self.d.cleanup()

    def test_mide_y_compara_con_base(self):
        r = rendimiento.medir(self.pcode, self.d.name, repeticiones=3)
        for k in ("arranque_ms", "studio_listo_ms", "reposo_mb", "pico_mb", "hilos"):
            self.assertIn(k, r)
        self.assertGreater(r["reposo_mb"], 0)
        self.assertEqual(rendimiento.comparar(r, r), [])

    def test_umbral_del_20_por_ciento(self):
        base = {"arranque_ms": 10.0, "reposo_mb": 50.0}
        self.assertEqual(rendimiento.comparar(base, {"arranque_ms": 11.9, "reposo_mb": 50.0}), [])
        malos = rendimiento.comparar(base, {"arranque_ms": 12.1, "reposo_mb": 50.0})
        self.assertEqual(len(malos), 1)
        self.assertIn("arranque_ms", malos[0])

    def test_presupuesto_lee_un_json(self):
        bien = os.path.join(self.d.name, "bien.json")
        mal = os.path.join(self.d.name, "mal.json")
        with open(bien, "w") as f:
            json.dump({"arranque_ms": 9, "studio_listo_ms": 100, "reposo_mb": 8}, f)
        with open(mal, "w") as f:
            json.dump({"arranque_ms": 9, "studio_listo_ms": 100, "reposo_mb": 600}, f)
        self.assertEqual(rendimiento.main(["presupuesto", "--actual", bien]), 0)
        self.assertEqual(rendimiento.main(["presupuesto", "--actual", mal]), 1)

    def test_primera_respuesta_con_binario_falso(self):
        self.assertGreater(rendimiento.primera_respuesta_ms(self.pcode, n=3), 0)
        self.assertIsNone(rendimiento.primera_respuesta_ms("/bin/false", n=1))

    def test_presupuesto_incluye_la_primera_respuesta(self):
        malos = rendimiento.fuera_de_presupuesto({"primera_respuesta_ms": 900})
        self.assertEqual(len(malos), 1)
        self.assertIn("primera_respuesta_ms", malos[0])
        self.assertEqual(rendimiento.fuera_de_presupuesto({"primera_respuesta_ms": 100}), [])

    def test_medir_ide_con_binario_falso(self):
        env = dict(os.environ)
        os.environ["DISPLAY"] = os.environ.get("DISPLAY") or ":99"
        try:
            r = rendimiento.medir_ide(self.pcode, self.d.name, espera=0.5)
        finally:
            os.environ.clear()
            os.environ.update(env)
        self.assertIn("ide_reposo_mb", r)
        self.assertGreater(r["ide_reposo_mb"], 0)


if __name__ == "__main__":
    unittest.main()
