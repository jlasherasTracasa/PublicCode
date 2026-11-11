# PublicCode en equipos con pocos recursos

Medido en un portátil de **2 núcleos (Gemini Lake) y 3,6 GB de RAM**, con VS Code y un navegador abiertos.

## 1. Dónde se va la memoria

| Programa | Memoria (PSS) | Procesos |
|---|---|---|
| VS Code (ventana + extensiones) | ~1,4 GB | 24 |
| `opencode serve` en reposo | ~190 MB | varios |
| `pcode studio` en reposo (v1.1.0) | **7,6 MB** | 1 (4 hilos) |
| Navegador con la pestaña de Studio | 200–400 MB | 3–12 |

El núcleo ya es ligero. Lo pesado está **alrededor**: el navegador, los servidores de lenguaje (rust-analyzer, pyright) y compilar. Ahí se actúa.

## 2. Qué se ataca

| Problema | Solución | Tarea |
|---|---|---|
| Navegador entero para la interfaz | IDE nativo `pcode-ide` (egui + OpenGL) | F9.05 |
| LSP que ocupa cientos de MB | máximo de servidores vivos, parada por inactividad, alternativas ligeras | F9.02, F9.03 |
| Compilar tarda y agota la RAM | perfil `ligero` (sin LTO, `-j` según RAM) | F9.07 |
| Trabajar en un servidor potente | `pcode remoto` por SSH: lo pesado corre allí | F9.04, F9.16 |
| Extensiones de VS Code | solo lo declarativo (temas, fragmentos, lenguajes); lo demás por LSP, DAP y MCP | F9.19, F9.20 |
| Depurar | DAP con debugpy, en el demonio | F9.21 |

## 3. `[recursos]`

```toml
[recursos]
perfil = "auto"        # auto | ligero | normal
lsp_max = 1            # servidores de lenguaje vivos a la vez
lsp_inactivo_min = 5   # 0 = no parar nunca
```

`auto` elige **ligero** con menos de 6 GB de RAM o 2 núcleos o menos. Ligero: 1 servidor y 5 min; normal: 4 y 30 min. El proyecto manda sobre el global campo a campo.

## 4. Servidores remotos

```bash
pcode remoto usuario@servidor --instalar     # copia pcode si falta y abre Studio por un túnel SSH
pcode remoto servidor --tui                  # la TUI en el servidor
pcode remoto servidor --ide --ruta ~/web     # el IDE nativo contra el servidor
pcode remoto servidor --reenviar 11434       # trae el Ollama del servidor
```

Studio escucha **solo en loopback** del servidor; el túnel `ssh -L` lo trae aquí. Al salir se cierra todo. No hace falta Node ni Internet en el servidor.

## 5. IDE nativo

`pcode ide` o `pcode-ide --url …`. Es un cliente más del demonio (JSON-RPC y SSE): sirve igual en local y por `pcode remoto`. Solo OpenGL y X11 (también funciona en Wayland vía XWayland); repinta únicamente cuando llega un evento.

## 6. Compilar en poco espacio

`tools/compilar-ligero.sh` usa el perfil `ligero`. La compilación `release` con LTO tardó 11 min con picos de 560 MB por proceso.

## 7. Extensiones

Lo que hace pesado a VS Code es su anfitrión de extensiones. Aquí no lo hay: se leen temas, fragmentos y lenguajes de un `.vsix`; lo ejecutable se ignora y se avisa.

## 8. Estado y pendiente (11 nov 2026)

Hecho y probado en este portátil: `[recursos]`, LSP con máximo e inactividad, `pcode remoto` (prueba de punta a punta con
ssh falso), IDE nativo (`pcode ide`, ~88 MB con llvmpipe, 0 % de CPU en reposo; VS Code ~750 MB), depurador DAP probado
con debugpy real, extensiones declarativas, lanzador en el menú, `pcode doctor` con recursos, medidor de rendimiento.

| Pendiente | Tarea |
|---|---|
| Barra de título: no se ven el *command center* ni los conmutadores; iconos que salen como cuadros; bienvenida centrada | F9.28 |
| **Modo opencode** (TUI con aspecto y atajos de opencode, más ligera) | F9.29 |
| IDE: snippets y temas de las extensiones dentro del editor | F9.20 |
| IDE: menú «Conectar a un servidor…» con los hosts de `~/.ssh/config` | F9.16 (parte IDE) |
| IDE sin GPU: reintento automático en software y aviso | F9.26 |
| Panel de depuración en el IDE (el cliente DAP ya está en `pcode-dap`) y métodos `debug.*` en el demonio | F9.21 |
| Wayland nativo y AccessKit | F9.22 |
| Primera release v1.2.0 con `tools/release.sh 1.2.0` (necesita permiso de push de etiquetas) | F9.25 |

Nota de este equipo: el DRI3 del servidor X dejó de responder durante la sesión (GLX se cuelga y `glxinfo` también). El
IDE ya no depende de GLX (solo EGL), pero mientras DRI3 falle la ventana sale negra en la sesión actual: **cerrar
sesión y volver a entrar** lo arregla. En un X limpio (Xephyr) se ve bien.
