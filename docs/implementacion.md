# Plan de implementación desgranado (para Claude Haiku 5.5)

Este documento convierte el [PLAN](PLAN.md) en **tareas pequeñas que un modelo pequeño puede hacer
solo**. La fuente de verdad es [`tareas/plan.toml`](../tareas/plan.toml). De ahí,
[`tools/tareas/generar.py`](../tools/tareas/generar.py) genera un archivo por tarea en
[`tareas/`](../tareas/) y la tabla del final de este documento.

## Por qué tan desgranado

Los modelos pequeños rinden bien con **ejecución estructurada** y mal con objetivos abiertos.
Adaptarlos exige reestructurar la ejecución, no solo reescribir prompts (*From Monolithic Prompts to
Scaffolded Execution*, Findings EMNLP 2026). Los paradigmas multiagente añaden sobrecoste con poca
ganancia en modelos pequeños (*Rethinking Scale*, ACL 2026 Industry). Y la guía escrita por un
modelo fuerte mejora a uno débil, no al revés (*GRASP*, EMNLP 2026). Por eso:

* **El modelo fuerte desgrana y el pequeño ejecuta.** Las tareas las diseña el coordinador
  (Claude Opus o una persona); las ejecuta **Haiku 5.5**.
* **Cada tarea cabe en una sesión corta**: de 1 a 4 archivos, firmas Rust exactas, tests con nombre
  y **una** orden de aceptación.
* **Sin decisiones de arquitectura en la tarea.** Si algo no encaja, Haiku lo dice en el informe y
  para (regla 8 de [AGENTS.md](../AGENTS.md)).

## Reglas de tamaño de una tarea

| Regla | Valor |
|---|---|
| Archivos que toca | ≤ 4 (lo comprueba `generar.py`) |
| Tiempo estimado | 30–90 min de agente |
| API pública nueva | solo la de la sección *Firmas* |
| Tests | nombrados en la tarea; deterministas, sin red, de segundos |
| Aceptación | una orden; la mayoría `cargo test -p <crate>` más alguna comprobación con `grep` |
| Dependencias | completas antes de empezar (las marca la tabla) |

## Ejecutores

| Ejecutor | Quién | Cuándo |
|---|---|---|
| `haiku` | Claude Haiku 5.5 vía Claude Code (o GLM vía opencode o `pcode`) | código con firmas cerradas |
| `coordinador` | Claude Opus 5.5 o una persona | tareas que necesitan CLIs instalados, credenciales, decisiones o juicio |

## Cómo ejecutar una tarea

Con Claude Code y Haiku 5.5, desde la raíz del repo:

```bash
claude --model claude-haiku-5-5 -p "Usa la skill tarea-haiku para ejecutar tareas/F1.01-config.md"
```

Varias tareas sin dependencias entre sí, en paralelo y aisladas: cada una en su mini sandbox, y se
aplica la que pase su aceptación.

```bash
tools/minisandbox/minisandbox.py ejecutar --red --escribible ~/.cargo --estado /tmp/t-F1.02 -- claude --model claude-haiku-5-5 -p "Usa la skill tarea-haiku para ejecutar tareas/F1.02-tipos-mensajes.md"
```

Las cabeceras `<!-- acepta -->`, `<!-- modo -->` y `<!-- depende -->` de cada tarea son compatibles
con `ci/orquestador.py` de SO_IA, así que puede lanzarlas tal cual hasta que exista `pcode flow` (F3).
Desde F3, PublicCode se construye a sí mismo:

```bash
pcode flow run --tareas tareas/ --backend haiku=claude-code:claude-haiku-5-5
```

## Ruta crítica y paralelismo

```
F0.01 ─► F1.01 config ─────────────────────────────┐
     └─► F1.02 tipos ─► F1.03 Provider ─► F1.04/05/06/07 (LiteLLM, SSE, reparación) ─┐
     └─► F1.08 Tool ─► F1.09–F1.12 (tools nativas, en paralelo) ─────────────────────┤
     └─► F1.13 política ─► F1.14 shell ─► F1.15 auditoría ───────────────────────────┤
                                                                                     ▼
                         F1.16 bucle ─► F1.17 compresión ─► F1.18 hook ─► F1.20 `pcode run`
                                                                                     │
  F2: sandbox (F2.01–05) ∥ backends (F2.06–12) ∥ skills (F2.13–15) ∥ MCP (F2.16) ◄───┘
                                                                                     │
  F3: flujos ─► sustituye a orquestador.py en SO_IA (hito) ─► F4 ∥ F5 ─► F6 ─► F7 ◄──┘
```

Dentro de cada fase, las tareas que solo dependen de algo ya hecho pueden ir en paralelo: tools
nativas, proveedores, backends y exportadores de skills.

## Hitos de fase (los comprueba el coordinador)

| Fase | Hito | Cómo se comprueba |
|---|---|---|
| F0 | workspace, CI, línea base y banco smoke listos | CI en verde; `bench/linea-base.json` y `bench/smoke.jsonl` existen |
| F1 | `pcode run` resuelve tareas aisladas con GLM vía LiteLLM | 3 de 5 tareas TP del smoke en verde |
| F2 | la misma tarea corre con Claude Code, Copilot, Gemini, opencode y modelo propio, cada una en mini sandbox | `pcode run --backend X` con los 5 backends sobre una tarea de prueba |
| F3 | `pcode flow` sustituye a `orquestador.py` | una semana en sombra en SO_IA sin integraciones fallidas por lockfiles ni CI fantasma |
| F4 | +12 puntos de aceptación a la primera y −30 % de tokens frente a la línea base | SinapsisBench completo |
| F5 | uso diario desde TUI, web e IDE | demo grabada |
| F6 | skills aprendidas sin regresiones | regresión nocturna verde 7 días |
| F7 | release firmada y red teaming superado | checklist de F7.03 |

## Cómo cambiar el plan

1. Edita `tareas/plan.toml`. Nunca toques los `tareas/*.md` a mano.
2. Regenera:

```bash
tools/tareas/generar.py
```

3. La CI comprueba que todo está al día:

```bash
tools/tareas/generar.py --comprobar
```

## Tabla de tareas

El estado se calcula solo: una tarea está `hecha` si existe su `informes/<id>.md`.

<!-- tabla:inicio (generado por tools/tareas/generar.py; no editar a mano) -->
### F0

| Tarea | Qué | Ejecutor | Depende | Estado |
|---|---|---|---|---|
| [F0.01-workspace](../tareas/F0.01-workspace.md) | Workspace Rust y binario `pcode --version` | haiku | — | pendiente |
| [F0.02-ci](../tareas/F0.02-ci.md) | CI de GitHub: Rust, tools de Python y plan de tareas | haiku | F0.01 | pendiente |
| [F0.03-verificar-clis](../tareas/F0.03-verificar-clis.md) | Verificar las CLIs de Claude Code, Copilot, Gemini y opencode (modo no interactivo) | coordinador | — | pendiente |
| [F0.04-bench-smoke](../tareas/F0.04-bench-smoke.md) | Línea base y banco smoke de SinapsisBench (20 tareas) | coordinador | — | pendiente |
| [F0.05-adrs](../tareas/F0.05-adrs.md) | ADRs iniciales: lenguaje, backends, sandbox, skills | coordinador | — | hecha |
| [F0.06-eval-reglas](../tareas/F0.06-eval-reglas.md) | Eval de cumplimiento de reglas: archivos fuera de `toca` y órdenes git prohibidas | haiku | — | pendiente |

### F1

| Tarea | Qué | Ejecutor | Depende | Estado |
|---|---|---|---|---|
| [F1.01-config](../tareas/F1.01-config.md) | `pcode-config`: leer `pcode.toml` con sustitución `{env:VAR}` | haiku | F0.01 | pendiente |
| [F1.02-tipos-mensajes](../tareas/F1.02-tipos-mensajes.md) | `pcode-models`: tipos de mensajes, tools y uso | haiku | F0.01 | pendiente |
| [F1.03-provider-mock](../tareas/F1.03-provider-mock.md) | Trait `Provider`, errores y `MockProvider` con guion | haiku | F1.02 | pendiente |
| [F1.04-openai-formato](../tareas/F1.04-openai-formato.md) | Formato OpenAI-compatible (LiteLLM, vLLM, Ollama): petición y respuesta | haiku | F1.03 | pendiente |
| [F1.05-openai-http](../tareas/F1.05-openai-http.md) | `OpenAiCompatProvider`: cliente HTTP para LiteLLM y similares | haiku | F1.04 | pendiente |
| [F1.06-sse](../tareas/F1.06-sse.md) | Streaming: parser SSE y acumulador de deltas OpenAI | haiku | F1.04 | pendiente |
| [F1.07-reparar-tool-calls](../tareas/F1.07-reparar-tool-calls.md) | Reparación de argumentos de tool calls mal formados (modelos abiertos) | haiku | F1.04 | pendiente |
| [F1.08-tool-trait](../tareas/F1.08-tool-trait.md) | `pcode-tools`: trait `Tool`, efectos, registro por privilegio y rutas seguras | haiku | F1.02 | pendiente |
| [F1.09-tool-leer](../tareas/F1.09-tool-leer.md) | Tool `leer`: archivo con números de línea, rango y límites | haiku | F1.08 | pendiente |
| [F1.10-tool-escribir-editar](../tareas/F1.10-tool-escribir-editar.md) | Tools `escribir` y `editar` (reemplazo exacto y único) | haiku | F1.08 | pendiente |
| [F1.11-tool-buscar](../tareas/F1.11-tool-buscar.md) | Tools `buscar` (regex) y `listar` (glob) ignorando .git y target | haiku | F1.08 | pendiente |
| [F1.12-tool-bash](../tareas/F1.12-tool-bash.md) | Tool `bash` con tiempo máximo y salida recortada | haiku | F1.08 | pendiente |
| [F1.13-politica](../tareas/F1.13-politica.md) | `pcode-policy`: capacidades, decisiones y política por capas (atenuación) | haiku | F1.08 | pendiente |
| [F1.14-analisis-shell](../tareas/F1.14-analisis-shell.md) | Análisis de órdenes de shell: separar órdenes compuestas, `bash -c`, `sudo`, `env` | haiku | F1.13 | pendiente |
| [F1.15-auditoria](../tareas/F1.15-auditoria.md) | Auditoría append-only con hash encadenado y procedencia | haiku | F1.13 | pendiente |
| [F1.16-bucle](../tareas/F1.16-bucle.md) | `pcode-core`: bucle del agente con política, auditoría y presupuestos | haiku | F1.03, F1.08, F1.13, F1.15 | pendiente |
| [F1.17-compresion-salidas](../tareas/F1.17-compresion-salidas.md) | Compresión de salidas de compilador y tests antes de devolverlas al modelo | haiku | F1.16 | pendiente |
| [F1.18-hook-parada](../tareas/F1.18-hook-parada.md) | Hook de parada: no hay «terminado» sin la aceptación en verde y la lista de restricciones | haiku | F1.17 | pendiente |
| [F1.19-store](../tareas/F1.19-store.md) | `pcode-store`: sesiones y eventos en SQLite | haiku | F0.01 | pendiente |
| [F1.20-cli-run](../tareas/F1.20-cli-run.md) | `pcode run`: une configuración, proveedor, tools, política, bucle, hook y almacenamiento | haiku | F1.01, F1.05, F1.09, F1.10, F1.11, F1.12, F1.14, F1.18, F1.19 | pendiente |
| [F1.21-detector-atasco](../tareas/F1.21-detector-atasco.md) | Detector de atasco, pistas por fallo típico y parada temprana | haiku | F1.18 | pendiente |
| [F1.22-anthropic](../tareas/F1.22-anthropic.md) | Proveedor Anthropic (Messages API): formato y cliente HTTP | haiku | F1.05 | pendiente |
| [F1.23-gemini](../tareas/F1.23-gemini.md) | Proveedor Gemini (generateContent): formato y cliente HTTP | haiku | F1.05 | pendiente |
<!-- tabla:fin -->
