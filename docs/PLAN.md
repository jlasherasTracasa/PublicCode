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
* Evaluación continua (PublicBench) y telemetría.
* Interfaces: **terminal** (CLI para scripts y TUI interactiva, con el agente en proceso), **Studio**
  (editor web servido por el demonio), **IDE nativo** ligero (`pcode-ide`, egui), **APK de Android**,
  **editores** por ACP y MCP, y **GitHub** ([interfaces.md](interfaces.md), [ligero.md](ligero.md)).
* Planificación para modelos pequeños: lint de tareas, desglose asistido y tablero de estado, primero
  como herramienta del repo (`tools/tareas/generar.py`) y después como parte del producto (`pcode flow`).

**Fuera (no-objetivos)**

* Entrenar modelos propios. Se dejan trazas listas para hacerlo (F6.05), nada más.
* Una aplicación de escritorio con webview (Tauri) y una tercera interfaz en React: las sustituyen
  Studio y el IDE nativo, que caben en un equipo de 4 GB (ADR-0011). Las tareas F5.12–F5.23 quedan
  cerradas como sustituidas o descartadas.
* Ejecutar extensiones de VS Code: solo se lee lo declarativo (temas, fragmentos, lenguajes; ADR-0012).
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
7. **Todo se mide.** Un cambio en el arnés que no mejora PublicBench no se activa
   (*Harness Updating Is Not Harness Benefit*).

## 4. Plataformas

PublicCode se usa en **Linux y Windows**. El núcleo, la CLI, la TUI y el escritorio compilan para las
dos; el aislamiento de las mini sandboxes es N1 nativo en Linux y, en Windows, N1 dentro de WSL2 (o N2
con Docker Desktop). Detalle en [interfaces.md](interfaces.md) y [mini-sandboxes.md](mini-sandboxes.md).

## 5. Fases

Detalle tarea a tarea en [implementacion.md](implementacion.md). Duraciones para una persona
coordinando y agentes (Haiku 5.5 o GLM) implementando.

| Fase | Semanas | Contenido | Hito |
|---|---|---|---|
| **F0** Fundaciones | 1 | workspace, CI, verificación de CLIs, línea base, banco smoke, ADRs, eval de reglas | CI en verde, línea base reproducible |
| **F1** Núcleo | 3–4 | configuración; modelos (OpenAI-compatible, Anthropic, Gemini; SSE; reparación); tools nativas; política por capas; análisis de shell; auditoría; bucle con presupuestos; compresión; hook de parada; detector de atasco; SQLite; `pcode run` | `pcode run` resuelve 3 de 5 tareas TP del smoke con GLM |
| **F2** Backends, sandbox, skills, MCP | 4 | mini sandbox en Rust (overlay, namespaces, Landlock, seccomp, proxy de salida); backends Claude Code, Copilot CLI, Copilot coding agent, Gemini CLI, opencode y modelo propio; skills (registro, contrato, exportación); servidor MCP; sincronización de instrucciones | la misma tarea corre con los 5 backends |
| **F3** Flujos | 4 | tareas, DAG, eventos, planificador de ejecución, aceptación en dos etapas, reintentos con relevo, cola de integración con lockfiles, clasificador de CI, planificador LLM, revisión cruzada, `pcode bench` | **orquestación completa sin pegamento externo** |
| **F4** Contexto, memoria, router | 3–4 | contexto como tool, compactación con prefijo estable, memoria por facetas, tree-sitter, búsqueda híbrida y bibliotecario, contexto por rol, router por histórico, puerta de incertidumbre, memoria episódica verificada, búsqueda de tools, `preguntar`, modo guion | +12 pp de aceptación a la primera y −30 % de tokens |
| **F5** Interfaces | 6–8 (en paralelo a F3–F4) | protocolo con tipos TS generados, demonio, aprobaciones, cliente Rust; terminal: CLI para máquinas y TUI (chat, órdenes, `@`, multiagente, atajos y temas); UI común React (sesión, revisión, centro de control, ajustes, skills y banco); escritorio Tauri (bandeja, notificaciones, enlaces, llavero, paquetes para las 3 plataformas, WSL); web servida, PWA, ACP, VS Code y GitHub | uso diario desde terminal, escritorio, web e IDE |
| **F6** Aprendizaje | 3 | destilador de skills, puerta de regresión, gobierno, regresión nocturna, calidad de trayectorias | skills aprendidas sin regresiones |
| **F7** Endurecimiento | 2 | red teaming de skills y tools, evals de inyección y envenenamiento, release firmada | release 1.0 |
| **F8** 1.x: crecer | continua | editor (plegado, multicursor, vista dividida, vista previa), LSP completo (formato, completado, renombrar, tools para el agente), git (blame, ramas, conflictos), depurador DAP y explorador de tests, agente (deshacer, órdenes y agentes propios, coste, memoria, web, imágenes, modelos locales, exportar), TUI (selectores, diff del turno, ratón), plataforma (winget/msi/deb/rpm, ARM64, notificaciones y TLS para el APK, varios proyectos), extensiones, GitHub, rendimiento, accesibilidad e idiomas | sustituir del todo a VS Code y opencode en el día a día |
| **F9** Equipos modestos | hecha en su mayor parte | `[recursos]`, LSP acotado, `pcode remoto` por SSH, IDE nativo, depurador DAP, extensiones declarativas | uso diario en 2 núcleos y 4 GB |
| **F10** Agente al nivel de los mejores | 3–4 | hooks del usuario, lista de pendientes, órdenes en segundo plano, permisos persistentes, modelo de respaldo, `pcode init`, notebooks, MCP remoto, sesiones bifurcables, estadísticas; órdenes y ergonomía de la TUI | nada que envidiar a Claude Code u opencode en el día a día |
| **F11** Planificación y flujos | 2–3 | `pcode flow comprobar/siguiente/estado/desglosar`, `toca` impuesto por el motor, flujos reanudables con presupuesto e informe, acción de GitHub | el producto planifica para modelos pequeños igual que este repo |
| **F12** Seguridad, gobierno y operación | 2 | detector y censura de secretos, política gestionada, verificación de la auditoría, modo sin red | apto para un equipo con política central |

Las fases agrupan por tema. **El orden de trabajo lo dan los hitos** (sección 5.3): cada tarea abierta
pertenece a uno y `tools/tareas/generar.py --siguiente` ofrece primero las del hito más temprano.

Total: unas 26–30 semanas (F5 se solapa con F3–F4). Si se usan varios agentes en paralelo (tareas independientes en
mini sandboxes), la F1 y la F2 pueden comprimirse a la mitad.

## 5.1 Alcance de la 1.0

La 1.0 es «la herramienta de programación por defecto en Linux y Windows»: lo que hace falta para trabajar
todos los días, probado y documentado. Lo demás va a la 1.x (F8) sin bloquearla.

| Entra en la 1.0 | Estado |
|---|---|
| Agente en terminal (TUI y chat) con streaming, aprobaciones, modo plan, `@archivo`, `!orden`, MCP | hecho (F5.29–F5.34, F2.22) |
| Proveedores Anthropic, Gemini y OpenAI-compatible con reintentos, caché y compactación | hecho (F1.22–F1.26) |
| PublicCode Studio: editor, LSP, git, terminal, búsqueda y agente | hecho (F5.32, F5.35) |
| Agentes externos en sandbox y flujos; N1 en Linux y N0 en Windows | hecho (F2.01–F2.18, F2.21, F3) |
| Instalación de una línea, `doctor`, `actualizar`, release con SHA-256 | hecho (F0.07, F0.08) |
| Evals de cumplimiento de reglas e inyección de prompts | hecho (F0.06, F7.02) |
| Guía de usuario, guía de instalación, CHANGELOG y procedencia firmada de los binarios | hecho (F7.03) |
| Deshacer lo que hace el agente: puntos de control por turno, tarjeta de cambios y Actividad | hecho (F8.15, F8.43) |
| Asistente ✦ basado en papers: commit, revisión, arreglar con diagnóstico, concretar, explicar y tests | hecho (F8.37, F8.38, F8.40, F8.42) |
| Esquema, migas de pan, símbolos del proyecto, marcas de git, temas y paneles en Studio | hecho (F8.39) |
| `pcode preguntar`, ediciones mínimas y medidor de contexto | hecho (F8.41, F8.44) |

| Queda para la 1.x | Por qué no bloquea |
|---|---|
| Landlock, seccomp y proxy de red (F2.04, F2.05, F2.19, F2.20) | N1 ya aísla disco y red; esto lo endurece (hito 1.5) |
| Memoria, router y contexto avanzados (F4), aprendizaje de skills (F6) | mejoran resultados, no habilitan el uso (hito 1.7) |
| El resto de F8 | crecimiento continuo (hitos 1.3, 1.6 y 1.8) |

## 5.2 La 1.1: un agente que localiza, pregunta, recuerda y comprueba

La 1.1 mejora el agente propio con funciones sacadas de papers de ACL/EMNLP 2026 ([papers.md §12](papers.md#12-lo-que-entra-en-la-11)).
Todo vive en el núcleo, así que llega a la vez a la TUI, el chat, Studio y el APK.

| Entra en la 1.1 | Tarea | Estado |
|---|---|---|
| Tool `esquema` para el agente (RepoNav, SHERLOC) | F8.45 | hecho |
| Preguntas de aclaración con presupuesto (Ask or Assume?, AgentAsk) | F4.11 | hecho |
| Memoria del proyecto y compactación que conserva restricciones (SLUMP, AMIA) | F8.19 | hecho |
| Verificar antes de terminar (Polished but Unresolved) | F8.46 | hecho |
| Coste de los esquemas MCP (ProMCP) | F8.47 | hecho |
| GitHub Copilot, Claude Code, Gemini CLI y opencode con la cuenta del usuario, elegibles en Studio | F8.48 | hecho |

Siguientes candidatos, por orden: la tool `contexto` (F4.01), las tools del LSP para el agente (F8.09), la búsqueda de
tools cuando hay muchas (F4.10) y la revisión por archivo en la TUI (F8.25).

## 5.3 Hoja de ruta: hitos de la 1.2 a la 1.8

El estado real del código se auditó el 11 nov 2026 tarea a tarea. De ahí salieron tres correcciones: las
tareas de F9 hechas sin informe se verificaron con su aceptación y ya constan como hechas; las de F5 que
Studio, la TUI local o el IDE nativo hacían innecesarias se cerraron como sustituidas; y **todas las
abiertas se reescribieron** para que un modelo pequeño pueda ejecutarlas (pasos, firmas reales, ejemplo,
tests descritos), partiendo las grandes en subtareas. Los números al día están en
[implementacion.md](implementacion.md#estado-del-plan).

| Hito | Qué entrega | Fases que toca |
|---|---|---|
| **1.2** | Cerrar lo empezado: pulido del IDE nativo, depurador en el demonio y en el IDE, conectar a servidores desde el IDE, Open VSX, modo opencode, release 1.2.0 | F9, F8 |
| **1.3** | Agente al nivel de los mejores: hooks, pendientes, segundo plano, permisos persistentes, respaldo de modelo, órdenes y agentes propios, coste, tool `web`, imágenes, tools del LSP, TUI completa | F10, F8, F4, F5 |
| **1.4** | Planificación y flujos medidos: PublicBench y `pcode bench`, lint y desglose de tareas en `pcode flow`, revisión cruzada, planificador, flujos en el demonio y en Studio | F0, F3, F11, F5 |
| **1.5** | Seguridad: Landlock, seccomp, proxy de salida y red filtrada, red teaming de skills, secretos, política gestionada, llavero | F2, F7, F12, F5 |
| **1.6** | Studio e IDE completos: módulos y tests de Studio, plegado, vista dividida, vista previa, formato, completado, renombrar, blame, ramas, conflictos, tests, depurador, accesibilidad, idiomas | F8, F9 |
| **1.7** | Contexto, memoria y aprendizaje: compactación con prefijo estable, facetas, tree-sitter, búsqueda híbrida, contexto por rol, router, memoria episódica, destilador y gobierno de skills | F4, F6 |
| **1.8** | Plataformas e integraciones: instaladores nativos, ARM64, avisos y TLS en la APK, ACP, WSL, extensiones con manifiesto, servidor MCP, PR y checks de GitHub | F8, F5, F2 |

Cómo se trabaja cada hito: `tools/tareas/generar.py --siguiente` da las tareas listas; cada una se ejecuta
con la skill `tarea-haiku` en una mini sandbox; al aceptar, informe, commit y push. El coordinador solo
interviene en las tareas marcadas `coordinador`, que dicen por qué.

## 6. Métricas de éxito

Ver [evaluacion.md](evaluacion.md). La línea base se mide en F0 con opencode y el mismo modelo; los
objetivos son mejoras sobre ella:

| Métrica | Objetivo |
|---|---|
| Aceptación a la primera (mismo modelo) | +12 puntos |
| Tareas con 3 o más intentos | mínimas; ninguna bloqueada |
| Integraciones fallidas por lockfiles | 0 |
| Falsos rojos de CI | 0 |
| Violaciones de reglas de git | 0 (imposibles) |
| Tokens por tarea aceptada | −30 % |
| Pegamento escrito a mano | 0 |

## 7. Riesgos

| Riesgo | Prob. | Impacto | Mitigación |
|---|---|---|---|
| Las CLIs de terceros cambian sus flags o su formato de salida | alta | medio | adaptadores finos con fixtures reales (F0.03, F2.07–F2.10); tests de contrato que fallan pronto; ACP donde exista |
| Términos de uso de Copilot, Claude o Gemini | media | alto | solo vías oficiales con las credenciales del usuario; nada de proxies de suscripción |
| Los namespaces de usuario no están disponibles (Ubuntu 24.04 con AppArmor, Docker con seccomp por defecto) | media | medio | detección al arrancar y caída a N2 (contenedor); documentar el ajuste de sysctl o del perfil |
| GLM es demasiado débil para algunas tareas | media | medio | tareas desgranadas, hook de parada, detector de atasco y escalado a Claude en el tercer intento |
| Las skills aprendidas envenenan el sistema | media | alto | cuarentena, puerta de regresión, procedencia y red teaming (F6.02, F7.01) |
| Alcance enorme | alta | alto | orden por valor: F1→F3 entrega el núcleo útil |
| Haiku se sale del guion (toca archivos de más) | media | bajo | eval de reglas (F0.06), `toca` en cada tarea y revisión de la integración |
| Diferencias entre webviews (WebKitGTK y WebView2) | media | medio | CI de empaquetado con prueba de humo en Linux y Windows (F5.21); UI sin APIs exóticas |
| Dos pilas (Rust y TypeScript) | alta | bajo | AGENTS.md fija convenciones y comprobaciones por área; tipos de la API generados desde Rust (`ts-rs`) |
| Dependencia de un solo coordinador humano | media | medio | las tareas `coordinador` abiertas justifican por qué lo son (`motivo_coordinador`); el resto, autónomas |
| El plan se desfasa del código (tareas hechas sin informe, rutas que ya no existen) | alta | medio | `generar.py` rechaza contextos que citan rutas inexistentes; auditoría del estado al cerrar cada hito; una tarea solo está hecha con informe |
| Tareas demasiado abiertas para el modelo pequeño | alta | alto | lint de calidad en la CI ([tareas-guia.md](tareas-guia.md)); las grandes se parten en subtareas de lógica pura y de cableado |
| Tres interfaces gráficas que mantener | media | medio | decidido: Studio (web) e IDE nativo; Tauri y React fuera (sección 2) |

## 8. Decisiones abiertas

| Decisión | Propuesta | Necesita |
|---|---|---|
| Licencia | Apache-2.0 | confirmación del propietario |
| Nombre del CLI | `pcode` | confirmación |
| Repo público | desde F3 | confirmación |
| Planificador por defecto | Claude Code (Opus) | presupuesto de API |
| Copilot coding agent | sí, para tareas `aislado` asíncronas | plan de Copilot con coding agent habilitado |
| Extensión de VS Code (F5.25) | esbozo: antes ACP (F5.24), que sirve a más editores | decidir si compensa mantenerla |

## 9. Siguientes pasos

1. Modelo pequeño: las tareas listas del hito 1.2, en el orden de `tools/tareas/generar.py --siguiente`.
2. Coordinador: publicar la 1.2.0 (F9.25) y, para abrir el hito 1.4, F0.03 (verificar CLIs) y F0.04
   (PublicBench y línea base con opencode): sin línea base no se puede demostrar ninguna mejora.
3. Al cerrar cada hito: auditar el estado real, regenerar el plan y revisar si el formato de tarea
   necesita ajustes (¿en qué tropieza siempre el modelo pequeño?).
