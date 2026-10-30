# AGENTS.md — guía para cualquier agente que trabaje en PublicCode

Es la **fuente única** de instrucciones. `CLAUDE.md` la importa, y `GEMINI.md` y
`.github/copilot-instructions.md` se generan desde aquí (tarea F2.17): no los edites a mano.
Está escrita para que **Claude Haiku 5.5** pueda cumplirla al pie de la letra. Si una regla choca con
la tarea, manda la regla y lo explicas en el informe.

**Qué leer**: las secciones 0 a 4 siempre. De las secciones 5 a 9, solo la parte del área de tu tarea.

---

## 0. En 30 segundos

1. Tu trabajo es **una** tarea: `tareas/<id>.md`.
2. Solo creas o modificas los archivos de su lista **Toca**, más tu informe `informes/<id>.md`.
3. Has terminado cuando la orden **`acepta`** de la tarea devuelve 0. No antes.
4. Commit solo de tus archivos: `git add <archivos> informes/<id>.md && git commit -m "<id>: …"`.
5. **Nunca**: `git add -A`, `git add .`, `git stash`, `git reset --hard`, `git checkout -- .`,
   `git clean`, `git rebase` ni `git push`; secretos en archivos; dependencias que no estén en la
   sección 7; tocar archivos generados.

## 1. Qué es PublicCode

Una plataforma de programación agéntica que **orquesta varios agentes** (Claude Code, GitHub
Copilot, Gemini, opencode y modelos propios vía LiteLLM) con mini sandboxes, skills y tools
compartidas, y criterios de aceptación ejecutables. Visión: [README.md](README.md). Plan:
[docs/PLAN.md](docs/PLAN.md). Arquitectura: [docs/arquitectura.md](docs/arquitectura.md).
Interfaces: [docs/interfaces.md](docs/interfaces.md).

```
 CLIENTES (presentan y envían intenciones)            NÚCLEO (toda la lógica)
 ┌──────────────────────────────────────────┐
 │ terminal   pcode (CLI) · pcode tui        │
 │ escritorio apps/desktop (Tauri 2)         │      pcoded ─┬─ flujos, router, reintentos, integración
 │ web        apps/ui en el navegador        │ ──API──▶     ├─ backends + mini sandboxes
 │ IDE        ACP · extensión de VS Code     │ (pcode-      ├─ política, aprobaciones, auditoría
 │ móvil      PWA de aprobaciones            │  protocol)   ├─ skills, memoria, contexto
 │ GitHub     PR, comentarios, checks        │              └─ store (SQLite, eventos)
 └──────────────────────────────────────────┘
```

**Regla de arquitectura**: la lógica vive en el núcleo (`crates/`). Los clientes no deciden nada:
muestran el estado y envían peticiones por la API. Si una tarea de interfaz parece necesitar
lógica, falta un método en la API: dilo en el informe.

## 2. Flujo obligatorio de una tarea

0. **Dependencias**: comprueba que existe `informes/<dep>.md` para cada `depende` de la tarea. Si
   falta alguno, no empieces: escríbelo en el informe y para.
1. **Lee la tarea entera**. Anota tres cosas: `acepta`, la lista **Toca** y las listas **Firmas** y **Tests**.
2. **Lee solo lo que cite la tarea en Contexto.** No explores el repo: cuesta contexto y no hace
   falta. Si necesitas ver una API ya hecha, abre solo ese archivo.
3. **Implementa exactamente las firmas** (nombres, tipos, módulos). Si una firma no compila tal
   cual, haz el cambio mínimo y explícalo en *Decisiones*.
4. **Escribe los tests de la lista** con esos nombres. Deterministas, sin red y de segundos.
5. **Comprueba según tu área** (sección 4): formato, lint y tests.
6. **Ejecuta `acepta` una vez al final.** Si falla, corrige y repite. Si el mismo error sale 3 veces
   seguidas, para (sección 9).
7. **Informe** `informes/<id>.md` (plantilla en la sección 10).
8. **Commit** solo de los archivos de *Toca* más el informe.

## 3. Reglas duras

1. **Git**: nada de lo prohibido en la sección 0. Otros agentes trabajan en paralelo en el mismo
   repo (en sus propias sandboxes) y el integrador depende de commits limpios.
2. **Alcance**: no toques archivos fuera de *Toca*. En particular, nunca toques `AGENTS.md`,
   `CLAUDE.md`, `docs/`, `tareas/` ni `.claude/` si la tarea no los pone en *Toca*.
3. **Archivos generados** (nunca a mano): `tareas/*.md` (salen de `tareas/plan.toml`),
   `apps/ui/src/protocol/` (sale de `crates/pcode-protocol`), las secciones entre marcadores
   `<!-- …:inicio -->` / `<!-- …:fin -->`, `GEMINI.md` y `.github/copilot-instructions.md`.
4. **Secretos**: ni claves ni tokens en archivos, tests, fixtures ni logs. La configuración usa
   `{env:VARIABLE}` y la app de escritorio guarda las credenciales en el llavero del sistema.
5. **Red**: los tests no usan red; si hacen falta servidores, son falsos y en `127.0.0.1`. Solo
   `cargo` y `npm ci` descargan dependencias permitidas. Nada de telemetría hacia fuera.
6. **Dependencias**: solo las de la sección 7. Si necesitas otra, no la añadas: va a *Pendiente*.
7. **`unsafe`** solo en `crates/pcode-sandbox`, y cada bloque con `// SAFETY:`.
8. **Sin bucles sin cota** ni esperas sin tiempo máximo, ni en tests ni en código que espere algo externo.
9. **No hagas trampas para pasar `acepta`**: no desactives tests, no añadas `#[ignore]`, no bajes
   lints con `#[allow]` sin un comentario que lo justifique y no cambies la orden de aceptación.
10. **No cambies la arquitectura.** Si la tarea parece imposible tal como está escrita, deja el
    código compilando, explica el bloqueo y termina.

## 4. Cómo comprobar según el área

| Área | Rutas | Comprobación (antes de `acepta`) |
|---|---|---|
| Núcleo Rust | `crates/pcode-*` (salvo TUI) | `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test -p <crate>` |
| Terminal: CLI | `crates/pcode-cli` | lo anterior; tests de integración con `assert_cmd`; comprobar `--formato json` si la tarea lo toca |
| Terminal: TUI | `crates/pcode-tui` | lo anterior; render con `ratatui::backend::TestBackend` a **80×24** y a 120×40; snapshots por igualdad de texto |
| UI web y escritorio | `apps/ui` | `npm ci` · `npm run -w apps/ui lint` · `npm run -w apps/ui typecheck` · `npm run -w apps/ui test` · `npm run -w apps/ui build` |
| Escritorio (Tauri) | `apps/desktop` | `cargo clippy -p pcode-desktop -- -D warnings` · `cargo test -p pcode-desktop`. El empaquetado (`tauri build`) lo hace la CI o el coordinador, no tú |
| Tools Python | `tools/` | `python3 -m unittest <test> -v` |
| Plan | `tareas/plan.toml` | `tools/tareas/generar.py --comprobar` |

Las tareas de sandbox que necesitan namespaces de usuario **se saltan solas** si el sistema no los
permite: no lo consideres un fallo tuyo, pero dilo en el informe.

## 5. Mapa del repositorio

```
crates/
  pcode-config     pcode.toml            pcode-models    proveedores y router
  pcode-tools      tools nativas         pcode-policy    capacidades, shell, auditoría
  pcode-core       bucle del agente      pcode-store     SQLite (sesiones y eventos)
  pcode-sandbox    mini sandbox N1       pcode-backends  Claude Code, Copilot, Gemini, opencode, modelo
  pcode-skills     skills e instrucciones pcode-flow     tareas, DAG, integración, CI, bench
  pcode-memory     facetas, búsqueda, episodios
  pcode-mcp        servidor MCP          pcode-protocol  tipos de la API (fuente del TS generado)
  pcode-server     demonio pcoded        pcode-client    cliente Rust del demonio
  pcode-tui        terminal interactiva  pcode-acp       servidor ACP para IDEs
  pcode-cli        binario `pcode`
apps/
  ui/              React + TypeScript + Vite: interfaz común para web y escritorio
  desktop/         Tauri 2: aplicación de escritorio (carga apps/ui y lanza pcoded como sidecar)
  vscode/          extensión fina de VS Code (opcional)
tools/             prototipos y utilidades en Python (minisandbox, papers, bench, tareas)
tareas/            plan.toml (fuente) y una tarea .md por archivo (generadas)
informes/          un informe por tarea (lo escribe el agente)
docs/              plan, arquitectura, interfaces, backends, sandboxes, evaluación, papers, ADRs
.claude/skills/    skills del repo (formato SKILL.md)
```

## 6. Convenciones por área

### 6.1 Rust (todo `crates/`)

| Tema | Regla |
|---|---|
| Toolchain | estable, edición 2024, versión fijada en `rust-toolchain.toml` |
| Identificadores | en **inglés** (es la API pública) |
| Comentarios, docs, mensajes al usuario, commits e informes | en **español** |
| Errores | `thiserror` en bibliotecas; `anyhow` solo en binarios |
| Asíncrono | `tokio`; nada bloqueante en tareas async (`spawn_blocking`) |
| Logs | `tracing`; nunca `println!` en bibliotecas |
| Tests | unitarios en `#[cfg(test)]` junto al código; de integración en `crates/<c>/tests/` |
| Documentación | `///` en todo lo público, con una frase de para qué sirve |
| Plataformas | el código específico va tras `#[cfg(target_os = "…")]` y siempre hay una alternativa que compila en las demás |

### 6.2 Protocolo y API (`pcode-protocol`, `pcode-server`, `pcode-client`)

- Todos los tipos de la API viven en `pcode-protocol` (serde) y exportan TypeScript con `ts-rs` a
  `apps/ui/src/protocol/`. Un cambio de tipo = cambio en Rust + regenerar; nunca editar el TS.
- JSON-RPC 2.0 para peticiones; eventos por SSE (web) o eventos de Tauri (escritorio).
- `PROTOCOL_VERSION` (semver). Dentro de una versión mayor, solo cambios compatibles: campos nuevos
  opcionales y métodos nuevos.
- Transporte local: socket Unix (`.pcode/pcoded.sock`) en Linux; en Windows, TCP en loopback
  con token. Remoto (PWA, otra máquina): TCP con token y TLS si no es loopback.

### 6.3 Terminal: CLI (`pcode`) y TUI (`pcode tui`, o `pcode` sin argumentos en una TTY)

- **CLI**: salida legible por defecto; `--formato json|stream-json` para scripts y CI; códigos de
  salida documentados (0 hecho, 2 fallo, 3 presupuesto, 4 atasco); si stdout no es una TTY, nada
  interactivo ni colores; respeta `NO_COLOR`; autocompletado con `pcode completions <shell>`.
- **TUI**: estado puro `App` con `update(Event) -> Vec<Command>` y `render(&App, Frame)` sin efectos;
  el render nunca bloquea (la E/S va en tareas aparte); usable a 80×24; toda acción tiene atajo y
  los atajos se configuran en `keybindings.toml`; nunca dependas solo del color (añade símbolos:
  `✓ ✗ ⏳ ⚠`); los textos salen del catálogo i18n.

### 6.4 UI común web y escritorio (`apps/ui`)

- React 19 + TypeScript **estricto** + Vite; solo componentes de función; estado global con
  `zustand`; estilos con Tailwind y **tokens de tema** (CSS variables) para claro y oscuro.
- **Transporte abstracto**: los componentes usan `Transport` (`tauri` o `http`), así que no saben
  si corren en escritorio o en navegador. Nunca llames a `fetch` ni a `@tauri-apps/api` desde un
  componente.
- **Textos** en el catálogo i18n (`src/i18n/es.json`, `src/i18n/en.json`); nunca literales en JSX.
- **Accesibilidad**: todo usable con teclado (foco visible, orden lógico), roles ARIA, contraste AA
  y atajos anunciados. Los tests con Testing Library buscan por rol y texto, no por clases.
- **Seguridad**: sin contenido remoto (ni CDNs ni fuentes externas); el Markdown de los agentes se
  renderiza **sin HTML crudo**; los diffs y salidas se muestran como texto, nunca con `dangerouslySetInnerHTML`.

### 6.5 Escritorio (`apps/desktop`, Tauri 2)

- La app es un cliente más: carga `apps/ui` y lanza `pcoded` como **sidecar** (o se conecta al que
  ya esté corriendo).
- **Capacidades mínimas** en `src-tauri/capabilities/*.json`. Prohibido exponer el plugin `shell` o
  `fs` genérico al frontend; cada acción nativa es un comando Tauri concreto y tipado.
- Credenciales en el **llavero del sistema** (`keyring`), nunca en archivos ni en `localStorage`.
- CSP estricta; sin navegación a orígenes remotos; actualizaciones solo firmadas.
- Integración con el sistema: icono en la bandeja con el estado, notificaciones nativas, enlaces
  `pcode://`, una sola instancia y estado de ventana recordado.

### 6.6 Python (`tools/`)

Biblioteca estándar únicamente; `#!/usr/bin/env python3` y bit de ejecución; `argparse` con
docstring de uso; tests con `unittest` en el mismo directorio (`test_*.py`).

### 6.7 Documentación

En español, con tablas antes que párrafos largos. Solo se toca si la tarea la pone en *Toca*.
Enlaces relativos que existan (la CI los comprueba).

## 7. Dependencias permitidas

**Rust (núcleo)**: `tokio`, `serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `clap` (derive) y
`clap_complete`, `tracing`, `tracing-subscriber`, `reqwest` (solo `rustls-tls`, `json`, `stream`),
`rusqlite` (`bundled`), `sha2`, `hex`, `globset`, `regex`, `similar`, `uuid`, `time`, `async-trait`,
`futures`, `shell-words`, `nix` (solo `pcode-sandbox`). Solo en dev: `tempfile`, `assert_cmd`,
`predicates`.

**Rust (cuando la tarea lo diga)**: `landlock`, `seccompiler`, `rmcp`, `tree-sitter`,
`tree-sitter-rust`, `tree-sitter-python`, `axum`, `tokio-stream`, `ts-rs`, `ratatui`, `crossterm`,
`keyring`, `tauri` y sus plugins oficiales (`single-instance`, `notification`, `deep-link`,
`window-state`, `autostart`, `updater`).

**JavaScript/TypeScript (`apps/`)**: `react`, `react-dom`, `typescript`, `vite`,
`@vitejs/plugin-react`, `zustand`, `tailwindcss`, `react-markdown` (sin `rehype-raw`), `vitest`,
`@testing-library/react`, `@testing-library/user-event`, `jsdom`, `eslint` con la configuración del
repo, `@tauri-apps/api`, `@tauri-apps/cli` y los paquetes JS de los plugins de Tauri de arriba.
Versiones exactas (sin `^` ni `~`) y `package-lock.json` versionado.

**Python**: solo la biblioteca estándar.

## 8. Plataformas

| Plataforma | Núcleo y CLI | TUI | Escritorio | Mini sandbox |
|---|---|---|---|---|
| Linux x86_64 / aarch64 | ✓ | ✓ | ✓ (.deb, .rpm, AppImage) | **N1** (namespaces + overlay) |
| Windows 10/11 | ✓ | ✓ | ✓ (.msi) | vía WSL2 (N1 dentro) o N2 |

La CI ejecuta los tests en Linux; las tareas de otras plataformas aíslan su lógica en funciones
puras que se prueban en Linux, y lo específico queda tras `cfg`.

## 9. Si te atascas

| Situación | Qué hacer |
|---|---|
| El mismo error 3 veces seguidas | para; informe con el error y lo que probaste |
| Una firma no compila tal cual | cambio mínimo; explícalo en *Decisiones* |
| La tarea es ambigua | la interpretación más conservadora; anótala en *Decisiones* |
| Necesitas una dependencia que no está en la lista | no la añadas; *Pendiente* |
| Necesitas tocar un archivo fuera de *Toca* | no lo toques; *Pendiente* con el cambio exacto que harías |
| Una dependencia de la tarea no está hecha | no empieces; informe de bloqueo |
| El entorno no permite algo (namespaces, KVM, red) | deja que los tests se salten; dilo en el informe |

## 10. Informe y commit

`informes/<id>.md`:

```markdown
# <id>
## Hecho
- 2 a 5 viñetas
## Archivos
- lista exacta
## Salida de acepta
(últimas 30 líneas, tal cual)
## Decisiones
- cambios mínimos respecto a las firmas, o "ninguna"
## Pendiente
- bloqueos, dependencias o archivos fuera de Toca que harían falta, o "nada"
```

El informe cuenta **hechos**: no afirmes nada que `acepta` o los tests no hayan comprobado.

```bash
git add <archivos de Toca> informes/<id>.md && git commit -m "<id>: <resumen en una línea>"
```

## 11. Herramientas del repo que puedes usar

- `tools/minisandbox/minisandbox.py ejecutar -- <orden>`: prueba algo sin tocar el repo (skill `mini-sandbox`).
- `tools/papers/buscar_papers.py`: papers de ACL/EMNLP. Nunca cites uno que no salga de aquí (skill `papers-acl-emnlp`).
- `tools/bench/reglas.py` (lo crea F0.06): cumplimiento de reglas. El banco PublicBench lo monta la skill `banco` (F0.04).
- `tools/tareas/generar.py`: regenerar y validar el plan (solo el coordinador cambia `plan.toml`).

## 12. Para el coordinador (no aplica a agentes de tarea)

- Cambiar el plan: editar `tareas/plan.toml` y ejecutar `tools/tareas/generar.py`; la CI exige `--comprobar`.
- Revisar cada informe antes de lanzar las tareas que dependen de él; ajustar el formato de tarea si
  Haiku tropieza siempre en lo mismo.
- Este archivo solo lo cambia el coordinador. Después hay que regenerar las instrucciones derivadas
  (`pcode instrucciones sincronizar`, F2.17).
