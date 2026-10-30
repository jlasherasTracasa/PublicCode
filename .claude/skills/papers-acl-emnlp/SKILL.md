---
name: papers-acl-emnlp
description: Busca papers de ACL/EMNLP (2026 por defecto, o cualquier evento de la ACL Anthology) relevantes para PublicCode y los adapta al plan en docs/papers.md sin inventar nada. Úsala cuando pidan papers, estado del arte, "qué dice la investigación" o actualizar docs/papers.md.
---

# Papers de ACL/EMNLP → PublicCode

Regla de oro: **todo paper citado sale de la ACL Anthology vía la tool**, con su ID
(`2026.emnlp-main.758`) y su URL. Si no está ahí, no se cita.

## 1. Buscar

Lista general (ACL 2026 + EMNLP 2026; la primera vez descarga ~47 MB y se cachea):

```bash
tools/papers/buscar_papers.py --top 80
```

Por temas (`agente-codigo`, `subagentes`, `tools`, `skills`, `contexto`, `memoria`, `verificacion`, `sandbox-seguridad`, `evaluacion`, `codigo-rag`, `modelos-pequenos`):

```bash
tools/papers/buscar_papers.py --tema skills --tema contexto --top 40
```

Con resúmenes, para leerlos:

```bash
tools/papers/buscar_papers.py --tema sandbox-seguridad --formato jsonl --top 20
```

Otro evento: `--evento naacl-2026`, `--evento acl-2025`, etc.

## 2. Filtrar

Quédate con los papers que cambien una decisión concreta de PublicCode (contexto, subagentes,
tools/MCP, skills, memoria, verificación, sandbox/seguridad, evaluación o modelos pequeños). Descarta
los de dominios ajenos (medicina, juegos, finanzas…) aunque puntúen alto.

## 3. Leer

Lee el **resumen completo** (campo `resumen` del JSONL). Si hace falta más detalle, abre la URL de la
Anthology o el PDF. No atribuyas al paper nada que no hayas leído.

## 4. Adaptar

Añade una fila en la sección que toque de `docs/papers.md`:

```markdown
| [Título](https://aclanthology.org/2026.xxx.NNN/) | venue | qué propone (1 frase, de su resumen) | qué cambia en PublicCode (componente + tarea) |
```

Si la adaptación genera trabajo nuevo, añade la tarea a `docs/implementacion.md` con el ID del paper
en el contexto.
