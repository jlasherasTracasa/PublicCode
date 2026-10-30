# AGENTS.md — instrucciones para cualquier agente que trabaje en PublicCode

Lo leen Claude Code (vía `CLAUDE.md`), GitHub Copilot, Gemini CLI, opencode y los agentes del propio
PublicCode. Está escrito para que **un modelo pequeño (Claude Haiku 5.5)** pueda cumplirlo al pie de
la letra: si una regla y la tarea chocan, manda la regla y lo explicas en el informe.

## Qué es este repo

PublicCode es una plataforma de programación agéntica que orquesta Claude Code, GitHub Copilot,
Gemini y modelos propios vía LiteLLM, con mini sandboxes, skills y tools compartidas, y criterios de
aceptación ejecutables. Visión: [README.md](README.md). Plan: [docs/PLAN.md](docs/PLAN.md).
Arquitectura: [docs/arquitectura.md](docs/arquitectura.md). Tareas: [docs/implementacion.md](docs/implementacion.md).

## Flujo obligatorio de una tarea

1. **Lee la tarea entera**: `tareas/<id>.md`. Su cabecera tiene `acepta` (la única orden que decide
   si has terminado), `depende` y `toca` (los únicos archivos que puedes crear o modificar).
2. **Lee solo lo que la tarea cite.** No explores el repo entero: cuesta contexto y no hace falta.
3. **Implementa exactamente las firmas** que da la tarea: mismos nombres, tipos y módulos. Si una
   firma no compila tal cual, haz el cambio mínimo y explícalo en el informe.
4. **Escribe los tests que pide la tarea**, con los nombres que da. Cada test termina en segundos y
   no usa red.
5. **Formato y lint**: `cargo fmt --all` y `cargo clippy --workspace --all-targets -- -D warnings`.
6. **Ejecuta la orden `acepta` una sola vez al final.** Si falla, corrige y vuelve a ejecutarla. Si
   falla 3 veces seguidas con el mismo error, para y escríbelo en el informe.
7. **Informe**: `informes/<id>.md` con cinco apartados: *Hecho*, *Archivos*, *Salida de acepta*
   (las últimas 30 líneas), *Decisiones*, *Pendiente*. Breve.
8. **Commit** solo de los archivos de `toca` más tu informe:
   `git add <archivos> informes/<id>.md && git commit -m "<id>: <resumen en una línea>"`.

## Reglas duras

1. **Nunca** `git add -A`, `git add .`, `git stash`, `git reset --hard`, `git checkout -- .`,
   `git clean`, `git rebase` ni `git push`. Otros agentes trabajan en el mismo árbol.
2. **No toques** `docs/`, `AGENTS.md`, `CLAUDE.md`, `tareas/` ni `.claude/` salvo que tu tarea los
   ponga en `toca`.
3. **Sin secretos**: ni claves ni tokens en ningún archivo. La configuración usa `{env:VARIABLE}`.
4. **Sin red** salvo la que necesite `cargo` para descargar las dependencias permitidas.
5. **Dependencias**: solo las de la lista de abajo. Si necesitas otra, no la añadas: escríbelo en
   *Pendiente*.
6. **`unsafe` prohibido** salvo en `crates/pcode-sandbox`, y allí cada bloque lleva `// SAFETY:`.
7. **Sin bucles sin cota** en tests ni en código de producción que espere algo externo: siempre con
   tiempo máximo o número máximo de iteraciones.
8. **No cambies la arquitectura.** Si la tarea parece imposible tal como está escrita, deja el código
   compilando, explica el bloqueo en el informe y termina.

## Convenciones de código

| Tema | Regla |
|---|---|
| Lenguaje | Rust, edición 2024, toolchain estable fijado en `rust-toolchain.toml` |
| Workspace | un crate por subsistema en `crates/pcode-<nombre>/`; el binario es `crates/pcode-cli` (`pcode`) |
| Identificadores | en **inglés** (es la API pública) |
| Comentarios, docs, mensajes de error al usuario, commits e informes | en **español** |
| Errores | `thiserror` en bibliotecas, `anyhow` solo en el binario |
| Asíncrono | `tokio`; nada bloqueante dentro de tareas async (usa `spawn_blocking`) |
| Serialización | `serde` + `serde_json` / `toml` |
| Logs | `tracing`; nunca `println!` en bibliotecas |
| Tests | unitarios junto al código (`#[cfg(test)]`), de integración en `crates/<c>/tests/`; deterministas, sin red, sin sleeps largos |
| Documentación | `///` en todo lo público, con una frase que diga para qué sirve |

## Dependencias permitidas

`tokio`, `serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `clap` (derive), `tracing`,
`tracing-subscriber`, `reqwest` (solo `rustls-tls`, `json`, `stream`), `rusqlite` (`bundled`),
`sha2`, `hex`, `globset`, `similar`, `uuid`, `time`, `async-trait`, `futures`, `regex`,
`shell-words`, `nix` (solo en `pcode-sandbox`), `tempfile` (solo en dev), `assert_cmd` y
`predicates` (solo en dev). Más adelante, cuando la tarea lo diga: `tree-sitter`, `wasmtime`,
`axum`, `ratatui`, `rmcp`.

## Estructura

```
crates/pcode-core      bucle del agente, contexto, compactación
crates/pcode-models    proveedores (LiteLLM/OpenAI-compatible, Anthropic, Gemini) y router
crates/pcode-tools     tools nativas con efectos declarados
crates/pcode-policy    capacidades, hooks, auditoría encadenada
crates/pcode-sandbox   mini sandbox N1 (userns + overlay + landlock + seccomp)
crates/pcode-backends  adaptadores: claude-code, copilot, gemini, opencode, modelo propio
crates/pcode-skills    registro, carga progresiva, exportación a cada agente
crates/pcode-flow      tareas, DAG, aceptación, reintentos, integración
crates/pcode-store     SQLite (eventos, sesiones)
crates/pcode-mcp       servidor MCP de PublicCode
crates/pcode-cli       binario `pcode`
tools/                 prototipos en Python (minisandbox, papers, bench)
tareas/                una tarea por archivo
informes/              un informe por tarea (lo escribe el agente)
```

## Herramientas del repo que puedes usar

* `tools/minisandbox/minisandbox.py ejecutar -- <orden>`: prueba algo sin tocar el repo.
* `tools/papers/buscar_papers.py`: busca papers de ACL/EMNLP. Nunca cites un paper que no salga de aquí.
* `tools/bench/linea_base.py --repo <SO_IA>`: métricas de la línea base.

## Hecho significa

La orden `acepta` devuelve 0, `cargo fmt --check` y `clippy -D warnings` están limpios, el informe
existe y el commit contiene solo los archivos de `toca` y el informe.
