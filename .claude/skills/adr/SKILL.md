---
name: adr
description: Registra una decisión de arquitectura de PublicCode como ADR en docs/adr/. Úsala cuando se tome o cambie una decisión técnica relevante (lenguaje, dependencia nueva, formato, protocolo, seguridad).
---

# Escribir un ADR

1. Mira el último número en `docs/adr/` y usa el siguiente: `ADR-NNNN-titulo-corto.md`.
2. Plantilla:

```markdown
# ADR-NNNN: <título>

- Estado: propuesto | aceptado | sustituido por ADR-MMMM
- Fecha: AAAA-MM-DD

## Contexto
<qué problema hay y qué datos lo sostienen; enlaza métricas o papers de docs/papers.md>

## Decisión
<qué se decide, en una o dos frases>

## Alternativas
<opción: por qué no>

## Consecuencias
+ <ventajas>
− <costes>
```

3. Añade la fila al índice de `docs/adr/README.md`.
4. Si la decisión cambia una regla de `AGENTS.md`, dilo en *Consecuencias*. AGENTS.md solo lo edita
   quien coordina, no un agente de tarea.
