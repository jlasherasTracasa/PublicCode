# Cambios

Formato: [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Versionado semántico.

## [1.1.0] — sin publicar

El agente trabaja mejor: localiza antes de leer, pregunta cuando hace falta, recuerda las decisiones del proyecto y
comprueba antes de terminar. Cada función sale de un paper de ACL/EMNLP 2026 ([docs/papers.md §12](docs/papers.md#12-lo-que-entra-en-la-11)).

### Añadido
- **Tool `esquema`** para el agente: símbolos de un archivo, archivos de una carpeta con sus símbolos principales y
  dónde se define un símbolo; el prompt pide localizar antes de leer (RepoNav, SHERLOC).
- **Preguntas de aclaración** (tool `preguntar`): en la TUI, el chat y Studio el turno termina con la pregunta y sus
  opciones; en un flujo, la tarea se detiene con la pregunta pendiente; 2 por petición (Ask or Assume?, AgentAsk).
- **Memoria del proyecto** (`.pcode/memoria.md`): `/recuerda`, `/memoria` y `/olvida` en la TUI, el chat y Studio;
  entra en el prompt de cada turno y sobrevive a la compactación; el agente propone notas con aprobación (SLUMP, AMIA).
  En Studio, menú ✦ → «Memoria del proyecto».
- **Verificar antes de terminar**: si el turno cambió archivos sin ejecutar nada, se le pide una vez que compruebe
  (Polished but Unresolved).
- **Coste de los esquemas MCP**: `pcode mcp` muestra los tokens por tool, por servidor y en total, y el chat lo dice al
  arrancar (ProMCP).

### Cambiado
- La compactación conserva literalmente restricciones, decisiones, rutas, órdenes y errores pendientes.
- Studio: la conversación ya no se reanima entera en cada trozo del streaming.
- `/recuerda`, `/memoria`, `/olvida`, `/deshacer` y `/rehacer` funcionan también desde el panel del agente de Studio.

## [1.0.0] — 2026-10-31

La primera versión estable: PublicCode sirve como herramienta de programación por defecto en Linux y Windows.

### Añadido
- **Deshacer lo que hace el agente**: puntos de control por turno (`/deshacer`, `/rehacer` en la TUI y el chat;
  «Deshacer el turno», Ctrl+Alt+Z y la vista Actividad en Studio) y resumen de archivos cambiados al final de cada turno.
- **Asistente ✦ en Studio**, basado en papers de ACL/EMNLP 2026: generar el mensaje de commit, revisar los cambios,
  arreglar con diagnóstico estructurado (desde Problemas, el hover o una orden fallida), concretar la petición,
  explicar y escribir tests.
- **Studio pro**: temas automático, claro, oscuro y medianoche; paneles redimensionables; migas de pan; vista Esquema;
  Ctrl+T y Ctrl+Mayús+O; marcas de git en el margen; barra de estado a lo ancho; avisos apilados; vista Actividad
  con la integridad de la auditoría.
- `pcode preguntar`: respuestas sin tools con `@archivo` y stdin (`git diff | pcode preguntar …`), texto o JSON.
- Medidor de contexto en la TUI y `/uso` con el contexto ocupado.
- Instrucción de ediciones mínimas en el prompt de sistema.
- `pcode --continuar` abre la TUI retomando la última conversación.
- Guía de instalación (`docs/instalacion.md`) y prueba de extremo a extremo de Studio (`tools/studio/e2e.js`).
- **LSP en Studio**: ir a la definición (F12, Ctrl+clic), referencias (Mayús+F12), información (Ctrl+K), errores
  subrayados y panel Problemas, con rust-analyzer, typescript-language-server, pyright, gopls o clangd
  (configurables en `[lsp.*]`).
- **Windows sin WSL**: mini sandbox N0 con `git worktree` para `pcode run --backend` y `pcode flow`.
- **Studio más cómodo**: pantalla de inicio, insignias por lenguaje, paleta de órdenes (Ctrl+Mayús+P),
  sugerencias del agente y animaciones suaves; tarjeta de bienvenida en la TUI.
- **Evals de seguridad** (20 casos de inyección y órdenes destructivas) y de cumplimiento de reglas, en la CI.
- Guía de usuario (`docs/usuario.md`) y procedencia firmada de los binarios de la release.
- Plan de la 1.x (fase F8, 36 tareas).

### Cambiado
- La capa de política por defecto impide escribir en `.git/` y `.pcode/`.
- Cancelar una sesión o una orden del terminal para también a los procesos que lanzó.
- Protocolo del demonio 1.0.0.

## [0.5.0] — 2026-10-31
### Añadido
- **PublicCode Studio** (`pcode studio`): explorador, pestañas, resaltado, buscar y reemplazar, Ctrl+P,
  búsqueda en el proyecto, git con diff y commit, terminal con tareas y el agente al lado.
- Sesiones del demonio con varios turnos, streaming y aprobaciones (`pcode chat --formato jsonl`).
- Cliente **MCP** (`[mcp.*]`, `pcode mcp`).
- **Modo plan**, `@archivo` con autocompletado y `!orden` en la TUI y el chat.

## [0.4.0] — 2026-10-31
### Añadido
- Instalación en una línea (`install.sh`, `install.ps1`), `pcode doctor`, `pcode config` y `pcode actualizar`.
- Configuración global, reintentos ante 429/5xx, caché de prompt de Anthropic, compactación del contexto y
  capa de política segura por defecto; certificados del sistema para proxies de empresa.
- Diseño Material azul en la TUI, la web y el APK.

## [0.3.0] — 2026-10-30
### Añadido
- TUI a pantalla completa y streaming de texto en Anthropic, Gemini y OpenAI-compatible.

## [0.2.0] — 2026-10-30
### Añadido
- `pcode chat` y proveedores nativos de Anthropic y Gemini.

## [0.1.0] — 2026-10-30
### Añadido
- Núcleo (bucle del agente, política, auditoría, sandbox N1, backends externos, flujos), demonio con web,
  APK de Android, CI y release para Linux y Windows.
