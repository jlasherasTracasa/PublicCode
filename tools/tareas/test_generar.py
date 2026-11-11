#!/usr/bin/env python3
"""Tests de tools/tareas/generar.py: validación, calidad para modelo pequeño, estados y orden.

Uso: python3 -m unittest tools/tareas/test_generar.py -v
"""
import io
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import generar  # noqa: E402

INDICE = {"fase": [{"id": "F1", "titulo": "Uno"}, {"id": "F10", "titulo": "Diez"}],
          "hito": [{"id": "1.2", "titulo": "primero"}, {"id": "1.3", "titulo": "segundo"}]}
OBJETIVO = ("1. Crea `a.rs` con la función `suma` de la firma, que devuelve la suma de dos enteros sin desbordar: "
            "usa `checked_add` y devuelve `None` si desborda.\n"
            "2. Añade en `a.rs` el módulo de tests con los dos casos de la lista. No toques nada más del crate. "
            "El resultado se comprueba con la orden de aceptación, que solo ejecuta los tests de ese módulo.\n")


def tarea(i, **extra):
    """Una tarea para haiku que cumple todas las reglas."""
    t = {"id": i, "nombre": "t" + i.lower().replace(".", "-"), "titulo": "Título", "fase": i.split(".")[0],
         "ejecutor": "haiku", "depende": [], "toca": ["crates/x/src/a.rs"], "acepta": "cargo test -p x a::",
         "objetivo": OBJETIVO, "tests": ["suma_dos: 2 y 3 dan Some(5)"], "contexto": ["el objetivo basta"],
         "firmas": "pub fn suma(a: i32, b: i32) -> Option<i32>;", "hito": "1.2", "talla": "S"}
    t.update(extra)
    return t


def lint(tareas, raiz):
    tareas = generar.completar(tareas)
    errores = generar.validar(tareas, INDICE)
    if errores:
        return errores
    return generar.calidad(tareas, generar.estados(tareas, raiz), INDICE, raiz)


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.raiz = self.tmp.name
        os.makedirs(os.path.join(self.raiz, "informes"))
        self.addCleanup(self.tmp.cleanup)

    def informe(self, t):
        with open(os.path.join(self.raiz, "informes", generar.fid(t) + ".md"), "w") as f:
            f.write("# hecho\n")


class Estructura(Base):
    def test_tarea_buena_no_tiene_errores(self):
        self.assertEqual(lint([tarea("F1.01")], self.raiz), [])

    def test_orden_natural_de_ids(self):
        ids = ["F10.01", "F9.10", "F9.02a", "F9.02", "F1.01"]
        self.assertEqual(sorted(ids, key=generar.clave), ["F1.01", "F9.02", "F9.02a", "F9.10", "F10.01"])

    def test_id_mal_formado_y_fase_que_no_coincide(self):
        e = generar.validar([tarea("F1.01", id="F1.1"), tarea("F1.02", fase="F10")], INDICE)
        self.assertTrue(any("formato" in x for x in e))
        self.assertTrue(any("la fase debe ser F1" in x for x in e))

    def test_dependencia_inexistente_posterior_y_ciclo(self):
        e = generar.validar([tarea("F1.01", depende=["F1.02"]), tarea("F1.02", depende=["F1.09"])], INDICE)
        self.assertTrue(any("va después" in x for x in e))
        self.assertTrue(any("no existe" in x for x in e))
        e = generar.validar([tarea("F1.01", depende=["F10.01"]), tarea("F10.01", depende=["F1.01"])], INDICE)
        self.assertTrue(any(x.startswith("ciclo") for x in e))

    def test_campo_desconocido_y_estado_no_valido(self):
        e = generar.validar([tarea("F1.01", prioridad=1, estado="hecha")], INDICE)
        self.assertTrue(any("campo desconocido" in x for x in e))
        self.assertTrue(any("estado debe ser" in x for x in e))

    def test_sustituida_necesita_por_y_motivo(self):
        e = generar.validar([tarea("F1.01", estado="sustituida")], INDICE)
        self.assertTrue(any("necesita `por`" in x for x in e))


class Subtareas(Base):
    def test_la_madre_depende_sola_de_sus_subtareas(self):
        ts = generar.completar([tarea("F1.02", toca=[]), tarea("F1.02b"), tarea("F1.02a")])
        self.assertEqual([t["id"] for t in ts], ["F1.02", "F1.02a", "F1.02b"])
        self.assertEqual(ts[0]["depende"], ["F1.02a", "F1.02b"])
        self.assertEqual(generar.validar(ts, INDICE), [])

    def test_subtarea_sin_madre(self):
        self.assertTrue(any("subtarea de F1.02" in x for x in generar.validar([tarea("F1.02a")], INDICE)))

    def test_cierre_sin_toca_vale_solo_con_subtareas(self):
        cierre = dict(toca=[], objetivo="1. Ejecuta acepta.\n2. Escribe el informe.", firmas="")
        self.assertEqual(lint([tarea("F1.02", **cierre), tarea("F1.02a")], self.raiz), [])
        self.assertTrue(any("`toca` vacío" in x for x in lint([tarea("F1.03", **cierre)], self.raiz)))


class Calidad(Base):
    def falla(self, texto, **extra):
        errores = lint([tarea("F1.01", **extra)], self.raiz)
        self.assertTrue(any(texto in x for x in errores), errores)

    def test_objetivo_abierto(self):
        self.falla("demasiado abierto", objetivo="Haz la función suma en a.rs con sus tests.")

    def test_sin_firmas_tocando_codigo(self):
        self.falla("no tiene `firmas`", firmas="")
        self.assertEqual(lint([tarea("F1.01", firmas="", sin_firmas="solo cambia textos")], self.raiz), [])

    def test_test_sin_descripcion(self):
        self.falla("no dice qué comprueba", tests=["suma_dos"])

    def test_archivo_de_toca_sin_mencionar(self):
        self.falla("ni el objetivo ni las firmas dicen", toca=["crates/x/src/a.rs", "crates/x/src/otro.rs"])

    def test_falta_hito_talla_y_contexto(self):
        t = tarea("F1.01")
        del t["hito"], t["talla"], t["contexto"]
        e = lint([t], self.raiz)
        for texto in ("falta `hito`", "falta `talla`", "falta `contexto`"):
            self.assertTrue(any(texto in x for x in e), e)

    def test_talla_grande_no_vale_para_haiku(self):
        self.falla("desgránala", talla="L")

    def test_acepta_tramposa_o_de_varias_lineas(self):
        self.falla("siempre en éxito", acepta="cargo test -p x || true")
        self.falla("una sola línea", acepta="cargo test -p x\ncargo test -p y")

    def test_contexto_cita_ruta_que_no_existe(self):
        self.falla("no existe ni lo crea", contexto=["crates/x/src/nada.rs (función `f`)"])

    def test_contexto_puede_citar_lo_que_crea_una_dependencia(self):
        a = tarea("F1.01")
        b = tarea("F1.02", depende=["F1.01"], contexto=["crates/x/src/a.rs (función `suma`)"])
        self.assertEqual(lint([a, b], self.raiz), [])

    def test_coordinador_abierto_explica_el_motivo(self):
        e = lint([tarea("F1.01", ejecutor="coordinador", objetivo="Publica la release.")], self.raiz)
        self.assertTrue(any("motivo_coordinador" in x for x in e))
        t = tarea("F1.01", ejecutor="coordinador", objetivo="Publica.", motivo_coordinador="necesita la clave de firma")
        self.assertEqual(lint([t], self.raiz), [])

    def test_las_tareas_cerradas_no_se_exigen(self):
        vieja = tarea("F1.01", objetivo="Antigua.", tests=["x"])
        del vieja["hito"], vieja["talla"]
        self.informe(vieja)
        self.assertEqual(lint([vieja], self.raiz), [])

    def test_no_se_depende_de_un_esbozo(self):
        e = lint([tarea("F1.01", estado="esbozo", motivo="falta decidir"), tarea("F1.02", depende=["F1.01"])],
                 self.raiz)
        self.assertTrue(any("estado esbozo" in x for x in e))


class Estados(Base):
    def test_hecha_lista_bloqueada_y_sustituida(self):
        a, b, c = tarea("F1.01"), tarea("F1.02", depende=["F1.01"]), tarea("F1.03", depende=["F1.02"])
        d = tarea("F1.04", estado="sustituida", por=["F1.01"], motivo="ya está")
        e = tarea("F1.05", depende=["F1.04"])
        ts = generar.completar([a, b, c, d, e])
        self.informe(a)
        est = generar.estados(ts, self.raiz)
        self.assertEqual(est, {"F1.01": "hecha", "F1.02": "lista", "F1.03": "bloqueada", "F1.04": "sustituida",
                               "F1.05": "lista"})

    def test_siguiente_ordena_por_hito_y_por_lo_que_desbloquea(self):
        ts = generar.completar([
            tarea("F1.01", hito="1.3"), tarea("F1.02"), tarea("F1.03"),
            tarea("F1.04", depende=["F1.03"]), tarea("F1.05", depende=["F1.04"])])
        est = generar.estados(ts, self.raiz)
        orden = [(t["id"], n) for t, n in generar.siguientes(ts, est, INDICE)]
        self.assertEqual(orden, [("F1.03", 2), ("F1.02", 0), ("F1.01", 0)])
        solo = generar.siguientes(ts, est, INDICE, ejecutor="coordinador")
        self.assertEqual(solo, [])


class Generacion(Base):
    def test_render_incluye_cabeceras_ejemplo_y_no_hagas(self):
        t = tarea("F1.01", ejemplo="entrada: 2, 3\nsalida: Some(5)", no_hagas=["No añadas dependencias"])
        md = generar.render(t, {"F1.01": t})
        self.assertTrue(md.startswith("<!-- acepta: cargo test -p x a:: -->\n<!-- modo: aislado -->\n"))
        for trozo in ("## Ejemplo", "## No hagas", "- `suma_dos`: 2 y 3 dan Some(5)", "`cargo fmt --all`",
                      "git push origin HEAD", "**Hito** 1.2 · **Talla** S"):
            self.assertIn(trozo, md)

    def test_comprobaciones_segun_el_area(self):
        self.assertEqual(generar.comprobaciones(tarea("F1.01", toca=["tools/x/a.py"])),
                         ["`python3 -m unittest <tu test> -v`"])
        self.assertEqual(generar.comprobaciones(tarea("F1.01", toca=["docs/a.md"])), [])

    def test_ciclo_completo_generar_y_comprobar(self):
        os.makedirs(os.path.join(self.raiz, "tareas", "plan"))
        os.makedirs(os.path.join(self.raiz, "docs"))
        with open(os.path.join(self.raiz, "tareas", "plan.toml"), "w") as f:
            f.write('[[fase]]\nid = "F1"\ntitulo = "Uno"\n[[hito]]\nid = "1.2"\ntitulo = "primero"\n')
        with open(os.path.join(self.raiz, "tareas", "plan", "F1.toml"), "w") as f:
            f.write('[[tarea]]\nid = "F1.01"\nnombre = "suma"\ntitulo = "Suma"\nfase = "F1"\nhito = "1.2"\n'
                    'talla = "S"\nejecutor = "haiku"\ndepende = []\ntoca = ["crates/x/src/a.rs"]\n'
                    'acepta = "cargo test -p x a::"\ncontexto = ["el objetivo basta"]\n'
                    f"objetivo = '''\n{OBJETIVO}'''\nfirmas = 'pub fn suma(a: i32, b: i32) -> Option<i32>;'\n"
                    'tests = ["suma_dos: 2 y 3 dan Some(5)"]\n')
        with open(os.path.join(self.raiz, "docs", "implementacion.md"), "w") as f:
            f.write("\n".join(m for par in generar.MARCAS.values() for m in par) + "\n")
        with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()) as salida:
            self.assertEqual(generar.main(["--comprobar"], self.raiz), 1)  # aún no se ha generado
            self.assertEqual(generar.main([], self.raiz), 0)
            self.assertEqual(generar.main(["--comprobar"], self.raiz), 0)
            self.assertEqual(generar.main(["--siguiente", "--ids"], self.raiz), 0)
        self.assertEqual(salida.getvalue().strip(), "F1.01")
        self.assertTrue(os.path.exists(os.path.join(self.raiz, "tareas", "F1.01-suma.md")))
        with open(os.path.join(self.raiz, "docs", "implementacion.md")) as f:
            doc = f.read()
        self.assertIn("| [F1.01-suma](../tareas/F1.01-suma.md) | Suma | haiku | S | 1.2 | — | lista |", doc)


if __name__ == "__main__":
    unittest.main()
