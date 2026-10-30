---
name: banco
description: Construye y ejecuta PublicBench (tareas del propio plan con su criterio de aceptación y commit base) para medir la línea base y comparar backends (opencode, Claude Code, Copilot, Gemini, pcode con LiteLLM). Úsala cuando pidan medir, comparar agentes o comprobar si un cambio mejora algo.
---

# PublicBench

Protocolo y métricas en `docs/evaluacion.md`.

## 1. Preparar el banco (tarea F0.04)

Elige tareas ya hechas de `tareas/` y registra en `bench/smoke.jsonl` una línea por tarea:

```json
{"id": "F1.09-tool-leer", "acepta": "cargo test -p pcode-tools read", "tarea": "tareas/F1.09-tool-leer.md", "base": "<commit anterior>", "aceptada_en": "<commit de la tarea>"}
```

Los commits salen de `git log --format=%h -- informes/<id>.md`.

## 2. Ejecutar (cuando exista `pcode bench`, tarea F3.13)

Por cada tarea y backend: checkout de `base` en una mini sandbox, lanzar el backend con el texto de la
tarea, ejecutar `acepta` y guardar resultado, intentos, tokens, tiempo y diff.

## 3. Informar

Tabla por backend: % a la primera, % final, tokens y tiempo por tarea aceptada, violaciones de
política y fallos de integración. Compara siempre con el **mismo modelo** cuando midas el arnés, y
mide primero la línea base con opencode.
