# Plan de PublicCode

## 1. Objetivo

Construir **la mejor herramienta posible de programación agéntica**. No es «otro agente», sino la
capa que **orquesta varios**: Claude Code, GitHub Copilot, Gemini, opencode y modelos propios
servidos con LiteLLM. Todos trabajan con las mismas reglas, skills y tools, aislados en mini
sandboxes y juzgados por criterios de aceptación ejecutables.

**Mejor que opencode** se define de forma medible (sección 6): más tareas aceptadas a la primera con
el mismo modelo, cero fallos de integración por lockfiles, cero violaciones de reglas, menos tokens
por tarea y nada de pegamento escrito a mano alrededor.

## 2. Alcance

**Dentro**

* Orquestación de tareas (DAG, cola, reintentos, integración, CI) y de agentes (subagentes, roles,
  router).
* Adaptadores a agentes externos por sus vías oficiales y un bucle de agente propio para modelos.
* Mini sandboxes (N1), contenedores (N2) y microVM (N3).
* Política de capacidades, hooks y auditoría encadenada.
* Skills (formato único exportado a cada agente) y tools propias (nativas, MCP y WASM).
* Contexto, memoria y caché pensados para modelos pequeños.
* Evaluación continua (SinapsisBench) y telemetría.
* Interfaces: CLI, TUI, panel web, ACP para IDE y GitHub.

**Fuera (no-objetivos)**

* Entrenar modelos propios. Se dejan trazas listas para hacerlo (F6.05), nada más.
* Un IDE propio.
* Reimplementar los 75+ proveedores de opencode: LiteLLM ya lo hace; aquí van nativos solo
  OpenAI-compatible, Anthropic y Gemini.
* Un servicio en la nube. PublicCode es local-first: un demonio en tu máquina o en tu servidor.

## 3. Principios

1. **El LLM es un principal no confiable.** Toda regla que importa la impone el motor (política,
   sandbox, hooks), nunca solo el prompt. Lo demuestran *Prompts Don't Protect* y *SABER*
   ([papers](papers.md)).
2. **«Terminado» lo decide una orden.** Cada tarea tiene su `acepta`. Un juez LLM añade información,
   pero nunca decide solo (*CodeJudgeBench*).
3. **El modelo fuerte desgrana y el pequeño ejecuta.** Planificación y skills las escribe un modelo
   fuerte; la ejecución masiva la hacen Haiku 5.5 o GLM (*GRASP*, *Rethinking Scale*).
4. **Subagentes solo cuando compensa.** Coordinar tiene coste (*The Collaboration Tax*): paralelismo
   real o capacidades distintas, y siempre con capacidades atenuadas y procedencia.
5. **Determinista donde se pueda.** El orquestador no es un LLM; el LLM solo decide lo que requiere
   juicio.
6. **Local-first y abierto a cualquier modelo.** Ningún backend es obligatorio.
7. **Todo se mide.** Un cambio en el arnés que no mejora SinapsisBench no se activa
   (*Harness Updating Is Not Harness Benefit*).

## 4. Relación con SO_IA

* **Cliente cero**: SO_IA ya funciona con opencode + GLM y un orquestador casero. PublicCode debe
  sustituirlo en la fase F3 sin perder nada (prueba en sombra durante una semana).
* **Banco de pruebas**: las 84 tareas de SO_IA, con su criterio `acepta` y su commit base, forman
  SinapsisBench.
* **Prototipo de `agentd` y `flujod`**: los documentos `docs/03-ia/06` y `docs/03-ia/09 §4` de SO_IA
  prevén un orquestador nativo con capacidades auditadas, sandbox con snapshots y MCP. PublicCode es
  ese diseño ejecutándose hoy en Linux, en Rust, con conceptos compatibles (efectos declarados,
  capacidades atenuadas, auditoría con hash encadenado) para portarlo algún día.

## 5. Fases

Detalle tarea a tarea en [implementacion.md](implementacion.md). Duraciones para una persona
coordinando y agentes (Haiku 5.5 o GLM) implementando.

| Fase | Semanas | Contenido | Hito |
|---|---|---|---|
| **F0** Fundaciones | 1 | workspace, CI, verificación de CLIs, línea base, banco smoke, ADRs, eval de reglas | CI en verde, línea base reproducible |
| **F1** Núcleo | 3–4 | configuración; modelos (OpenAI-compatible, Anthropic, Gemini; SSE; reparación); tools nativas; política por capas; análisis de shell; auditoría; bucle con presupuestos; compresión; hook de parada; detector de atasco; SQLite; `pcode run` | `pcode run` resuelve 3 de 5 tareas TP del smoke con GLM |
| **F2** Backends, sandbox, skills, MCP | 4 | mini sandbox en Rust (overlay, namespaces, Landlock, seccomp, proxy de salida); backends Claude Code, Copilot CLI, Copilot coding agent, Gemini CLI, opencode y modelo propio; skills (registro, contrato, exportación); servidor MCP; sincronización de instrucciones | la misma tarea corre con los 5 backends |
| **F3** Flujos | 4 | tareas, DAG, eventos, planificador de ejecución, aceptación en dos etapas, reintentos con relevo, cola de integración con lockfiles, clasificador de CI, planificador LLM, revisión cruzada, `pcode bench`, modo SO_IA | **sustituye a `orquestador.py`** |
| **F4** Contexto, memoria, router | 3–4 | contexto como tool, compactación con prefijo estable, memoria por facetas, tree-sitter, búsqueda híbrida y bibliotecario, contexto por rol, router por histórico, puerta de incertidumbre, memoria episódica verificada, búsqueda de tools, `preguntar`, modo guion | +12 pp de aceptación a la primera y −30 % de tokens |
| **F5** Interfaces | 3 | API del demonio, TUI, panel web, ACP, GitHub | uso diario sin terminal |
| **F6** Aprendizaje | 3 | destilador de skills, puerta de regresión, gobierno, regresión nocturna, calidad de trayectorias | skills aprendidas sin regresiones |
| **F7** Endurecimiento | 2 | red teaming de skills y tools, evals de inyección y envenenamiento, release firmada | release 1.0 |

Total: unas 23–26 semanas. Si se usan varios agentes en paralelo (tareas independientes en
mini sandboxes), la F1 y la F2 pueden comprimirse a la mitad.

## 6. Métricas de éxito

Ver [evaluacion.md](evaluacion.md). Resumen frente a la línea base de SO_IA (commit `2ef8d59`):

| Métrica | Hoy | Objetivo |
|---|---|---|
| Aceptación a la primera (mismo modelo GLM-5.3-Flash) | 72,6 % | ≥ 85 % |
| Tareas con 3 o más intentos | 8 de 84 | ≤ 2 |
| Integraciones fallidas | 17 | 0 |
| Tareas de CI fantasma | ≥ 7 | 0 |
| Violaciones de reglas de git | posibles | 0 (imposibles) |
| Tokens por tarea aceptada | se mide en F0 | −30 % |
| Pegamento escrito a mano | 456 líneas | 0 |

## 7. Riesgos

| Riesgo | Prob. | Impacto | Mitigación |
|---|---|---|---|
| Las CLIs de terceros cambian sus flags o su formato de salida | alta | medio | adaptadores finos con fixtures reales (F0.03, F2.07–F2.10); tests de contrato que fallan pronto; ACP donde exista |
| Términos de uso de Copilot, Claude o Gemini | media | alto | solo vías oficiales con las credenciales del usuario; nada de proxies de suscripción |
| Los namespaces de usuario no están disponibles (Ubuntu 24.04 con AppArmor, Docker con seccomp por defecto) | media | medio | detección al arrancar y caída a N2 (contenedor); documentar el ajuste de sysctl o del perfil |
| GLM es demasiado débil para algunas tareas | media | medio | tareas desgranadas, hook de parada, detector de atasco y escalado a Claude en el tercer intento |
| Las skills aprendidas envenenan el sistema | media | alto | cuarentena, puerta de regresión, procedencia y red teaming (F6.02, F7.01) |
| Alcance enorme | alta | alto | orden por valor: F1→F3 entrega lo que más falta en SO_IA; F5 (interfaces) puede esperar |
| Haiku se sale del guion (toca archivos de más) | media | bajo | eval de reglas (F0.06), `toca` en cada tarea y revisión de la integración |
| Dependencia de un solo coordinador humano | media | medio | tareas `coordinador` mínimas (3 en F0–F1); el resto, autónomas |

## 8. Decisiones abiertas

| Decisión | Propuesta | Necesita |
|---|---|---|
| Licencia | Apache-2.0 | confirmación del propietario |
| Nombre del CLI | `pcode` | confirmación |
| Repo público | desde F3 | confirmación |
| Planificador por defecto | Claude Code (Opus) | presupuesto de API |
| Copilot coding agent en SO_IA | sí, para tareas `aislado` asíncronas | plan de Copilot con coding agent habilitado |

## 9. Primeros pasos (esta semana)

1. Coordinador: F0.03 (verificar CLIs) y F0.04 (línea base y smoke en un clon completo de SO_IA).
2. Haiku 5.5: F0.01 → F0.02 → F0.06, y después F1.01, F1.02, F1.08 y F1.19 en paralelo (no dependen
   entre sí más allá de F0.01).
3. Revisar el primer informe de Haiku y ajustar el formato de tarea si hace falta, antes de lanzar
   el resto de F1.
