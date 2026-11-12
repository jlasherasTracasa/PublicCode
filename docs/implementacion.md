# Plan de implementación desgranado (para modelos pequeños)

Este documento convierte el [PLAN](PLAN.md) en **tareas pequeñas que un modelo pequeño puede hacer
solo**. La fuente de verdad es [`tareas/plan.toml`](../tareas/plan.toml) (fases e hitos) más un archivo
por fase en [`tareas/plan/`](../tareas/plan/). De ahí, [`tools/tareas/generar.py`](../tools/tareas/generar.py)
genera un archivo por tarea en [`tareas/`](../tareas/) y las secciones «Estado del plan» y «Tabla de
tareas» de este documento. Cómo se escribe una tarea: [tareas-guia.md](tareas-guia.md).

## Por qué tan desgranado

Los modelos pequeños rinden bien con **ejecución estructurada** y mal con objetivos abiertos.
Adaptarlos exige reestructurar la ejecución, no solo reescribir prompts (*From Monolithic Prompts to
Scaffolded Execution*, Findings EMNLP 2026). Los paradigmas multiagente añaden sobrecoste con poca
ganancia en modelos pequeños (*Rethinking Scale*, ACL 2026 Industry). Y la guía escrita por un
modelo fuerte mejora a uno débil, no al revés (*GRASP*, EMNLP 2026). Por eso:

* **El modelo fuerte desgrana y el pequeño ejecuta.** Las tareas las diseña el coordinador
  (un modelo fuerte o una persona); las ejecuta **Haiku 5.5** o GLM.
* **Cada tarea cabe en una sesión corta**: de 1 a 4 archivos, pasos numerados, firmas exactas, un
  ejemplo, tests con nombre y descripción, y **una** orden de aceptación.
* **Sin decisiones en la tarea.** Nombres, formatos, valores por defecto y casos límite los fija quien
  desgrana. Si algo no encaja, el ejecutor lo dice en el informe y para ([AGENTS.md](../AGENTS.md) §9).
* **Una tarea grande se parte**: subtareas `F8.14a`, `F8.14b`… (lógica pura primero, cableado después) y
  la tarea madre queda como cierre.

## Reglas de tamaño de una tarea (las comprueba `generar.py`)

| Regla | Valor |
|---|---|
| Archivos que toca | ≤ 4, y cada uno nombrado en el objetivo o en las firmas |
| Talla | `S` (≤ 30 min de agente) o `M` (30–90 min); más grande, se parte |
| Objetivo | pasos numerados (≥ 2); con firmas, ejemplo y notas no pasa de 7000 caracteres |
| API pública nueva | solo la de la sección *Firmas* (obligatoria si toca código) |
| Tests | `nombre: qué entra y qué se espera`; deterministas, sin red, de segundos |
| Contexto | qué archivo y qué función leer; las rutas citadas existen o las crea una dependencia |
| Aceptación | una línea, específica de la tarea |
| Dependencias | cerradas antes de empezar; nunca un esbozo |

Las tareas ya cerradas (hechas, sustituidas o descartadas) conservan su redacción original: las reglas se
exigen a las **abiertas**.

## Estados

| Estado | Significado | Quién lo fija |
|---|---|---|
| `hecha` | existe `informes/<id>-<nombre>.md` y no dice `Estado: bloqueada` | el ejecutor, al escribir el informe |
| `lista` | pendiente y con todas sus dependencias cerradas: **se puede empezar** | se calcula |
| `bloqueada` | pendiente y con alguna dependencia abierta | se calcula |
| `sustituida` | otra tarea ya cubre lo mismo (`por`, `motivo`) | el coordinador, en el plan |
| `descartada` | ya no se hará (`motivo`) | el coordinador, en el plan |
| `esbozo` | se quiere, pero falta desgranarla: nadie debe empezarla | el coordinador, en el plan |

## Ejecutores

| Ejecutor | Quién | Cuándo |
|---|---|---|
| `haiku` | Claude Haiku 5.5 vía Claude Code (o GLM vía opencode o `pcode`) | todo lo que tenga firmas cerradas y se pruebe sin servicios externos |
| `coordinador` | un modelo fuerte o una persona | solo lo que exige credenciales, servicios externos, hardware o una decisión del propietario (cada tarea dice por qué en `motivo_coordinador`) |

## Cómo ejecutar una tarea

Qué toca ahora (tareas listas, por orden de hito y de cuántas desbloquean):

```bash
tools/tareas/generar.py --siguiente 5 --ejecutor haiku
```

Con Claude Code y Haiku 5.5, desde la raíz del repo:

```bash
claude --model claude-haiku-5-5 -p "Usa la skill tarea-haiku para ejecutar tareas/F8.10a-git-blame-nucleo.md"
```

Varias tareas listas en paralelo, cada una en su mini sandbox; se aplica la que pase su aceptación:

```bash
tools/minisandbox/minisandbox.py ejecutar --red --escribible ~/.cargo --estado /tmp/t-F8.10a -- claude --model claude-haiku-5-5 -p "Usa la skill tarea-haiku para ejecutar tareas/F8.10a-git-blame-nucleo.md"
```

Las cabeceras `<!-- acepta -->`, `<!-- modo -->` y `<!-- depende -->` de cada tarea las lee `pcode flow`:

```bash
pcode flow run --tareas tareas/ --backend haiku=claude-code:claude-haiku-5-5
```

Otras consultas:

```bash
tools/tareas/generar.py --estado          # resumen por fase e hito
tools/tareas/generar.py --tarea F8.14     # estado de una tarea y qué la bloquea
```

## Orden de trabajo: hitos

Las fases agrupan por tema; los **hitos** dicen en qué entrega entra cada tarea abierta y marcan la
prioridad (`--siguiente` ofrece primero el hito más temprano). Están definidos en `tareas/plan.toml`.

| Hito | Qué entrega | Cómo se comprueba |
|---|---|---|
| **1.2** | lo empezado, cerrado: IDE nativo pulido, depurador, remoto desde el IDE, modo opencode | `rendimiento.py medir` y `medir-ide` dentro del presupuesto; depurar un script de Python desde el IDE; release 1.2.0 |
| **1.3** | agente al nivel de los mejores: hooks, lista de pendientes, segundo plano, permisos persistentes, órdenes y agentes propios, coste, TUI completa | las evals de reglas e inyección siguen en verde; una sesión real usa hooks, pendientes y subagentes |
| **1.4** | planificación y flujos medidos: PublicBench, `pcode bench`, lint y desglose de tareas, revisión cruzada, flujos en el demonio | línea base medida con opencode; `pcode flow comprobar` valida este mismo plan |
| **1.5** | seguridad: Landlock, seccomp, red filtrada, secretos, política gestionada, llavero | red teaming de skills superado; ningún secreto en auditoría ni transcripciones |
| **1.6** | Studio e IDE completos: plegado, vista dividida, LSP (formato, completado, renombrar), git completo, tests, depurador | una semana de trabajo sin abrir VS Code |
| **1.7** | contexto, memoria y aprendizaje: compactación con prefijo estable, facetas, búsqueda híbrida, router, skills aprendidas | +12 puntos de aceptación a la primera y −30 % de tokens en PublicBench |
| **1.8** | plataformas e integraciones: instaladores nativos, ARM64, APK con avisos y TLS, ACP, extensiones, servidor MCP, GitHub | paquetes instalables; la misma tarea desde un editor con ACP |
| **despues** | esbozos sin desgranar | — |

## Cómo cambiar el plan

1. Edita `tareas/plan/F<n>.toml` (o `tareas/plan.toml` para fases e hitos) siguiendo
   [tareas-guia.md](tareas-guia.md). Nunca toques los `tareas/*.md` a mano.
2. Un borrador se valida sin escribir nada: `tools/tareas/generar.py --lint borrador.toml`.
3. Regenera:

```bash
tools/tareas/generar.py
```

4. La CI comprueba que todo es válido y está al día:

```bash
tools/tareas/generar.py --comprobar
```

## Estado del plan

<!-- resumen:inicio (generado por tools/tareas/generar.py; no editar a mano) -->
### Por fase

| Fase | Hecha | Sustituida | Descartada | Lista | Bloqueada | Esbozo | Total |
|---|---|---|---|---|---|---|---|
| F0 Fundaciones | 6 | 0 | 0 | 2 | 3 | 0 | 11 |
| F1 Núcleo | 26 | 0 | 0 | 0 | 0 | 0 | 26 |
| F2 Backends, sandbox, skills y MCP | 15 | 0 | 0 | 6 | 22 | 0 | 43 |
| F3 Flujos | 9 | 0 | 0 | 0 | 10 | 0 | 19 |
| F4 Contexto, memoria y router | 10 | 0 | 0 | 8 | 26 | 0 | 44 |
| F5 Interfaces | 11 | 11 | 1 | 10 | 35 | 1 | 69 |
| F6 Aprendizaje | 0 | 0 | 0 | 3 | 13 | 0 | 16 |
| F7 Endurecimiento | 2 | 0 | 0 | 1 | 3 | 0 | 6 |
| F8 1.x: editor, agente y plataforma | 17 | 0 | 0 | 37 | 164 | 0 | 218 |
| F9 Equipos modestos, remoto e IDE nativo | 39 | 0 | 0 | 15 | 22 | 6 | 82 |
| F10 Agente al nivel de los mejores | 0 | 0 | 0 | 6 | 23 | 0 | 29 |
| F11 Planificación y flujos para modelos pequeños | 0 | 0 | 0 | 1 | 34 | 0 | 35 |
| F12 Seguridad, gobierno y operación | 0 | 0 | 0 | 6 | 18 | 0 | 24 |
| **Total** | 135 | 11 | 1 | 95 | 373 | 7 | 622 |

### Por hito (solo tareas abiertas)

| Hito | Qué entrega | Listas | Bloqueadas | Esbozos | Para haiku | Para coordinador |
|---|---|---|---|---|---|---|
| **1.2** | Cerrar lo empezado: IDE nativo, depurador, remoto y modo opencode | 6 | 2 | 2 | 9 | 1 |
| **1.3** | Agente al nivel de los mejores: hooks, pendientes, segundo plano, permisos, órdenes y agentes propios, TUI | 19 | 63 | 0 | 82 | 0 |
| **1.4** | Planificación y flujos medidos: banco, lint y desglose de tareas, revisión cruzada, GitHub | 7 | 64 | 0 | 68 | 3 |
| **1.5** | Seguridad: Landlock, seccomp, red filtrada, secretos y política gestionada | 12 | 40 | 0 | 52 | 0 |
| **1.6** | Studio e IDE completos: editor, LSP, git, tests y depurador | 30 | 138 | 1 | 169 | 0 |
| **1.7** | Contexto, memoria y aprendizaje: compactación, facetas, búsqueda híbrida, router y skills aprendidas | 10 | 35 | 0 | 45 | 0 |
| **1.8** | Plataformas e integraciones: instaladores, ARM64, APK, ACP, extensiones y servidor MCP | 11 | 31 | 0 | 36 | 6 |
| **despues** | Sin fecha: esbozos que hay que desgranar antes de empezarlos | 0 | 0 | 4 | 3 | 1 |

### Qué se puede empezar ahora

Tareas **listas** (dependencias cerradas), por orden de hito y de cuántas desbloquean. La lista al día sale de `tools/tareas/generar.py --siguiente`.

| Tarea | Qué | Ejecutor | Talla | Hito | Desbloquea |
|---|---|---|---|---|---|
| [F9.40b-ide-tema-de-extension-ajustes](../tareas/F9.40b-ide-tema-de-extension-ajustes.md) | IDE: elegir en Ajustes un tema de una extensión instalada y recordarlo | haiku | M | 1.2 | 1 |
| [F9.45-instalar-desde-codigo](../tareas/F9.45-instalar-desde-codigo.md) | `install.sh --local [carpeta]`: instalar pcode y pcode-ide compilados en este equipo | haiku | S | 1.2 | 1 |
| [F9.46-version-con-commit](../tareas/F9.46-version-con-commit.md) | `pcode --version` y `pcode doctor` dicen de qué commit sale el binario: `pcode 1.1.0 (e2e30ab)` | haiku | S | 1.2 | 1 |
| [F8.34-rendimiento](../tareas/F8.34-rendimiento.md) | Rendimiento medido: arranque, memoria y latencia, con presupuesto en la CI | haiku | S | 1.2 | 0 |
| [F9.25-releases-periodicas](../tareas/F9.25-releases-periodicas.md) | Releases periódicas: versión, CHANGELOG y etiqueta tras cada bloque de tareas aceptadas | coordinador | S | 1.2 | 0 |
| [F9.48-plan-hook-precommit](../tareas/F9.48-plan-hook-precommit.md) | Hook `pre-commit` que valida el plan cuando el commit toca `tareas/plan*` (que la CI no se rompa) | haiku | S | 1.2 | 0 |
| [F5.10a-tui-arbol-sesiones](../tareas/F5.10a-tui-arbol-sesiones.md) | TUI: árbol de sesiones y subagentes para la barra lateral (lógica pura) | haiku | S | 1.3 | 37 |
| [F5.11a-tui-atajos-mapa](../tareas/F5.11a-tui-atajos-mapa.md) | TUI: mapa de atajos y `keybindings.toml` (lógica pura) | haiku | M | 1.3 | 35 |
| [F8.24a-tui-selector-nucleo](../tareas/F8.24a-tui-selector-nucleo.md) | TUI: lista seleccionable con búsqueda difusa (lógica pura) | haiku | S | 1.3 | 32 |
| [F8.25a-checkpoint-diff-archivo](../tareas/F8.25a-checkpoint-diff-archivo.md) | Puntos de control: diff de un archivo de un turno y revertir solo ese archivo | haiku | S | 1.3 | 29 |
| [F8.26a-tui-osc52](../tareas/F8.26a-tui-osc52.md) | TUI: secuencia OSC 52 para copiar al portapapeles (lógica pura) | haiku | S | 1.3 | 26 |
| [F10.34a-tui-historial-archivo](../tareas/F10.34a-tui-historial-archivo.md) | TUI: archivo de historial de entradas y búsqueda hacia atrás (lógica pura) | haiku | S | 1.3 | 16 |
| [F5.06a-cli-formato-salida](../tareas/F5.06a-cli-formato-salida.md) | CLI: formatos de salida de `pcode run` (`texto`, `json`, `stream-json`), lógica pura | haiku | S | 1.3 | 14 |
| [F10.35a-tui-editor-lanzar](../tareas/F10.35a-tui-editor-lanzar.md) | TUI: elegir el editor (`$VISUAL`, `$EDITOR`) y editar un texto en un archivo temporal | haiku | S | 1.3 | 14 |
| [F10.36a-tui-edicion-cursor](../tareas/F10.36a-tui-edicion-cursor.md) | TUI: operaciones de edición de texto con cursor (lógica pura) | haiku | M | 1.3 | 13 |
| [F8.16a-ordenes-propias-nucleo](../tareas/F8.16a-ordenes-propias-nucleo.md) | `pcode-skills`: leer órdenes propias (`*.md` con frontmatter y `$ARGUMENTOS`) de varias carpetas | haiku | S | 1.3 | 7 |
| [F10.39a-tui-estado-nucleo](../tareas/F10.39a-tui-estado-nucleo.md) | TUI: elementos de la línea de estado y su texto (lógica pura) | haiku | S | 1.3 | 5 |
| [F10.40a-store-buscar-bifurcar](../tareas/F10.40a-store-buscar-bifurcar.md) | `pcode-store`: sesión por id, número de mensajes, búsqueda de texto y bifurcación | haiku | M | 1.3 | 5 |
| [F8.18a-coste-calculo](../tareas/F8.18a-coste-calculo.md) | `pcode-models`: precio por millón de tokens con caché y estado del presupuesto (lógica pura) | haiku | S | 1.3 | 4 |
| [F8.21a-imagenes-tipo](../tareas/F8.21a-imagenes-tipo.md) | `Message` con imágenes (base64 propio, sin dependencias) y su envío a Anthropic | haiku | M | 1.3 | 3 |
| [F8.20a-web-tool](../tareas/F8.20a-web-tool.md) | `pcode-tools`: tool `web` (GET con tiempo y tamaño máximos, HTML a texto sin scripts ni estilos) | haiku | M | 1.3 | 2 |
| [F10.41a-revisar-objetivo](../tareas/F10.41a-revisar-objetivo.md) | Revisión: prompt para los cambios preparados (staged) o respecto a una rama | haiku | S | 1.3 | 2 |
| [F4.12c-guion-cableado](../tareas/F4.12c-guion-cableado.md) | Bucle del agente: ofrecer e interceptar la tool `guion` | haiku | M | 1.3 | 1 |
| [F8.09c-lsp-tools-chat](../tareas/F8.09c-lsp-tools-chat.md) | Registrar las tools del LSP en el agente del chat y de la TUI | haiku | S | 1.3 | 1 |
| [F4.10-busqueda-tools](../tareas/F4.10-busqueda-tools.md) | Búsqueda y deduplicación de tools: anunciar solo las relevantes | haiku | S | 1.3 | 0 |

…y 70 más.
<!-- resumen:fin -->

## Tabla de tareas

El estado se calcula solo (ver «Estados»).

<!-- tabla:inicio (generado por tools/tareas/generar.py; no editar a mano) -->
### F0 — Fundaciones

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F0.01-workspace](../tareas/F0.01-workspace.md) | Workspace Rust y binario `pcode --version` | haiku | — | — | — | hecha |
| [F0.02-ci](../tareas/F0.02-ci.md) | CI de GitHub: Rust, tools de Python y plan de tareas | haiku | — | — | F0.01 | hecha |
| [F0.03-verificar-clis](../tareas/F0.03-verificar-clis.md) | Verificar las CLIs de Claude Code, Copilot, Gemini y opencode (modo no interactivo) | coordinador | M | 1.4 | F0.03a | bloqueada |
| [F0.03a-verificado-plantilla](../tareas/F0.03a-verificado-plantilla.md) | Plantilla de `docs/backends-verificado.md` y validador `tools/backends/verificado.py` | haiku | M | 1.4 | — | lista |
| [F0.04-bench-smoke](../tareas/F0.04-bench-smoke.md) | Banco PublicBench: 20 tareas del propio plan con su commit base | coordinador | M | 1.4 | F0.04a, F0.04b | bloqueada |
| [F0.04a-bench-conjunto-validador](../tareas/F0.04a-bench-conjunto-validador.md) | Banco: formato de `bench/*.jsonl` y validador `tools/bench/conjunto.py` | haiku | S | 1.4 | — | lista |
| [F0.04b-bench-extraer-candidatas](../tareas/F0.04b-bench-extraer-candidatas.md) | Banco: `tools/bench/extraer.py` saca las tareas candidatas del historial de git | haiku | M | 1.4 | F0.04a | bloqueada |
| [F0.05-adrs](../tareas/F0.05-adrs.md) | ADRs iniciales: lenguaje, backends, sandbox, skills | coordinador | — | — | — | hecha |
| [F0.06-eval-reglas](../tareas/F0.06-eval-reglas.md) | Eval de cumplimiento de reglas: archivos fuera de `toca` y órdenes git prohibidas | haiku | — | — | — | hecha |
| [F0.07-ci-windows-release](../tareas/F0.07-ci-windows-release.md) | CI en Linux y Windows, y release de la CLI con instaladores | coordinador | — | — | F0.02, F1.20 | hecha |
| [F0.08-instalacion](../tareas/F0.08-instalacion.md) | Instalación en una línea, `pcode doctor`, `pcode config` y `pcode actualizar` | coordinador | — | — | F0.07 | hecha |

### F1 — Núcleo

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F1.01-config](../tareas/F1.01-config.md) | `pcode-config`: leer `pcode.toml` con sustitución `{env:VAR}` | haiku | — | — | F0.01 | hecha |
| [F1.02-tipos-mensajes](../tareas/F1.02-tipos-mensajes.md) | `pcode-models`: tipos de mensajes, tools y uso | haiku | — | — | F0.01 | hecha |
| [F1.03-provider-mock](../tareas/F1.03-provider-mock.md) | Trait `Provider`, errores y `MockProvider` con guion | haiku | — | — | F1.02 | hecha |
| [F1.04-openai-formato](../tareas/F1.04-openai-formato.md) | Formato OpenAI-compatible (LiteLLM, vLLM, Ollama): petición y respuesta | haiku | — | — | F1.03 | hecha |
| [F1.05-openai-http](../tareas/F1.05-openai-http.md) | `OpenAiCompatProvider`: cliente HTTP para LiteLLM y similares | haiku | — | — | F1.04 | hecha |
| [F1.06-sse](../tareas/F1.06-sse.md) | Streaming: parser SSE y acumulador de deltas OpenAI | haiku | — | — | F1.04 | hecha |
| [F1.07-reparar-tool-calls](../tareas/F1.07-reparar-tool-calls.md) | Reparación de argumentos de tool calls mal formados (modelos abiertos) | haiku | — | — | F1.04 | hecha |
| [F1.08-tool-trait](../tareas/F1.08-tool-trait.md) | `pcode-tools`: trait `Tool`, efectos, registro por privilegio y rutas seguras | haiku | — | — | F1.02 | hecha |
| [F1.09-tool-leer](../tareas/F1.09-tool-leer.md) | Tool `leer`: archivo con números de línea, rango y límites | haiku | — | — | F1.08 | hecha |
| [F1.10-tool-escribir-editar](../tareas/F1.10-tool-escribir-editar.md) | Tools `escribir` y `editar` (reemplazo exacto y único) | haiku | — | — | F1.08 | hecha |
| [F1.11-tool-buscar](../tareas/F1.11-tool-buscar.md) | Tools `buscar` (regex) y `listar` (glob) ignorando .git y target | haiku | — | — | F1.08 | hecha |
| [F1.12-tool-bash](../tareas/F1.12-tool-bash.md) | Tool `bash` con tiempo máximo y salida recortada | haiku | — | — | F1.08 | hecha |
| [F1.13-politica](../tareas/F1.13-politica.md) | `pcode-policy`: capacidades, decisiones y política por capas (atenuación) | haiku | — | — | F1.08 | hecha |
| [F1.14-analisis-shell](../tareas/F1.14-analisis-shell.md) | Análisis de órdenes de shell: separar órdenes compuestas, `bash -c`, `sudo`, `env` | haiku | — | — | F1.13 | hecha |
| [F1.15-auditoria](../tareas/F1.15-auditoria.md) | Auditoría append-only con hash encadenado y procedencia | haiku | — | — | F1.13 | hecha |
| [F1.16-bucle](../tareas/F1.16-bucle.md) | `pcode-core`: bucle del agente con política, auditoría y presupuestos | haiku | — | — | F1.03, F1.08, F1.13, F1.15 | hecha |
| [F1.17-compresion-salidas](../tareas/F1.17-compresion-salidas.md) | Compresión de salidas de compilador y tests antes de devolverlas al modelo | haiku | — | — | F1.16 | hecha |
| [F1.18-hook-parada](../tareas/F1.18-hook-parada.md) | Hook de parada: no hay «terminado» sin la aceptación en verde y la lista de restricciones | haiku | — | — | F1.17 | hecha |
| [F1.19-store](../tareas/F1.19-store.md) | `pcode-store`: sesiones y eventos en SQLite | haiku | — | — | F0.01 | hecha |
| [F1.20-cli-run](../tareas/F1.20-cli-run.md) | `pcode run`: une configuración, proveedor, tools, política, bucle, hook y almacenamiento | haiku | — | — | F1.01, F1.05, F1.09, F1.10, F1.11, F1.12, F1.14, F1.18, F1.19 | hecha |
| [F1.21-detector-atasco](../tareas/F1.21-detector-atasco.md) | Detector de atasco, pistas por fallo típico y parada temprana | haiku | — | — | F1.18 | hecha |
| [F1.22-anthropic](../tareas/F1.22-anthropic.md) | Proveedor Anthropic (Messages API): formato y cliente HTTP | haiku | — | — | F1.05 | hecha |
| [F1.23-gemini](../tareas/F1.23-gemini.md) | Proveedor Gemini (generateContent): formato y cliente HTTP | haiku | — | — | F1.05 | hecha |
| [F1.24-shell-portable](../tareas/F1.24-shell-portable.md) | Shell portable (sh en Linux, cmd en Windows) y política que entra en cmd y PowerShell | coordinador | — | — | F1.12, F1.14 | hecha |
| [F1.25-streaming](../tareas/F1.25-streaming.md) | Streaming de texto en los proveedores y en el agente | coordinador | — | — | F1.06, F1.22, F1.23 | hecha |
| [F1.26-robustez](../tareas/F1.26-robustez.md) | Robustez para el uso diario: reintentos, caché de prompt, compactación, política segura y configuración global | coordinador | — | — | F1.25, F5.29 | hecha |

### F2 — Backends, sandbox, skills y MCP

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F2.01-sandbox-capa](../tareas/F2.01-sandbox-capa.md) | `pcode-sandbox`: leer la capa superior del overlay como lista de cambios | haiku | — | — | F0.01 | hecha |
| [F2.02-sandbox-ejecutar](../tareas/F2.02-sandbox-ejecutar.md) | Ejecutar una orden en N1 (namespaces + overlay + raíz ro) con tiempo máximo | haiku | — | — | F2.01 | hecha |
| [F2.03-sandbox-aplicar](../tareas/F2.03-sandbox-aplicar.md) | Aplicar y descartar los cambios de una sandbox | haiku | — | — | F2.02 | hecha |
| [F2.04-sandbox-landlock](../tareas/F2.04-sandbox-landlock.md) | Landlock dentro de la sandbox con `pcode __sandbox-exec` | haiku | S | 1.5 | F2.04a, F2.04b, F2.04c, F2.04d | bloqueada |
| [F2.04a-landlock-restringir](../tareas/F2.04a-landlock-restringir.md) | Landlock: `restrict_self` y ayuda para tests en proceso hijo | haiku | M | 1.5 | F2.03 | lista |
| [F2.04b-landlock-exec](../tareas/F2.04b-landlock-exec.md) | Landlock: `exec_restricted` (restringe y sustituye el proceso por la orden) | haiku | S | 1.5 | F2.04a | bloqueada |
| [F2.04c-landlock-prepare](../tareas/F2.04c-landlock-prepare.md) | Landlock: `prepare.sh` termina en el ayudante `pcode __sandbox-exec` si está configurado | haiku | S | 1.5 | F2.04b | bloqueada |
| [F2.04d-landlock-cli](../tareas/F2.04d-landlock-cli.md) | CLI: subcomando oculto `pcode __sandbox-exec` y activación del ayudante | haiku | M | 1.5 | F2.04c | bloqueada |
| [F2.05-sandbox-seccomp-limites](../tareas/F2.05-sandbox-seccomp-limites.md) | Seccomp (lista de syscalls denegadas) y límites de recursos | haiku | S | 1.5 | F2.05a, F2.05b, F2.05c | bloqueada |
| [F2.05a-sandbox-limites](../tareas/F2.05a-sandbox-limites.md) | Límites de recursos del proceso de la sandbox (`setrlimit`) | haiku | S | 1.5 | F2.04a | bloqueada |
| [F2.05b-sandbox-seccomp](../tareas/F2.05b-sandbox-seccomp.md) | Seccomp: filtro que devuelve EPERM en las syscalls peligrosas | haiku | M | 1.5 | F2.04a | bloqueada |
| [F2.05c-sandbox-endurecer-exec](../tareas/F2.05c-sandbox-endurecer-exec.md) | `exec_restricted` aplica Landlock, después los límites y por último seccomp | haiku | S | 1.5 | F2.04d, F2.05a, F2.05b | bloqueada |
| [F2.06-backend-trait](../tareas/F2.06-backend-trait.md) | `pcode-backends`: trait `Backend`, `Job`, relevo tipado y `MockBackend` | haiku | — | — | F1.16, F2.03 | hecha |
| [F2.07-backend-claude-code](../tareas/F2.07-backend-claude-code.md) | Backend Claude Code (modo headless stream-json) | haiku | — | — | F2.06, F0.03 | hecha |
| [F2.08-backend-gemini-cli](../tareas/F2.08-backend-gemini-cli.md) | Backend Gemini CLI (no interactivo, salida JSON) | haiku | — | — | F2.06, F0.03 | hecha |
| [F2.09-backend-copilot-cli](../tareas/F2.09-backend-copilot-cli.md) | Backend GitHub Copilot CLI (no interactivo) | haiku | — | — | F2.06, F0.03 | hecha |
| [F2.10-backend-opencode](../tareas/F2.10-backend-opencode.md) | Backend opencode (`opencode run`) | haiku | — | — | F2.06, F0.03 | hecha |
| [F2.11-backend-copilot-github](../tareas/F2.11-backend-copilot-github.md) | Backend Copilot coding agent: issue → PR → verificación local | coordinador | M | 1.8 | F2.06, F0.03, F2.11a, F2.11b, F2.11c | bloqueada |
| [F2.11a-github-api-trait](../tareas/F2.11a-github-api-trait.md) | GitHub: trait `GitHubApi`, implementación falsa y texto del issue | haiku | M | 1.8 | F2.06 | lista |
| [F2.11b-copilot-github-backend](../tareas/F2.11b-copilot-github-backend.md) | Backend `CopilotGitHub`: crear el issue, esperar al PR y aplicar su diff en la sandbox | haiku | M | 1.8 | F2.11a | bloqueada |
| [F2.11c-github-http](../tareas/F2.11c-github-http.md) | GitHub: implementación HTTP de `GitHubApi` (`GitHubHttp`) probada con un servidor falso | haiku | M | 1.8 | F2.11b | bloqueada |
| [F2.12-backend-modelo](../tareas/F2.12-backend-modelo.md) | Backend «modelo»: `pcode run` dentro de la sandbox con LiteLLM, Anthropic o Gemini | haiku | — | — | F2.06, F1.20 | hecha |
| [F2.13-skills-registro](../tareas/F2.13-skills-registro.md) | `pcode-skills`: leer `SKILL.md`, registro, índice y cierre de dependencias | haiku | — | — | F0.01 | hecha |
| [F2.14-skills-exportar](../tareas/F2.14-skills-exportar.md) | Exportar skills a Claude Code, Copilot, Gemini y opencode | haiku | — | — | F2.13, F0.03 | hecha |
| [F2.15-skills-contrato](../tareas/F2.15-skills-contrato.md) | Sección `## Contrato` y checkpoints de estado en las skills | haiku | — | — | F2.13 | hecha |
| [F2.16-mcp-servidor](../tareas/F2.16-mcp-servidor.md) | Servidor MCP (stdio) con las tools de PublicCode filtradas por la política: `pcode mcp --servir` | haiku | S | 1.8 | F2.03, F1.18, F1.11, F2.16a, F2.16b | bloqueada |
| [F2.16a-mcp-servidor-nucleo](../tareas/F2.16a-mcp-servidor-nucleo.md) | Crate `pcode-mcp`: `McpServer` responde initialize, tools/list y tools/call aplicando la política | haiku | M | 1.8 | F1.11, F1.13 | lista |
| [F2.16b-mcp-servidor-cli](../tareas/F2.16b-mcp-servidor-cli.md) | `pcode mcp --servir [--politica <toml>]`: arrancar el servidor MCP por stdio | haiku | S | 1.8 | F2.16a | bloqueada |
| [F2.17-sincronizar-instrucciones](../tareas/F2.17-sincronizar-instrucciones.md) | `pcode instrucciones sincronizar`: AGENTS.md → CLAUDE.md, GEMINI.md, copilot-instructions | haiku | S | 1.4 | F2.17a, F2.17b | bloqueada |
| [F2.17a-instrucciones-nucleo](../tareas/F2.17a-instrucciones-nucleo.md) | Instrucciones derivadas de AGENTS.md: generar y comprobar (núcleo) | haiku | M | 1.4 | F2.14 | lista |
| [F2.17b-instrucciones-cli](../tareas/F2.17b-instrucciones-cli.md) | CLI: `pcode instrucciones sincronizar [--comprobar]` | haiku | S | 1.4 | F2.17a | bloqueada |
| [F2.18-cli-backend-sandbox](../tareas/F2.18-cli-backend-sandbox.md) | CLI: `pcode run --backend X --sandbox N1` y `pcode sandbox …` | haiku | — | — | F2.07, F2.08, F2.09, F2.10, F2.12 | hecha |
| [F2.19-proxy-salida](../tareas/F2.19-proxy-salida.md) | `pcode-netproxy`: proxy CONNECT en socket Unix con lista de dominios y auditoría | haiku | S | 1.5 | F2.19a, F2.19b, F2.19c | bloqueada |
| [F2.19a-proxy-reglas](../tareas/F2.19a-proxy-reglas.md) | Proxy de salida: lista de dominios y parseo de la primera línea (lógica pura) | haiku | S | 1.5 | F2.03 | lista |
| [F2.19b-proxy-connect](../tareas/F2.19b-proxy-connect.md) | Proxy de salida: servidor en socket Unix con túnel CONNECT y aviso de cada decisión | haiku | M | 1.5 | F2.19a | bloqueada |
| [F2.19c-proxy-http-plano](../tareas/F2.19c-proxy-http-plano.md) | Proxy de salida: reenvío de peticiones HTTP en claro (URL absoluta) | haiku | S | 1.5 | F2.19b | bloqueada |
| [F2.20-puente-sandbox](../tareas/F2.20-puente-sandbox.md) | Puente TCP→Unix dentro de la sandbox y red `Filtered` | haiku | S | 1.5 | F2.20a, F2.20b, F2.20c, F2.20d | bloqueada |
| [F2.20a-puente-nucleo](../tareas/F2.20a-puente-nucleo.md) | Puente: reenviar cada conexión TCP a un socket Unix | haiku | S | 1.5 | F2.03 | lista |
| [F2.20b-puente-cli](../tareas/F2.20b-puente-cli.md) | CLI: subcomando oculto `pcode __puente` | haiku | M | 1.5 | F2.20a | bloqueada |
| [F2.20c-red-filtrada-sandbox](../tareas/F2.20c-red-filtrada-sandbox.md) | Sandbox: `Network::Filtered` arranca el proxy y `prepare.sh` lanza el puente | haiku | M | 1.5 | F2.04c, F2.19c, F2.20a | bloqueada |
| [F2.20d-red-filtrada-cli](../tareas/F2.20d-red-filtrada-cli.md) | CLI: `[sandbox] red = "filtrar"` y `pcode sandbox ejecutar --red-permitir` | haiku | M | 1.5 | F2.04d, F2.20b, F2.20c | bloqueada |
| [F2.21-windows-sin-n1](../tareas/F2.21-windows-sin-n1.md) | Windows sin WSL: copia de trabajo aislada (N0) para backends y flujos | coordinador | — | — | F2.03, F2.18, F3.10 | hecha |
| [F2.22-mcp-cliente](../tareas/F2.22-mcp-cliente.md) | Cliente MCP: tools de servidores MCP para el agente | coordinador | — | — | F1.08, F1.26 | hecha |

### F3 — Flujos

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F3.01-tareas-parser](../tareas/F3.01-tareas-parser.md) | `pcode-flow`: leer tareas (frontmatter y cabeceras HTML) | haiku | — | — | F0.01 | hecha |
| [F3.02-dag](../tareas/F3.02-dag.md) | DAG de tareas: ciclos, dependencias inexistentes y tareas listas | haiku | — | — | F3.01 | hecha |
| [F3.03-eventos-flujo](../tareas/F3.03-eventos-flujo.md) | Estado del flujo por eventos en SQLite y su proyección | haiku | — | — | F3.02, F1.19 | hecha |
| [F3.04-planificador-ejecucion](../tareas/F3.04-planificador-ejecucion.md) | Planificador de ejecución: paralelismo por modo y exclusión de kernel | haiku | — | — | F3.03 | hecha |
| [F3.05-aceptacion-dos-etapas](../tareas/F3.05-aceptacion-dos-etapas.md) | Aceptación en dos etapas: rápida en N1 y completa en N2 (contenedor) | haiku | — | — | F3.04, F2.03, F1.18 | hecha |
| [F3.06-reintentos-relevo](../tareas/F3.06-reintentos-relevo.md) | Reintentos con informe de relevo, paso crítico y escalado de backend | haiku | — | — | F3.05, F2.06 | hecha |
| [F3.07-integracion](../tareas/F3.07-integracion.md) | Cola de integración: aplicar, commit, rebase, lockfiles regenerados y re-validación | haiku | — | — | F3.05 | hecha |
| [F3.08-clasificador-ci](../tareas/F3.08-clasificador-ci.md) | Clasificador de ejecuciones de GitHub Actions (sin tareas fantasma) | haiku | — | — | F3.01 | hecha |
| [F3.09-planificador-llm](../tareas/F3.09-planificador-llm.md) | Planificador LLM: generar tareas cuando la cola baja y validarlas antes de aceptarlas | haiku | S | 1.4 | F3.04, F2.07, F3.09a, F3.09b | bloqueada |
| [F3.09a-planificador-nucleo](../tareas/F3.09a-planificador-nucleo.md) | Planificador: cuándo planificar, prompt y validación de las tareas nuevas (lógica pura) | haiku | M | 1.4 | F3.04, F11.15 | bloqueada |
| [F3.09b-planificador-cli](../tareas/F3.09b-planificador-cli.md) | CLI: `pcode flow planificar "<objetivo>"` escribe tareas nuevas como borrador | haiku | M | 1.4 | F3.09a | bloqueada |
| [F3.10-cli-flow](../tareas/F3.10-cli-flow.md) | `pcode flow run|estado|parar` y `estado.md` | haiku | — | — | F3.06, F3.07, F3.08, F2.18 | hecha |
| [F3.12-revision-cruzada](../tareas/F3.12-revision-cruzada.md) | Revisión cruzada del diff por otro backend (rúbrica del repo y seguridad del parche) | haiku | S | 1.4 | F3.10, F3.12a, F3.12b | bloqueada |
| [F3.12a-revision-nucleo](../tareas/F3.12a-revision-nucleo.md) | Revisión cruzada: rúbrica, prompt y lectura del veredicto (lógica pura) | haiku | M | 1.4 | F3.09 | bloqueada |
| [F3.12b-revision-runner](../tareas/F3.12b-revision-runner.md) | Revisión cruzada en el `Runner`: revisor opcional tras la aceptación y antes de integrar | haiku | M | 1.4 | F3.12a | bloqueada |
| [F3.13-bench](../tareas/F3.13-bench.md) | `pcode bench`: ejecutar PublicBench con cualquier backend | haiku | S | 1.4 | F3.10, F3.13a, F3.13b, F3.13c | bloqueada |
| [F3.13a-bench-nucleo](../tareas/F3.13a-bench-nucleo.md) | Banco: casos, filas de resultado, resumen e informe Markdown (lógica pura) | haiku | M | 1.4 | F3.12 | bloqueada |
| [F3.13b-bench-ejecutar](../tareas/F3.13b-bench-ejecutar.md) | Banco: ejecutar un caso (worktree en `base`, sandbox, backend, aceptación y reintentos con relevo) | haiku | M | 1.4 | F3.13a | bloqueada |
| [F3.13c-bench-cli](../tareas/F3.13c-bench-cli.md) | CLI: `pcode bench ejecutar` y `pcode bench informe` | haiku | M | 1.4 | F3.13b | bloqueada |

### F4 — Contexto, memoria y router

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F4.01-contexto-tool](../tareas/F4.01-contexto-tool.md) | Tool `contexto` (fijar, soltar, listar) sobre resultados de tools con id corto `[rN]` | haiku | S | 1.3 | F1.21, F4.01a, F4.01b, F4.01c | hecha |
| [F4.01a-contexto-registro](../tareas/F4.01a-contexto-registro.md) | `ContextLedger`: ids `[rN]` para los resultados de tools, soltar y fijar (lógica pura) | haiku | S | 1.3 | F1.21 | hecha |
| [F4.01b-contexto-tool-logica](../tareas/F4.01b-contexto-tool-logica.md) | Tool `contexto`: especificación y `ContextLedger::handle` (soltar, fijar, lista) | haiku | S | 1.3 | F4.01a | hecha |
| [F4.01c-contexto-cableado](../tareas/F4.01c-contexto-cableado.md) | Bucle del agente: etiquetar resultados con `[rN]` e interceptar la tool `contexto` | haiku | M | 1.3 | F4.01b | hecha |
| [F4.02-compactacion-prefijo](../tareas/F4.02-compactacion-prefijo.md) | Compactación por segmentos con prefijo estable (no rompe la caché) | haiku | S | 1.7 | F4.01, F4.02a, F4.02b, F4.02c | bloqueada |
| [F4.02a-compactacion-plan](../tareas/F4.02a-compactacion-plan.md) | `compact.rs`: estimar tokens, segmentos y plan de compactación (lógica pura) | haiku | S | 1.7 | F1.21 | lista |
| [F4.02b-compactacion-resumen](../tareas/F4.02b-compactacion-resumen.md) | `compact.rs`: texto para el resumidor, rutas mencionadas y aplicar el resumen (lógica pura) | haiku | S | 1.7 | F4.02a | bloqueada |
| [F4.02c-compactacion-cableado](../tareas/F4.02c-compactacion-cableado.md) | `Agent::compact` usa `compact.rs`: segmentos, resultados fijados y disparo por tokens estimados | haiku | M | 1.7 | F4.02b, F4.01c | bloqueada |
| [F4.03-memoria-facetas](../tareas/F4.03-memoria-facetas.md) | `pcode-memory`: memoria de archivos por facetas invalidada por hash | haiku | S | 1.7 | F1.19, F4.03a, F4.03b, F4.03c, F4.03d | bloqueada |
| [F4.03a-memoria-crate](../tareas/F4.03a-memoria-crate.md) | Crear el crate `pcode-memory` con `sha256_hex` y `sha256_file` | haiku | S | 1.7 | F1.19 | lista |
| [F4.03b-memoria-facetas-store](../tareas/F4.03b-memoria-facetas-store.md) | `FacetStore`: facetas de un archivo en SQLite, válidas solo si el hash coincide | haiku | M | 1.7 | F4.03a | bloqueada |
| [F4.03c-memoria-archivo-tool](../tareas/F4.03c-memoria-archivo-tool.md) | Tool `memoria_archivo`: el agente lee y guarda las facetas de un archivo | haiku | M | 1.7 | F4.03b | bloqueada |
| [F4.03d-memoria-archivo-chat](../tareas/F4.03d-memoria-archivo-chat.md) | Registrar la tool `memoria_archivo` en el agente del chat y de la TUI | haiku | S | 1.7 | F4.03c | bloqueada |
| [F4.04-tree-sitter](../tareas/F4.04-tree-sitter.md) | Mapa del repo con tree-sitter y tool `editar_simbolo` | haiku | S | 1.7 | F1.10, F4.04a, F4.04b, F4.04c | bloqueada |
| [F4.04a-simbolos-rango](../tareas/F4.04a-simbolos-rango.md) | `symbols.rs`: rango de líneas y nombre cualificado de cada símbolo (sobre `outline`, sin tree-sitter) | haiku | M | 1.7 | F1.10 | lista |
| [F4.04b-editar-simbolo](../tareas/F4.04b-editar-simbolo.md) | Tool `editar_simbolo`: reemplazar una función, método o tipo entero por su nombre | haiku | M | 1.7 | F4.04a | bloqueada |
| [F4.04c-simbolos-tree-sitter](../tareas/F4.04c-simbolos-tree-sitter.md) | Rangos exactos de símbolos con tree-sitter para Rust y Python | haiku | M | 1.7 | F4.04b | bloqueada |
| [F4.05-busqueda-hibrida](../tareas/F4.05-busqueda-hibrida.md) | Búsqueda híbrida (BM25 + símbolos, RRF), compresión de código y subagente bibliotecario | haiku | S | 1.7 | F4.04, F4.03, F4.05a, F4.05b, F4.05c, F4.05d, F4.05e, F4.05f | bloqueada |
| [F4.05a-bm25](../tareas/F4.05a-bm25.md) | `bm25.rs`: tokenizador de código y ranking BM25 (lógica pura, en `pcode-tools`) | haiku | S | 1.3 | F1.11 | hecha |
| [F4.05b-busqueda-fragmentos-rrf](../tareas/F4.05b-busqueda-fragmentos-rrf.md) | `search.rs`: fragmentos de un archivo por símbolos y fusión de listas con RRF (lógica pura) | haiku | S | 1.7 | F4.03a, F4.04a | bloqueada |
| [F4.05c-busqueda-indice](../tareas/F4.05c-busqueda-indice.md) | `RepoIndex`: índice del repositorio incremental por hash, con búsqueda BM25 + símbolos | haiku | M | 1.7 | F4.05a, F4.05b | bloqueada |
| [F4.05d-comprimir-codigo](../tareas/F4.05d-comprimir-codigo.md) | `compress_code`: conservar firmas y cuerpos relevantes, omitir el resto (lógica pura) | haiku | M | 1.7 | F4.05a, F4.05b | bloqueada |
| [F4.05e-buscar-codigo-tool](../tareas/F4.05e-buscar-codigo-tool.md) | Tool `buscar_codigo`: búsqueda aproximada en el código con `RepoIndex` | haiku | S | 1.7 | F4.05c | bloqueada |
| [F4.05f-buscar-codigo-chat](../tareas/F4.05f-buscar-codigo-chat.md) | Registrar la tool `buscar_codigo` en el agente del chat y de la TUI | haiku | S | 1.7 | F4.05e, F4.03 | bloqueada |
| [F4.06-contexto-por-rol](../tareas/F4.06-contexto-por-rol.md) | Contexto por rol para subagentes y tablero compartido acotado | haiku | S | 1.7 | F2.06, F4.06a, F4.06b | bloqueada |
| [F4.06a-contexto-roles](../tareas/F4.06a-contexto-roles.md) | `roles.rs`: mensajes iniciales de un subagente según su rol y capacidades de cada rol | haiku | M | 1.7 | F2.06 | lista |
| [F4.06b-tablero-compartido](../tareas/F4.06b-tablero-compartido.md) | `Board`: tablero clave-valor acotado que comparten los subagentes de un flujo | haiku | S | 1.7 | F2.06 | lista |
| [F4.07-router-historico](../tareas/F4.07-router-historico.md) | Router por histórico del banco y rol crítico opcional | haiku | S | 1.7 | F3.13, F4.07a, F4.07b | bloqueada |
| [F4.07a-router-nucleo](../tareas/F4.07a-router-nucleo.md) | Router: estadísticas por backend y rasgo de tarea, y elección (lógica pura) | haiku | M | 1.7 | F3.13, F6.04b | bloqueada |
| [F4.07b-router-en-el-flujo](../tareas/F4.07b-router-en-el-flujo.md) | Router en `pcode flow run --router <carpeta>`: elegir backend por el histórico del banco | haiku | M | 1.7 | F4.07a | bloqueada |
| [F4.08-parada-temprana](../tareas/F4.08-parada-temprana.md) | Parada temprana guiada por experiencia y puerta de incertidumbre antes de acciones caras | haiku | S | 1.7 | F4.07, F1.21, F4.08a, F4.08b | bloqueada |
| [F4.08a-parada-temprana-nucleo](../tareas/F4.08a-parada-temprana-nucleo.md) | Parada temprana y puerta de incertidumbre (lógica pura en `early_stop.rs`) | haiku | S | 1.7 | F1.21 | lista |
| [F4.08b-parada-temprana-agente](../tareas/F4.08b-parada-temprana-agente.md) | Agente: parar pronto con `with_experience` y pedir aprobación ante la duda con `with_uncertainty_gate` | haiku | M | 1.7 | F4.08a | bloqueada |
| [F4.09-memoria-episodica](../tareas/F4.09-memoria-episodica.md) | Memoria episódica verificada: experiencia de tareas anteriores solo si pasó `acepta` | haiku | S | 1.7 | F4.09a, F4.09b | bloqueada |
| [F4.09a-episodio-de-informe](../tareas/F4.09a-episodio-de-informe.md) | `episodic.rs`: convertir un informe de tarea en un `Episode` y pintarlo compacto (lógica pura) | haiku | S | 1.7 | F4.03a | bloqueada |
| [F4.09b-memoria-episodica-recall](../tareas/F4.09b-memoria-episodica-recall.md) | `EpisodicMemory`: cargar los informes y recordar solo episodios verificados (BM25) | haiku | S | 1.7 | F4.09a, F4.05a | bloqueada |
| [F4.10-busqueda-tools](../tareas/F4.10-busqueda-tools.md) | Búsqueda y deduplicación de tools: anunciar solo las relevantes | haiku | S | 1.3 | F2.22, F4.10a, F4.10b | lista |
| [F4.10a-catalogo-tools](../tareas/F4.10a-catalogo-tools.md) | `catalog.rs`: deduplicar tools de MCP, anunciar solo las visibles y tool `buscar_tools` (lógica pura) | haiku | M | 1.3 | F4.05a, F2.22 | hecha |
| [F4.10b-busqueda-tools-cableado](../tareas/F4.10b-busqueda-tools-cableado.md) | Bucle del agente: anunciar las tools por el catálogo e interceptar `buscar_tools` | haiku | M | 1.3 | F4.10a, F4.01c | hecha |
| [F4.11-preguntar](../tareas/F4.11-preguntar.md) | Tool `preguntar` con presupuesto (interactivo, flujo y banco) | haiku | — | — | F3.10 | hecha |
| [F4.12-guion](../tareas/F4.12-guion.md) | Tool `guion`: secuencia determinista de órdenes en una sola llamada | haiku | S | 1.3 | F1.17, F4.12a, F4.12b, F4.12c | bloqueada |
| [F4.12a-guion-formato](../tareas/F4.12a-guion-formato.md) | `script.rs`: especificación de `guion`, leer los pasos y pintar el resultado (lógica pura) | haiku | S | 1.3 | F1.17 | hecha |
| [F4.12b-guion-ejecutar](../tareas/F4.12b-guion-ejecutar.md) | `run_script`: ejecutar los pasos con `bash`, cada uno comprobado por la política | haiku | S | 1.3 | F4.12a | hecha |
| [F4.12c-guion-cableado](../tareas/F4.12c-guion-cableado.md) | Bucle del agente: ofrecer e interceptar la tool `guion` | haiku | M | 1.3 | F4.12b, F4.10b | lista |

### F5 — Interfaces

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F5.01-protocolo](../tareas/F5.01-protocolo.md) | `pcode-protocol`: tipos de la API (JSON-RPC y eventos) con exportación a TypeScript | haiku | — | — | F0.01 | hecha |
| [F5.02-demonio](../tareas/F5.02-demonio.md) | `pcode-server`: demonio `pcoded` con JSON-RPC, eventos SSE y sesiones | haiku | — | — | F5.01, F2.18 | hecha |
| [F5.03-aprobaciones](../tareas/F5.03-aprobaciones.md) | Cola de aprobaciones: las decisiones `Ask` esperan a un humano desde cualquier cliente | haiku | — | — | F5.02 | hecha |
| [F5.04-api-flujo](../tareas/F5.04-api-flujo.md) | Métodos y eventos de flujos en el demonio | haiku | S | 1.4 | F5.02, F3.10, F5.04a, F5.04b, F5.04c | bloqueada |
| [F5.04a-api-flujo-protocolo](../tareas/F5.04a-api-flujo-protocolo.md) | Protocolo: métodos `flow.run` y `flow.stop` y tipo `FlowStatusResult` | haiku | S | 1.4 | F5.02 | lista |
| [F5.04b-api-flujo-estado](../tareas/F5.04b-api-flujo-estado.md) | Demonio: leer el estado del flujo del proyecto (`flow_api.rs`, lógica) | haiku | M | 1.4 | F5.04a, F3.10 | bloqueada |
| [F5.04c-api-flujo-servidor](../tareas/F5.04c-api-flujo-servidor.md) | Demonio: métodos `flow.status`, `flow.run` y `flow.stop` y evento `TaskUpdated` | haiku | M | 1.4 | F5.04b | bloqueada |
| [F5.05-cliente-rust](../tareas/F5.05-cliente-rust.md) | `pcode-client`: cliente Rust del demonio con arranque automático | haiku | — | — | F5.02 | sustituida |
| [F5.06-cli-maquinas](../tareas/F5.06-cli-maquinas.md) | CLI: `--formato json|stream-json`, autocompletado y `pcode` sin argumentos abre la TUI | haiku | S | 1.3 | F5.06a, F5.06b, F5.06c | bloqueada |
| [F5.06a-cli-formato-salida](../tareas/F5.06a-cli-formato-salida.md) | CLI: formatos de salida de `pcode run` (`texto`, `json`, `stream-json`), lógica pura | haiku | S | 1.3 | — | lista |
| [F5.06b-cli-run-formato](../tareas/F5.06b-cli-run-formato.md) | CLI: `pcode run --formato texto|json|stream-json` | haiku | M | 1.3 | F5.06a | bloqueada |
| [F5.06c-cli-completions](../tareas/F5.06c-cli-completions.md) | CLI: `pcode completions <bash|zsh|fish|powershell>` | haiku | S | 1.3 | F5.06b | bloqueada |
| [F5.07-tui-estado](../tareas/F5.07-tui-estado.md) | `pcode-tui`: estado puro `App`, disposición de paneles e i18n | haiku | — | — | F5.05 | sustituida |
| [F5.08-tui-chat](../tareas/F5.08-tui-chat.md) | TUI: chat interactivo con transcripción en vivo, tools plegables y entrada multilínea | haiku | — | — | F5.07 | sustituida |
| [F5.09-tui-ordenes](../tareas/F5.09-tui-ordenes.md) | TUI: órdenes `/…`, menciones `@archivo` con autocompletado difuso y `!orden` | haiku | — | — | F5.08 | sustituida |
| [F5.10-tui-multiagente](../tareas/F5.10-tui-multiagente.md) | TUI: árbol de agentes y subagentes, aprobaciones en línea y revisión de cambios | haiku | S | 1.3 | F5.03, F9.29, F5.10a, F5.10b | bloqueada |
| [F5.10a-tui-arbol-sesiones](../tareas/F5.10a-tui-arbol-sesiones.md) | TUI: árbol de sesiones y subagentes para la barra lateral (lógica pura) | haiku | S | 1.3 | F9.29g | lista |
| [F5.10b-tui-arbol-cableado](../tareas/F5.10b-tui-arbol-cableado.md) | TUI: sesiones del proyecto y subagentes en la barra lateral | haiku | M | 1.3 | F5.10a | bloqueada |
| [F5.11-tui-atajos-temas](../tareas/F5.11-tui-atajos-temas.md) | TUI: atajos configurables, temas claro, oscuro y alto contraste | haiku | S | 1.3 | F9.29, F5.11a, F5.11b, F5.11c | bloqueada |
| [F5.11a-tui-atajos-mapa](../tareas/F5.11a-tui-atajos-mapa.md) | TUI: mapa de atajos y `keybindings.toml` (lógica pura) | haiku | M | 1.3 | F9.29g | lista |
| [F5.11b-tui-atajos-cableado](../tareas/F5.11b-tui-atajos-cableado.md) | TUI: usar el mapa de atajos en `App` y cargar `keybindings.toml` | haiku | M | 1.3 | F5.10b, F5.11a | bloqueada |
| [F5.11c-tui-ayuda-atajos](../tareas/F5.11c-tui-ayuda-atajos.md) | TUI: F1 muestra la ayuda de atajos | haiku | S | 1.3 | F5.11b | bloqueada |
| [F5.12-ui-base](../tareas/F5.12-ui-base.md) | `apps/ui`: base React + TypeScript + Vite, tema, i18n y `Transport` (http y mock) | haiku | — | — | F5.01 | sustituida |
| [F5.13-ui-chat](../tareas/F5.13-ui-chat.md) | UI: lista de sesiones y chat con transcripción en vivo | haiku | — | — | F5.12, F5.02 | sustituida |
| [F5.14-ui-revision](../tareas/F5.14-ui-revision.md) | UI: revisión de cambios (diff unificado y lado a lado) y cola de aprobaciones | haiku | S | 1.6 | F5.03, F8.01, F5.14a, F5.14b, F5.14c, F5.14d, F5.14e | bloqueada |
| [F5.14a-aprobaciones-recordar](../tareas/F5.14a-aprobaciones-recordar.md) | Aprobaciones: «permitir y recordar» en `ApprovalBroker` | haiku | S | 1.6 | F5.03 | lista |
| [F5.14b-aprobaciones-recordar-api](../tareas/F5.14b-aprobaciones-recordar-api.md) | API: `approvals.decide` acepta `remember` | haiku | S | 1.6 | F5.14a | bloqueada |
| [F5.14c-diff-js](../tareas/F5.14c-diff-js.md) | Studio: módulo `diff.js` (parsear un diff unificado y emparejarlo lado a lado, puro) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F5.14d-diff-lado-a-lado-studio](../tareas/F5.14d-diff-lado-a-lado-studio.md) | Studio: ver los diffs lado a lado o unificados, con números de línea | haiku | M | 1.6 | F5.14c | bloqueada |
| [F5.14e-aprobaciones-recordar-studio](../tareas/F5.14e-aprobaciones-recordar-studio.md) | Studio: botón «Permitir y recordar» y procedencia en las aprobaciones | haiku | S | 1.6 | F5.14b, F8.01 | bloqueada |
| [F5.15-ui-centro-control](../tareas/F5.15-ui-centro-control.md) | UI: centro de control multiagente (tablero de tareas, árbol de agentes, registro en vivo) | haiku | S | 1.4 | F5.04, F8.01, F5.15a, F5.15b, F5.15c | bloqueada |
| [F5.15a-tablero-js](../tareas/F5.15a-tablero-js.md) | Studio: módulo `tablero.js` (columnas del flujo, árbol de agentes y foco por teclado, puro) | haiku | M | 1.4 | F8.01 | bloqueada |
| [F5.15b-centro-control-tablero](../tareas/F5.15b-centro-control-tablero.md) | Studio: pestaña «Centro de control» con el tablero de tareas del flujo en vivo | haiku | M | 1.4 | F5.15a, F5.04 | bloqueada |
| [F5.15c-centro-control-agentes](../tareas/F5.15c-centro-control-agentes.md) | Studio: árbol de agentes, «Requiere coordinador» y parar el flujo en el centro de control | haiku | S | 1.4 | F5.15b | bloqueada |
| [F5.16-ui-ajustes](../tareas/F5.16-ui-ajustes.md) | UI y demonio: ajustes de backends (detección e inicio de sesión), proveedores, tema e idioma | haiku | — | — | F5.13 | sustituida |
| [F5.17-ui-skills-banco](../tareas/F5.17-ui-skills-banco.md) | UI: gestor de skills y gráficas del banco | haiku | S | 1.4 | F2.14, F8.01, F5.17a, F5.17b, F5.17c, F5.17d, F5.17e, F5.17f | bloqueada |
| [F5.17a-skills-api-nucleo](../tareas/F5.17a-skills-api-nucleo.md) | Demonio: listar skills (activas, en cuarentena, retiradas) y exportarlas | haiku | M | 1.4 | F2.14 | lista |
| [F5.17b-banco-api-nucleo](../tareas/F5.17b-banco-api-nucleo.md) | Demonio: resumen de los resultados del banco por backend | haiku | S | 1.4 | F5.02 | lista |
| [F5.17c-skills-banco-api](../tareas/F5.17c-skills-banco-api.md) | API: `skills.list`, `skills.export` y `bench.summary` | haiku | S | 1.4 | F5.17a, F5.17b | bloqueada |
| [F5.17d-banco-js](../tareas/F5.17d-banco-js.md) | Studio: módulo `banco.js` (porcentajes, barras y formato de tokens, puro) | haiku | S | 1.4 | F8.01 | bloqueada |
| [F5.17e-skills-studio](../tareas/F5.17e-skills-studio.md) | Studio: pestaña «Skills» con las tres listas, el `SKILL.md` y la exportación | haiku | M | 1.4 | F5.17c, F8.01 | bloqueada |
| [F5.17f-banco-studio](../tareas/F5.17f-banco-studio.md) | Studio: pestaña «Banco» con tabla y barras SVG por backend | haiku | M | 1.4 | F5.17c, F5.17d | bloqueada |
| [F5.18-escritorio-base](../tareas/F5.18-escritorio-base.md) | `apps/desktop`: Tauri 2 con `apps/ui`, `pcoded` como sidecar y transporte Tauri | haiku | — | — | F5.12, F5.05 | sustituida |
| [F5.19-escritorio-sistema](../tareas/F5.19-escritorio-sistema.md) | Escritorio: bandeja con estado, notificaciones nativas, enlaces `pcode://` y arranque con el sistema | haiku | — | — | F5.18, F5.03 | descartada |
| [F5.20-escritorio-credenciales](../tareas/F5.20-escritorio-credenciales.md) | Claves en el llavero del sistema: `{llavero:nombre}` en la configuración y `pcode secreto` | haiku | S | 1.5 | F5.18, F1.01, F5.20a, F5.20b | bloqueada |
| [F5.20a-llavero-referencias](../tareas/F5.20a-llavero-referencias.md) | `pcode-config`: trait `SecretStore`, almacén en memoria y sustitución de `{llavero:nombre}` | haiku | S | 1.5 | F1.01 | lista |
| [F5.20b-llavero-cli](../tareas/F5.20b-llavero-cli.md) | Llavero real (`keyring`) al cargar la configuración y `pcode secreto poner|quitar|listar` | haiku | M | 1.5 | F5.20a | bloqueada |
| [F5.21-escritorio-empaquetado](../tareas/F5.21-escritorio-empaquetado.md) | Empaquetado del escritorio para Linux y Windows, y actualizaciones firmadas | coordinador | — | — | F5.19, F5.20 | sustituida |
| [F5.22-web-servida](../tareas/F5.22-web-servida.md) | El demonio sirve `apps/ui` con token de un solo uso y CSP estricta | haiku | — | — | F5.13 | sustituida |
| [F5.23-pwa-movil](../tareas/F5.23-pwa-movil.md) | PWA para el móvil: estado y aprobaciones | haiku | — | — | F5.22, F5.14 | sustituida |
| [F5.24-acp](../tareas/F5.24-acp.md) | Servidor ACP (Agent Client Protocol) para editores: `pcode acp` | haiku | S | 1.8 | F5.03, F5.05, F5.24a, F5.24b | bloqueada |
| [F5.24a-acp-mensajes](../tareas/F5.24a-acp-mensajes.md) | Crate `pcode-acp`: leer los mensajes del editor y construir las respuestas y avisos ACP (lógica pura) | haiku | M | 1.8 | F5.03 | lista |
| [F5.24b-acp-orden](../tareas/F5.24b-acp-orden.md) | `pcode acp`: sesión ACP por stdio con el agente en proceso, avisos en vivo y permisos | haiku | M | 1.8 | F5.24a | bloqueada |
| [F5.25-vscode](../tareas/F5.25-vscode.md) | Extensión fina de VS Code: webview con `apps/ui` y órdenes contextuales | haiku | — | despues | F5.22 | esbozo |
| [F5.26-github](../tareas/F5.26-github.md) | GitHub: PR del trabajo integrado, comentarios con resultados y checks | haiku | S | 1.8 | F3.10, F2.11c, F5.26a, F5.26b, F5.26c | bloqueada |
| [F5.26a-github-pr-y-checks-api](../tareas/F5.26a-github-pr-y-checks-api.md) | GitHub: `create_pr` y `create_check_run` en `GitHubApi` (falsa y HTTP) | haiku | M | 1.8 | F2.11c | bloqueada |
| [F5.26b-github-pr-de-tarea](../tareas/F5.26b-github-pr-de-tarea.md) | Flujos: PR de una tarea con informe, salida de `acepta`, check y comentario de revisión | haiku | M | 1.8 | F5.26a, F4.07 | bloqueada |
| [F5.26c-github-rama-de-tarea](../tareas/F5.26c-github-rama-de-tarea.md) | Flujos: integrar una tarea en su propia rama y subirla (sin forzar) | haiku | M | 1.8 | F5.26b | bloqueada |
| [F5.27-windows-wsl](../tareas/F5.27-windows-wsl.md) | Windows: detectar WSL2 y trabajar dentro de él con la sandbox N1 (`pcode wsl`) | haiku | S | 1.8 | F5.18, F5.27a, F5.27b | bloqueada |
| [F5.27a-wsl-nucleo](../tareas/F5.27a-wsl-nucleo.md) | WSL: leer `wsl.exe -l -v` (UTF-16LE), traducir rutas de Windows y montar la orden (lógica pura) | haiku | S | 1.8 | F2.21 | lista |
| [F5.27b-wsl-orden](../tareas/F5.27b-wsl-orden.md) | `pcode wsl [--distro D] [-- args]` en Windows y línea de WSL2 en `pcode doctor` | haiku | S | 1.8 | F5.27a | bloqueada |
| [F5.28-apk-android](../tareas/F5.28-apk-android.md) | APK de Android: cliente fino de pcoded (WebView) y demonio accesible desde la red | coordinador | — | — | F5.02, F0.07 | hecha |
| [F5.29-chat](../tareas/F5.29-chat.md) | `pcode chat`: conversación interactiva con el agente (sustituto de Claude Code y opencode) | coordinador | — | — | F1.16, F1.22, F1.23, F5.28 | hecha |
| [F5.30-tui-local](../tareas/F5.30-tui-local.md) | TUI a pantalla completa con el agente en proceso (`pcode`, `pcode tui`) | coordinador | — | — | F5.29, F1.25 | hecha |
| [F5.31-diseno](../tareas/F5.31-diseno.md) | Sistema de diseño Material azul en la TUI, la web del demonio y el APK | coordinador | — | — | F5.30, F5.28 | hecha |
| [F5.32-studio](../tareas/F5.32-studio.md) | PublicCode Studio: editor web con explorador, git, terminal y agente (`pcode studio`) | coordinador | — | — | F5.31, F5.29 | hecha |
| [F5.33-sesiones-conversacion](../tareas/F5.33-sesiones-conversacion.md) | Sesiones de conversación en el demonio: varios turnos, streaming y aprobaciones | coordinador | — | — | F5.03, F5.29 | hecha |
| [F5.34-tui-modos](../tareas/F5.34-tui-modos.md) | Modo plan, `@archivo` con autocompletado y `!orden` en la TUI y el chat | coordinador | — | — | F5.30 | hecha |
| [F5.35-lsp](../tareas/F5.35-lsp.md) | LSP en Studio y la TUI: ir a la definición, referencias y errores en línea | coordinador | — | — | F5.32 | hecha |

### F6 — Aprendizaje

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F6.01-destilador-skills](../tareas/F6.01-destilador-skills.md) | Destilador de skills desde parejas fallo/acierto (a cuarentena) | haiku | S | 1.7 | F6.01a, F6.01b, F6.01c | bloqueada |
| [F6.01a-destilador-prompt](../tareas/F6.01a-destilador-prompt.md) | Destilador: prompt desde parejas fallo/acierto y extracción del SKILL.md de la respuesta | haiku | S | 1.7 | F2.15 | lista |
| [F6.01b-destilador-cuarentena](../tareas/F6.01b-destilador-cuarentena.md) | Destilador: validar una skill aprendida y guardarla en cuarentena | haiku | S | 1.7 | F6.01a, F7.01a | bloqueada |
| [F6.01c-destilador-candidatas](../tareas/F6.01c-destilador-candidatas.md) | Destilador: tareas candidatas (aceptadas tras 2 o más intentos) desde los eventos del flujo | haiku | S | 1.7 | F3.06 | lista |
| [F6.02-puerta-regresion](../tareas/F6.02-puerta-regresion.md) | Puerta de regresión: una skill sale de cuarentena solo si mejora sin romper nada | haiku | S | 1.7 | F6.02a, F6.02b | bloqueada |
| [F6.02a-puerta-decision](../tareas/F6.02a-puerta-decision.md) | Puerta de regresión: decidir con los resultados del banco antes y después (lógica pura) | haiku | S | 1.7 | F6.01b, F3.13 | bloqueada |
| [F6.02b-puerta-activar](../tareas/F6.02b-puerta-activar.md) | Puerta de regresión: activar una skill en cuarentena con su `EVIDENCIA.md` | haiku | S | 1.7 | F6.02a | bloqueada |
| [F6.03-gobierno-skills](../tareas/F6.03-gobierno-skills.md) | Gobierno de skills y subagentes reutilizables | haiku | S | 1.7 | F6.03a, F6.03b | bloqueada |
| [F6.03a-gobierno-estadisticas](../tareas/F6.03a-gobierno-estadisticas.md) | Gobierno de skills: estadísticas de uso, recomendación y candidatas a retirada (lógica pura) | haiku | S | 1.7 | F6.02b | bloqueada |
| [F6.03b-gobierno-registro-retirada](../tareas/F6.03b-gobierno-registro-retirada.md) | Gobierno de skills: registrar usos en disco y retirar una skill sin borrarla | haiku | S | 1.7 | F6.03a | bloqueada |
| [F6.04-regresion-nocturna](../tareas/F6.04-regresion-nocturna.md) | Regresión nocturna de PublicBench con aviso automático | coordinador | M | 1.4 | F3.13, F0.04, F6.04a, F6.04b | bloqueada |
| [F6.04a-regresion-comparar-nucleo](../tareas/F6.04a-regresion-comparar-nucleo.md) | Banco: comparar dos ejecuciones y redactar el aviso de regresión (lógica pura) | haiku | S | 1.4 | F3.13 | bloqueada |
| [F6.04b-regresion-comparar-cli](../tareas/F6.04b-regresion-comparar-cli.md) | CLI: `pcode bench comparar <anterior> <actual>` con umbral y cuerpo del issue | haiku | S | 1.4 | F6.04a | bloqueada |
| [F6.05-trayectorias](../tareas/F6.05-trayectorias.md) | Puntuación estructural de trayectorias y exportación de trazas | haiku | S | 1.7 | F6.05a, F6.05b | bloqueada |
| [F6.05a-trayectorias-tqs](../tareas/F6.05a-trayectorias-tqs.md) | Trayectorias: puntuación estructural `tqs` en tiempo lineal y sin LLM | haiku | M | 1.7 | F3.03 | lista |
| [F6.05b-trayectorias-exportar](../tareas/F6.05b-trayectorias-exportar.md) | Trayectorias: exportar trazas a JSONL en formato estable | haiku | S | 1.7 | F6.05a | bloqueada |

### F7 — Endurecimiento

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F7.01-redteam-skills](../tareas/F7.01-redteam-skills.md) | Red teaming de skills: exfiltración, instalación de paquetes y órdenes ocultas | haiku | S | 1.5 | F7.01a, F7.01b, F7.01c | bloqueada |
| [F7.01a-lint-seguridad-skills](../tareas/F7.01a-lint-seguridad-skills.md) | Skills: `lint_security`, avisos estáticos de seguridad (lógica pura) | haiku | M | 1.5 | F2.15 | lista |
| [F7.01b-redteam-skills-fixtures](../tareas/F7.01b-redteam-skills-fixtures.md) | Red team: skills maliciosas de ejemplo contra `lint_security` | haiku | S | 1.5 | F7.01a | bloqueada |
| [F7.01c-redteam-skills-efecto](../tareas/F7.01c-redteam-skills-efecto.md) | Red team: los ataques de una skill no tienen efecto fuera de la mini sandbox | haiku | M | 1.5 | F2.20, F7.01b, F12.08 | bloqueada |
| [F7.02-evals-inyeccion](../tareas/F7.02-evals-inyeccion.md) | Evals de inyección indirecta, envenenamiento de repo y seguridad operativa | coordinador | — | — | F1.26, F0.06 | hecha |
| [F7.03-release](../tareas/F7.03-release.md) | Release 1.0: guía de usuario, CHANGELOG, procedencia firmada y versión 1.0.0 | coordinador | — | — | F7.02, F0.08, F5.35, F2.21 | hecha |

### F8 — 1.x: editor, agente y plataforma

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F8.01-studio-modulos](../tareas/F8.01-studio-modulos.md) | Studio: módulos JS servidos por el demonio y tests con el runner nativo de Node | haiku | S | 1.6 | F5.35, F8.01a, F8.01b, F8.01c | bloqueada |
| [F8.01a-studio-assets](../tareas/F8.01a-studio-assets.md) | Studio: tabla de módulos `web_assets.rs`, ruta `/studio/<x>.js` y `window.Studio` | haiku | M | 1.6 | F5.35 | lista |
| [F8.01b-studio-arnes-tests](../tareas/F8.01b-studio-arnes-tests.md) | Studio: arnés de tests JS con `node --test` (sin npm), `tools/studio/test.sh` y job en la CI | haiku | S | 1.6 | F5.35 | lista |
| [F8.01c-studio-difusa](../tareas/F8.01c-studio-difusa.md) | Studio: primer módulo, `difusa.js` (puntuación difusa de Ctrl+P) con test y uso desde `index.html` | haiku | S | 1.6 | F8.01a, F8.01b | bloqueada |
| [F8.02-editor-plegado-minimapa](../tareas/F8.02-editor-plegado-minimapa.md) | Editor: plegado de código por sangría y minimapa | haiku | S | 1.6 | F8.01, F8.02a, F8.02b, F8.02c, F8.02d | bloqueada |
| [F8.02a-plegado-nucleo](../tareas/F8.02a-plegado-nucleo.md) | Plegado: rangos por sangría y proyección del texto (módulo puro `plegado.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.02b-plegado-cableado](../tareas/F8.02b-plegado-cableado.md) | Plegado: triángulos en el margen, Ctrl+Mayús+[ y ], y vista plegada de solo lectura | haiku | M | 1.6 | F8.02a | bloqueada |
| [F8.02c-minimapa-nucleo](../tareas/F8.02c-minimapa-nucleo.md) | Minimapa: filas, zona visible y línea de un clic (módulo puro `minimapa.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.02d-minimapa-cableado](../tareas/F8.02d-minimapa-cableado.md) | Minimapa: canvas a la derecha del editor con la zona visible y clic para saltar | haiku | S | 1.6 | F8.02c | bloqueada |
| [F8.03-editor-multicursor](../tareas/F8.03-editor-multicursor.md) | Editor: multicursor (Alt+clic, Ctrl+D) y selección en columna | haiku | S | 1.6 | F8.01, F8.03a, F8.03b, F8.03c | bloqueada |
| [F8.03a-multicursor-nucleo](../tareas/F8.03a-multicursor-nucleo.md) | Multicursor: apariciones, edición en varios rangos y cursores en columna (módulo puro `multicursor.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.03b-multicursor-cableado](../tareas/F8.03b-multicursor-cableado.md) | Multicursor: Ctrl+D, Ctrl+Mayús+L, capa de cursores y edición en todos a la vez | haiku | M | 1.6 | F8.03a | bloqueada |
| [F8.03c-multicursor-columna](../tareas/F8.03c-multicursor-columna.md) | Multicursor: Alt+clic añade un cursor y Ctrl+Alt+↑/↓ apila cursores en columna | haiku | S | 1.6 | F8.03b | bloqueada |
| [F8.04-editor-dividido](../tareas/F8.04-editor-dividido.md) | Editor: vista dividida y comparar dos archivos | haiku | S | 1.6 | F8.01, F8.04a, F8.04b, F8.04c, F8.04d, F8.04e | bloqueada |
| [F8.04a-comparar-nucleo](../tareas/F8.04a-comparar-nucleo.md) | Comparar: diff por líneas y filas para dos columnas (módulo puro `comparar.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.04b-git-show](../tareas/F8.04b-git-show.md) | Comparar: `git_show` en `workspace.rs` y método `git.show` de la API | haiku | S | 1.6 | F5.35 | lista |
| [F8.04c-comparar-cableado](../tareas/F8.04c-comparar-cableado.md) | Comparar: «Comparar con el archivo abierto» y «Comparar con HEAD» en el explorador, en dos columnas | haiku | M | 1.6 | F8.04a, F8.04b | bloqueada |
| [F8.04d-division-nucleo](../tareas/F8.04d-division-nucleo.md) | Vista dividida: estado de dos paneles (módulo puro `division.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.04e-division-cableado](../tareas/F8.04e-division-cableado.md) | Vista dividida: Ctrl+\ muestra otra pestaña a la derecha en solo lectura, con intercambio | haiku | M | 1.6 | F8.04d | bloqueada |
| [F8.05-vista-previa](../tareas/F8.05-vista-previa.md) | Vista previa de Markdown e imágenes en Studio | haiku | S | 1.6 | F8.01, F8.05a, F8.05b, F8.05c, F8.05d, F8.05e | bloqueada |
| [F8.05a-markdown-nucleo](../tareas/F8.05a-markdown-nucleo.md) | Vista previa: Markdown a bloques sin HTML crudo (módulo puro `markdown.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.05b-markdown-cableado](../tareas/F8.05b-markdown-cableado.md) | Vista previa: Ctrl+Mayús+V abre la vista previa de un Markdown como pestaña | haiku | M | 1.6 | F8.05a | bloqueada |
| [F8.05c-imagen-nucleo](../tareas/F8.05c-imagen-nucleo.md) | Imágenes: leer una imagen del proyecto en base64 (`image.rs`, lógica pura) | haiku | S | 1.6 | F5.35 | lista |
| [F8.05d-imagen-api](../tareas/F8.05d-imagen-api.md) | Imágenes: método `fs.readImage` y `img-src data:` en la CSP | haiku | S | 1.6 | F8.05c | bloqueada |
| [F8.05e-imagen-cableado](../tareas/F8.05e-imagen-cableado.md) | Imágenes: abrir png, jpg, gif, webp y svg como imagen en Studio | haiku | M | 1.6 | F8.05b, F8.05d | bloqueada |
| [F8.06-lsp-formato](../tareas/F8.06-lsp-formato.md) | Formatear documento y formatear al guardar (LSP) | haiku | S | 1.6 | F5.35, F8.01, F8.06a, F8.06b, F8.06c, F8.06d | bloqueada |
| [F8.06a-lsp-ediciones](../tareas/F8.06a-lsp-ediciones.md) | LSP: aplicar ediciones de texto con posiciones UTF-16 (lógica pura) | haiku | S | 1.6 | F5.35 | lista |
| [F8.06b-lsp-formato-cliente](../tareas/F8.06b-lsp-formato-cliente.md) | LSP: `LspClient::formatting` (pedir `textDocument/formatting`) | haiku | S | 1.6 | F8.06a | bloqueada |
| [F8.06c-lsp-formato-api](../tareas/F8.06c-lsp-formato-api.md) | API: método `lsp.format` (devuelve el texto formateado) | haiku | S | 1.6 | F8.06b | bloqueada |
| [F8.06d-lsp-formato-studio](../tareas/F8.06d-lsp-formato-studio.md) | Studio: formatear con Mayús+Alt+F y formatear al guardar | haiku | S | 1.6 | F8.06c, F8.01 | bloqueada |
| [F8.07-lsp-completado](../tareas/F8.07-lsp-completado.md) | Autocompletado del LSP en Studio | haiku | S | 1.6 | F8.01, F5.35, F8.07a, F8.07b, F8.07c, F8.07d, F8.07e | bloqueada |
| [F8.07a-lsp-completado-nucleo](../tareas/F8.07a-lsp-completado-nucleo.md) | LSP: leer la respuesta de `textDocument/completion` (lógica pura) | haiku | S | 1.6 | F5.35 | lista |
| [F8.07b-lsp-completado-cliente](../tareas/F8.07b-lsp-completado-cliente.md) | LSP: `LspClient::completion` | haiku | S | 1.6 | F8.07a | bloqueada |
| [F8.07c-lsp-completado-api](../tareas/F8.07c-lsp-completado-api.md) | API: método `lsp.completion` | haiku | S | 1.6 | F8.07b | bloqueada |
| [F8.07d-lsp-completado-js](../tareas/F8.07d-lsp-completado-js.md) | Studio: módulo `completado.js` (prefijo, filtro e inserción, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.07e-lsp-completado-studio](../tareas/F8.07e-lsp-completado-studio.md) | Studio: popup de autocompletado en el editor | haiku | M | 1.6 | F8.07c, F8.07d | bloqueada |
| [F8.08-lsp-renombrar](../tareas/F8.08-lsp-renombrar.md) | Renombrar símbolo y acciones de código (LSP) | haiku | S | 1.6 | F8.06, F8.01, F8.08a, F8.08b, F8.08c, F8.08d, F8.08e | bloqueada |
| [F8.08a-lsp-workspace-edit](../tareas/F8.08a-lsp-workspace-edit.md) | LSP: leer un `WorkspaceEdit` y rechazar lo que cae fuera del proyecto (lógica pura) | haiku | S | 1.6 | F8.06a | bloqueada |
| [F8.08b-lsp-renombrar-cliente](../tareas/F8.08b-lsp-renombrar-cliente.md) | LSP: `LspClient::rename` y `LspClient::code_actions` | haiku | M | 1.6 | F8.08a | bloqueada |
| [F8.08c-lsp-aplicar-ediciones](../tareas/F8.08c-lsp-aplicar-ediciones.md) | Demonio: aplicar ediciones del LSP a archivos del proyecto, con simulacro | haiku | S | 1.6 | F8.08a | bloqueada |
| [F8.08d-lsp-renombrar-api](../tareas/F8.08d-lsp-renombrar-api.md) | API: métodos `lsp.rename`, `lsp.codeActions` y `lsp.applyEdits` | haiku | M | 1.6 | F8.08b, F8.08c | bloqueada |
| [F8.08e-lsp-renombrar-studio](../tareas/F8.08e-lsp-renombrar-studio.md) | Studio: renombrar con F2 (con resumen previo) y acciones de código con Ctrl+. | haiku | M | 1.6 | F8.08d, F8.01 | bloqueada |
| [F8.09-lsp-tool-agente](../tareas/F8.09-lsp-tool-agente.md) | El agente consulta el LSP: tools `diagnosticos`, `definicion` y `referencias` | haiku | S | 1.3 | F5.35, F8.09a, F8.09b, F8.09c | bloqueada |
| [F8.09a-lsp-tools-formato](../tareas/F8.09a-lsp-tools-formato.md) | `lsp_tools.rs`: posición de un símbolo en una línea, textos de resultados y almacén de diagnósticos | haiku | M | 1.3 | F5.35 | hecha |
| [F8.09b-lsp-tools-agente](../tareas/F8.09b-lsp-tools-agente.md) | Tools `diagnosticos`, `definicion` y `referencias` sobre un `LspManager` compartido | haiku | M | 1.3 | F8.09a | hecha |
| [F8.09c-lsp-tools-chat](../tareas/F8.09c-lsp-tools-chat.md) | Registrar las tools del LSP en el agente del chat y de la TUI | haiku | S | 1.3 | F8.09b | lista |
| [F8.10-git-blame-historial](../tareas/F8.10-git-blame-historial.md) | Git: blame en línea e historial de un archivo | haiku | S | 1.6 | F5.32, F8.01, F8.10a, F8.10b, F8.10c, F8.10d, F8.10e | bloqueada |
| [F8.10a-git-blame-nucleo](../tareas/F8.10a-git-blame-nucleo.md) | Git: parsear `git blame --porcelain` (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.10b-git-historial-nucleo](../tareas/F8.10b-git-historial-nucleo.md) | Git: `git_blame`, `git_log` y `git_show` de un archivo en el demonio | haiku | M | 1.6 | F8.10a | bloqueada |
| [F8.10c-git-historial-api](../tareas/F8.10c-git-historial-api.md) | API: métodos `git.blame`, `git.log` y `git.show` | haiku | S | 1.6 | F8.10b | bloqueada |
| [F8.10d-git-historial-js](../tareas/F8.10d-git-historial-js.md) | Studio: módulo `historial.js` (texto del blame y fechas relativas, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.10e-git-historial-studio](../tareas/F8.10e-git-historial-studio.md) | Studio: anotación de blame en la línea actual y panel «Historial» | haiku | M | 1.6 | F8.10c, F8.10d | bloqueada |
| [F8.11-git-ramas](../tareas/F8.11-git-ramas.md) | Git: ramas, pull y push con aprobación | haiku | S | 1.6 | F8.10, F8.01, F8.11a, F8.11b, F8.11c, F8.11d, F8.11e, F8.11f | bloqueada |
| [F8.11a-git-ramas-nucleo](../tareas/F8.11a-git-ramas-nucleo.md) | Git: parsear la lista de ramas y validar nombres (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.11b-git-ramas-locales](../tareas/F8.11b-git-ramas-locales.md) | Git: listar ramas, crear y cambiar de rama en el demonio | haiku | S | 1.6 | F8.11a | bloqueada |
| [F8.11c-git-ramas-remoto](../tareas/F8.11c-git-ramas-remoto.md) | Git: pull y push con tiempo máximo y sin pedir credenciales | haiku | M | 1.6 | F8.11b | bloqueada |
| [F8.11d-git-ramas-api](../tareas/F8.11d-git-ramas-api.md) | API: `git.branches`, `git.checkout`, `git.pull` y `git.push` (los dos últimos piden aprobación) | haiku | M | 1.6 | F8.11c | bloqueada |
| [F8.11e-git-ramas-js](../tareas/F8.11e-git-ramas-js.md) | Studio: módulo `ramas.js` (opciones del selector de ramas, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.11f-git-ramas-studio](../tareas/F8.11f-git-ramas-studio.md) | Studio: selector de ramas, rama nueva, pull y push en el panel de git | haiku | M | 1.6 | F8.11d, F8.11e | bloqueada |
| [F8.12-git-conflictos](../tareas/F8.12-git-conflictos.md) | Git: resolver conflictos de merge en tres paneles | haiku | S | 1.6 | F8.11, F8.01, F8.12a, F8.12b, F8.12c, F8.12d | bloqueada |
| [F8.12a-git-conflictos-nucleo](../tareas/F8.12a-git-conflictos-nucleo.md) | Git: parsear y resolver bloques de conflicto (lógica pura) | haiku | M | 1.6 | F5.32 | lista |
| [F8.12b-git-conflictos-api](../tareas/F8.12b-git-conflictos-api.md) | API: `git.conflict` (bloques de un archivo) y `git.resolveConflict` (escribe y hace `git add`) | haiku | S | 1.6 | F8.12a | bloqueada |
| [F8.12c-git-conflictos-js](../tareas/F8.12c-git-conflictos-js.md) | Studio: módulo `conflictos.js` (elecciones y vista previa, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.12d-git-conflictos-studio](../tareas/F8.12d-git-conflictos-studio.md) | Studio: vista de conflictos en tres columnas (nuestro, suyo, resultado) | haiku | M | 1.6 | F8.12b, F8.12c | bloqueada |
| [F8.13-depurador-dap](../tareas/F8.13-depurador-dap.md) | Depurador (DAP): puntos de ruptura, pila y variables | haiku | S | 1.6 | F8.01, F9.21, F8.13a, F8.13b, F8.13c | bloqueada |
| [F8.13a-depurador-js](../tareas/F8.13a-depurador-js.md) | Studio: módulo `depurador.js` (puntos de ruptura y estado de la sesión, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.13b-depurador-rupturas-studio](../tareas/F8.13b-depurador-rupturas-studio.md) | Studio: puntos de ruptura en el margen, iniciar (F5) y detener la depuración | haiku | M | 1.6 | F8.13a, F9.21 | bloqueada |
| [F8.13c-depurador-panel-studio](../tareas/F8.13c-depurador-panel-studio.md) | Studio: panel «Depurar» con controles, pila, variables y salida | haiku | M | 1.6 | F8.13b | bloqueada |
| [F8.14-explorador-tests](../tareas/F8.14-explorador-tests.md) | Explorador de tests: descubrir, ejecutar y marcar ✓ ✗ por test | haiku | S | 1.6 | F5.32, F8.01, F8.14a, F8.14b, F8.14c, F8.14d, F8.14e, F8.14f | bloqueada |
| [F8.14a-tests-descubrir-nucleo](../tareas/F8.14a-tests-descubrir-nucleo.md) | Tests: parsear las listas de cargo, pytest y go (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.14b-tests-resultados-nucleo](../tareas/F8.14b-tests-resultados-nucleo.md) | Tests: parsear los resultados de cargo, pytest, go y TAP (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.14c-tests-ejecutores-nucleo](../tareas/F8.14c-tests-ejecutores-nucleo.md) | Tests: detectar ejecutores del proyecto y construir las órdenes | haiku | S | 1.6 | F5.32 | lista |
| [F8.14d-tests-api](../tareas/F8.14d-tests-api.md) | API: `tests.runners`, `tests.command` y `tests.parse` | haiku | S | 1.6 | F8.14a, F8.14b, F8.14c | bloqueada |
| [F8.14e-tests-js](../tareas/F8.14e-tests-js.md) | Studio: módulo `pruebas.js` (agrupar tests, aplicar resultados y contar, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.14f-tests-studio](../tareas/F8.14f-tests-studio.md) | Studio: panel «Tests» que descubre, ejecuta y marca ✓ ✗ | haiku | M | 1.6 | F8.14d, F8.14e | bloqueada |
| [F8.15-checkpoints](../tareas/F8.15-checkpoints.md) | Puntos de control y deshacer del agente (`/deshacer`, `/rehacer`) | haiku | — | — | F5.34 | hecha |
| [F8.16-ordenes-propias](../tareas/F8.16-ordenes-propias.md) | Órdenes propias del proyecto: `.pcode/ordenes/*.md` como `/nombre` | haiku | S | 1.3 | F5.34, F8.16a, F8.16b, F8.16c | bloqueada |
| [F8.16a-ordenes-propias-nucleo](../tareas/F8.16a-ordenes-propias-nucleo.md) | `pcode-skills`: leer órdenes propias (`*.md` con frontmatter y `$ARGUMENTOS`) de varias carpetas | haiku | S | 1.3 | F5.34 | lista |
| [F8.16b-ordenes-propias-chat](../tareas/F8.16b-ordenes-propias-chat.md) | Chat de línea: `/nombre args` ejecuta una orden propia y `/ayuda` las lista | haiku | S | 1.3 | F8.16a | bloqueada |
| [F8.16c-ordenes-propias-tui](../tareas/F8.16c-ordenes-propias-tui.md) | TUI: las órdenes propias aparecen al escribir `/` y se ejecutan como un mensaje | haiku | M | 1.3 | F8.16a, F9.29b | bloqueada |
| [F8.17-agentes-propios](../tareas/F8.17-agentes-propios.md) | Agentes propios y subagentes: `.pcode/agentes/*.md` y la tool `delegar` | haiku | S | 1.3 | F8.16, F2.22, F8.17a, F8.17b, F8.17c | bloqueada |
| [F8.17a-agentes-propios-definiciones](../tareas/F8.17a-agentes-propios-definiciones.md) | `pcode-skills`: leer agentes propios (`*.md` con prompt, modelo, tools y solo lectura) | haiku | S | 1.3 | F8.16a | bloqueada |
| [F8.17b-subagente-delegar](../tareas/F8.17b-subagente-delegar.md) | `pcode-core`: tool `delegar` que lanza un subagente con capacidades atenuadas y devuelve su informe | haiku | M | 1.3 | F8.17a | bloqueada |
| [F8.17c-agentes-propios-chat](../tareas/F8.17c-agentes-propios-chat.md) | Chat y TUI: registrar `delegar` con los agentes de `.pcode/agentes` y `@agente tarea` | haiku | S | 1.3 | F8.17b, F8.16b | bloqueada |
| [F8.18-coste](../tareas/F8.18-coste.md) | Coste por sesión con precios por modelo y presupuesto que avisa al 80 % y para al 100 % | haiku | S | 1.3 | F1.26, F8.18a, F8.18b, F8.18c, F8.18d | bloqueada |
| [F8.18a-coste-calculo](../tareas/F8.18a-coste-calculo.md) | `pcode-models`: precio por millón de tokens con caché y estado del presupuesto (lógica pura) | haiku | S | 1.3 | F1.26 | lista |
| [F8.18b-coste-config](../tareas/F8.18b-coste-config.md) | Configuración: `[precios.<modelo>]` y `[preferencias] presupuesto` (dólares por sesión) | haiku | S | 1.3 | F8.18a | bloqueada |
| [F8.18c-coste-corte-agente](../tareas/F8.18c-coste-corte-agente.md) | Agente: `Budget` con precio y tope en dólares; para con `BudgetExhausted` al superarlo | haiku | S | 1.3 | F8.18a | bloqueada |
| [F8.18d-coste-chat](../tareas/F8.18d-coste-chat.md) | Chat: coste acumulado de la sesión, `/coste`, aviso al 80 % y corte al 100 % del presupuesto | haiku | M | 1.3 | F8.18b, F8.18c | bloqueada |
| [F8.19-memoria-proyecto](../tareas/F8.19-memoria-proyecto.md) | Memoria del proyecto: `/recuerda` y `.pcode/memoria.md` en el prompt | haiku | — | — | F5.34 | hecha |
| [F8.20-web-fetch](../tareas/F8.20-web-fetch.md) | Tool `web`: leer una URL como texto, con dominios permitidos o aprobación | haiku | S | 1.3 | F1.13, F8.20a, F8.20b | bloqueada |
| [F8.20a-web-tool](../tareas/F8.20a-web-tool.md) | `pcode-tools`: tool `web` (GET con tiempo y tamaño máximos, HTML a texto sin scripts ni estilos) | haiku | M | 1.3 | F1.13 | lista |
| [F8.20b-web-politica](../tareas/F8.20b-web-politica.md) | Agente: la tool `web` pide aprobación fuera de `red_permitir`; el chat la registra | haiku | S | 1.3 | F8.20a | bloqueada |
| [F8.21-imagenes](../tareas/F8.21-imagenes.md) | Imágenes en el chat para modelos con visión (`@captura.png`) | haiku | S | 1.3 | F1.25, F8.21a, F8.21b, F8.21c | bloqueada |
| [F8.21a-imagenes-tipo](../tareas/F8.21a-imagenes-tipo.md) | `Message` con imágenes (base64 propio, sin dependencias) y su envío a Anthropic | haiku | M | 1.3 | F1.25 | lista |
| [F8.21b-imagenes-gemini-openai](../tareas/F8.21b-imagenes-gemini-openai.md) | Imágenes en Gemini (`inline_data`) y en OpenAI-compatible (`image_url` con data URL) | haiku | S | 1.3 | F8.21a | bloqueada |
| [F8.21c-imagenes-adjuntar](../tareas/F8.21c-imagenes-adjuntar.md) | Agente con imágenes y `@captura.png` en la TUI y el chat | haiku | M | 1.3 | F8.21a | bloqueada |
| [F8.22-modelos-locales](../tareas/F8.22-modelos-locales.md) | Modelos locales: sin clave, usar Ollama o LM Studio si están en marcha | haiku | S | 1.7 | F1.26 | lista |
| [F8.23-exportar-sesion](../tareas/F8.23-exportar-sesion.md) | Exportar y compartir una sesión (Markdown y HTML autocontenido) | haiku | S | 1.3 | F5.29, F8.23a, F8.23b, F8.23c | bloqueada |
| [F8.23a-exportar-markdown](../tareas/F8.23a-exportar-markdown.md) | Exportar: conversación a Markdown y enmascarado de claves y rutas (lógica pura) | haiku | M | 1.3 | F5.29, F5.06 | bloqueada |
| [F8.23b-exportar-html](../tareas/F8.23b-exportar-html.md) | Exportar: conversación a HTML autocontenido (lógica pura) | haiku | S | 1.3 | F8.23a | bloqueada |
| [F8.23c-exportar-orden](../tareas/F8.23c-exportar-orden.md) | CLI: `pcode exportar [sesión] --formato md|html [--salida archivo]` | haiku | M | 1.3 | F8.23b | bloqueada |
| [F8.24-tui-selectores](../tareas/F8.24-tui-selectores.md) | TUI: selectores de sesiones y de modelos | haiku | S | 1.3 | F5.34, F5.10, F5.11, F8.24a, F8.24b, F8.24c | bloqueada |
| [F8.24a-tui-selector-nucleo](../tareas/F8.24a-tui-selector-nucleo.md) | TUI: lista seleccionable con búsqueda difusa (lógica pura) | haiku | S | 1.3 | F5.34 | lista |
| [F8.24b-tui-selector-ordenes](../tareas/F8.24b-tui-selector-ordenes.md) | TUI: `/sesiones` y `/modelos` abren un selector (y Ctrl+X l / Ctrl+X m) | haiku | M | 1.3 | F8.24a, F5.10b, F5.11c | bloqueada |
| [F8.24c-tui-selector-datos](../tareas/F8.24c-tui-selector-datos.md) | TUI: lista real de modelos y retomar una conversación guardada | haiku | M | 1.3 | F8.24b | bloqueada |
| [F8.25-tui-diff](../tareas/F8.25-tui-diff.md) | TUI: revisar el diff del turno y aceptar o revertir por archivo | haiku | S | 1.3 | F8.15, F8.25a, F8.25b, F8.25c | bloqueada |
| [F8.25a-checkpoint-diff-archivo](../tareas/F8.25a-checkpoint-diff-archivo.md) | Puntos de control: diff de un archivo de un turno y revertir solo ese archivo | haiku | S | 1.3 | F8.15 | lista |
| [F8.25b-tui-diff-vista](../tareas/F8.25b-tui-diff-vista.md) | TUI: vista de revisión de cambios (lista de archivos y diff), lógica pura | haiku | M | 1.3 | F8.25a, F8.24b | bloqueada |
| [F8.25c-tui-diff-cableado](../tareas/F8.25c-tui-diff-cableado.md) | TUI: `/cambios` abre la revisión del último turno y `r` revierte un archivo | haiku | M | 1.3 | F8.25b, F8.24c | bloqueada |
| [F8.26-tui-raton](../tareas/F8.26-tui-raton.md) | TUI: ratón opcional, enlaces y copiar al portapapeles | haiku | S | 1.3 | F5.30, F8.26a, F8.26b, F8.26c, F8.26d | bloqueada |
| [F8.26a-tui-osc52](../tareas/F8.26a-tui-osc52.md) | TUI: secuencia OSC 52 para copiar al portapapeles (lógica pura) | haiku | S | 1.3 | F5.30 | lista |
| [F8.26b-tui-copiar](../tareas/F8.26b-tui-copiar.md) | TUI: `/copiar` copia la última respuesta al portapapeles | haiku | S | 1.3 | F8.26a, F8.25c | bloqueada |
| [F8.26c-tui-raton-rueda](../tareas/F8.26c-tui-raton-rueda.md) | TUI: `[preferencias] raton = true` activa el ratón y la rueda desplaza la conversación | haiku | M | 1.3 | F8.26b | bloqueada |
| [F8.26d-tui-raton-clic](../tareas/F8.26d-tui-raton-clic.md) | TUI: clic en una tool despliega o pliega su salida | haiku | M | 1.3 | F8.26c | bloqueada |
| [F8.27-instaladores-nativos](../tareas/F8.27-instaladores-nativos.md) | Instaladores nativos: winget, .msi, .deb y .rpm (cierre) | haiku | S | 1.8 | F7.03, F8.27a, F8.27b, F8.27c, F8.27d | bloqueada |
| [F8.27a-paquetes-deb-rpm](../tareas/F8.27a-paquetes-deb-rpm.md) | Linux: generador del paquete .deb y de la especificación .rpm a partir de los binarios compilados | haiku | M | 1.8 | F7.03 | lista |
| [F8.27b-manifiesto-winget](../tareas/F8.27b-manifiesto-winget.md) | Windows: generador de los manifiestos de winget (PublicCode.pcode) a partir de la versión y la suma SHA-256 | haiku | S | 1.8 | F7.03 | lista |
| [F8.27c-fuente-msi](../tareas/F8.27c-fuente-msi.md) | Windows: generador de la fuente WiX (.wxs) del instalador .msi | haiku | S | 1.8 | F7.03 | lista |
| [F8.27d-instaladores-en-la-release](../tareas/F8.27d-instaladores-en-la-release.md) | Release: construir .deb, .rpm y .msi, probar su instalación en la CI y publicar en winget | coordinador | M | 1.8 | F8.27a, F8.27b, F8.27c | bloqueada |
| [F8.28-arm64](../tareas/F8.28-arm64.md) | Binarios para Linux aarch64 y Windows ARM64 (cierre) | haiku | S | 1.8 | F7.03, F8.28a, F8.28b | bloqueada |
| [F8.28a-arm64-eleccion-de-archivo](../tareas/F8.28a-arm64-eleccion-de-archivo.md) | ARM64: que `pcode actualizar`, install.sh e install.ps1 elijan el archivo de la arquitectura | haiku | S | 1.8 | F7.03 | lista |
| [F8.28b-arm64-release](../tareas/F8.28b-arm64-release.md) | Release: compilar y publicar los binarios de Linux aarch64 y Windows ARM64 | coordinador | S | 1.8 | F8.28a | bloqueada |
| [F8.29-notificaciones-apk](../tareas/F8.29-notificaciones-apk.md) | APK: notificaciones cuando el agente pide aprobación o termina (cierre) | haiku | S | 1.8 | F5.33, F8.29a, F8.29b | bloqueada |
| [F8.29a-apk-eventos-nucleo](../tareas/F8.29a-apk-eventos-nucleo.md) | APK: leer los eventos SSE del demonio y decidir qué se notifica (Java puro, probado en la JVM) | haiku | M | 1.8 | F5.33 | lista |
| [F8.29b-apk-servicio-de-avisos](../tareas/F8.29b-apk-servicio-de-avisos.md) | APK: servicio en primer plano que escucha /eventos y muestra notificaciones con Permitir y Denegar | coordinador | M | 1.8 | F8.29a | bloqueada |
| [F8.30-tls-qr](../tareas/F8.30-tls-qr.md) | Acceso remoto seguro: TLS autofirmado y emparejamiento por QR (cierre) | haiku | S | 1.8 | F8.29a, F8.30a, F8.30b, F8.30c, F8.30d | bloqueada |
| [F8.30a-emparejamiento-nucleo](../tareas/F8.30a-emparejamiento-nucleo.md) | Emparejamiento: enlace `pcode://emparejar`, huella SHA-256 de un certificado y orden del QR (lógica pura) | haiku | M | 1.8 | F5.33 | lista |
| [F8.30b-emparejar-orden](../tareas/F8.30b-emparejar-orden.md) | `pcode emparejar <url>`: enseña el enlace de emparejamiento y su código QR en la terminal | haiku | S | 1.8 | F8.30a | bloqueada |
| [F8.30c-apk-emparejamiento](../tareas/F8.30c-apk-emparejamiento.md) | APK: aceptar el enlace `pcode://emparejar` al conectar (Java puro) | haiku | S | 1.8 | F8.29a, F8.30a | bloqueada |
| [F8.30d-tls-autofirmado](../tareas/F8.30d-tls-autofirmado.md) | Demonio con TLS autofirmado, huella fijada en la app y lectura del QR con la cámara | coordinador | M | 1.8 | F8.30b, F8.30c | bloqueada |
| [F8.31-multiraiz](../tareas/F8.31-multiraiz.md) | Studio con varios proyectos: abrir otra carpeta y espacios de trabajo | haiku | S | 1.6 | F5.32, F8.31a, F8.31b, F8.31c, F8.31d, F8.31e | bloqueada |
| [F8.31a-multiraiz-nucleo](../tareas/F8.31a-multiraiz-nucleo.md) | Varias carpetas: lista de raíces con nombre (`roots.rs`, lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.31b-multiraiz-api](../tareas/F8.31b-multiraiz-api.md) | Varias carpetas: parámetro `root` en `fs.*`, `search.*` y `git.*`, y método `workspace.roots` | haiku | M | 1.6 | F8.31a | bloqueada |
| [F8.31c-multiraiz-cli](../tareas/F8.31c-multiraiz-cli.md) | Varias carpetas: `pcode studio [CARPETAS]…` añade raíces extra | haiku | S | 1.6 | F8.31b | bloqueada |
| [F8.31d-multiraiz-js](../tareas/F8.31d-multiraiz-js.md) | Varias carpetas: parámetro `root` de las peticiones e ids de pestaña por raíz (módulo puro `raices.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.31e-multiraiz-cableado](../tareas/F8.31e-multiraiz-cableado.md) | Varias carpetas: selector de raíz en el explorador y pestañas que recuerdan su raíz | haiku | M | 1.6 | F8.31b, F8.31d | bloqueada |
| [F8.32-extensiones](../tareas/F8.32-extensiones.md) | Extensiones: plugins con manifiesto (MCP local o WASM) y capacidades declaradas (cierre) | haiku | S | 1.8 | F2.22, F8.32a, F8.32b, F8.32c, F8.32d | bloqueada |
| [F8.32a-extension-manifiesto](../tareas/F8.32a-extension-manifiesto.md) | Extensiones: leer y validar `pcode-extension.toml` (servidores MCP, órdenes, skills y capacidades) | haiku | M | 1.8 | F2.22 | lista |
| [F8.32b-extension-instalar](../tareas/F8.32b-extension-instalar.md) | Extensiones: instalar solo con las capacidades aprobadas, listar y sacar sus servidores MCP | haiku | M | 1.8 | F8.32a | bloqueada |
| [F8.32c-extension-cli](../tareas/F8.32c-extension-cli.md) | `pcode extensiones instalar <carpeta>`: enseñar las capacidades y pedir aprobación | haiku | M | 1.8 | F8.32b, F9.19 | bloqueada |
| [F8.32d-extension-mcp-en-el-agente](../tareas/F8.32d-extension-mcp-en-el-agente.md) | Agente: conectar los servidores MCP de las extensiones aprobadas (siempre con aprobación por llamada) | haiku | S | 1.8 | F8.32b | bloqueada |
| [F8.33-github-pr](../tareas/F8.33-github-pr.md) | GitHub: crear un PR con descripción generada y revisar PRs con el agente | coordinador | M | 1.8 | F5.26, F8.33a, F8.33b, F8.33c | bloqueada |
| [F8.33a-pr-textos](../tareas/F8.33a-pr-textos.md) | PR: prompts y lectura de la descripción y de los comentarios de revisión (lógica pura) | haiku | M | 1.8 | F5.26 | bloqueada |
| [F8.33b-pr-crear-cli](../tareas/F8.33b-pr-crear-cli.md) | CLI: `pcode pr crear` genera título y descripción del PR (y lo publica solo con `--publicar --si`) | haiku | M | 1.8 | F8.33a | bloqueada |
| [F8.33c-pr-revisar-cli](../tareas/F8.33c-pr-revisar-cli.md) | CLI: `pcode pr revisar <n>` deja los comentarios en borrador y solo los publica con `--publicar --si` | haiku | M | 1.8 | F8.33b | bloqueada |
| [F8.34-rendimiento](../tareas/F8.34-rendimiento.md) | Rendimiento medido: arranque, memoria y latencia, con presupuesto en la CI | haiku | S | 1.2 | F0.07 | lista |
| [F8.35-accesibilidad](../tareas/F8.35-accesibilidad.md) | Accesibilidad de Studio: lector de pantalla, contraste AA y solo teclado | haiku | S | 1.6 | F8.01, F8.35a, F8.35b, F8.35c, F8.35d | bloqueada |
| [F8.35a-contraste-aa](../tareas/F8.35a-contraste-aa.md) | Accesibilidad: contraste AA de los tres temas comprobado con un test (`contraste.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.35b-foco-nucleo](../tareas/F8.35b-foco-nucleo.md) | Accesibilidad: zonas de F6 y movimiento con flechas en árbol y pestañas (módulo puro `foco.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.35c-foco-zonas-pestanas](../tareas/F8.35c-foco-zonas-pestanas.md) | Accesibilidad: F6 recorre las zonas y las pestañas se manejan con flechas | haiku | S | 1.6 | F8.35b | bloqueada |
| [F8.35d-arbol-teclado](../tareas/F8.35d-arbol-teclado.md) | Accesibilidad: el explorador con flechas, `aria-expanded` y `aria-level` | haiku | S | 1.6 | F8.35c | bloqueada |
| [F8.36-idiomas](../tareas/F8.36-idiomas.md) | Idiomas: inglés completo en Studio y la CLI, y catálogo para más | haiku | S | 1.6 | F8.01, F8.36a, F8.36b, F8.36c, F8.36d, F8.36e | bloqueada |
| [F8.36a-i18n-nucleo](../tareas/F8.36a-i18n-nucleo.md) | Idiomas: traducción por clave en español, elección de idioma y catálogo inglés (`i18n.js`, `catalogos.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.36b-i18n-cableado](../tareas/F8.36b-i18n-cableado.md) | Idiomas: `tr()`, elección de idioma y textos fijos de la página de Studio traducibles | haiku | M | 1.6 | F8.36a | bloqueada |
| [F8.36c-i18n-textos-js](../tareas/F8.36c-i18n-textos-js.md) | Idiomas: paleta de órdenes, temas, estados y avisos de Studio traducibles | haiku | M | 1.6 | F8.36b | bloqueada |
| [F8.36d-i18n-cli](../tareas/F8.36d-i18n-cli.md) | Idiomas: catálogo es/en de la CLI y ayuda del chat en inglés | haiku | S | 1.6 | F5.29 | lista |
| [F8.36e-i18n-cli-ayuda](../tareas/F8.36e-i18n-cli-ayuda.md) | Idiomas: `pcode --help` en inglés cuando el idioma del sistema es inglés | haiku | M | 1.6 | F8.36d | bloqueada |
| [F8.37-commit-generado](../tareas/F8.37-commit-generado.md) | Mensaje de commit generado a partir del diff jerárquico (HierDiff) | coordinador | — | — | F8.15 | hecha |
| [F8.38-arreglar-diagnostico](../tareas/F8.38-arreglar-diagnostico.md) | «✦ Arreglar» con diagnóstico estructurado desde Problemas, hover y terminal (SHERLOC, LintEval) | coordinador | — | — | F8.37 | hecha |
| [F8.39-esquema-simbolos](../tareas/F8.39-esquema-simbolos.md) | Esquema del archivo, migas de pan e «ir a símbolo» del proyecto (RepoNav) | coordinador | — | — | — | hecha |
| [F8.40-concretar-peticion](../tareas/F8.40-concretar-peticion.md) | «✦ Concretar»: enunciado accionable y preguntas antes de mandar al agente (CodeScout, Ask or Assume) | coordinador | — | — | F8.37, F8.39 | hecha |
| [F8.41-edicion-minima](../tareas/F8.41-edicion-minima.md) | Ediciones mínimas: instrucción de preservación y tamaño del cambio por turno | coordinador | — | — | F8.15 | hecha |
| [F8.42-acciones-rapidas](../tareas/F8.42-acciones-rapidas.md) | Acciones rápidas: revisar cambios, tests, explicar y estilo (OmniCode, taxonomía de preguntas) | coordinador | — | — | F8.37 | hecha |
| [F8.43-actividad](../tareas/F8.43-actividad.md) | Actividad: línea de tiempo de las acciones del agente enlazada con los turnos (RECAP) | coordinador | — | — | F8.15 | hecha |
| [F8.44-preguntar-repo](../tareas/F8.44-preguntar-repo.md) | `pcode preguntar` con contexto del repositorio (SWE-QA) | coordinador | — | — | F8.37, F8.39 | hecha |
| [F8.45-esquema-agente](../tareas/F8.45-esquema-agente.md) | Tool `esquema` para el agente: localizar sin leer archivos enteros (RepoNav, SHERLOC) | coordinador | — | — | F8.39 | hecha |
| [F8.46-verificar-antes-de-terminar](../tareas/F8.46-verificar-antes-de-terminar.md) | Verificar antes de terminar: recordatorio si el turno cambió archivos sin comprobar nada (Polished but Unresolved) | coordinador | — | — | F8.15 | hecha |
| [F8.47-coste-esquemas-mcp](../tareas/F8.47-coste-esquemas-mcp.md) | Coste de los esquemas de tools MCP en `pcode mcp` y en el arranque del chat (ProMCP) | coordinador | — | — | — | hecha |
| [F8.48-agentes-con-tu-cuenta](../tareas/F8.48-agentes-con-tu-cuenta.md) | GitHub Copilot (y Claude Code, Gemini CLI, opencode) con la cuenta del usuario: Ajustes, conversación en Studio y `--aplicar` | coordinador | — | — | F8.15 | hecha |
| [F8.49-desarrollo-con-especificaciones](../tareas/F8.49-desarrollo-con-especificaciones.md) | Desarrollo guiado por especificaciones, opcional: pcode spec, panel de Studio y ajuste desarrollo = "specs" | coordinador | — | — | F8.48 | hecha |
| [F8.50-studio-token-un-uso](../tareas/F8.50-studio-token-un-uso.md) | Studio: el enlace `?t=` se canjea una sola vez por una cookie de sesión propia | haiku | S | 1.5 | F5.32, F8.50a, F8.50b | bloqueada |
| [F8.50a-sesion-web-nucleo](../tareas/F8.50a-sesion-web-nucleo.md) | Enlace de un solo uso: canje del token y cookies de sesión (`web_session.rs`, lógica pura) | haiku | S | 1.5 | F5.32 | lista |
| [F8.50b-sesion-web-servidor](../tareas/F8.50b-sesion-web-servidor.md) | Enlace de un solo uso: `index` canjea el token, `authorized` acepta la cookie de sesión y `pcode studio` lo activa | haiku | M | 1.5 | F8.50a | bloqueada |
| [F8.51-borradores](../tareas/F8.51-borradores.md) | Studio: recuperación de archivos sin guardar y guardado automático | haiku | S | 1.6 | F8.01, F8.51a, F8.51b | bloqueada |
| [F8.51a-borradores-nucleo](../tareas/F8.51a-borradores-nucleo.md) | Borradores: clave, estado frente al disco y limpieza (módulo puro `borradores.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.51b-borradores-cableado](../tareas/F8.51b-borradores-cableado.md) | Borradores: guardar lo tecleado en `localStorage`, ofrecer recuperarlo al abrir y guardado automático | haiku | M | 1.6 | F8.51a | bloqueada |
| [F8.52-pestanas-mover-fijar](../tareas/F8.52-pestanas-mover-fijar.md) | Studio: arrastrar para reordenar pestañas, fijar una pestaña y cerrar las demás | haiku | S | 1.6 | F8.01, F8.52a, F8.52b | bloqueada |
| [F8.52a-pestanas-nucleo](../tareas/F8.52a-pestanas-nucleo.md) | Pestañas: mover, fijar y cerrar varias (módulo puro `pestanas.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.52b-pestanas-cableado](../tareas/F8.52b-pestanas-cableado.md) | Pestañas: arrastrar y soltar, menú contextual (fijar, cerrar las demás) y atajos para moverlas | haiku | S | 1.6 | F8.52a | bloqueada |
| [F8.53-ajustes-editor](../tareas/F8.53-ajustes-editor.md) | Studio: ajustes del editor (tamaño de letra, espacios o tabuladores) y `.editorconfig` | haiku | S | 1.6 | F8.01, F8.53a, F8.53b, F8.53c | bloqueada |
| [F8.53a-ajustes-editor-nucleo](../tareas/F8.53a-ajustes-editor-nucleo.md) | Ajustes del editor: valores válidos, unidad de sangría y limpieza al guardar (módulo puro `ajustes_editor.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.53b-editorconfig-nucleo](../tareas/F8.53b-editorconfig-nucleo.md) | Ajustes del editor: leer `.editorconfig` y aplicarlo a una ruta (módulo puro `editorconfig.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.53c-ajustes-editor-cableado](../tareas/F8.53c-ajustes-editor-cableado.md) | Ajustes del editor: tamaño de letra, sangría con espacios o tabuladores y `.editorconfig` en Studio | haiku | M | 1.6 | F8.53a, F8.53b | bloqueada |
| [F8.54-reemplazar-proyecto](../tareas/F8.54-reemplazar-proyecto.md) | Studio: buscar y reemplazar en todo el proyecto con vista previa | haiku | S | 1.6 | F8.01, F8.54a, F8.54b, F8.54c | bloqueada |
| [F8.54a-reemplazar-nucleo](../tareas/F8.54a-reemplazar-nucleo.md) | Reemplazar en el proyecto: reemplazo línea a línea con vista previa (`replace.rs`, lógica pura) | haiku | S | 1.6 | F5.35 | lista |
| [F8.54b-reemplazar-api](../tareas/F8.54b-reemplazar-api.md) | Reemplazar en el proyecto: recorrer los archivos y método `search.replace` (vista previa o aplicar) | haiku | M | 1.6 | F8.54a | bloqueada |
| [F8.54c-reemplazar-cableado](../tareas/F8.54c-reemplazar-cableado.md) | Reemplazar en el proyecto: campo «Reemplazar por», vista previa y «Reemplazar todo» en el panel Buscar | haiku | M | 1.6 | F8.01, F8.54b | bloqueada |
| [F8.55-parejas](../tareas/F8.55-parejas.md) | Editor: emparejado de paréntesis, auto-cierre y resaltado de la pareja | haiku | S | 1.6 | F8.01, F8.55a, F8.55b | bloqueada |
| [F8.55a-parejas-nucleo](../tareas/F8.55a-parejas-nucleo.md) | Paréntesis: pareja de un paréntesis, auto-cierre y borrado en pareja (módulo puro `parejas.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.55b-parejas-cableado](../tareas/F8.55b-parejas-cableado.md) | Paréntesis: auto-cierre al teclear y resaltado de la pareja del cursor | haiku | S | 1.6 | F8.55a | bloqueada |
| [F8.56-acciones-linea](../tareas/F8.56-acciones-linea.md) | Editor: mover y duplicar líneas y comentar o descomentar (Alt+↑/↓, Mayús+Alt+↓, Ctrl+/) | haiku | S | 1.6 | F8.01, F8.56a, F8.56b | bloqueada |
| [F8.56a-lineas-nucleo](../tareas/F8.56a-lineas-nucleo.md) | Acciones de línea: mover, duplicar y comentar (módulo puro `lineas.js`) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.56b-lineas-cableado](../tareas/F8.56b-lineas-cableado.md) | Acciones de línea: Alt+↑/↓ mueve, Mayús+Alt+↓ duplica y Ctrl+/ comenta en el editor de Studio | haiku | S | 1.6 | F8.56a | bloqueada |
| [F8.57-atajos-configurables](../tareas/F8.57-atajos-configurables.md) | Studio: atajos de teclado configurables por el usuario | haiku | S | 1.6 | F8.01, F8.57a, F8.57b | bloqueada |
| [F8.57a-atajos-nucleo](../tareas/F8.57a-atajos-nucleo.md) | Atajos: nombre canónico de una combinación, resolución y conflictos (módulo puro `atajos.js`) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.57b-atajos-cableado](../tareas/F8.57b-atajos-cableado.md) | Atajos: asignar una combinación a cualquier orden de la paleta y aplicarla en Studio | haiku | M | 1.6 | F8.57a | bloqueada |
| [F8.58-modo-zen](../tareas/F8.58-modo-zen.md) | Studio: modo zen (solo el editor) y pantalla completa | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.61-git-descartar-stash](../tareas/F8.61-git-descartar-stash.md) | Git: descartar cambios por archivo o por trozo y guardar cambios aparte (stash) | haiku | S | 1.6 | F5.32, F8.01, F8.61a, F8.61b, F8.61c, F8.61d, F8.61e | bloqueada |
| [F8.61a-git-trozos-nucleo](../tareas/F8.61a-git-trozos-nucleo.md) | Git: partir un diff en trozos y construir el parche de uno (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.61b-git-descartar-nucleo](../tareas/F8.61b-git-descartar-nucleo.md) | Git: descartar los cambios de un archivo o de un trozo en el demonio | haiku | M | 1.6 | F8.61a | bloqueada |
| [F8.61c-git-stash-nucleo](../tareas/F8.61c-git-stash-nucleo.md) | Git: guardar, listar y recuperar cambios aparte (stash) en el demonio | haiku | S | 1.6 | F5.32 | lista |
| [F8.61d-git-descartar-api](../tareas/F8.61d-git-descartar-api.md) | API: `git.discard`, `git.stash`, `git.stashList` y `git.stashPop` | haiku | S | 1.6 | F8.61b, F8.61c | bloqueada |
| [F8.61e-git-descartar-studio](../tareas/F8.61e-git-descartar-studio.md) | Studio: botones de descartar (archivo y trozo) y de guardar aparte en el panel de git | haiku | M | 1.6 | F8.61d, F8.01 | bloqueada |
| [F8.62-git-ver-commit](../tareas/F8.62-git-ver-commit.md) | Git: ver un commit completo (autor, mensaje, archivos y diff) | haiku | S | 1.6 | F8.10, F8.01, F8.62a, F8.62b, F8.62c, F8.62d | bloqueada |
| [F8.62a-git-commit-nucleo](../tareas/F8.62a-git-commit-nucleo.md) | Git: datos de un commit (`git show --numstat`) en el demonio | haiku | S | 1.6 | F8.10b | bloqueada |
| [F8.62b-git-commit-api](../tareas/F8.62b-git-commit-api.md) | API: método `git.commitInfo` | haiku | S | 1.6 | F8.62a, F8.10c | bloqueada |
| [F8.62c-git-commit-js](../tareas/F8.62c-git-commit-js.md) | Studio: módulo `commit.js` (cabecera y resumen de archivos de un commit, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.62d-git-commit-studio](../tareas/F8.62d-git-commit-studio.md) | Studio: abrir un commit completo en una pestaña (desde el historial, el blame o la paleta) | haiku | M | 1.6 | F8.62b, F8.62c, F8.10e | bloqueada |
| [F8.63-terminal-pestanas](../tareas/F8.63-terminal-pestanas.md) | Terminal: varias pestañas y entrada para procesos en marcha | haiku | S | 1.6 | F5.32, F8.01, F8.63a, F8.63b, F8.63c | bloqueada |
| [F8.63a-terminal-entrada-api](../tareas/F8.63a-terminal-entrada-api.md) | API: `terminal.run {interactive}` y `terminal.input` (escribir en la entrada de una orden en marcha) | haiku | M | 1.6 | F5.32 | lista |
| [F8.63b-terminal-pestanas-js](../tareas/F8.63b-terminal-pestanas-js.md) | Studio: módulo `terminales.js` (estado de las pestañas de terminal, puro) | haiku | M | 1.6 | F8.01 | bloqueada |
| [F8.63c-terminal-pestanas-studio](../tareas/F8.63c-terminal-pestanas-studio.md) | Studio: pestañas en la terminal, una orden por pestaña y entrada a la orden en marcha | haiku | M | 1.6 | F8.63a, F8.63b | bloqueada |
| [F8.64-completado-modelo](../tareas/F8.64-completado-modelo.md) | Completado en línea con el modelo (texto fantasma), opcional y desactivado en el perfil ligero | haiku | S | 1.6 | F5.32, F8.01, F8.64a, F8.64b, F8.64c, F8.64d | bloqueada |
| [F8.64a-completado-modelo-nucleo](../tareas/F8.64a-completado-modelo-nucleo.md) | Completado con el modelo: construir el prompt y limpiar la respuesta (lógica pura) | haiku | S | 1.6 | F5.32 | lista |
| [F8.64b-completado-modelo-api](../tareas/F8.64b-completado-modelo-api.md) | API: método `assist.inline` (propuesta del modelo para el cursor) | haiku | S | 1.6 | F8.64a | bloqueada |
| [F8.64c-completado-modelo-js](../tareas/F8.64c-completado-modelo-js.md) | Studio: módulo `fantasma.js` (cuándo sugerir, aceptar y avanzar la sugerencia, puro) | haiku | S | 1.6 | F8.01 | bloqueada |
| [F8.64d-completado-modelo-studio](../tareas/F8.64d-completado-modelo-studio.md) | Studio: texto fantasma en el editor (Tab acepta, Esc descarta), desactivado por defecto | haiku | M | 1.6 | F8.64b, F8.64c | bloqueada |

### F9 — Equipos modestos, remoto e IDE nativo

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F9.01-push-por-tarea](../tareas/F9.01-push-por-tarea.md) | Entrega continua: push de la rama tras cada tarea aceptada (sin --force) y reglas.py al día | coordinador | — | — | — | hecha |
| [F9.02-recursos](../tareas/F9.02-recursos.md) | Sección [recursos] y perfil automático ligero/normal según la RAM y los núcleos | coordinador | — | — | F9.01 | hecha |
| [F9.03-lsp-ligero](../tareas/F9.03-lsp-ligero.md) | LSP ligero: alternativas que gastan menos, máximo de servidores vivos y parada por inactividad | coordinador | — | — | F9.02 | hecha |
| [F9.04-remoto-ssh](../tareas/F9.04-remoto-ssh.md) | `pcode remoto <servidor>`: Studio, TUI o IDE contra otro equipo por SSH, instalando pcode allí si falta | coordinador | — | — | F9.02 | hecha |
| [F9.05-ide-base](../tareas/F9.05-ide-base.md) | IDE nativo sin navegador (egui + OpenGL): explorador, pestañas, resaltado, Ctrl+P, buscar, git y agente | coordinador | — | — | F9.02 | hecha |
| [F9.06-doctor-recursos](../tareas/F9.06-doctor-recursos.md) | `pcode doctor` enseña el perfil de recursos: RAM, núcleos, perfil elegido y límites LSP | haiku | — | — | F9.02 | hecha |
| [F9.07-perfil-compilacion-ligero](../tareas/F9.07-perfil-compilacion-ligero.md) | Perfil de compilación `ligero` para equipos modestos (menos RAM y tiempo que release con LTO) | haiku | — | — | F9.01 | hecha |
| [F9.08-ide-numeros-de-linea](../tareas/F9.08-ide-numeros-de-linea.md) | IDE: números de línea en el margen y Ctrl+G para ir a una línea | haiku | — | — | F9.05 | hecha |
| [F9.09-ide-buscar-reemplazar](../tareas/F9.09-ide-buscar-reemplazar.md) | IDE: buscar y reemplazar en el archivo (Ctrl+F, Ctrl+H) con mayúsculas opcionales | haiku | — | — | F9.05 | hecha |
| [F9.10-ide-problemas](../tareas/F9.10-ide-problemas.md) | IDE: panel de Problemas con los diagnósticos del LSP (evento `diagnostics`) y saltar al error | haiku | — | — | F9.05 | hecha |
| [F9.11-ide-lsp](../tareas/F9.11-ide-lsp.md) | IDE: ir a la definición (F12) e información al pasar (Ctrl+K) con posiciones UTF-16 | haiku | — | — | F9.10 | hecha |
| [F9.12-ide-terminal](../tareas/F9.12-ide-terminal.md) | IDE: terminal integrada (terminal.run) con salida por eventos y memoria acotada | haiku | — | — | F9.05 | hecha |
| [F9.13-ide-git](../tareas/F9.13-ide-git.md) | IDE: git completo en el panel (preparar, quitar, diff coloreado y commit) | haiku | — | — | F9.05 | hecha |
| [F9.14-ide-turno-del-agente](../tareas/F9.14-ide-turno-del-agente.md) | IDE: tarjeta de cambios de cada turno del agente y deshacer/rehacer (checkpoint.undo/redo) | haiku | — | — | F9.05 | hecha |
| [F9.15-ide-ajustes](../tareas/F9.15-ide-ajustes.md) | IDE: ajustes persistentes (tema, tamaño de letra, paneles) en ~/.config/pcode/ide.toml | haiku | — | — | F9.05 | hecha |
| [F9.16-remoto-hosts](../tareas/F9.16-remoto-hosts.md) | `pcode remoto --listar` y conectar desde el IDE: servidores de ~/.ssh/config | haiku | — | — | F9.04, F9.05 | hecha |
| [F9.17-rendimiento-en-ci](../tareas/F9.17-rendimiento-en-ci.md) | Presupuesto de rendimiento en la CI: arranque, Studio listo y memoria en reposo del demonio y del IDE | haiku | — | — | F9.05 | hecha |
| [F9.18-empaquetar-ide](../tareas/F9.18-empaquetar-ide.md) | Release e instalador con pcode-ide al lado de pcode (Linux y Windows) | haiku | — | — | F9.05 | hecha |
| [F9.19-extensiones-vscode-declarativas](../tareas/F9.19-extensiones-vscode-declarativas.md) | Extensiones de VS Code (solo lo declarativo): temas, fragmentos y lenguajes desde .vsix u Open VSX (cierre) | haiku | S | 1.2 | F9.05, F9.19a, F9.19b | hecha |
| [F9.19a-openvsx-nucleo](../tareas/F9.19a-openvsx-nucleo.md) | Extensiones: URL de Open VSX y lectura de su respuesta (lógica pura) | haiku | S | 1.2 | F9.05 | hecha |
| [F9.19b-openvsx-instalar](../tareas/F9.19b-openvsx-instalar.md) | `pcode extensiones instalar editor.nombre`: descargar el .vsix de Open VSX e instalarlo | haiku | M | 1.2 | F9.19a | hecha |
| [F9.20-ide-fragmentos-y-temas](../tareas/F9.20-ide-fragmentos-y-temas.md) | IDE: usar los fragmentos (snippets) y temas de las extensiones instaladas | haiku | — | — | F9.19, F9.15 | hecha |
| [F9.21-depurador-python](../tareas/F9.21-depurador-python.md) | Depurador en el IDE: DAP con debugpy (puntos de ruptura, pasos, variables) a través del demonio | coordinador | — | — | F9.05 | hecha |
| [F9.22-ide-wayland-y-accesibilidad](../tareas/F9.22-ide-wayland-y-accesibilidad.md) | IDE: Wayland nativo (carga dinámica) y lector de pantalla (AccessKit) como funciones opcionales | haiku | S | 1.6 | F9.05 | lista |
| [F9.23-lanzador](../tareas/F9.23-lanzador.md) | PublicCode en el menú de aplicaciones como VS Code: `pcode lanzador instalar|quitar` con icono y acciones | coordinador | — | — | F9.05 | hecha |
| [F9.24-ide-proyectos-recientes](../tareas/F9.24-ide-proyectos-recientes.md) | `pcode ide [ruta]`: abrir carpeta, recordar el último proyecto y lista de recientes | haiku | — | — | F9.23 | hecha |
| [F9.25-releases-periodicas](../tareas/F9.25-releases-periodicas.md) | Releases periódicas: versión, CHANGELOG y etiqueta tras cada bloque de tareas aceptadas | coordinador | S | 1.2 | F9.01 | lista |
| [F9.26-ide-sin-gpu](../tareas/F9.26-ide-sin-gpu.md) | IDE en equipos sin aceleración: aviso claro y modo software (llvmpipe) automático si OpenGL falla | haiku | — | — | F9.05 | hecha |
| [F9.27-medir-ide](../tareas/F9.27-medir-ide.md) | Medir el IDE nativo: arranque hasta la primera ventana y memoria en reposo, con Xvfb si no hay pantalla | haiku | — | — | F9.17 | hecha |
| [F9.28-ide-pulido-visual](../tareas/F9.28-ide-pulido-visual.md) | IDE: pulido visual pendiente (command center, iconos con glifos que existen, bienvenida centrada) | haiku | — | — | F9.05 | hecha |
| [F9.29-modo-opencode](../tareas/F9.29-modo-opencode.md) | Modo opencode: la TUI con el aspecto y los atajos de opencode, más ligera (`pcode` en la terminal) | haiku | S | 1.2 | F5.34, F9.29a, F9.29b, F9.29c, F9.29d, F9.29e, F9.29f, F9.29g, F9.29h | hecha |
| [F9.29a-opencode-catalogo-ordenes](../tareas/F9.29a-opencode-catalogo-ordenes.md) | TUI: catálogo de órdenes `/` (tabla única con nombre, alias y ayuda) | haiku | S | 1.2 | F5.34 | hecha |
| [F9.29b-opencode-paleta-ordenes](../tareas/F9.29b-opencode-paleta-ordenes.md) | TUI: al escribir `/` se abre la lista de órdenes (↑/↓, Tab completa, Intro ejecuta) | haiku | M | 1.2 | F9.29a | hecha |
| [F9.29c-opencode-tab-build-plan](../tareas/F9.29c-opencode-tab-build-plan.md) | TUI: Tab con la entrada vacía cambia entre Build y Plan, y el modo se ve en el borde de la entrada | haiku | S | 1.2 | F9.29b | hecha |
| [F9.29d-opencode-tema](../tareas/F9.29d-opencode-tema.md) | TUI: tema `opencode`, lista de temas y opción `pcode tui --tema <nombre>` | haiku | S | 1.2 | F5.34 | hecha |
| [F9.29e-opencode-tecla-lider](../tareas/F9.29e-opencode-tecla-lider.md) | TUI: tecla líder Ctrl+X (n nueva, m modelo, t tema, u/r deshacer y rehacer, c compactar, q salir) | haiku | M | 1.2 | F9.29c, F9.29d | hecha |
| [F9.29f-opencode-barra-lateral](../tareas/F9.29f-opencode-barra-lateral.md) | TUI: contenido de la barra lateral (sesión, contexto y archivos tocados), lógica pura | haiku | M | 1.2 | F9.29e | hecha |
| [F9.29g-opencode-cabecera-lateral](../tareas/F9.29g-opencode-cabecera-lateral.md) | TUI: dibujar la barra lateral (Ctrl+X b) y el título de la sesión en la cabecera | haiku | M | 1.2 | F9.29f | hecha |
| [F9.29h-opencode-lanzador](../tareas/F9.29h-opencode-lanzador.md) | Lanzador: acción «Modo terminal (como opencode)» en el menú de aplicaciones | haiku | S | 1.2 | F9.29d | hecha |
| [F9.30-ide-cambios-sin-guardar](../tareas/F9.30-ide-cambios-sin-guardar.md) | IDE: preguntar antes de cerrar una pestaña o la ventana con cambios sin guardar (cierre) | haiku | S | 1.6 | F9.05, F9.30a, F9.30b | bloqueada |
| [F9.30a-ide-cierre-nucleo](../tareas/F9.30a-ide-cierre-nucleo.md) | IDE: qué pestañas con cambios afecta un cierre (lógica pura) | haiku | S | 1.6 | F9.05 | lista |
| [F9.30b-ide-cierre-dialogo](../tareas/F9.30b-ide-cierre-dialogo.md) | IDE: diálogo «¿Guardar los cambios?» al cerrar una pestaña, al salir y al cerrar la ventana | haiku | M | 1.6 | F9.30a | bloqueada |
| [F9.31-ide-reemplazar-en-proyecto](../tareas/F9.31-ide-reemplazar-en-proyecto.md) | IDE: buscar en el proyecto con reemplazo en todos los archivos (cierre) | haiku | S | 1.6 | F9.09, F9.31a, F9.31b | bloqueada |
| [F9.31a-ide-reemplazar-en-proyecto-nucleo](../tareas/F9.31a-ide-reemplazar-en-proyecto-nucleo.md) | IDE: reemplazar un texto en una lista de archivos, con lectura y escritura inyectadas (lógica pura) | haiku | S | 1.6 | F9.09 | lista |
| [F9.31b-ide-reemplazar-en-proyecto-barra](../tareas/F9.31b-ide-reemplazar-en-proyecto-barra.md) | IDE: campo «Reemplazar» en la búsqueda del proyecto, con confirmación y recarga de las pestañas | haiku | M | 1.6 | F9.31a | bloqueada |
| [F9.32-ide-completado-lsp](../tareas/F9.32-ide-completado-lsp.md) | IDE: autocompletado del servidor de lenguaje en el editor (cierre) | haiku | S | 1.6 | F8.07, F9.20, F9.32a, F9.32b | bloqueada |
| [F9.32a-ide-completado-nucleo](../tareas/F9.32a-ide-completado-nucleo.md) | IDE: leer las propuestas de `lsp.completion`, filtrarlas y convertirlas en fragmentos (lógica pura) | haiku | S | 1.6 | F9.20 | lista |
| [F9.32b-ide-completado-en-el-editor](../tareas/F9.32b-ide-completado-en-el-editor.md) | IDE: Ctrl+Espacio pide `lsp.completion` y añade sus propuestas a la lista de fragmentos | haiku | M | 1.6 | F9.32a, F8.07 | bloqueada |
| [F9.33-ide-formatear](../tareas/F9.33-ide-formatear.md) | IDE: formatear el documento con el servidor de lenguaje (Mayús+Alt+F), también al guardar | haiku | M | 1.6 | F8.06, F9.11 | bloqueada |
| [F9.34-ide-renombrar-simbolo](../tareas/F9.34-ide-renombrar-simbolo.md) | IDE: renombrar un símbolo en todo el proyecto con el servidor de lenguaje (F2) (cierre) | haiku | S | 1.6 | F8.08, F9.31, F9.34a, F9.34b | bloqueada |
| [F9.34a-ide-renombrar-nucleo](../tareas/F9.34a-ide-renombrar-nucleo.md) | IDE: la palabra bajo el cursor y si un nombre nuevo es un identificador válido (lógica pura) | haiku | S | 1.6 | F9.11 | lista |
| [F9.34b-ide-renombrar-orden](../tareas/F9.34b-ide-renombrar-orden.md) | IDE: orden «Renombrar símbolo» (F2): pedir el nombre, llamar a `lsp.rename` y recargar las pestañas | haiku | M | 1.6 | F9.34a, F8.08, F9.31 | bloqueada |
| [F9.35-ide-notificaciones-de-escritorio](../tareas/F9.35-ide-notificaciones-de-escritorio.md) | IDE: notificación de escritorio cuando el agente termina o pide aprobación y la ventana no está delante (cierre) | haiku | S | 1.6 | F9.14, F9.35a, F9.35b | bloqueada |
| [F9.35a-ide-notificaciones-nucleo](../tareas/F9.35a-ide-notificaciones-nucleo.md) | IDE: qué evento merece una notificación y cómo se envía con `notify-send` (tras un trait) | haiku | S | 1.6 | F9.14 | lista |
| [F9.35b-ide-notificaciones-cableado](../tareas/F9.35b-ide-notificaciones-cableado.md) | IDE: mandar la notificación al recibir el evento, con ajuste para desactivarla | haiku | S | 1.6 | F9.35a | bloqueada |
| [F9.36-ide-vista-previa-markdown](../tareas/F9.36-ide-vista-previa-markdown.md) | IDE: vista previa de Markdown (Ctrl+Mayús+V) sin HTML crudo (cierre) | haiku | S | 1.6 | F9.05, F9.36a, F9.36b | bloqueada |
| [F9.36a-ide-markdown-nucleo](../tareas/F9.36a-ide-markdown-nucleo.md) | IDE: partir un Markdown en bloques (títulos, párrafos, listas, citas, código) para la vista previa | haiku | M | 1.6 | F9.05 | lista |
| [F9.36b-ide-markdown-vista](../tareas/F9.36b-ide-markdown-vista.md) | IDE: orden «Vista previa de Markdown» (Ctrl+Mayús+V) que dibuja los bloques en lugar del editor | haiku | M | 1.6 | F9.36a | bloqueada |
| [F9.37-ide-vista-dividida](../tareas/F9.37-ide-vista-dividida.md) | IDE: vista dividida, con un segundo editor a la derecha (cierre) | haiku | S | 1.6 | F9.05, F9.37a, F9.37b | bloqueada |
| [F9.37a-ide-vista-dividida-nucleo](../tareas/F9.37a-ide-vista-dividida-nucleo.md) | IDE: estado de la vista dividida (qué archivo va a la derecha) sin interfaz | haiku | S | 1.6 | F9.05 | lista |
| [F9.37b-ide-vista-dividida-panel](../tareas/F9.37b-ide-vista-dividida-panel.md) | IDE: orden «Dividir el editor» (Ctrl+\) y segundo editor a la derecha | haiku | M | 1.6 | F9.37a | bloqueada |
| [F9.38-ide-arrastrar-pestanas](../tareas/F9.38-ide-arrastrar-pestanas.md) | IDE: reordenar las pestañas arrastrándolas | haiku | S | 1.6 | F9.05 | esbozo |
| [F9.39-ide-atajos-configurables](../tareas/F9.39-ide-atajos-configurables.md) | IDE: atajos de teclado configurables en `keybindings.toml` (cierre) | haiku | S | 1.6 | F9.15, F9.39a, F9.39b | bloqueada |
| [F9.39a-ide-atajos-nucleo](../tareas/F9.39a-ide-atajos-nucleo.md) | IDE: leer atajos («Ctrl+Mayús+P») y el archivo `keybindings.toml` (lógica pura) | haiku | M | 1.6 | F9.15 | lista |
| [F9.39b-ide-atajos-cableado](../tareas/F9.39b-ide-atajos-cableado.md) | IDE: usar los atajos de `keybindings.toml` en el teclado y enseñarlos en menús y paleta | haiku | M | 1.6 | F9.39a | bloqueada |
| [F9.40-ide-temas-de-extension](../tareas/F9.40-ide-temas-de-extension.md) | IDE: usar los temas de color de las extensiones instaladas (cierre) | haiku | S | 1.2 | F9.20, F9.40a, F9.40b | bloqueada |
| [F9.40a-ide-tema-de-extension-nucleo](../tareas/F9.40a-ide-tema-de-extension-nucleo.md) | IDE: convertir un tema de color de VS Code en la paleta del IDE (lógica pura) | haiku | S | 1.2 | F9.20 | hecha |
| [F9.40b-ide-tema-de-extension-ajustes](../tareas/F9.40b-ide-tema-de-extension-ajustes.md) | IDE: elegir en Ajustes un tema de una extensión instalada y recordarlo | haiku | M | 1.2 | F9.40a | lista |
| [F9.41-ide-explorador-de-tests](../tareas/F9.41-ide-explorador-de-tests.md) | IDE: explorador de tests (descubrir, ejecutar y marcar ✓ ✗) sobre los métodos `tests.*` del demonio (cierre) | haiku | S | 1.6 | F8.14, F9.12, F9.41a, F9.41b, F9.41c | bloqueada |
| [F9.41a-ide-tests-estado](../tareas/F9.41a-ide-tests-estado.md) | IDE: estado del explorador de tests y captura de la salida de la terminal (lógica pura) | haiku | M | 1.6 | F9.12 | lista |
| [F9.41b-ide-tests-peticiones](../tareas/F9.41b-ide-tests-peticiones.md) | IDE: pedir ejecutores, lanzar las órdenes de tests con `terminal.run` y mandar su salida a `tests.parse` | haiku | M | 1.6 | F9.41a, F8.14 | bloqueada |
| [F9.41c-ide-tests-vista](../tareas/F9.41c-ide-tests-vista.md) | IDE: vista lateral «Tests» con ejecutores, árbol de tests con ✓ ✗ y «pedir al agente que lo arregle» | haiku | M | 1.6 | F9.41b | bloqueada |
| [F9.42-ide-plegado](../tareas/F9.42-ide-plegado.md) | IDE: plegar y desplegar bloques de código | haiku | — | despues | F9.05 | esbozo |
| [F9.43-ide-multicursor](../tareas/F9.43-ide-multicursor.md) | IDE: multicursor básico (Ctrl+D añade la siguiente coincidencia; Alt+clic añade un cursor) | haiku | — | despues | F9.42 | esbozo |
| [F9.44-extensiones-wasm](../tareas/F9.44-extensiones-wasm.md) | Extensiones: tools en módulos WASM sin red ni disco salvo lo declarado | coordinador | — | despues | F8.32 | esbozo |
| [F9.45-instalar-desde-codigo](../tareas/F9.45-instalar-desde-codigo.md) | `install.sh --local [carpeta]`: instalar pcode y pcode-ide compilados en este equipo | haiku | S | 1.2 | F9.18 | lista |
| [F9.46-version-con-commit](../tareas/F9.46-version-con-commit.md) | `pcode --version` y `pcode doctor` dicen de qué commit sale el binario: `pcode 1.1.0 (e2e30ab)` | haiku | S | 1.2 | — | lista |
| [F9.47-actualizar-local](../tareas/F9.47-actualizar-local.md) | `tools/actualizar-local.sh`: pull, compilar ligero, instalar y comprobar en una orden | haiku | S | 1.2 | F9.45, F9.46 | bloqueada |
| [F9.48-plan-hook-precommit](../tareas/F9.48-plan-hook-precommit.md) | Hook `pre-commit` que valida el plan cuando el commit toca `tareas/plan*` (que la CI no se rompa) | haiku | S | 1.2 | — | lista |
| [F9.49-ide-abrir-carpeta](../tareas/F9.49-ide-abrir-carpeta.md) | IDE: «Abrir carpeta…» (Ctrl+O) y «Abrir reciente» sin salir a la terminal | haiku | S | 1.2 | F9.24, F9.49a, F9.49b | esbozo |
| [F9.49a-ide-abrir-carpeta-nucleo](../tareas/F9.49a-ide-abrir-carpeta-nucleo.md) | IDE: selector de carpetas (navegar, filtrar, recientes) y lanzar `pcode ide <ruta>` (lógica pura) | haiku | S | 1.2 | F9.24 | hecha |
| [F9.49b-ide-abrir-carpeta-ventana](../tareas/F9.49b-ide-abrir-carpeta-ventana.md) | IDE: menú Archivo con «Abrir carpeta…» (Ctrl+O) y «Abrir reciente», y la ventana del selector | haiku | M | 1.2 | F9.49a | esbozo |

### F10 — Agente al nivel de los mejores

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F10.31-tui-ordenes-externas](../tareas/F10.31-tui-ordenes-externas.md) | TUI: órdenes `/` que resuelve quien arranca la TUI (`Command::Extra`) | haiku | M | 1.3 | F8.26 | bloqueada |
| [F10.32-ordenes-skills-mcp-permisos](../tareas/F10.32-ordenes-skills-mcp-permisos.md) | TUI: `/skills`, `/mcp` y `/permisos` muestran lo que hay configurado | haiku | M | 1.3 | F8.23, F10.31 | bloqueada |
| [F10.33-ordenes-init-revisar-exportar](../tareas/F10.33-ordenes-init-revisar-exportar.md) | TUI: `/init`, `/revisar` y `/exportar` | haiku | M | 1.3 | F8.23, F10.32 | bloqueada |
| [F10.34-tui-historial](../tareas/F10.34-tui-historial.md) | TUI: historial de entradas persistente con ↑/↓ y búsqueda con Ctrl+R | haiku | S | 1.3 | F5.30, F10.34a, F10.34b | bloqueada |
| [F10.34a-tui-historial-archivo](../tareas/F10.34a-tui-historial-archivo.md) | TUI: archivo de historial de entradas y búsqueda hacia atrás (lógica pura) | haiku | S | 1.3 | F5.30 | lista |
| [F10.34b-tui-historial-busqueda](../tareas/F10.34b-tui-historial-busqueda.md) | TUI: el historial se guarda en `.pcode/historial.jsonl` y Ctrl+R busca en él | haiku | M | 1.3 | F10.31, F10.34a | bloqueada |
| [F10.35-tui-editor-externo](../tareas/F10.35-tui-editor-externo.md) | TUI: editar el mensaje en `$EDITOR` con Ctrl+G | haiku | S | 1.3 | F5.30, F10.35a, F10.35b | bloqueada |
| [F10.35a-tui-editor-lanzar](../tareas/F10.35a-tui-editor-lanzar.md) | TUI: elegir el editor (`$VISUAL`, `$EDITOR`) y editar un texto en un archivo temporal | haiku | S | 1.3 | F5.30 | lista |
| [F10.35b-tui-editor-ctrl-g](../tareas/F10.35b-tui-editor-ctrl-g.md) | TUI: Ctrl+G abre el mensaje en el editor y vuelve con el texto | haiku | M | 1.3 | F10.34b, F10.35a | bloqueada |
| [F10.36-tui-entrada-multilinea](../tareas/F10.36-tui-entrada-multilinea.md) | TUI: cursor dentro de la entrada, edición multilínea y pegado sin enviar | haiku | S | 1.3 | F5.30, F10.36a, F10.36b, F10.36c | bloqueada |
| [F10.36a-tui-edicion-cursor](../tareas/F10.36a-tui-edicion-cursor.md) | TUI: operaciones de edición de texto con cursor (lógica pura) | haiku | M | 1.3 | F5.30 | lista |
| [F10.36b-tui-cursor-entrada](../tareas/F10.36b-tui-cursor-entrada.md) | TUI: cursor movible en la entrada (flechas, Inicio/Fin, Ctrl+A/E/W/K) | haiku | M | 1.3 | F10.35b, F10.36a | bloqueada |
| [F10.36c-tui-pegado-multilinea](../tareas/F10.36c-tui-pegado-multilinea.md) | TUI: pegar varias líneas no envía, y un pegado grande se muestra resumido | haiku | M | 1.3 | F10.36b | bloqueada |
| [F10.37-tui-modo-vi](../tareas/F10.37-tui-modo-vi.md) | TUI: modo vi opcional en la entrada | haiku | S | 1.3 | F10.36, F10.37a, F10.37b, F10.37c | bloqueada |
| [F10.37a-tui-vi-nucleo](../tareas/F10.37a-tui-vi-nucleo.md) | TUI: modo vi de la entrada (modos normal e inserción), lógica pura | haiku | M | 1.3 | F10.36a | bloqueada |
| [F10.37b-tui-vi-entrada](../tareas/F10.37b-tui-vi-entrada.md) | TUI: usar el modo vi en la entrada y mostrar el modo | haiku | S | 1.3 | F10.36c, F10.37a | bloqueada |
| [F10.37c-tui-vi-preferencia](../tareas/F10.37c-tui-vi-preferencia.md) | Configuración: `[preferencias] modo_vi = true` activa el modo vi de la TUI | haiku | S | 1.3 | F10.37b | bloqueada |
| [F10.38-tui-cola-mensajes](../tareas/F10.38-tui-cola-mensajes.md) | TUI: escribir el siguiente mensaje mientras el agente trabaja (cola) | haiku | M | 1.3 | F10.37 | bloqueada |
| [F10.39-tui-linea-estado](../tareas/F10.39-tui-linea-estado.md) | TUI: línea de estado configurable (modelo, tokens, contexto, coste, rama, modo) | haiku | S | 1.3 | F10.38, F10.39a, F10.39b, F10.39c | bloqueada |
| [F10.39a-tui-estado-nucleo](../tareas/F10.39a-tui-estado-nucleo.md) | TUI: elementos de la línea de estado y su texto (lógica pura) | haiku | S | 1.3 | F5.30 | lista |
| [F10.39b-tui-estado-dibujo](../tareas/F10.39b-tui-estado-dibujo.md) | TUI: dibujar la línea de estado configurada | haiku | S | 1.3 | F10.38, F10.39a | bloqueada |
| [F10.39c-tui-estado-preferencias](../tareas/F10.39c-tui-estado-preferencias.md) | Configuración: `[preferencias] estado`, precios por millón de tokens y rama de git en la TUI | haiku | M | 1.3 | F10.37c, F10.39b | bloqueada |
| [F10.40-cli-sesiones](../tareas/F10.40-cli-sesiones.md) | CLI: `pcode sesiones` (listar, buscar, bifurcar) y retomar con `--sesion <id>` | haiku | S | 1.3 | F5.33, F10.40a, F10.40b, F10.40c | bloqueada |
| [F10.40a-store-buscar-bifurcar](../tareas/F10.40a-store-buscar-bifurcar.md) | `pcode-store`: sesión por id, número de mensajes, búsqueda de texto y bifurcación | haiku | M | 1.3 | F5.33 | lista |
| [F10.40b-cli-sesiones-orden](../tareas/F10.40b-cli-sesiones-orden.md) | CLI: `pcode sesiones [listar | buscar <texto> | bifurcar <id>]` | haiku | M | 1.3 | F10.33, F10.40a | bloqueada |
| [F10.40c-cli-retomar-sesion](../tareas/F10.40c-cli-retomar-sesion.md) | CLI: `pcode chat --sesion <id>` y `pcode tui --sesion <id>` retoman una conversación concreta | haiku | S | 1.3 | F8.24, F10.39, F10.40b | bloqueada |
| [F10.41-cli-revisar](../tareas/F10.41-cli-revisar.md) | CLI: `pcode revisar` (revisión de los cambios sin commit, de lo preparado o de una rama) | haiku | S | 1.3 | F5.32, F10.41a, F10.41b | bloqueada |
| [F10.41a-revisar-objetivo](../tareas/F10.41a-revisar-objetivo.md) | Revisión: prompt para los cambios preparados (staged) o respecto a una rama | haiku | S | 1.3 | F5.32 | lista |
| [F10.41b-cli-revisar-orden](../tareas/F10.41b-cli-revisar-orden.md) | CLI: `pcode revisar [--staged | --base <rama>] [--formato texto|json]` | haiku | M | 1.3 | F10.40b, F10.41a | bloqueada |

### F11 — Planificación y flujos para modelos pequeños

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F11.01-flow-comprobar](../tareas/F11.01-flow-comprobar.md) | `pcode flow comprobar`: lint de tareas aptas para un modelo pequeño | haiku | S | 1.4 | F3.10, F11.01a, F11.01b, F11.01c | bloqueada |
| [F11.01a-tarea-secciones](../tareas/F11.01a-tarea-secciones.md) | Flujos: leer las secciones de una tarea (`Brief`) desde su Markdown | haiku | M | 1.4 | F3.10 | lista |
| [F11.01b-lint-tareas](../tareas/F11.01b-lint-tareas.md) | Flujos: reglas del lint de tareas (`lint_task`, `lint_all`) | haiku | M | 1.4 | F11.01a | bloqueada |
| [F11.01c-flow-comprobar-cli](../tareas/F11.01c-flow-comprobar-cli.md) | CLI: `pcode flow comprobar` con salida legible y `--formato json` | haiku | S | 1.4 | F11.01b | bloqueada |
| [F11.02-flow-siguiente-estado](../tareas/F11.02-flow-siguiente-estado.md) | `pcode flow siguiente` y `pcode flow estado`: tablero de hechas, listas y bloqueadas con su motivo | haiku | S | 1.4 | F11.01, F11.02a, F11.02b | bloqueada |
| [F11.02a-tablero-nucleo](../tareas/F11.02a-tablero-nucleo.md) | Flujos: tablero del plan (`board`, `next`, `render_board`), lógica pura | haiku | M | 1.4 | F11.01 | bloqueada |
| [F11.02b-flow-siguiente-cli](../tareas/F11.02b-flow-siguiente-cli.md) | CLI: `pcode flow siguiente` y `pcode flow estado` con el tablero | haiku | M | 1.4 | F11.02a | bloqueada |
| [F11.03-flow-desglosar](../tareas/F11.03-flow-desglosar.md) | `pcode flow desglosar <tarea>`: partir en subtareas una tarea que no pasa el lint | haiku | S | 1.4 | F11.02, F11.03a, F11.03b | bloqueada |
| [F11.03a-desglose-nucleo](../tareas/F11.03a-desglose-nucleo.md) | Flujos: prompt de desglose, lectura de bloques y validación de subtareas (lógica pura) | haiku | M | 1.4 | F11.02 | bloqueada |
| [F11.03b-flow-desglosar-cli](../tareas/F11.03b-flow-desglosar-cli.md) | CLI: `pcode flow desglosar <tarea>` escribe las subtareas como borrador | haiku | M | 1.4 | F11.03a | bloqueada |
| [F11.04-flow-hacer-cumplir-toca](../tareas/F11.04-flow-hacer-cumplir-toca.md) | Flujos: rechazar el resultado de una tarea si toca archivos fuera de su lista | haiku | S | 1.4 | F11.03, F11.04a, F11.04b | bloqueada |
| [F11.04a-toca-alcance](../tareas/F11.04a-toca-alcance.md) | Flujos: rutas permitidas de una tarea y cambios fuera de alcance (lógica pura) | haiku | S | 1.4 | F11.03 | bloqueada |
| [F11.04b-toca-en-el-runner](../tareas/F11.04b-toca-en-el-runner.md) | Flujos: el `Runner` rechaza un intento que cambia archivos fuera de `toca` | haiku | M | 1.4 | F11.04a | bloqueada |
| [F11.05-flow-reanudar](../tareas/F11.05-flow-reanudar.md) | Flujos: reanudar un flujo interrumpido y `pcode flow parar` | haiku | S | 1.4 | F11.04, F11.05a, F11.05b | bloqueada |
| [F11.05a-reanudar-eventos](../tareas/F11.05a-reanudar-eventos.md) | Flujos: eventos `Interrumpida` y `Reanudada` y `Runner::recover` | haiku | M | 1.4 | F11.04 | bloqueada |
| [F11.05b-reanudar-cli](../tareas/F11.05b-reanudar-cli.md) | CLI: `pcode flow run` retoma lo interrumpido, `--nuevo` y `pcode flow parar` | haiku | S | 1.4 | F11.05a | bloqueada |
| [F11.06-flow-presupuesto](../tareas/F11.06-flow-presupuesto.md) | Flujos: presupuesto por flujo (tokens, euros y tiempo) con parada limpia | haiku | S | 1.4 | F11.05, F11.06a, F11.06b, F11.06c | bloqueada |
| [F11.06a-presupuesto-nucleo](../tareas/F11.06a-presupuesto-nucleo.md) | Flujos: `FlowBudget` y `Spent` (lógica pura) | haiku | S | 1.4 | F11.05 | bloqueada |
| [F11.06b-presupuesto-eventos](../tareas/F11.06b-presupuesto-eventos.md) | Flujos: eventos `Consumo` y `PresupuestoAgotado` y gasto acumulado en `FlowState` | haiku | S | 1.4 | F11.06a | bloqueada |
| [F11.06c-presupuesto-runner-cli](../tareas/F11.06c-presupuesto-runner-cli.md) | Flujos: el `Runner` registra el consumo y para al agotar el presupuesto; flags de `pcode flow run` | haiku | M | 1.4 | F11.06b | bloqueada |
| [F11.07-flow-informe-final](../tareas/F11.07-flow-informe-final.md) | Flujos: informe final del flujo en Markdown (aceptadas, intentos, coste y bloqueos) | haiku | S | 1.4 | F11.06, F11.07a, F11.07b | bloqueada |
| [F11.07a-informe-flujo-nucleo](../tareas/F11.07a-informe-flujo-nucleo.md) | Flujos: `flow_report` (Markdown a partir de las tareas y los eventos) | haiku | M | 1.4 | F11.06 | bloqueada |
| [F11.07b-informe-flujo-cli](../tareas/F11.07b-informe-flujo-cli.md) | CLI: `pcode flow informe` y `.pcode/informe-flujo.md` al terminar `flow run` | haiku | S | 1.4 | F11.07a | bloqueada |
| [F11.08-flow-paralelismo-recursos](../tareas/F11.08-flow-paralelismo-recursos.md) | Flujos: paralelismo limitado por el perfil de recursos (`[recursos] flujo_paralelo`) | haiku | S | 1.4 | F11.07 | bloqueada |
| [F11.09-flow-escalado-configurable](../tareas/F11.09-flow-escalado-configurable.md) | Flujos: escalado de backend y de modelo en el intento N configurable | haiku | M | 1.4 | F11.08 | bloqueada |
| [F11.10-accion-github](../tareas/F11.10-accion-github.md) | Acción de GitHub (`action.yml`) para ejecutar `pcode run` o `pcode flow` en CI | haiku | S | 1.4 | F11.09 | bloqueada |
| [F11.11-flow-nueva](../tareas/F11.11-flow-nueva.md) | `pcode flow nueva <id>`: plantilla de una tarea con todas sus secciones | haiku | S | 1.4 | F11.10, F11.11a, F11.11b | bloqueada |
| [F11.11a-plantilla-tarea](../tareas/F11.11a-plantilla-tarea.md) | Flujos: `task_template`, el esqueleto de una tarea (lógica pura) | haiku | S | 1.4 | F11.10 | bloqueada |
| [F11.11b-flow-nueva-cli](../tareas/F11.11b-flow-nueva-cli.md) | CLI: `pcode flow nueva <id>` crea el archivo de la tarea sin pisar nada | haiku | S | 1.4 | F11.11a | bloqueada |
| [F11.12-flow-simular](../tareas/F11.12-flow-simular.md) | `pcode flow simular`: plan en seco por olas (qué se lanzaría y en qué orden) | haiku | M | 1.4 | F11.11 | bloqueada |
| [F11.13-flow-solapes-toca](../tareas/F11.13-flow-solapes-toca.md) | Flujos: no lanzar a la vez dos tareas que tocan el mismo archivo | haiku | S | 1.4 | F11.12 | bloqueada |
| [F11.14-spec-exportar-tareas](../tareas/F11.14-spec-exportar-tareas.md) | `pcode spec exportar`: pasar las tareas de una especificación al formato del flujo (borradores) | haiku | S | 1.4 | F11.13, F11.14a, F11.14b | bloqueada |
| [F11.14a-spec-exportar-nucleo](../tareas/F11.14a-spec-exportar-nucleo.md) | Especificaciones: convertir `SpecTask` en archivos de tarea del flujo (lógica pura) | haiku | S | 1.4 | F11.13 | bloqueada |
| [F11.14b-spec-exportar-cli](../tareas/F11.14b-spec-exportar-cli.md) | CLI: `pcode spec exportar <nombre>` escribe los borradores de tarea | haiku | S | 1.4 | F11.14a | bloqueada |
| [F11.15-flow-adoptar](../tareas/F11.15-flow-adoptar.md) | `pcode flow adoptar`: pasar los borradores a `tareas/` solo si todos pasan el lint | haiku | M | 1.4 | F11.14 | bloqueada |

### F12 — Seguridad, gobierno y operación

| Tarea | Qué | Ejecutor | Talla | Hito | Depende | Estado |
|---|---|---|---|---|---|---|
| [F12.01-detector-secretos](../tareas/F12.01-detector-secretos.md) | Detector de secretos: patrones de claves y tokens, y entropía (lógica pura) | haiku | M | 1.5 | F1.13 | lista |
| [F12.02-censurar-secretos-auditoria](../tareas/F12.02-censurar-secretos-auditoria.md) | Censurar secretos: `redact` y su uso al escribir la auditoría | haiku | S | 1.5 | F12.01, F1.15 | bloqueada |
| [F12.03-secretos-en-el-agente](../tareas/F12.03-secretos-en-el-agente.md) | Agente: pedir aprobación al escribir un secreto y censurar las salidas de las tools | haiku | S | 1.5 | F12.03a, F12.03b | bloqueada |
| [F12.03a-secretos-escritura](../tareas/F12.03a-secretos-escritura.md) | Agente: escribir un archivo que contiene un secreto requiere aprobación | haiku | S | 1.5 | F12.02 | bloqueada |
| [F12.03b-secretos-salidas](../tareas/F12.03b-secretos-salidas.md) | Agente: censurar los secretos de las salidas de las tools antes de enviarlas al modelo | haiku | S | 1.5 | F12.03a | bloqueada |
| [F12.04-politica-gestionada](../tareas/F12.04-politica-gestionada.md) | Política gestionada: un archivo del equipo o del sistema que el proyecto y el usuario no pueden relajar | haiku | S | 1.5 | F12.04a, F12.04b, F12.04c, F12.04d | bloqueada |
| [F12.04a-politica-gestionada-nucleo](../tareas/F12.04a-politica-gestionada-nucleo.md) | Política gestionada: formato, ruta y comprobación de modelos (núcleo) | haiku | S | 1.5 | F1.13 | lista |
| [F12.04b-politica-gestionada-run](../tareas/F12.04b-politica-gestionada-run.md) | CLI: `pcode run` aplica la política gestionada como capa base y comprueba el modelo | haiku | M | 1.5 | F12.04a, F1.20 | bloqueada |
| [F12.04c-politica-gestionada-chat-spec](../tareas/F12.04c-politica-gestionada-chat-spec.md) | CLI: la TUI, `pcode chat` y `pcode spec` aplican la política gestionada | haiku | S | 1.5 | F12.04b | bloqueada |
| [F12.04d-politica-gestionada-doctor](../tareas/F12.04d-politica-gestionada-doctor.md) | `pcode doctor` muestra la política gestionada vigente | haiku | S | 1.5 | F12.04c | bloqueada |
| [F12.05-auditoria-verificar-exportar](../tareas/F12.05-auditoria-verificar-exportar.md) | `pcode auditoria verificar` y `pcode auditoria exportar` | haiku | S | 1.5 | F12.05a, F12.05b | bloqueada |
| [F12.05a-auditoria-exportar-nucleo](../tareas/F12.05a-auditoria-exportar-nucleo.md) | Auditoría: leer las entradas y exportarlas a JSON o CSV (núcleo) | haiku | S | 1.5 | F1.15 | lista |
| [F12.05b-auditoria-cli](../tareas/F12.05b-auditoria-cli.md) | CLI: `pcode auditoria verificar` y `pcode auditoria exportar` | haiku | M | 1.5 | F12.05a, F1.20 | bloqueada |
| [F12.06-modo-sin-red](../tareas/F12.06-modo-sin-red.md) | Modo sin red: `--sin-red` impide las salidas del agente salvo la del proveedor del modelo | haiku | S | 1.5 | F12.06a, F12.06b | bloqueada |
| [F12.06a-sin-red-capa](../tareas/F12.06a-sin-red-capa.md) | Política: capa `Capabilities::offline()` (sin tools de red ni órdenes que salen a la red) | haiku | S | 1.5 | F1.13 | lista |
| [F12.06b-sin-red-cli](../tareas/F12.06b-sin-red-cli.md) | CLI: opción `--sin-red` (y `PCODE_SIN_RED=1`) en `pcode run`, `pcode chat` y la TUI | haiku | M | 1.5 | F12.04c, F12.06a | bloqueada |
| [F12.07-ordenes-denegadas-explicadas](../tareas/F12.07-ordenes-denegadas-explicadas.md) | Órdenes de shell: explicar qué regla denegó y lista siempre denegada también fuera del modo interactivo | haiku | S | 1.5 | F12.07a, F12.07b | bloqueada |
| [F12.07a-ordenes-explicar-regla](../tareas/F12.07a-ordenes-explicar-regla.md) | Política: el motivo de una orden denegada o con pregunta dice qué regla la paró | haiku | S | 1.5 | F12.06a | bloqueada |
| [F12.07b-ordenes-siempre-denegadas](../tareas/F12.07b-ordenes-siempre-denegadas.md) | Lista de órdenes siempre denegadas, también en `pcode run` y `pcode spec` | haiku | S | 1.5 | F12.07a, F12.04d | bloqueada |
| [F12.08-sandbox-aplicar-sin-escape](../tareas/F12.08-sandbox-aplicar-sin-escape.md) | Sandbox: `aplicar` nunca trae al repo hooks ni configuración de git (escape por `.git/`) | haiku | S | 1.5 | F2.03 | lista |
| [F12.09-lecturas-protegidas](../tareas/F12.09-lecturas-protegidas.md) | Lecturas protegidas: `no_leer` y `preguntar_leer`, y el agente comprueba la política al leer | haiku | S | 1.5 | F12.09a, F12.09b | bloqueada |
| [F12.09a-lecturas-protegidas-politica](../tareas/F12.09a-lecturas-protegidas-politica.md) | Política: listas `no_leer` y `preguntar_leer` con valores seguros en el uso interactivo | haiku | S | 1.5 | F12.07b | bloqueada |
| [F12.09b-lecturas-protegidas-agente](../tareas/F12.09b-lecturas-protegidas-agente.md) | Agente: comprobar `check_read` antes de ejecutar una tool de lectura | haiku | S | 1.5 | F12.03b, F12.09a | bloqueada |
| [F12.10-bash-sin-secretos-en-el-entorno](../tareas/F12.10-bash-sin-secretos-en-el-entorno.md) | Tool `bash`: no heredar las variables de entorno con secretos (`*_API_KEY`, `*_TOKEN`…) | haiku | S | 1.5 | F1.12 | lista |
<!-- tabla:fin -->
