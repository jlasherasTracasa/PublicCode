# Guía de instalación de PublicCode 1.0

Todo lo necesario para pasar de cero a usar `pcode` en **Linux** o **Windows**: instalador de una línea,
instalación manual con comprobación de sumas, compilación desde el código, primer arranque, modelos,
servidores de lenguaje para Studio, agentes externos, el APK de Android, proxies de empresa,
actualizar y desinstalar.

> **Resumen**
>
> | Sistema | Una línea |
> |---|---|
> | Linux x86_64 | `curl -fsSL https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.sh \| sh` |
> | Windows 10/11 (PowerShell) | `irm https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.ps1 \| iex` |
>
> Después: define una clave de modelo (`ANTHROPIC_API_KEY`, `GEMINI_API_KEY` u `OPENAI_API_KEY`), abre una
> terminal nueva y ejecuta `pcode doctor`.

## Índice

1. [Requisitos](#1-requisitos)
2. [Linux: instalador de una línea](#2-linux-instalador-de-una-línea)
3. [Windows: instalador de una línea](#3-windows-instalador-de-una-línea)
4. [Instalación manual con comprobación de sumas](#4-instalación-manual-con-comprobación-de-sumas)
5. [Compilar desde el código](#5-compilar-desde-el-código)
6. [Primer arranque](#6-primer-arranque)
7. [Elegir modelo y proveedor](#7-elegir-modelo-y-proveedor)
8. [Servidores de lenguaje para Studio](#8-servidores-de-lenguaje-para-studio)
9. [Agentes externos (opcional)](#9-agentes-externos-opcional)
10. [Mini sandboxes en Linux y Windows](#10-mini-sandboxes-en-linux-y-windows)
11. [App de Android](#11-app-de-android)
12. [Detrás de un proxy de empresa](#12-detrás-de-un-proxy-de-empresa)
13. [Actualizar y desinstalar](#13-actualizar-y-desinstalar)
14. [Problemas frecuentes](#14-problemas-frecuentes)

## 1. Requisitos

| Qué | Linux | Windows |
|---|---|---|
| Sistema | x86_64 con glibc 2.35 o superior (Ubuntu 22.04+, Debian 12+, Fedora 36+…) | Windows 10 u 11 de 64 bits |
| Permisos | ninguno especial: se instala en tu usuario | ninguno especial: se instala en tu usuario |
| `git` | muy recomendable (Studio, marcas del margen, commits, sandbox N0) | muy recomendable (Git for Windows) |
| Navegador | cualquiera moderno, para `pcode studio` | Edge, Chrome o Firefox |
| Una clave de modelo | Anthropic, Gemini u OpenAI-compatible (LiteLLM, vLLM, Ollama…) | la misma |
| Sandbox N1 (opcional) | namespaces de usuario sin privilegios (cualquier distribución moderna) | dentro de WSL2; fuera de WSL se usa N0 |

Nada más: `pcode` es un solo binario sin dependencias en tiempo de ejecución.

## 2. Linux: instalador de una línea

```bash
curl -fsSL https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.sh | sh
```

Qué hace, paso a paso (puedes leer [install.sh](../install.sh) antes de ejecutarlo):

1. Comprueba que es Linux x86_64.
2. Averigua la última versión publicada (o la de `PCODE_VERSION`).
3. Descarga `pcode-<versión>-x86_64-unknown-linux-gnu.tar.gz` y `SHA256SUMS` de la release.
4. **Comprueba el SHA-256**: si no coincide, para sin instalar nada.
5. Copia `pcode` en `~/.local/bin` (o en `PCODE_DIR`) de forma atómica, así puedes actualizar con `pcode` abierto.
6. Si esa carpeta no está en el `PATH`, añade una línea a `~/.bashrc`, `~/.zshrc` y `~/.profile` (los que existan).

| Variable | Para qué | Ejemplo |
|---|---|---|
| `PCODE_VERSION` | instalar una versión concreta | `PCODE_VERSION=v1.0.0` |
| `PCODE_DIR` | otra carpeta de instalación | `PCODE_DIR=/opt/pcode/bin` |
| `PCODE_REPO` | instalar desde un fork | `PCODE_REPO=mi-usuario/PublicCode` |

```bash
curl -fsSL https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.sh | PCODE_VERSION=v1.0.0 sh
```

## 3. Windows: instalador de una línea

En **PowerShell** (no hace falta abrirlo como administrador):

```powershell
irm https://raw.githubusercontent.com/jlasherasTracasa/PublicCode/main/install.ps1 | iex
```

Qué hace (puedes leer [install.ps1](../install.ps1) antes):

1. Descarga `pcode-<versión>-x86_64-pc-windows-msvc.zip` y `SHA256SUMS` de la release.
2. **Comprueba el SHA-256** con `Get-FileHash`; si no coincide, para.
3. Deja `pcode.exe` en `%LOCALAPPDATA%\PublicCode\bin` (si ya había uno abierto, lo aparta como `pcode.antiguo.exe`).
4. Añade esa carpeta al `PATH` **de tu usuario**: las terminales nuevas ya ven `pcode`.

Para una versión concreta: `$env:PCODE_VERSION = "v1.0.0"` antes de la orden. Si PowerShell bloquea scripts
descargados, el instalador de una línea funciona igual porque no se guarda como archivo; para el `install.ps1`
del `.zip`, usa `powershell -ExecutionPolicy Bypass -File install.ps1`.

## 4. Instalación manual con comprobación de sumas

Descarga de la [página de releases](https://github.com/jlasherasTracasa/PublicCode/releases) el archivo de tu
sistema y `SHA256SUMS`:

| Archivo | Para |
|---|---|
| `pcode-<versión>-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64 (trae `pcode` e `install.sh`) |
| `pcode-<versión>-x86_64-pc-windows-msvc.zip` | Windows 10/11 x64 (trae `pcode.exe` e `install.ps1`) |
| `pcode-<versión>-android.apk` | Android 8+: cliente del demonio para el móvil |
| `SHA256SUMS` | las sumas de los tres |

Comprueba la suma y ejecuta el instalador que viene dentro (usa el binario del paquete, sin descargar nada más):

```bash
sha256sum -c SHA256SUMS --ignore-missing
```

```bash
tar xzf pcode-v1.0.0-x86_64-unknown-linux-gnu.tar.gz && sh install.sh
```

```powershell
(Get-FileHash pcode-v1.0.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash.ToLower()
```

Compara el resultado con la línea del `.zip` en `SHA256SUMS`, descomprime y ejecuta `install.ps1`.

La release publica además, cuando GitHub lo permite en el repositorio, la **procedencia firmada** de los
binarios (attestation de GitHub). Si está, se comprueba con la CLI de GitHub:

```bash
gh attestation verify pcode-v1.0.0-x86_64-unknown-linux-gnu.tar.gz --repo jlasherasTracasa/PublicCode
```

## 5. Compilar desde el código

Necesitas [rustup](https://rustup.rs); la versión de Rust está fijada en `rust-toolchain.toml` y se instala sola.

```bash
git clone https://github.com/jlasherasTracasa/PublicCode.git
```

```bash
cd PublicCode && cargo install --path crates/pcode-cli --locked
```

El binario queda en `~/.cargo/bin/pcode` (`%USERPROFILE%\.cargo\bin\pcode.exe` en Windows). Para desarrollar,
`cargo build -p pcode-cli` y `target/debug/pcode`; las comprobaciones de la CI están en [AGENTS.md](../AGENTS.md#4-cómo-comprobar-según-el-área).

## 6. Primer arranque

1. **Una clave de modelo** en el entorno (nunca en archivos):

   ```bash
   export ANTHROPIC_API_KEY=...
   ```

   ```powershell
   [Environment]::SetEnvironmentVariable("ANTHROPIC_API_KEY", "...", "User")
   ```

2. **Comprueba la instalación**. `pcode doctor` revisa la versión, la configuración, el modelo y su clave, git,
   la sandbox disponible y los agentes externos instalados, y dice qué falta (`pcode mcp` revisa los servidores MCP):

   ```bash
   pcode doctor
   ```

3. **(Opcional) configuración global comentada**, con proveedor, modelo, tema y contexto:

   ```bash
   pcode config init
   ```

4. **A trabajar**, en la carpeta de tu proyecto:

   | Quieres… | Orden |
   |---|---|
   | la TUI a pantalla completa | `pcode` |
   | el editor en el navegador | `pcode studio` |
   | una pregunta rápida sin tools | `pcode preguntar "¿qué hace @src/main.rs?"` |
   | un mensaje de commit desde el diff | `git diff --cached \| pcode preguntar "escribe el mensaje de commit"` |
   | una tarea para scripts o CI | `pcode run "arregla el test" --acepta "cargo test"` |

La [guía de usuario](usuario.md) explica cada interfaz con detalle.

## 7. Elegir modelo y proveedor

Sin `pcode.toml`, `pcode` usa la primera clave que encuentra en el entorno:

| Variable | Proveedor | Modelo por defecto |
|---|---|---|
| `ANTHROPIC_API_KEY` | Anthropic (Messages API, con caché de prompt) | el de `[preferencias] modelo` o el del proveedor |
| `GEMINI_API_KEY` o `GOOGLE_API_KEY` | Gemini (`generateContent`, streaming SSE) | ídem |
| `OPENAI_API_KEY` + `OPENAI_BASE_URL` | cualquier API OpenAI-compatible | ídem |

Modelos locales u on-premise con la API OpenAI-compatible:

| Servidor | `OPENAI_BASE_URL` |
|---|---|
| Ollama | `http://127.0.0.1:11434/v1` |
| LM Studio | `http://127.0.0.1:1234/v1` |
| vLLM | `http://127.0.0.1:8000/v1` |
| LiteLLM (proxy de varios modelos) | la URL de tu proxy, p. ej. `http://127.0.0.1:4000/v1` |

Para fijar proveedor y modelo, en `~/.config/pcode/pcode.toml` (Linux) o `%APPDATA%\pcode\pcode.toml` (Windows):

```toml
[preferencias]
proveedor = "anthropic"
modelo = "claude-sonnet-5-5"

[proveedores.anthropic]
tipo = "anthropic"
clave = "{env:ANTHROPIC_API_KEY}"    # {env:…}: la clave nunca se escribe en el archivo
```

`pcode chat --modelo …`, `PCODE_MODEL` y `/modelo` en la TUI cambian el modelo sin tocar la configuración.

## 8. Servidores de lenguaje para Studio

Studio usa un servidor de lenguaje (LSP) por lenguaje para ir a la definición, referencias, información del
símbolo y errores en línea. Si falta, la barra de estado muestra **LSP ✗** y el editor sigue funcionando
(resaltado, esquema, búsqueda y el agente no dependen del LSP).

| Lenguaje | Servidor que busca | Cómo instalarlo |
|---|---|---|
| Rust | `rust-analyzer` | `rustup component add rust-analyzer` |
| TypeScript / JavaScript | `typescript-language-server --stdio` | `npm i -g typescript typescript-language-server` |
| Python | `pyright-langserver --stdio` | `npm i -g pyright` (o `pip install pyright`) |
| Go | `gopls` | `go install golang.org/x/tools/gopls@latest` |
| C / C++ | `clangd` | el paquete `clangd` de tu sistema o LLVM |

Para usar otro servidor, en `pcode.toml`:

```toml
[lsp.python]
orden = ["pylsp"]
```

## 9. Agentes externos (opcional)

`pcode` trae su propio agente. Además puede lanzar otros agentes como subagentes en una sandbox
(`pcode run --backend …` y `pcode flow`). Instálalos si los quieres usar; `pcode doctor` dice cuáles encuentra.

| Agente | Binario | `--backend` |
|---|---|---|
| Claude Code | `claude` | `claude-code` |
| Gemini CLI | `gemini` | `gemini` |
| GitHub Copilot CLI | `copilot` | `copilot` |
| opencode | `opencode` | `opencode` |

Cada agente usa **tus** credenciales y su vía oficial; PublicCode no comparte suscripciones.

## 10. Mini sandboxes en Linux y Windows

| Nivel | Dónde | Qué aísla | Cómo se elige |
|---|---|---|---|
| **N1** | Linux (y WSL2) con namespaces de usuario | disco (overlay copy-on-write), red y procesos | automático si el sistema lo permite |
| **N0** | Windows sin WSL, o cuando no hay namespaces | solo separa los cambios en un `git worktree` | automático, o `PCODE_SANDBOX=N0`, o `[sandbox] nivel_por_defecto = "N0"` |

Algunas distribuciones desactivan los namespaces de usuario (p. ej. Ubuntu 24.04 con AppArmor). `pcode doctor`
lo detecta; para activarlos, `sudo sysctl kernel.apparmor_restrict_unprivileged_userns=0` o usa N0.

## 11. App de Android

1. Instala `pcode-<versión>-android.apk` desde la release (permite «instalar apps de origen desconocido» para tu
   navegador o gestor de archivos).
2. En el equipo, arranca el demonio escuchando en la red local, con un token fijo para no volver a emparejar:

   ```bash
   PCODE_DAEMON_TOKEN=$(openssl rand -hex 24) pcode daemon --escuchar 0.0.0.0
   ```

3. Pega en la app la URL `http://IP:7878/?t=…` que imprime. Verás las sesiones en vivo y podrás aprobar o
   denegar lo que pida el agente.

Sin TLS todavía: úsalo en tu red o tras un túnel (WireGuard, Tailscale).

## 12. Detrás de un proxy de empresa

`pcode` usa los **certificados del sistema**, así que funciona con la CA corporativa si está instalada en el
sistema. Define el proxy como siempre:

```bash
export HTTPS_PROXY=http://proxy.empresa:8080
```

`pcode actualizar` e `install.sh` (con `curl` o `wget`) respetan `HTTPS_PROXY`; en Windows, `install.ps1` usa el
proxy configurado en el sistema.

## 13. Actualizar y desinstalar

```bash
pcode actualizar              # descarga e instala la última versión, comprobando el SHA-256
```

```bash
pcode actualizar --comprobar  # solo avisa si hay versión nueva
```

Para desinstalar, borra el binario y, si quieres, la configuración y el estado:

| Qué | Linux | Windows |
|---|---|---|
| binario | `~/.local/bin/pcode` | `%LOCALAPPDATA%\PublicCode\` |
| configuración global | `~/.config/pcode/` | `%APPDATA%\pcode\` |
| estado de cada proyecto (conversaciones, auditoría, puntos de control) | `.pcode/` en el proyecto | `.pcode\` en el proyecto |
| línea del `PATH` | `# PublicCode` en `~/.bashrc`, `~/.zshrc`, `~/.profile` | `PATH` del usuario |

`.pcode/` se añade solo a `.git/info/exclude`, así que nunca acaba en tus commits.

## 14. Problemas frecuentes

| Síntoma | Causa probable | Solución |
|---|---|---|
| `pcode: command not found` / no se reconoce | la terminal no ve el nuevo `PATH` | abre una terminal nueva o ejecuta la línea `export PATH=…` que imprimió el instalador |
| «no hay ningún modelo configurado» | no hay clave en el entorno ni proveedor en `pcode.toml` | define `ANTHROPIC_API_KEY` (u otra) o `pcode config init` |
| `invalid peer certificate` | proxy con CA propia | instala la CA de la empresa en el sistema y define `HTTPS_PROXY` |
| `La suma SHA-256 … no coincide` | descarga corrupta o manipulada | vuelve a intentarlo; si persiste, no instales y abre un issue |
| `GLIBC_2.35 not found` | distribución anterior a 2022 | compila desde el código (sección 5) |
| «LSP ✗» en Studio | falta el servidor de lenguaje | instálalo (sección 8) o configúralo en `[lsp.*]` |
| `--backend` o `flow` sin aislamiento en Windows | Windows sin WSL usa N0 | WSL2 para N1 |
| el navegador no se abre con `pcode studio` | sin entorno gráfico (SSH) | abre a mano la URL que imprime, o usa un túnel SSH (`ssh -L`) |
| cualquier otra cosa | — | `pcode doctor` |
