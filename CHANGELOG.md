# Cambios

Formato: [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Versionado semántico.

## [1.0.0] — 2026-10-31

La primera versión estable: PublicCode sirve como herramienta de programación por defecto en Linux y Windows.

### Añadido
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
