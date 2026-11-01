# Guía de usuario de PublicCode 1.1

PublicCode (`pcode`) es tu herramienta de programación con agentes: una TUI en la terminal, un editor en
el navegador (**PublicCode Studio**), una app Android para aprobar desde el móvil y un motor que impone
reglas de seguridad a cualquier agente: el propio, Claude Code, Copilot, Gemini u opencode.

## 1. Instalar

| Sistema | Orden |
|---|---|
| Linux x86_64 | `curl -fsSL https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.sh \| sh` |
| Windows 10/11 (PowerShell) | `irm https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.ps1 \| iex` |
| Desde el código | `cargo install --path crates/pcode-cli --locked` |

Los instaladores son por usuario (sin administrador), comprueban el SHA-256 de la release y añaden
`pcode` al PATH. Para actualizar más adelante: `pcode actualizar`.

## 2. Primer uso

1. Define una clave de modelo en el entorno: `ANTHROPIC_API_KEY`, `GEMINI_API_KEY` (o `GOOGLE_API_KEY`) u
   `OPENAI_API_KEY` (con `OPENAI_BASE_URL` para LiteLLM, vLLM u Ollama).
2. Comprueba que todo está bien con `pcode doctor`.
3. En la carpeta de tu proyecto (mejor si es un repositorio git), ejecuta `pcode`.

| Quieres… | Orden |
|---|---|
| la TUI a pantalla completa | `pcode` (o `pcode tui`) |
| el editor en el navegador | `pcode studio` |
| una conversación en modo línea o por stdin | `pcode chat` |
| una tarea suelta para scripts y CI | `pcode run "…" --acepta "cargo test"` |
| una pregunta rápida, sin tools ni cambios | `pcode preguntar "¿qué hace @src/main.rs?"` |
| algo a partir de una salida | `git diff --cached \| pcode preguntar "escribe el mensaje de commit"` |
| retomar la última conversación | `pcode --continuar` o `pcode chat --continuar` |

La instalación detallada (manual, desde el código, modelos locales, LSP, Android, proxies, desinstalar) está en
[instalacion.md](instalacion.md).

## 3. Configurar

| Archivo | Dónde | Para |
|---|---|---|
| global | `~/.config/pcode/pcode.toml` (Linux), `%APPDATA%\pcode\pcode.toml` (Windows); `pcode config init` lo crea | proveedores, modelo, tema y servidores MCP para todos los proyectos |
| proyecto | `pcode.toml` en la raíz | lo propio del repositorio; manda sobre el global |

```toml
[preferencias]
proveedor = "anthropic"
modelo = "claude-sonnet-5-5"
tema = "azul"              # azul, claro, oscuro, alto-contraste
contexto_max = 160000      # a partir de aquí la conversación se compacta sola

[proveedores.anthropic]
tipo = "anthropic"
clave = "{env:ANTHROPIC_API_KEY}"   # las claves nunca se escriben en el archivo

[mcp.github]
orden = ["npx", "-y", "@modelcontextprotocol/server-github"]
entorno = { GITHUB_PERSONAL_ACCESS_TOKEN = "{env:GITHUB_TOKEN}" }

[lsp.python]
orden = ["pylsp"]          # otro servidor de lenguaje

[politica]
preguntar_ordenes = ["docker *"]
denegar_ordenes = ["git push --force*"]
```

### 3.1 Modelos desde la pantalla de Ajustes o desde la terminal

No hace falta editar el TOML a mano. En Studio, **Ajustes** (el engranaje de la barra lateral, Ctrl+, o el modelo de la
barra de estado):

| Qué | Cómo |
|---|---|
| Añadir un proveedor | una plantilla (Anthropic, Gemini, OpenAI, OpenRouter, Ollama, LM Studio, vLLM, LiteLLM) u «Otro (URL propia)» para cualquier API compatible con OpenAI |
| Probar la conexión | «⚡ Probar conexión» lista los modelos de esa URL; un clic en uno lo elige |
| La clave | se indica la **variable de entorno** que la contiene (p. ej. `OPENROUTER_API_KEY`); la tarjeta dice si está definida (✓) o no (✗). La clave no se escribe en ningún archivo |
| Modelos locales | Ajustes busca solo Ollama, LM Studio y vLLM en este equipo y ofrece añadirlos |
| Predeterminado | «★ Usar por defecto»; para una conversación concreta, Opciones → modelo en el panel del agente |
| Dónde se guarda | «Este equipo» (configuración global, todos los proyectos) o «Solo este proyecto» (`pcode.toml`) |

Desde la terminal, lo mismo:

```bash
pcode config proveedor ollama --modelo qwen3 --predeterminado   # una plantilla conocida rellena URL y variable
pcode config proveedor empresa --url https://llm.empresa.es/v1 --clave-env EMPRESA_API_KEY
pcode config proveedores --probar --detectar                     # estado, modelos y servidores locales
pcode config quitar empresa
```

Ambos editan el archivo conservando tus comentarios. Un proveedor cuya variable no esté definida no estropea el
resto: solo falla si lo eliges, y dice qué variable falta.

**Agentes con tu cuenta: GitHub Copilot, Claude Code, Gemini CLI y opencode.** Ajustes los enseña aparte (también
la plantilla «GitHub Copilot» lleva a su tarjeta). Son agentes completos que usan **tu** suscripción con su CLI
oficial, no APIs por URL:

| Qué | Cómo |
|---|---|
| ¿Está instalado? | la tarjeta dice ✓ Instalado (con la ruta) o ✗ Sin instalar, con la orden para instalarlo y «Copiar» |
| Instalar Copilot | Linux: `npm install -g @github/copilot` (Node.js 22 o posterior) · Windows: `winget install GitHub.Copilot` |
| Iniciar sesión en Copilot | abre `copilot` y escribe `/login`; o define `COPILOT_GITHUB_TOKEN` (o `GH_TOKEN`) con un token *fine-grained* de tu cuenta con el permiso «Copilot Requests». Necesita una suscripción de Copilot |
| Usarlo | «Usar en una conversación», o Opciones → modelo → *Agentes con tu cuenta* en el panel del agente; el campo «Modelo del agente» es opcional (vacío = el suyo) |
| Qué pasa al enviar | el agente trabaja en una sandbox; si termina bien, sus cambios se aplican al proyecto con un punto de control: «Deshacer» en Actividad (o `/deshacer`) los revierte |
| Seguir la conversación | escribe otro mensaje: el agente se relanza con lo hablado hasta ahora como contexto |
| Desde la terminal | `pcode run "…" --backend copilot --aplicar` (`--modelo` para elegir modelo) · `pcode config proveedores` lista los agentes y su estado |

GitHub Models, la API oficial compatible con OpenAI para los modelos de Copilot, se retiró el 30 de julio de 2026;
por eso Copilot no se añade como proveedor por URL. `pcode config proveedor copilot` explica esto mismo y cómo usarlo.

`pcode config mostrar` enseña la configuración que se usaría (sin claves) y `pcode mcp` las tools que aporta
cada servidor MCP, con los **tokens que ocupan sus esquemas** en cada petición al modelo (y el total, frente al de
las tools nativas). Los esquemas viajan en todas las peticiones, así que un servidor con muchas tools encarece toda
la conversación: por encima de 4 000 tokens, `pcode mcp` lo avisa; desactiva lo que no uses con `desactivado = true`.

## 4. La TUI

| Tecla u orden | Qué hace |
|---|---|
| texto e Intro | manda el mensaje; la respuesta llega en streaming |
| Alt+Intro, o `\` al final | nueva línea |
| `@archivo` | adjunta el archivo (al escribir `@` salen sugerencias; Tab completa) |
| `!orden` | ejecuta en el proyecto; la salida va como contexto del siguiente mensaje |
| Mayús+Tab, `/plan`, `/construir` | modo plan (solo lee y propone) o modo normal |
| Esc o Ctrl+C | interrumpe al agente (con el agente parado, Ctrl+C sale) |
| Ctrl+O | despliega la salida completa de las tools |
| ↑ ↓ · PgUp PgDn | historial · desplazar |
| `/deshacer`, `/rehacer` | deja los archivos como estaban antes del último turno del agente, o lo vuelve a aplicar |
| `/recuerda <texto>`, `/memoria`, `/olvida <n>` | guarda una decisión en la memoria del proyecto, la muestra numerada o borra la nota `n` |
| `/modelo`, `/nuevo`, `/compactar`, `/uso`, `/ayuda`, `/salir` | cambiar de modelo, empezar de nuevo, resumir, tokens y contexto, ayuda y salir |

Al terminar cada turno, la TUI resume lo que cambió (`✎ 2 archivos · src/a.rs +3 −1 · /deshacer lo revierte`) y la
barra de estado lleva un **medidor de contexto** (`▰▰▱▱▱▱▱▱ contexto 25 %`, en amarillo desde el 75 %): al 100 %
la conversación se compacta sola. `/deshacer` y `/rehacer` también funcionan en `pcode chat`.

### 4.1 Cómo trabaja el agente (novedades de la 1.1)

| Qué | Cómo se ve | Por qué |
|---|---|---|
| **Localiza antes de leer** | llama a `esquema` (símbolos de un archivo, archivos de una carpeta con sus funciones y tipos, o dónde se define un símbolo) antes de abrir archivos enteros | gasta menos contexto y acierta antes con el sitio (RepoNav, SHERLOC) |
| **Pregunta si hace falta** | si tu petición es ambigua y la respuesta cambia lo que haría, termina el turno con `❓ pregunta` y sus opciones; contesta en el siguiente mensaje. Como mucho 2 preguntas por petición | preguntar en lugar de suponer (Ask or Assume?, AgentAsk) |
| **Recuerda decisiones** | lee la memoria del proyecto en cada turno; puede **proponer** guardar algo («recordar …») y te pide aprobación | las decisiones de la sesión no se pierden al compactar (SLUMP); la aprobación evita que un texto malicioso se cuele en la memoria (AMIA) |
| **Comprueba antes de terminar** | si cambió archivos sin ejecutar nada, PublicCode le pide una vez que pase los tests o la compilación (o que diga por qué no hace falta) | evita entregar algo pulido sin verificar (Polished but Unresolved) |

**La memoria del proyecto** es `.pcode/memoria.md`: una nota por línea, con fecha, la más reciente primero en el
prompt (hasta 8 KB). Úsala para lo que debe durar más que una conversación: «los commits, en inglés», «no añadir
dependencias», «la API v1 no se toca». Se gestiona con `/recuerda`, `/memoria` y `/olvida` (TUI, `pcode chat` y el
panel del agente de Studio) o editándola a mano (Studio: menú ✦ → «Memoria del proyecto»). No guardes secretos: entra
en el prompt que se envía al proveedor del modelo.

## 5. PublicCode Studio

`pcode studio` arranca el editor solo en tu equipo (127.0.0.1, con un token) y abre el navegador.

| Zona | Qué tiene |
|---|---|
| Inicio | proyecto, rama, cambios, estado del LSP, acciones rápidas, recientes y atajos |
| Explorador | árbol con estado de git e insignias por lenguaje; clic derecho para renombrar, copiar ruta, preguntar al agente o borrar |
| Editor | pestañas, resaltado, sangría automática, buscar y reemplazar (Ctrl+F), ir a línea (Ctrl+G) |
| Lenguaje (LSP) | ir a la definición (F12 o Ctrl+clic), referencias (Mayús+F12), información (Ctrl+K o pasar el ratón), errores subrayados y panel Problemas (Ctrl+Mayús+M) |
| Búsqueda | Ctrl+P abre archivos; Ctrl+Mayús+F busca en el proyecto (texto o regex) |
| Git | cambios y preparados, diff, preparar, commit |
| Terminal | Ctrl+\` ; botones para compilar y probar según el proyecto |
| Agente | Ctrl+I; Ctrl+L le lleva la selección con su archivo y líneas; las aprobaciones salen en el propio panel |
| Paleta | Ctrl+Mayús+P (o `>` en Ctrl+P) con todas las órdenes |
| Esquema y símbolos | vista Esquema (Ctrl+Mayús+O para saltar), **migas de pan** con el símbolo del cursor y Ctrl+T para cualquier símbolo del proyecto; en Ctrl+P, `@` símbolos del archivo, `#` del proyecto y `:` línea |
| Marcas de git | barras en el margen para líneas nuevas (verde), cambiadas (azul) y borradas (rojo); clic para ver el diff |
| Actividad | Ctrl+Mayús+Y: cada turno del agente con sus archivos (+/−), deshacer y rehacer, y las acciones auditadas con la integridad comprobada |
| Diseño | temas automático, claro, oscuro y medianoche (botón de la barra o paleta); paneles redimensionables con el ratón o el teclado (doble clic los restablece) |

### 5.1 El asistente ✦

Acciones de un clic que arman el contexto justo para el modelo. Cada una sale de un paper de ACL/EMNLP 2026
([papers.md §11](papers.md#11-experiencia-del-desarrollador-editor-git-y-revisión)).

| Acción | Dónde | Qué hace |
|---|---|---|
| ✦ Generar mensaje | panel Git | escribe el commit con el resumen por archivo, las funciones tocadas, tu estilo y el diff preparado |
| ✦ Revisar | panel Git, menú ✦, tarjeta del turno | abre un documento con la revisión del diff: errores, alcance, mantenibilidad, tests y seguridad |
| ✦ Arreglar | Problemas (uno o todos), hover de un error, orden fallida del terminal | manda al agente archivo, línea, símbolo, mensaje y código alrededor, pidiendo la edición mínima |
| ✦ Concretar | caja del agente | reescribe tu petición como objetivo, archivos probables, pasos, criterio de hecho y preguntas |
| Explicar · Escribir tests | menú ✦ y paleta | explica el archivo o la selección con su esquema; pide al agente tests en el estilo del proyecto |
| ↶ Deshacer el turno | tarjeta de cambios, Actividad, Ctrl+Alt+Z | revierte los archivos que tocó el último turno del agente (Ctrl+Alt+Mayús+Z rehace) |

Generar, revisar, concretar y explicar usan `pcode preguntar` (sin tools: no cambian nada); arreglar y escribir
tests van al agente, con sus aprobaciones.

Los servidores de lenguaje que usa: `rust-analyzer`, `typescript-language-server`, `pyright-langserver`,
`gopls` y `clangd`. Si falta alguno, Studio lo dice en la barra de estado («LSP ✗») con qué instalar.

## 6. Seguridad: qué puede hacer el agente

| Situación | Qué pasa |
|---|---|
| `rm -rf /`, `rm -rf ~`, `mkfs`, `dd` a un disco, `format C:`, `shutdown` | se deniega siempre |
| `git push`, `git reset --hard`, `git clean`, `rm -r`/`rm -f`, `rmdir /s`, `npm publish`, `terraform apply/destroy`… | te pregunta: `s` sí, `t` sí y no volver a preguntar, `n` no |
| escribir fuera del proyecto, en `.git/` o en `.pcode/` | se rechaza |
| tu `[politica]` | se suma a lo anterior y solo puede endurecerlo |
| cualquier acción | queda en `.pcode/audit.jsonl`, con hash encadenado |
| cada archivo que escribe el agente | se guarda antes en un punto de control del turno: `/deshacer` o «Deshacer el turno» lo restaura |
| la memoria del proyecto | el agente solo propone notas; se guardan si las apruebas y quedan marcadas «· agente» |

`--aprobar-todo` evita las preguntas (lo denegado sigue denegado). El modo plan no deja tocar nada. La eval
de seguridad que acompaña a cada versión está en [evaluacion.md](evaluacion.md#51-resultados-de-la-eval-de-seguridad-10).

## 7. Agentes externos, flujos y sandboxes

| Orden | Qué hace |
|---|---|
| `pcode run "…" --backend claude-code` | ejecuta Claude Code (o `gemini`, `copilot`, `opencode`, o un modelo) en una sandbox; los cambios no tocan tu repo |
| `… --aplicar` | aplica los cambios si el agente termina bien (y pasa `--acepta`, si se da), con punto de control para `/deshacer` |
| `… --acepta "cargo test" --aplicar-si-acepta` | solo aplica los cambios si la aceptación pasa |
| `pcode flow run` | ejecuta un plan de tareas (`tareas/*.md`) con dependencias, reintentos e integración en commits |
| `pcode sandbox …` | ejecutar, ver cambios, diff, aplicar o descartar a mano |

| Nivel | Dónde | Aísla |
|---|---|---|
| N1 | Linux con namespaces de usuario (y WSL2) | disco (overlay), red y procesos |
| N0 | Windows sin WSL, o `PCODE_SANDBOX=N0` | solo separa los cambios (`git worktree`): sin aislamiento de red ni disco |

## 8. Móvil

Instala `pcode-<versión>-android.apk` de la release. En el equipo: `pcode daemon --escuchar 0.0.0.0`
(con `PCODE_DAEMON_TOKEN` para un token fijo) y pega en la app la URL que imprime. Verás las sesiones y
podrás aprobar o denegar. Sin TLS todavía: úsalo en tu red o tras un túnel (WireGuard, Tailscale).

## 9. Qué se guarda y dónde

| Qué | Dónde |
|---|---|
| conversaciones | `.pcode/pcode.db` del proyecto (SQLite) |
| auditoría | `.pcode/audit.jsonl` |
| puntos de control (últimos 50 turnos) | `.pcode/checkpoints/` |
| memoria del proyecto | `.pcode/memoria.md` (entra en el prompt de cada turno) |
| configuración | archivo global y `pcode.toml` |
| claves | solo en variables de entorno; nunca en archivos |

`.pcode/` se añade solo a `.git/info/exclude`. PublicCode no envía telemetría: solo habla con el proveedor de
modelos que elijas y, al actualizar, con GitHub.

## 10. Problemas frecuentes

| Síntoma | Solución |
|---|---|
| «no hay ningún modelo configurado» | define una clave (sección 2) o `pcode config init` |
| `invalid peer certificate` tras un proxy de empresa | PublicCode usa los certificados del sistema: instala la CA de la empresa en el sistema y define `HTTPS_PROXY` |
| «LSP ✗» en Studio | instala el servidor que indica o configúralo en `[lsp.<lenguaje>]` |
| 429 o 5xx del proveedor | se reintenta solo con espera exponencial; si persiste, cambia de modelo con `/modelo` |
| la conversación se hace lenta o cara | `/compactar`, o baja `contexto_max` |
| `--backend` o `flow` en Windows | funcionan con N0; para aislamiento completo, WSL2 |
| cualquier otra cosa | `pcode doctor` |
