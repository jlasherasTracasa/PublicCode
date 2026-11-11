# ADR-0012: extensiones de VS Code, solo lo declarativo

**Estado**: aceptada

## Contexto
Los usuarios quieren sus extensiones de VS Code. Lo que hace pesado a VS Code es el anfitrión de extensiones (un
proceso Node por ventana, cientos de MB). En un equipo de 4 GB no cabe.

## Decisión
- Crate `pcode-extensions`: de un `.vsix` o de una carpeta (`~/.vscode/extensions/...`) se usan **temas, fragmentos,
  lenguajes y gramáticas**. Lo ejecutable (`main`, `browser`) no se carga nunca y se avisa de lo que se ignora.
- El `.vsix` (un zip) se abre con `unzip` del sistema: sin dependencia nueva. Si falta, el error lo dice.
- Lo que en VS Code son extensiones «de verdad» llega por protocolos con procesos aparte y opcionales: **LSP**
  (lenguajes), **DAP** (depuradores, F9.21) y **MCP** (tools del agente).
- Open VSX (descargar por nombre) queda para una tarea posterior: solo si el usuario lo pide.

## Consecuencias
Las extensiones que solo son código (p. ej. Pylance) no funcionan; su equivalente es un servidor LSP (pyright, pylsp).
