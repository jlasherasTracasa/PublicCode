# PublicCode

**Programación agéntica con un equipo de agentes en lugar de uno solo.** PublicCode orquesta
**Claude Code**, **GitHub Copilot**, **Gemini** y **tus propios modelos servidos con LiteLLM**
(GLM, Qwen, vLLM, Ollama…) como subagentes intercambiables. Cada uno trabaja en su **mini sandbox**,
comparte las mismas **skills** y **tools** y responde ante un **criterio de aceptación ejecutable**.
Todo se audita y se mide.

> **Estado: diseño, fase 0.** Este repositorio contiene el plan completo, el plan de implementación
> desgranado para agentes pequeños (Haiku 5.5), las skills y las primeras tools que ya funcionan:
> el prototipo de [mini sandbox](tools/minisandbox/) (9 tests en verde) y el buscador de
> [papers de ACL/EMNLP 2026](tools/papers/). El CLI `pcode` todavía no existe: es lo primero que se
> construye (ver [docs/implementacion.md](docs/implementacion.md)).

---

## Índice

1. [Por qué existe](#por-qué-existe)
2. [Qué hace mejor que opencode](#qué-hace-mejor-que-opencode)
3. [Agentes y modelos que integra](#agentes-y-modelos-que-integra)
4. [Cómo funciona](#cómo-funciona)
5. [Conceptos](#conceptos)
6. [Mini sandboxes](#mini-sandboxes)
7. [Skills compartidas entre todos los agentes](#skills-compartidas-entre-todos-los-agentes)
8. [Tools propias](#tools-propias)
9. [Políticas de seguridad aplicadas por el motor](#políticas-de-seguridad-aplicadas-por-el-motor)
10. [Base científica: ACL y EMNLP 2026](#base-científica-acl-y-emnlp-2026)
11. [Cómo se mide que es mejor](#cómo-se-mide-que-es-mejor)
12. [Hoja de ruta](#hoja-de-ruta)
13. [Uso rápido (lo que ya funciona)](#uso-rápido-lo-que-ya-funciona)
14. [Configuración prevista](#configuración-prevista)
15. [Estructura del repositorio](#estructura-del-repositorio)
16. [Desarrollar PublicCode con agentes](#desarrollar-publiccode-con-agentes)
17. [Decisiones abiertas](#decisiones-abiertas)

---

## Por qué existe

Hoy cada agente de programación es una isla: Claude Code, Copilot, Gemini CLI y opencode tienen
fortalezas distintas, configuraciones distintas (`CLAUDE.md`, `.github/copilot-instructions.md`,
`GEMINI.md`, `AGENTS.md`) y ninguno de ellos sabe coordinar a los demás. Cuando un proyecto quiere
usarlos en serio acaba escribiendo su propio pegamento.

El proyecto hermano [SO_IA](https://github.com/jlasherasTracasa/SO_IA) (un sistema operativo en
Rust construido por agentes) es el caso real que motiva PublicCode. Allí opencode + GLM-5.3-Flash
trabajan coordinados por un orquestador casero (`ci/orquestador.py`, 356 líneas, más varios scripts
de shell). Los datos de su `ci/estado.json` y `ci/orquestador.log` en el commit `2ef8d59`:

| Señal | Valor | Qué indica |
|---|---|---|
| Tareas aceptadas a la primera | 61 de 84 (**73 %**) | el agente se da por terminado sin haber verificado |
| Tareas que necesitaron 3 o más intentos | 8 (una de ellas, T2.17, sigue **bloqueada**) | los reintentos no aprenden del fallo anterior |
| Fallos al integrar ramas | **17** (conflictos en `Cargo.toml`/`Cargo.lock`) | no hay cola de integración con regeneración de lockfiles |
| Tareas `CI-arreglo-*` | **27**, varias «fantasma» por un error del clasificador | el estado de la CI no se clasifica bien |
| Reinicios del orquestador | 15 | el estado no es durable y el proceso depende de hacks de recarga |
| Reglas críticas escritas en prosa | `AGENTS.md` prohíbe `git stash`, `git add -A`, `reset --hard`… | el modelo puede ignorarlas: nada las impone |
| Clave de API en el repo | `opencode.json` lleva la clave de LiteLLM en claro | los secretos no tienen un sitio propio |

PublicCode convierte todo ese pegamento en producto: tareas, verificación, reintentos, integración,
políticas, sandboxes y evaluación, sea cual sea el agente que haga el trabajo.

## Qué hace mejor que opencode

opencode es un buen agente de terminal: TUI cliente-servidor, muchos proveedores, agentes y
subagentes definidos en Markdown, comandos, plugins, MCP, LSP, permisos y deshacer. PublicCode no
compite en ser otro chat de terminal, sino en lo que pasa **alrededor**. La comparación parte de
lo que sé de opencode hasta mediados de 2026 y se verificará contra la versión actual en la tarea F0.2.

| Necesidad | opencode | PublicCode |
|---|---|---|
| Usar varios agentes externos (Claude Code, Copilot, Gemini) como subagentes | no; es su propio agente | **sí**: adaptadores para cada CLI y para modelos vía LiteLLM |
| Tareas con criterio de aceptación, dependencias, reintentos e integración | no (lo hace un orquestador externo) | **nativo** (`pcode flow`), con estado durable en SQLite |
| Aislamiento por subagente | comparte el árbol de trabajo | **mini sandbox CoW** por subagente o por orden (~55 ms en el prototipo) |
| Reglas de seguridad | permisos por herramienta y patrón | **capacidades** que impone el motor: rutas, órdenes git, red por dominio y efectos irreversibles; el prompt no las sustituye |
| Delegación segura | — | los subagentes reciben capacidades **atenuadas** y la procedencia viaja con cada handoff |
| Skills | soporte de skills | un único formato `SKILL.md` **exportado a todos los agentes**, con evals, cuarentena y aprendizaje desde trazas |
| No terminar sin verificar | depende del modelo | **hook de parada**: no hay «terminado» sin la aceptación en verde |
| Revisión cruzada | — | un agente distinto revisa el diff (p. ej. Gemini revisa lo que escribió GLM) |
| Elegir el mejor agente para cada tarea | un modelo por sesión | **enrutado** por tipo de tarea, coste y resultados históricos del banco |
| Medir mejoras | — | **SinapsisBench**: re-ejecuta tareas reales de SO_IA y compara agentes |
| Auditoría | logs de sesión | registro **append-only con hash encadenado** de cada efecto |

## Agentes y modelos que integra

Hay dos clases de backend:

* **Agentes completos**: traen su propio bucle y sus propias herramientas. PublicCode los lanza en
  modo no interactivo dentro de una mini sandbox, les da el contexto de la tarea, las skills y las
  tools (vía MCP) y recoge el diff, la traza y el resultado.
* **Modelos**: PublicCode pone su propio bucle de agente (`pcode-core`) y el modelo solo razona.
  Es la vía para modelos propios, donde controlamos todo: contexto, reparación de llamadas a tools
  y caché de prefijos.

| Backend | Clase | Cómo se integra | Para qué brilla |
|---|---|---|---|
| **Claude Code** | agente | modo headless (`claude -p … --output-format stream-json`) o Claude Agent SDK; skills en `.claude/skills/`; MCP | planificación, refactors grandes, depuración difícil |
| **GitHub Copilot** | agente | Copilot CLI en modo no interactivo; **Copilot coding agent** asíncrono vía issue y PR en GitHub; `.github/copilot-instructions.md` y `AGENTS.md` | trabajo asíncrono sobre issues y revisión de PR |
| **Gemini** | agente o modelo | Gemini CLI headless (`gemini -p …`) o ACP; o la API de Gemini como modelo en el bucle propio | contexto muy largo: lectura de repos enteros y revisión |
| **opencode** | agente | `opencode run` / `opencode serve` | continuidad con el flujo actual de SO_IA |
| **LiteLLM** (GLM, Qwen, DeepSeek, vLLM, Ollama…) | modelo | cualquier endpoint OpenAI-compatible; bucle propio optimizado para modelos abiertos | volumen: implementación masiva barata y en local, con datos confidenciales |
| **API de Anthropic / Gemini / OpenAI** | modelo | proveedores nativos en el bucle propio | roles concretos (planificador, juez) con control total del contexto |

Las órdenes exactas de cada CLI cambian entre versiones. Se fijan y se prueban contra las versiones
instaladas en la tarea F0.2. **Solo se usan vías oficiales y las credenciales del propio usuario.**
Nada de endpoints no documentados ni de compartir suscripciones.

## Cómo funciona

```
 tarea (Markdown + criterio `acepta`)          clientes: CLI · TUI · panel web · IDE (ACP) · GitHub
        │                                                   │
        ▼                                                   ▼
┌──────────────────────────────── pcoded (demonio local) ─────────────────────────────────┐
│  flow ── DAG de tareas · cola · reintentos con contexto del fallo · cola de integración  │
│   │                                                                                     │
│   ├─ router ── elige backend por tipo de tarea, coste, clase de datos e histórico       │
│   │                                                                                     │
│   ├─ backend ─┬─ Claude Code ─┐                                                         │
│   │           ├─ Copilot ─────┤  cada uno dentro de su MINI SANDBOX (overlay CoW,      │
│   │           ├─ Gemini ──────┤  sin red, raíz de solo lectura, PIDs propios)           │
│   │           ├─ opencode ────┤  + skills exportadas + tools vía MCP de PublicCode      │
│   │           └─ pcode-core ──┘  (bucle propio para modelos LiteLLM/API)                │
│   │                                                                                     │
│   ├─ verificación ── aceptación en entorno limpio · revisión cruzada por otro agente    │
│   └─ integración ── aplicar diff · rebase · regenerar lockfiles · re-validar · push     │
│                                                                                         │
│  policy (capacidades, hooks, aprobaciones) · audit (hash encadenado) · memory (índice   │
│  de código, memoria episódica) · skills (registro, evals, cuarentena) · telemetry (OTel)│
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

Ciclo de una tarea:

1. **Planificar**: la tarea declara su criterio de aceptación (`acepta`), sus dependencias, su
   presupuesto y su modo de aislamiento. Puede escribirla una persona o un agente planificador.
2. **Enrutar**: el router elige backend y modelo por rol, por ejemplo planificador = Claude,
   implementador = GLM local y revisor = Gemini.
3. **Ejecutar en mini sandbox**: el agente trabaja sobre una capa copy-on-write del repo. El repo
   real no cambia.
4. **Verificar**: el hook de parada ejecuta la aceptación. Si falla, el agente recibe la salida
   comprimida y sigue; si se atasca, se detiene pronto y se escala a otro modelo.
5. **Revisar**: otro agente revisa el diff con una rúbrica del repo.
6. **Integrar**: se aplica el diff, se regeneran lockfiles, se re-valida tras el merge y se hace push.
7. **Aprender**: si la tarea costó (2 o más intentos), se destila una skill candidata desde la traza.
   Entra en cuarentena hasta pasar sus evals.

## Conceptos

| Concepto | Qué es |
|---|---|
| **Tarea** | archivo Markdown con cabecera (`acepta`, `modo`, `depende`, `presupuesto`, `backend`, `skills`). Compatible con el formato de `ci/tareas/` de SO_IA |
| **Backend** | adaptador a un agente externo o a un modelo. Interfaz común: `preparar → ejecutar → resultado {diff, traza, uso, salida}` |
| **Subagente** | instancia de un backend lanzada por otro agente, con capacidades atenuadas, presupuesto propio y salida estructurada |
| **Skill** | carpeta con `SKILL.md` (estándar abierto Agent Skills) más scripts y recursos. Solo el nombre y la descripción entran en el contexto; el resto se carga al usarla |
| **Tool** | operación tipada con **efectos declarados** (`lee`, `escribe`, `ejecuta`, `red`, `irreversible`). Nativa, MCP, WASM o script |
| **Mini sandbox** | entorno aislado de milisegundos para una orden o un subagente (ver abajo) |
| **Política** | capacidades que impone el motor: qué rutas, qué órdenes, qué dominios, qué requiere aprobación |
| **Criterio de aceptación** | una orden que devuelve 0 si la tarea está bien. Es el único juez de «terminado» |
| **Flujo** | DAG de tareas con cola, paralelismo, reintentos e integración |

## Mini sandboxes

Un agente no debería necesitar un contenedor Docker ni un clon del repo para cada orden.
PublicCode define **cuatro niveles** y usa el más ligero que baste:

| Nivel | Mecanismo | Arranque | Para qué |
|---|---|---|---|
| **N0** | WASM (wasmtime) sin capacidades | µs | tools puras: parsear, formatear, transformar |
| **N1 · mini sandbox** | namespaces de usuario + **overlay CoW** sobre el repo + raíz de solo lectura + sin red + PIDs propios (más adelante, Landlock y seccomp) | **~55 ms** (prototipo, medido) | cada orden de shell y cada subagente. Paralelismo sin clones; deshacer = descartar la capa |
| **N2** | contenedor (Docker/Podman) con imagen del proyecto | segundos | toolchains completos (p. ej. la imagen `sinapsis-dev` con QEMU) |
| **N3** | microVM (Firecracker / Cloud Hypervisor) | ~100 ms–1 s | código no confiable, red abierta, cargas tipo kernel |

El prototipo [`tools/minisandbox/minisandbox.py`](tools/minisandbox/minisandbox.py) ya implementa
N1 sin privilegios:

* el repo queda cubierto por un overlay: las escrituras van a una capa aparte y el repo real no se toca;
* `cambios` lista lo añadido, modificado y borrado (respetando `.gitignore`) y puede mostrar el diff;
* `aplicar` lleva los cambios al repo y `descartar` los tira;
* sin red por defecto, el resto del disco en solo lectura, `/tmp` y `HOME` privados y `--tiempo` máximo;
* 9 tests en [`test_minisandbox.py`](tools/minisandbox/test_minisandbox.py), todos en verde.

Ventaja sobre el flujo actual de SO_IA: cuatro agentes en paralelo comparten una única copia del repo
como capa inferior, y cada uno tiene su capa superior. Desaparecen los `git clone` por tarea y la
integración pasa a ser «aplicar diff + re-validar». Diseño completo en
[docs/mini-sandboxes.md](docs/mini-sandboxes.md).

## Skills compartidas entre todos los agentes

* **Una fuente, todos los formatos.** Las skills viven en un solo sitio y `pcode skills exportar` las
  publica donde cada agente las busca: `.claude/skills/` para Claude Code, `AGENTS.md` y
  `.github/copilot-instructions.md` para Copilot, `GEMINI.md` y extensiones para Gemini, y la carpeta
  de skills de opencode.
* **Carga progresiva.** Solo el nombre y la descripción ocupan contexto; el cuerpo y los scripts se
  cargan al usarse. Con muchas skills se recupera por grafo de dependencias (Graph-of-Skills, EMNLP 2026).
* **Skills con contrato.** Entradas y salidas tipadas más plantillas de invocación concretas
  (Skill-as-Pseudocode, EMNLP 2026), sobre todo para modelos pequeños.
* **Aprendidas, pero con control.** Las skills se destilan de trazas (Trace2Skill, SKILL-DISCO) y
  entran en **cuarentena** hasta pasar evals sin regresiones (GRASP). La cuarentena existe porque
  las skills son superficie de ataque (SkillSafetyBench, EvoSkill Injection, SkillAttack).

Skills de este repo, en [`.claude/skills/`](.claude/skills/):

| Skill | Para qué |
|---|---|
| `tarea-haiku` | ejecutar una tarea de `tareas/` con disciplina de modelo pequeño: leer, implementar, aceptar una vez, informar |
| `papers-acl-emnlp` | buscar papers de ACL/EMNLP con la tool del repo y adaptarlos al plan sin inventar nada |
| `mini-sandbox` | ejecutar órdenes aisladas y revisar o aplicar sus cambios |
| `sinapsis-bench` | construir y correr el banco de evaluación y comparar backends |
| `adr` | registrar una decisión de arquitectura |

## Tools propias

| Tool | Estado | Qué hace |
|---|---|---|
| [`tools/minisandbox/`](tools/minisandbox/) | ✅ prototipo con tests | mini sandbox N1: `ejecutar`, `cambios`, `aplicar`, `descartar` |
| [`tools/papers/`](tools/papers/) | ✅ funciona | busca en la ACL Anthology (ACL 2026 + EMNLP 2026 por defecto) y puntúa por temas del plan |
| [`tools/bench/`](tools/bench/) | ✅ funciona | línea base del orquestador de SO_IA y extracción de tareas para SinapsisBench |
| servidor MCP `pcode-mcp` | planificado (F2) | expone sandbox, aceptación, memoria del repo y papers a **cualquier** agente: Claude Code, Copilot, Gemini y opencode usan las mismas tools |

## Políticas de seguridad aplicadas por el motor

Las reglas que hoy son prosa en `AGENTS.md` pasan a ser política que se impone:

```toml
[politica.ordenes]
denegar = ["git stash*", "git reset --hard*", "git checkout -- .", "git clean*",
           "git add -A*", "git add .", "git push*", "git rebase*", "sudo *", "apt*"]

[politica.archivos]
escribir = ["./**"]
no_escribir = ["docs/**", "REGISTRO.md"]

[politica.red]
permitir = ["index.crates.io", "static.crates.io"]   # el resto, denegado

[[hooks.parada]]          # no se puede terminar sin la aceptación en verde
orden = "pcode aceptar --comprobar"
```

Defensa en profundidad: cada orden se analiza (AST de shell, no solo el texto), se ejecuta en una
sandbox donde lo prohibido no puede ocurrir y se comprueba después. Es lo que miden SABER
(EMNLP 2026) en espacios de trabajo con estado y «Prompts Don't Protect» (EMNLP 2026 Industry):
una tool que el agente no ve no la puede usar.

## Base científica: ACL y EMNLP 2026

Las actas de **ACL 2026** y **EMNLP 2026** ya están en la ACL Anthology: 10.341 papers.
`tools/papers/buscar_papers.py` los descarga con sus resúmenes, los puntúa por temas del plan
(1.189 candidatos) y [docs/papers.md](docs/papers.md) recoge los ~60 que se adaptan, con enlace y
**qué cambia en PublicCode** por cada uno. Algunos ejemplos:

| Paper (venue) | Qué adopta PublicCode |
|---|---|
| Context as a Tool (Findings ACL) · SWE-MEM (EMNLP) | el contexto se gestiona con una tool explícita, no con compactación pasiva |
| TokenPilot (Findings EMNLP) | la compresión respeta los prefijos para no romper la caché |
| Prism (Findings EMNLP) | memoria de archivos por facetas entre tareas (su código parte de opencode) |
| EET (Findings ACL) | terminación temprana guiada por experiencia: detecta atascos y ahorra tokens |
| Handoff Debt (EMNLP) | los reintentos y relevos reciben un informe de relevo estructurado |
| Trust Cascade · Delegated Misalignment (EMNLP) | procedencia y capacidades atenuadas en cada delegación |
| Prompts Don't Protect (EMNLP Industry) | las tools no autorizadas ni aparecen en el contexto |
| Trace2Skill · GRASP · SkillsVote (EMNLP) | ciclo de vida de skills: destilar, validar sin regresiones y gobernar |
| SABER (EMNLP) · When "Correct" Is Not Safe (ACL) | evals de seguridad operativa y revisión de seguridad del parche |
| Dockerless (EMNLP) · SWE-World (Findings EMNLP) | verificación barata antes de la cara, en contenedor |
| AgentLens (EMNLP Industry) · How Coding Agents Fail Their Users (EMNLP) | evaluación por trayectoria, no solo aprobado o suspenso |
| Rethinking Scale (ACL Industry) · Scaffolded Execution for SLMs (Findings EMNLP) | para modelos pequeños se reestructura la ejecución, no solo el prompt |

## Cómo se mide que es mejor

**SinapsisBench**: las tareas reales de SO_IA (cada una con su commit base y su orden `acepta`) se
re-ejecutan con cada backend en el mismo contenedor. Objetivos frente a la línea base actual
(opencode + GLM-5.3-Flash + orquestador casero):

| Métrica | Línea base | Objetivo |
|---|---|---|
| Aceptación a la primera | 73 % | **≥ 85 %** con el mismo modelo |
| Tareas con 3 o más intentos | 8 de 84 | ≤ 2 |
| Tareas bloqueadas | 1 | 0 |
| Fallos de integración por lockfiles | 17 | **0** |
| Tareas de CI fantasma | ≥ 7 | **0** |
| Violaciones de reglas de git | posibles | **imposibles** (política) |
| Tokens por tarea aceptada | medir en F0 | −30 % |
| Pegamento fuera de la herramienta | ~450 líneas | 0 (configuración declarativa) |

Además se publica una comparación externa en un benchmark público (SWE-bench Verified o
Terminal-Bench) con el mismo modelo abierto en opencode y en PublicCode. Detalle en
[docs/evaluacion.md](docs/evaluacion.md).

## Hoja de ruta

| Fase | Qué | Hito verificable |
|---|---|---|
| **F0** Fundaciones | workspace Rust, CI, ADRs, banco y línea base, verificar CLIs | `cargo test` en verde; línea base reproducible |
| **F1** Núcleo | bucle propio, proveedor LiteLLM, tools nativas, política, sesiones, `pcode run` | resuelve tareas aisladas del banco con GLM |
| **F2** Backends y sandbox | adaptadores para Claude Code, Copilot, Gemini y opencode; mini sandbox en Rust; servidor MCP; skills | la misma tarea corre con cualquier backend |
| **F3** Flujos | tareas, DAG, aceptación, reintentos, integración, CI | **sustituye a `orquestador.py` en SO_IA** |
| **F4** Contexto y memoria | gestión de contexto, caché de prefijos, memoria de archivos, router por histórico | +12 puntos de aceptación y −30 % de tokens |
| **F5** Interfaces | TUI, panel web, ACP para IDE, GitHub | uso diario sin terminal |
| **F6** Aprendizaje | skills destiladas con cuarentena y evals continuos | regresión nocturna automática |
| **F7** Endurecimiento | red teaming de skills y tools, publicación | release firmada |

Plan completo: [docs/PLAN.md](docs/PLAN.md). Arquitectura: [docs/arquitectura.md](docs/arquitectura.md).
Plan de implementación tarea a tarea para Haiku 5.5: [docs/implementacion.md](docs/implementacion.md)
y [tareas/](tareas/).

## Uso rápido (lo que ya funciona)

Requisitos: Linux con namespaces de usuario sin privilegios (cualquier distribución moderna),
Python 3.10 o superior y `git`.

Ejecutar una orden en una mini sandbox sin red sobre el repo actual:

```bash
tools/minisandbox/minisandbox.py ejecutar --estado /tmp/ms1 -- cargo test
```

Ver qué cambió, con diff:

```bash
tools/minisandbox/minisandbox.py cambios /tmp/ms1 --diff
```

Llevar los cambios al repo, o tirarlos:

```bash
tools/minisandbox/minisandbox.py aplicar /tmp/ms1
```

```bash
tools/minisandbox/minisandbox.py descartar /tmp/ms1
```

Pasar los tests de la sandbox:

```bash
python3 -m unittest tools/minisandbox/test_minisandbox.py -v
```

Buscar papers de ACL y EMNLP 2026 sobre skills y sandboxes (la primera vez descarga unos 47 MB):

```bash
tools/papers/buscar_papers.py --tema skills --tema sandbox-seguridad --top 30
```

Calcular la línea base del orquestador de SO_IA:

```bash
tools/bench/linea_base.py --repo ../SO_IA
```

## Configuración prevista

`pcode.toml` en la raíz del proyecto. Es orientativo: el esquema definitivo sale de la tarea F1.3.

```toml
[proyecto]
instrucciones = ["AGENTS.md"]

[proveedores.litellm]
tipo = "openai-compatible"
url = "http://127.0.0.1:30400/v1"
clave = "{env:LITELLM_API_KEY}"        # nunca en el repo

[backends.claude-code]
tipo = "agente-cli"
binario = "claude"

[backends.copilot]
tipo = "agente-cli"
binario = "copilot"

[backends.copilot-github]
tipo = "agente-github"                 # Copilot coding agent: issue → PR

[backends.gemini]
tipo = "agente-cli"
binario = "gemini"

[backends.glm]
tipo = "modelo"
modelo = "litellm/GLM-5.3-Flash"

[roles]
planificador  = "claude-code"
implementador = "glm"
revisor       = "gemini"
asincrono     = "copilot-github"

[sandbox]
nivel_por_defecto = "N1"
red = "denegar"
```

## Estructura del repositorio

```
PublicCode/
├── README.md                 ← este documento
├── AGENTS.md                 ← reglas para cualquier agente que trabaje aquí (Haiku 5.5 incluido)
├── CLAUDE.md                 ← importa AGENTS.md para Claude Code
├── docs/
│   ├── PLAN.md               ← plan completo: objetivos, fases, riesgos, decisiones
│   ├── arquitectura.md       ← subsistemas en detalle
│   ├── backends.md           ← integración de Claude Code, Copilot, Gemini, opencode y LiteLLM
│   ├── mini-sandboxes.md     ← niveles N0–N3, diseño y amenazas
│   ├── evaluacion.md         ← SinapsisBench, métricas y protocolo
│   ├── papers.md             ← ACL/EMNLP 2026 adaptados al plan
│   ├── implementacion.md     ← plan de implementación desgranado para Haiku 5.5
│   └── adr/                  ← decisiones de arquitectura
├── tareas/                   ← una tarea por archivo, lista para un agente pequeño
├── tools/
│   ├── minisandbox/          ← mini sandbox N1 (prototipo + tests)
│   ├── papers/               ← buscador de papers en la ACL Anthology
│   └── bench/                ← línea base y extracción de tareas de SO_IA
└── .claude/skills/           ← skills del repo (formato SKILL.md)
```

## Desarrollar PublicCode con agentes

PublicCode se construye como SO_IA: con agentes. El plan de implementación está partido en
**87 tareas** (81 para **Claude Haiku 5.5** y 6 para el coordinador), todas generadas desde
[`tareas/plan.toml`](tareas/plan.toml) y validadas por `tools/tareas/generar.py`. Cada tarea toca 1–3 archivos, da las firmas
exactas, los tests que hay que escribir y una única orden de aceptación. Reglas en
[AGENTS.md](AGENTS.md); procedimiento en la skill `tarea-haiku`.

1. Elige la primera tarea pendiente de [docs/implementacion.md](docs/implementacion.md) cuyas
   dependencias estén hechas.
2. Lánzala con el agente que prefieras, por ejemplo Claude Code con Haiku 5.5:

```bash
claude --model claude-haiku-5-5 -p "Ejecuta la tarea tareas/F0.01-workspace.md siguiendo AGENTS.md"
```

3. Si la orden `acepta` de la tarea pasa, se integra. Desde la fase F3, PublicCode se construye a sí mismo.

## Decisiones abiertas

| Decisión | Propuesta | Por qué |
|---|---|---|
| Lenguaje del núcleo | **Rust** (ADR-0001) | binario único, rendimiento, sandboxes con Landlock/seccomp nativos, coherencia con SO_IA (puede convertirse en `agentd`/`flujod`) |
| Licencia | Apache-2.0 | permisiva con patentes; pendiente de confirmar |
| Nombre del CLI | `pcode` | corto; pendiente de confirmar |
| Planificador por defecto | Claude Code | mejor planificación; GLM como alternativa local |
| Publicación | repo público desde F3 | cuando el flujo se construya a sí mismo |
