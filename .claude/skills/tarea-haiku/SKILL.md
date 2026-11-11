---
name: tarea-haiku
description: Ejecuta una tarea de tareas/<id>.md de PublicCode de principio a fin con disciplina de modelo pequeño (leer, implementar firmas exactas, tests, fmt/clippy, aceptar una vez, informe, commit). Úsala siempre que te pidan "haz la tarea X", "ejecuta tareas/..." o "siguiente tarea".
---

# Ejecutar una tarea de PublicCode

Pensada para Claude Haiku 5.5, pero vale para cualquier modelo. Sigue los pasos en orden y no te saltes ninguno.

## 0. Elegir la tarea (solo si no te han dado una)

1. Ejecuta `tools/tareas/generar.py --siguiente 1 --ejecutor haiku`. La última columna es el archivo de la tarea.
2. Si te han dado una tarea, comprueba que se puede empezar: `tools/tareas/generar.py --tarea <id>` debe decir
   `estado: lista`. Si dice otra cosa (`bloqueada`, `esbozo`, `sustituida`…), no empieces y dilo.

## 1. Leer

- Lee `tareas/<id>.md` entero. Anota tres cosas:
  - `acepta`: la orden exacta;
  - `toca`: los archivos que puedes tocar (ninguno más);
  - la lista **Firmas** y la lista **Tests**.
- Lee solo los archivos que cite la tarea en **Contexto**.
- Si la tarea trae **Ejemplo**, es la referencia: tu código debe dar exactamente esa salida.
- Si trae **No hagas**, son los errores habituales: no cometas ninguno.
- Si es una tarea de cierre (lista **Subtareas** y no toca archivos), solo ejecuta `acepta` y escribe el informe.

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

Después, `git push origin HEAD` (nunca forzado; si falla, anótalo en *Pendiente*).

Nunca `git add -A`, `git add .`, `stash`, `reset --hard`, `clean`, `rebase` ni `push --force`.
