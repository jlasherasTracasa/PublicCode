# ADR-0001: núcleo en Rust

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

PublicCode es un demonio de larga vida que lanza procesos aislados (namespaces, Landlock, seccomp),
maneja mucha E/S concurrente (varios agentes, streaming, MCP) y debe repartirse como un único
binario. Su cliente cero, SO_IA, está escrito en Rust, sus agentes ya programan Rust a diario, y sus
documentos prevén portar el orquestador como `flujod`/`agentd` nativos.

## Decisión

Núcleo y CLI en **Rust** (edición 2024, toolchain estable fijado). Los prototipos y las tools
auxiliares (`tools/`) en **Python con biblioteca estándar**, sin dependencias.

## Alternativas

- **TypeScript/Bun (como opencode)**: iteración rápida y ecosistema npm, pero un binario pesado, más
  difícil para sandboxing de bajo nivel y fuera del ecosistema de SO_IA.
- **Go**: buen candidato para el demonio, pero no comparte código ni conceptos con SO_IA.
- **Python**: ideal para prototipos, pero no para el demonio ni para la distribución.

## Consecuencias

+ Binario único; Landlock y seccomp nativos; rendimiento; reutilización de ideas y crates de SO_IA
  (`sinllm-router`).
+ Haiku y GLM ya están entrenados en este estilo de tareas Rust (SO_IA).
− Más lento de iterar en la UI. Mitigación: panel web ligero en F5; la TUI con `ratatui`.
− Las extensiones de terceros no pueden ser «un archivo JS». Mitigación: MCP, WASM y scripts.
