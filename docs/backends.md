# Backends: Claude Code, GitHub Copilot, Gemini, opencode y modelos propios

Este documento recoge **cómo** se integra cada agente. Las órdenes exactas cambian entre versiones,
así que lo que aquí aparece como «previsto» se verifica y se fija en la tarea **F0.03**
(`docs/backends-verificado.md`) con las versiones instaladas, y cada adaptador lleva fixtures de
salida reales.

**Reglas comunes**

1. Solo **vías oficiales** (CLI en modo no interactivo, SDK, API pública) y **credenciales del
   usuario**. Nada de endpoints internos ni de reutilizar suscripciones de forma no prevista.
2. Cada ejecución va dentro de una **mini sandbox**: el agente ve una capa CoW del repo; sus cambios
   se revisan y se aplican después.
3. Red **solo hacia la API de ese agente** a través del proxy de salida de la sandbox
   ([mini-sandboxes.md](mini-sandboxes.md#red)).
4. Credenciales: los directorios de configuración del agente se montan **en solo lectura** (o en una
   capa CoW si la CLI necesita escribir estado), nunca dentro del repo.
5. Las instrucciones y skills llegan por el mecanismo nativo de cada agente (F2.14 y F2.17) y las
   tools de PublicCode por MCP (F2.16).

## Claude Code

| Aspecto | Previsto |
|---|---|
| Invocación | `claude -p "<prompt>" --output-format stream-json --verbose`, con `--model` y modo de permisos para autoaprobar dentro de la sandbox; alternativa: Claude Agent SDK |
| Salida | eventos JSON por línea: mensajes, uso de tools, resultado final con uso y coste |
| Instrucciones | `CLAUDE.md` (que importa `AGENTS.md`) |
| Skills | `.claude/skills/<nombre>/SKILL.md` |
| MCP | configuración de servidores MCP del proyecto o de la sesión |
| Red necesaria | API de Anthropic |
| Puntos fuertes | planificación, refactors grandes, depuración; skills y subagentes nativos |
| Tarea | F2.07 |

## GitHub Copilot

Dos modos complementarios:

**Copilot CLI (síncrono, local)**

| Aspecto | Previsto |
|---|---|
| Invocación | `copilot -p "<prompt>"` con autoaprobación de tools dentro de la sandbox |
| Instrucciones | `AGENTS.md` y `.github/copilot-instructions.md` |
| MCP | configuración de servidores MCP de la CLI |
| Red necesaria | API de GitHub Copilot |
| Tarea | F2.09 |

**Copilot coding agent (asíncrono, en GitHub)**

| Aspecto | Previsto |
|---|---|
| Flujo | PublicCode crea un issue con la tarea (texto + `acepta`), lo asigna a Copilot, espera al PR, lo baja a una sandbox y ejecuta la aceptación **localmente** antes de integrarlo |
| Instrucciones | `AGENTS.md`, `.github/copilot-instructions.md` y entorno de Copilot en el repo |
| Uso ideal | tareas `aislado` largas en paralelo sin gastar CPU local |
| Tarea | F2.11 |

## Gemini

| Aspecto | Previsto |
|---|---|
| Invocación | Gemini CLI: `gemini -p "<prompt>"` con salida JSON y autoaprobación en sandbox; o ACP |
| Como modelo | `GeminiProvider` (F1.23) en el bucle propio |
| Instrucciones | `GEMINI.md` (generado desde `AGENTS.md`) |
| Skills | exportadas como contexto o extensión (se fija en F0.03) |
| Red necesaria | API de Gemini |
| Puntos fuertes | contexto muy largo: revisión de diffs grandes y lectura de repos enteros |
| Tarea | F2.08 |

## opencode

| Aspecto | Previsto |
|---|---|
| Invocación | `opencode run "<prompt>"` o `opencode serve` y su API |
| Instrucciones | `AGENTS.md` y `opencode.json` (`instructions`) |
| Uso | continuidad con flujos ya montados sobre opencode y comparación directa en el banco |
| Tarea | F2.10 |

## Modelos propios (LiteLLM) y APIs directas

| Aspecto | Previsto |
|---|---|
| Invocación | bucle propio `pcode-core` con `OpenAiCompatProvider`, `AnthropicProvider` o `GeminiProvider` |
| Modelos | GLM-5.3-Flash, Qwen, DeepSeek, cualquier vLLM u Ollama |
| Ventajas | control total: reparación de tool calls, compresión, caché de prefijos, detector de atasco, contexto por rol |
| Tarea | F2.12 |

## Asignación de roles por defecto

| Rol | Backend por defecto | Alternativa |
|---|---|---|
| planificador (escribe tareas, desgrana) | Claude Code | Gemini |
| implementador | modelo propio (GLM) vía `pcode-core` | Claude Code con Haiku 5.5 |
| implementador asíncrono | Copilot coding agent | — |
| revisor cruzado | Gemini | Claude Code |
| crítico de plan (barato) | Haiku 5.5 o GLM | — |
| destilador de skills | Claude Code | — |

El router (F4.07) cambia esta tabla según los resultados de PublicBench por tipo de tarea.
