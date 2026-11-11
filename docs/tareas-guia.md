# Cómo escribir una tarea para un modelo pequeño

Quien desgrana es el coordinador (un modelo fuerte o una persona). Quien ejecuta es un modelo pequeño
(Haiku 5.5, GLM) que **no explora el repo, no decide arquitectura y no adivina**. Una tarea está bien
escrita cuando ese modelo puede hacerla leyendo solo la tarea y lo que cita en *Contexto*.

`tools/tareas/generar.py` rechaza las tareas abiertas que no cumplen estas reglas (la CI lo exige).

## 1. Reglas que comprueba el generador

| Regla | Valor |
|---|---|
| Archivos en `toca` | ≤ 4 (mejor ≤ 3); cada uno aparece por su nombre en el objetivo o en las firmas |
| `talla` | `S` (≤ 30 min de agente) o `M` (30–90 min). Si es mayor, se parte en subtareas |
| `objetivo` | pasos numerados (≥ 2), ≥ 300 caracteres; objetivo + firmas + ejemplo + notas ≤ 7000 |
| `firmas` | obligatorias si toca código; si no añade API, `sin_firmas = "motivo"` |
| `tests` | `nombre: qué entra y qué se espera` (la descripción es obligatoria) |
| `contexto` | no vacío; las rutas que cita existen o las crea una dependencia |
| `acepta` | una sola línea; no puede acabar siempre en éxito |
| `hito` | uno de los de `tareas/plan.toml` |
| Tarea de coordinador | `motivo_coordinador`: por qué no puede hacerla un modelo pequeño |
| Dependencias | ninguna en estado `esbozo` o `descartada` |

## 2. Reglas que no comprueba nadie más que tú

1. **Una tarea, una idea.** Un módulo de lógica pura con sus tests, *o* el cableado de ese módulo en un
   archivo grande, *o* un método de la API. No las tres cosas.
2. **Primero la lógica pura, luego el cableado.** La subtarea `a` crea funciones puras y las prueba; la
   `b` las conecta (un `match` nuevo en `server.rs`, una tecla en `app.rs`). Así la `b` casi no puede fallar.
3. **Di dónde, no solo qué.** «En `server.rs`, en el `match` de `dispatch`, junto a `"git.diff"`» en vez de
   «añade el método». Nombra la función que hay que tocar y la que hay que imitar.
4. **Firmas reales.** Cópialas de lo que compila hoy: abre el archivo y comprueba nombres y tipos. Una
   firma inventada hace perder la tarea entera.
5. **Decide tú todo lo decidible**: nombres, formato de los datos, mensajes de error, valores por
   defecto, qué pasa con la entrada vacía. El ejecutor no elige.
6. **Un ejemplo vale más que un párrafo** (`ejemplo`): entrada concreta y salida exacta.
7. **Los fallos previsibles van en `no_hagas`**: «no cambies la firma de `dispatch`», «no añadas
   dependencias», «no toques los tests que ya existen».
8. **`acepta` prueba lo que la tarea hace**: `cargo test -p <crate> <módulo>::` y, si hace falta, un
   `grep -q` de que el cableado existe. Nunca la suite entera del workspace.
9. **Tests sin red, sin reloj y de segundos.** Si hace falta un servidor, es falso y en `127.0.0.1`.
10. **Nada nuevo en dependencias** fuera de la sección 7 de [AGENTS.md](../AGENTS.md).
11. **Una tarea lista no depende de leer otra tarea.** Si necesita un tipo que crea su dependencia,
    copia esa firma en *Contexto* o en el objetivo.

## 3. Partir una tarea grande

La tarea madre conserva su id (`F8.14`) y pasa a ser **tarea de cierre**: `toca = []`, talla `S`, y su
`acepta` es la comprobación global. Las subtareas llevan letra (`F8.14a`, `F8.14b`…) y la madre depende
de ellas sin que haya que escribirlo. Quien dependía de `F8.14` no cambia.

| Partición habitual | Subtareas |
|---|---|
| Función de Studio | `a` lógica en Rust (`crates/pcode-server/src/<x>.rs`) · `b` método de la API en `server.rs` · `c` módulo JS puro en `web/studio/<x>.js` con test de `node --test` · `d` cableado en `index.html` |
| Función del IDE nativo | `a` módulo puro en `crates/pcode-ide/src/<x>.rs` · `b` cableado en `app.rs` y textos en `texts.rs` |
| Tool del agente | `a` la tool en `crates/pcode-tools/src/<x>.rs` · `b` registro y política · `c` uso en la TUI |
| Orden de la CLI | `a` lógica en el crate del núcleo · `b` subcomando en `pcode-cli` con test de `assert_cmd` |

## 4. Plantilla

```toml
[[tarea]]
id = "F8.10a"
nombre = "git-blame-nucleo"
titulo = "Git: parsear `git blame --porcelain` (lógica pura)"
fase = "F8"
hito = "1.6"
talla = "S"
ejecutor = "haiku"
depende = ["F5.32"]
toca = ["crates/pcode-server/src/blame.rs", "crates/pcode-server/src/lib.rs"]
acepta = "cargo test -p pcode-server blame::"
contexto = ["crates/pcode-server/src/workspace.rs (función `git_diff`: cómo se lanza git y se devuelven errores)"]
objetivo = '''
1. Crea `blame.rs` con `parse_blame` (firma abajo). La entrada es la salida de
   `git blame --porcelain <archivo>`. Cada bloque empieza con `<sha de 40> <línea-orig> <línea-final> [n]`;
   después vienen cabeceras `author …`, `author-time …`, `summary …` (solo la primera vez que sale ese
   sha) y una línea que empieza por tabulador con el texto.
2. Devuelve una entrada por línea del archivo, en orden. `sha` son los 8 primeros caracteres. Para un sha
   repetido, reutiliza autor, fecha y resumen del primer bloque (guárdalos en un `HashMap`).
3. El sha `0000000000000000000000000000000000000000` es «sin confirmar»: `author = ""` y `time = 0`.
4. `lib.rs`: añade `pub mod blame;`.
'''
firmas = '''
// crates/pcode-server/src/blame.rs
/// Una línea de `git blame`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BlameLine { pub line: u32, pub sha: String, pub author: String, pub time: i64, pub summary: String }

/// Parsea la salida de `git blame --porcelain`.
pub fn parse_blame(porcelain: &str) -> Vec<BlameLine>;
'''
ejemplo = '''
entrada:  "a1b2c3d4e5f6a7b8c9d0a1b2c3d4e5f6a7b8c9d0 1 1 1\nauthor Ana\nauthor-time 1700000000\nsummary arregla x\n\tfn main() {}\n"
salida:   [BlameLine { line: 1, sha: "a1b2c3d4", author: "Ana", time: 1700000000, summary: "arregla x" }]
'''
tests = [
  "parsea_un_bloque: la entrada del ejemplo da una línea con sha de 8 caracteres y el autor Ana",
  "reutiliza_la_cabecera_de_un_sha_repetido: dos líneas del mismo commit, la segunda sin cabeceras, tienen el mismo autor",
  "linea_sin_confirmar: el sha de ceros da autor vacío y time 0",
  "entrada_vacia: cadena vacía da vector vacío",
]
no_hagas = ["No lances `git` en esta tarea: solo se parsea texto", "No añadas dependencias"]
```

## 5. Cerrar tareas sin ejecutarlas

| Caso | Qué poner |
|---|---|
| Otra tarea ya cubre lo mismo | `estado = "sustituida"`, `por = ["F9.05"]`, `motivo = "…"` |
| Ya no se quiere | `estado = "descartada"`, `motivo = "…"` |
| Se quiere, pero no está desgranada | `estado = "esbozo"`, `hito = "despues"`, `motivo = "qué falta por decidir"` |

Una tarea está **hecha** solo cuando existe su informe en `informes/`. Eso no se escribe en el plan.

## 6. Comprobar un borrador

```bash
tools/tareas/generar.py --lint borrador.toml
```

Valida las tareas del borrador contra el plan actual (las del mismo id se sustituyen) sin escribir nada.
