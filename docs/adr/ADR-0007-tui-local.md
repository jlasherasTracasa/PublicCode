# ADR-0007: TUI con el agente en proceso antes de conectarla al demonio

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

Para sustituir a Claude Code u opencode hace falta ya una interfaz de terminal a pantalla completa.
Las tareas F5.07–F5.11 la diseñan como cliente del demonio, pero dependen del cliente Rust (F5.05) y
de la API de flujos (F5.04), que aún no existen.

## Decisión

1. Crate `pcode-tui` con el diseño de F5.07: estado puro `App`, `update(Event) -> Vec<Command>` y
   `render(&App, &mut Frame)` sin efectos; la E/S vive solo en `run_tui`.
2. Por ahora `run_tui` recibe un `Agent` ya construido y lo ejecuta en una tarea del propio proceso;
   los eventos del agente (`AgentEvent`) y las aprobaciones llegan a `App` como `Event`.
3. Dependencia: `ratatui` 0.30 (lista de AGENTS.md §7, «cuando la tarea lo diga»), usando el
   `crossterm` que reexporta para no tener dos versiones.
4. F5.07–F5.11 siguen pendientes para la versión conectada a `pcoded` (árbol de sesiones, paneles de
   cambios y aprobaciones, `@archivo`, `keybindings.toml`): solo cambia la fuente de `Event` y el
   destino de `Command`.

## Consecuencias

+ `pcode` ya abre una TUI usable, con streaming y aprobaciones, en Linux y Windows.
+ Los tests de render con `TestBackend` (80×24 y 120×40) y de `update` valen para la versión con demonio.
− Una sesión de la TUI no se ve desde la web ni el móvil hasta que se conecte al demonio.
