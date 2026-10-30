---
name: tarea-haiku
description: Ejecuta una tarea de tareas/<id>.md de PublicCode de principio a fin con disciplina de modelo pequeño (leer, implementar firmas exactas, tests, fmt/clippy, aceptar una vez, informe, commit). Úsala siempre que te pidan "haz la tarea X", "ejecuta tareas/..." o "siguiente tarea".
---

# Ejecutar una tarea de PublicCode

Pensada para Claude Haiku 5.5, pero vale para cualquier modelo. Sigue los pasos en orden y no te saltes ninguno.

## 0. Elegir la tarea (solo si no te han dado una)

1. Abre `docs/implementacion.md`, que tiene la tabla de tareas con su estado.
2. Elige la primera tarea con estado `pendiente` cuyas `depende` estén todas `hecha`
   (comprueba que existe `informes/<dep>.md` de cada una).

## 1. Leer

- Lee `tareas/<id>.md` entero. Anota tres cosas:
  - `acepta`: la orden exacta;
  - `toca`: los archivos que puedes tocar (ninguno más);
  - la lista **Firmas** y la lista **Tests**.
- Lee solo los archivos que cite la tarea en **Contexto**.

## 2. Implementar

- Crea o edita solo los archivos de `toca`.
- Copia las firmas tal cual. No añadas API pública que la tarea no pida.
- Si hace falta un helper privado, que sea privado (`fn`, no `pub fn`).

## 3. Tests

- Escribe cada test de la lista **Tests** con su nombre exacto.
- Nada de red, nada de `sleep` de más de 100 ms y nada de bucles sin cota.

## 4. Comprobar según el área

Usa la tabla de la sección 4 de `AGENTS.md`. Lo más habitual:

- **Rust** (crates, TUI, escritorio):

```bash
cargo fmt --all
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- **UI** (`apps/ui`):

```bash
npm run -w apps/ui lint && npm run -w apps/ui typecheck && npm run -w apps/ui test
```

Si la tarea es de interfaz (TUI, UI, escritorio), lee también la skill `interfaces`.

## 5. Aceptar (una vez)

- Ejecuta la orden `acepta` de la cabecera.
- Si falla: lee el **primer** error, corrígelo y repite. Si el mismo error se repite 3 veces, para
  y pasa al paso 6 con *Pendiente* explicando el bloqueo.

## 6. Informe

Escribe `informes/<id>.md`:

```markdown
# <id>
## Hecho
<2-5 viñetas>
## Archivos
<lista>
## Salida de acepta
<últimas 30 líneas>
## Decisiones
<cambios mínimos respecto a las firmas, si los hubo>
## Pendiente
<bloqueos o "nada">
```

## 7. Commit

```bash
git add <archivos de toca> informes/<id>.md && git commit -m "<id>: <resumen>"
```

Nunca `git add -A`, `git add .`, `stash`, `reset --hard`, `clean`, `rebase` ni `push`.
