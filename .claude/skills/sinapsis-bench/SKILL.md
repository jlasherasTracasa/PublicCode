---
name: sinapsis-bench
description: Construye y ejecuta SinapsisBench (tareas reales de SO_IA con su criterio de aceptación) para medir la línea base y comparar backends (opencode, Claude Code, Copilot, Gemini, pcode con LiteLLM). Úsala cuando pidan medir, comparar agentes, la línea base o comprobar si un cambio mejora algo.
---

# SinapsisBench

Protocolo completo en `docs/evaluacion.md`.

## 1. Línea base del flujo actual

```bash
tools/bench/linea_base.py --repo ../SO_IA
```

Valores de referencia en el commit `2ef8d59`: 84 tareas, 72,6 % aceptadas a la primera,
8 con 3 o más intentos (una bloqueada), 17 integraciones fallidas, 27 tareas CI-arreglo y
456 líneas de pegamento.

## 2. Extraer tareas

La extracción necesita la historia completa de SO_IA:

```bash
git -C ../SO_IA fetch --unshallow
```

```bash
tools/bench/extraer_tareas.py --repo ../SO_IA --solo-aceptadas > bench/tareas.jsonl
```

Cada línea trae `id`, `acepta`, `modo`, `depende`, `base` (commit desde el que se re-ejecuta) y
`aceptada_en`.

## 3. Ejecutar (cuando exista `pcode bench`, tarea F3.x)

Por cada tarea y backend: checkout de `base` en una mini sandbox o en la imagen `sinapsis-dev`,
lanzar el backend con el texto de la tarea, ejecutar `acepta` y guardar resultado, intentos, tokens,
tiempo y diff.

## 4. Informar

Tabla por backend: % a la primera, % final, tokens y tiempo por tarea aceptada, violaciones de
política y fallos de integración. Compara siempre con el **mismo modelo** cuando midas el arnés.
