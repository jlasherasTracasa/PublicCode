# Interfaces: terminal, escritorio, web, IDE, móvil y GitHub

## Principio: un núcleo, muchos clientes

Toda la lógica vive en el demonio **`pcoded`**: flujos, backends, sandboxes, política, skills y
memoria. Los clientes solo **presentan el estado y envían intenciones** por una API única, definida
en `crates/pcode-protocol` (JSON-RPC 2.0 más eventos). Consecuencias:

* Una sesión empezada en la terminal se sigue en el escritorio, y una aprobación pendiente se
  resuelve desde el móvil.
* Varios clientes a la vez sobre el mismo proyecto sin conflictos: el demonio es la única fuente de verdad.
* Las mismas palabras en todas partes: sesión, tarea, backend, sandbox, cambios, aprobación, skill.

```
                       ┌──────────────── pcoded (un proceso por usuario) ─────────────────┐
 pcode (CLI) ──────────┤                                                                  │
 pcode tui ────────────┤  socket Unix (Linux) · TCP en loopback con token (Windows) │
 escritorio (Tauri) ───┤                                                                  │
 navegador / PWA ──────┤  HTTP + SSE con token (local o por túnel SSH/Tailscale)          │
 IDE (ACP / VS Code) ──┤                                                                  │
 GitHub ───────────────┤  API de GitHub (PR, comentarios, checks)                         │
                       └──────────────────────────────────────────────────────────────────┘
```

## Qué ofrece cada cliente

| Capacidad | CLI | TUI | Escritorio | Web | Móvil (PWA) | IDE |
|---|---|---|---|---|---|---|
| Lanzar una tarea o una sesión de chat | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| Ver en vivo agentes y subagentes en paralelo | resumen | ✓ | ✓ | ✓ | estado | ✓ |
| Revisar diffs de la sandbox y aplicar o descartar | ✓ | ✓ por archivo | ✓ por bloque | ✓ por bloque | — | ✓ |
| Aprobar o denegar acciones | ✓ | ✓ | ✓ + notificación | ✓ | ✓ + notificación | ✓ |
| Tablero de flujos (tareas, intentos, integración, CI) | `pcode flow estado` | ✓ | ✓ | ✓ | estado | — |
| Skills (activas, cuarentena con evidencia) | ✓ | lista | ✓ | ✓ | — | — |
| Banco y métricas por backend | `pcode bench informe` | — | ✓ gráficas | ✓ gráficas | — | — |
| Ajustes: backends, proveedores, política, credenciales | `pcode.toml` | — | ✓ (llavero del sistema) | ✓ (sin credenciales) | — | — |
| Scripts y CI | ✓ `--formato json` | — | — | — | — | — |

## Terminal

### CLI (`pcode …`)

| Orden | Para qué |
|---|---|
| `pcode` | en una TTY abre la TUI; si no, muestra la ayuda |
| `pcode run "<tarea>" [--backend X] [--sandbox N1] [--acepta CMD] [--formato json\|stream-json]` | una tarea de principio a fin |
| `pcode flow run\|estado\|parar` | flujos de tareas (DAG, aceptación, reintentos e integración) |
| `pcode sandbox ejecutar\|cambios\|aplicar\|descartar` | mini sandboxes a mano |
| `pcode skills listar\|exportar\|cuarentena` | skills |
| `pcode instrucciones sincronizar [--comprobar]` | AGENTS.md → CLAUDE.md, GEMINI.md, copilot-instructions |
| `pcode bench …` | PublicBench |
| `pcode daemon` · `pcode mcp` · `pcode acp` | servidores |
| `pcode completions bash\|zsh\|fish\|powershell` | autocompletado |

Reglas: salida legible por defecto, `--formato json` y `stream-json` para máquinas, códigos de
salida estables, nada interactivo sin TTY y `NO_COLOR` respetado.

### TUI interactiva (`pcode` o `pcode tui`)

```
┌ Sesiones / agentes ────┬ Sesión: «arreglar test de sinvirtio» ───────────┬ Cambios (sandbox s-12) ─┐
│ ● s-12 GLM   ⏳ paso 7  │ tú › arregla el test que falla en sinvirtio      │ M lib/sinvirtio/src/q.rs │
│   └ explorador ✓        │ glm › leo lib/sinvirtio/src/queue.rs …           │ A lib/sinvirtio/tests/…  │
│ ○ s-11 Claude ✓ hecha   │   ▸ leer queue.rs (210 líneas)                   │                          │
│ ⚠ s-10 Copilot ⧗ aprob. │   ▸ bash cargo test -p sinvirtio  ✗ exit=101     │ [a] aplicar [d] descartar│
│                         │ glm › el índice de anillo no hace wrap …         ├ Aprobaciones (1) ────────┤
│ Flujo: 3 ⏳ 41 ✓        │   ▸ editar queue.rs  ✓                           │ s-10 git push origin …   │
│                         │                                                  │ procedencia: agente:s-10 │
│                         │                                                  │ [p] permitir [n] negar   │
├─────────────────────────┴──────────────────────────────────────────────────┴──────────────────────────┤
│ › /backend claude  @lib/sinvirtio/src/queue.rs  explica por qué falla_                                 │
├────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ GLM-5.3-Flash · sandbox N1 sin red · 41k↑ 3k↓ tokens · acepta: cargo test -p sinvirtio · F1 ayuda      │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

* **Entrada**: multilínea (Mayús+Intro); `@` menciona archivos con autocompletado difuso; `/` abre
  órdenes; `!` ejecuta una orden de shell en la sandbox y adjunta su salida.
* **Órdenes**: `/nuevo`, `/sesiones`, `/backend <x>`, `/modelo <m>`, `/rol <r>`, `/sandbox N1|ninguna`,
  `/acepta <cmd>`, `/cambios`, `/aplicar`, `/descartar`, `/compactar`, `/skills`, `/subagente <rol> <tarea>`,
  `/flujo`, `/ayuda` y `/salir`.
* **Multiagente**: árbol de sesiones y subagentes con estado (`⏳ ✓ ✗ ⚠ ⧗`) y salto entre ellos;
  panel lateral conmutables (cambios, aprobaciones, tareas del flujo, agentes).
* **Revisión**: diff por archivo con resaltado; aplicar o descartar por archivo; abrir en `$EDITOR`.
* **Atajos** configurables en `keybindings.toml`; temas claro, oscuro y alto contraste; funciona a
  80×24 (los paneles se pliegan).

## Escritorio (Tauri 2: Linux y Windows)

Es el **centro de control** del trabajo multiagente. Carga la misma UI que la web (`apps/ui`) y lanza
`pcoded` como sidecar.

| Pantalla | Contenido |
|---|---|
| **Inicio** | proyectos recientes (arrastrar una carpeta la abre); estado del demonio; backends detectados (Claude Code, Copilot, Gemini, opencode, LiteLLM) con versión y si la sesión está iniciada |
| **Sesión** | chat con transcripción en vivo (tools plegables, salidas comprimidas), selector de backend, modelo y rol, panel de cambios de la sandbox |
| **Centro de control** | tablero de tareas por estado (pendiente → en marcha → aceptada → integrada, más bloqueadas); cada tarjeta muestra backend, intento, sandbox y tokens; árbol de subagentes; registro en vivo |
| **Revisión** | diff lado a lado o unificado; revisión cruzada del otro backend; salida de `acepta`; aprobar, aplicar, descartar o abrir en el editor |
| **Aprobaciones** | cola con la acción, el motivo, la procedencia y el contexto; permitir una vez o siempre para este patrón |
| **Skills** | activas, en cuarentena (con su `EVIDENCIA.md`) y retiradas; exportar a cada agente |
| **Banco** | aceptación a la primera, tokens y tiempo por backend; evolución nocturna |
| **Ajustes** | backends (detectar, iniciar sesión, rol por defecto), proveedores (URL de LiteLLM), política, sandbox, atajos, tema e idioma; **credenciales en el llavero del sistema** |

Integración con el sistema: icono en la bandeja que indica si hay agentes en marcha y aprobaciones
pendientes; notificaciones nativas (tarea aceptada o fallida, aprobación necesaria); enlaces
`pcode://sesion/<id>` y `pcode://tarea/<id>`; una sola instancia; arranque con el sistema opcional;
actualizaciones firmadas.

## Web

`pcoded` sirve la misma UI en `http://127.0.0.1:<puerto>` con un token de un solo uso. Es el modo
para **servidores sin escritorio**: por ejemplo, un servidor de desarrollo, accesible por túnel
SSH (`ssh -L`) o Tailscale. Sin credenciales en el navegador: los ajustes de credenciales solo están
en el escritorio o en `pcode.toml` con `{env:…}`.

## Móvil (PWA)

La web como PWA instalable, con vistas adaptadas: estado de flujos y sesiones, **aprobaciones** y
notificaciones. Sin edición ni diffs largos: el móvil sirve para no bloquear a los agentes cuando no
estás delante del ordenador.

**APK de Android** (`apps/android`, [ADR-0006](adr/ADR-0006-apk-android.md)): mientras la PWA no tenga
TLS, la release publica un APK que muestra la web del demonio en un WebView. Arranca el demonio con
`pcode daemon --escuchar 0.0.0.0` (o la IP del túnel) y pega en la app la URL que imprime.

## IDE

* **ACP** (Agent Client Protocol): `pcode acp` convierte a PublicCode en un agente para los editores
  con ACP: sesión, streaming, permisos y diffs.
* **VS Code**: extensión fina con un webview que carga `apps/ui` y órdenes como «PublicCode: tarea con
  la selección» o «abrir cambios de la sandbox».
* **MCP**: los agentes del editor (Copilot en VS Code, Claude Code, Gemini) usan las tools de
  PublicCode (sandbox, aceptar, búsqueda) vía `pcode mcp`.

## GitHub

Modo «PR por tarea»: PR con diff, informe y salida de `acepta`; comentario con la revisión cruzada;
check run con el resultado. Copilot coding agent como backend asíncrono (issue → PR → verificación
local).

## Experiencia común

* **Teclado primero** en todos los clientes; los atajos se anuncian en la UI.
* **Accesibilidad**: contraste AA, foco visible, roles ARIA y lectores de pantalla en la UI; símbolos
  además de colores en la TUI.
* **i18n**: español e inglés desde el principio (catálogos), con el idioma del sistema por defecto.
* **Privacidad**: sin telemetría externa; todo es local salvo las APIs de los backends que elijas.
* **Rendimiento**: TUI < 16 ms por fotograma; UI con primer pintado < 1 s; escritorio < 150 MB en
  reposo; demonio < 50 MB en reposo.

## Plataformas y empaquetado

| Plataforma | Escritorio | CLI y TUI | Sandbox por defecto |
|---|---|---|---|
| Linux | `.deb`, `.rpm`, AppImage | binario estático o paquete | N1 |
| Windows | `.msi` | binario | WSL2 (N1 dentro de WSL) o N2 |

## Tareas

F5.01–F5.27 en [implementacion.md](implementacion.md): protocolo, demonio, aprobaciones, cliente
Rust, CLI para máquinas, TUI (estado, chat, órdenes, multiagente, atajos y temas), UI común (base,
chat, revisión, centro de control, ajustes, skills), escritorio (base, integración con el sistema,
credenciales, empaquetado, WSL), web servida, PWA, ACP, VS Code y GitHub. La F5 puede avanzar **en
paralelo** a F3 y F4: solo las vistas de flujos dependen de F3.
