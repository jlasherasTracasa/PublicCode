# ADR-0001: núcleo en Rust

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

PublicCode es un demonio de larga vida que lanza procesos aislados (namespaces, Landlock, seccomp),
maneja mucha E/S concurrente (varios agentes, streaming, MCP) y debe repartirse como un único
binario. Los agentes programan Rust con soltura y el ecosistema (tokio, axum, ratatui, Tauri) cubre toda la
pila, incluida la aplicación de escritorio.

## Decisión

Núcleo y CLI en **Rust** (edición 2024, toolchain estable fijado). Los prototipos y las tools
auxiliares (`tools/`) en **Python con biblioteca estándar**, sin dependencias.

## Alternativas

- **TypeScript/Bun (como opencode)**: iteración rápida y ecosistema npm, pero un binario pesado, más
  difícil para sandboxing de bajo nivel.
- **Go**: buen candidato para el demonio, pero obligaría a mantener otra pila junto a Tauri (Rust).
- **Python**: ideal para prototipos, pero no para el demonio ni para la distribución.

## Consecuencias

+ Binario único; Landlock y seccomp nativos; rendimiento.
+ Haiku y GLM trabajan bien con tareas Rust acotadas y verificables con `cargo`.
− Más lento de iterar en la UI. Mitigación: panel web ligero en F5; la TUI con `ratatui`.
− Las extensiones de terceros no pueden ser «un archivo JS». Mitigación: MCP, WASM y scripts.
