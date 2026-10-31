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
* Interfaces sobre un demonio único: **terminal** (CLI para scripts y TUI interactiva), **aplicación
  de escritorio** (Tauri 2 en Linux y Windows), **web**, **PWA móvil** de aprobaciones,
  **IDE** (ACP, VS Code y MCP) y **GitHub** ([interfaces.md](interfaces.md)).

**Fuera (no-objetivos)**

* Entrenar modelos propios. Se dejan trazas listas para hacerlo (F6.05), nada más.
* Un IDE propio (la app de escritorio es un centro de control de agentes, no un editor: abre los
  archivos en tu editor).
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
| App de escritorio Tauri, UI React común (F5.12–F5.21) | Studio ya cubre la interfaz gráfica en el navegador |
| Landlock, seccomp y proxy de red (F2.04, F2.05, F2.19, F2.20) | N1 ya aísla disco y red; esto lo endurece |
| Memoria, router y contexto avanzados (F4), aprendizaje de skills (F6) | mejoran resultados, no habilitan el uso |
| El resto de F8 | crecimiento continuo |

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
| Dependencia de un solo coordinador humano | media | medio | tareas `coordinador` mínimas (3 en F0–F1); el resto, autónomas |

## 8. Decisiones abiertas

| Decisión | Propuesta | Necesita |
|---|---|---|
| Licencia | Apache-2.0 | confirmación del propietario |
| Nombre del CLI | `pcode` | confirmación |
| Repo público | desde F3 | confirmación |
| Planificador por defecto | Claude Code (Opus) | presupuesto de API |
| Copilot coding agent | sí, para tareas `aislado` asíncronas | plan de Copilot con coding agent habilitado |

## 9. Primeros pasos (esta semana)

1. Coordinador: F0.03 (verificar CLIs) y F0.04 (banco PublicBench y línea base con opencode).
2. Haiku 5.5: F0.01 → F0.02 → F0.06, y después F1.01, F1.02, F1.08 y F1.19 en paralelo (no dependen
   entre sí más allá de F0.01).
3. Revisar el primer informe de Haiku y ajustar el formato de tarea si hace falta, antes de lanzar
   el resto de F1.
