# ADR-0004: un único formato de skills exportado a cada agente

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

Claude Code lee `.claude/skills/*/SKILL.md`; Copilot, `AGENTS.md` y `.github/copilot-instructions.md`;
Gemini, `GEMINI.md` y extensiones; opencode, su configuración y sus skills. Mantener cinco copias es
inviable. Además, la investigación de 2026 muestra que las skills en prosa confunden a los modelos
pequeños (*Skill-as-Pseudocode*), que con muchas skills hay que recuperar por dependencias
(*Graph-of-Skills*) y que las skills son superficie de ataque (*SkillSafetyBench*, *SkillAttack*,
*EvoSkill Injection*).

## Decisión

1. Fuente única: carpetas con `SKILL.md` (estándar abierto Agent Skills: frontmatter `name` y
   `description`), con campos opcionales `depende` y `no_usar_cuando` y una sección `## Contrato`
   (entradas, salidas y ejemplo de invocación).
2. `pcode skills exportar` publica cada skill en el formato y la ruta de cada agente (F2.14).
3. Las skills **aprendidas** entran en cuarentena y solo se activan si pasan evals con y sin ellas
   sin regresiones (F6.02); llevan procedencia.
4. Una skill nunca amplía capacidades: hereda la política del agente que la usa.

## Consecuencias

+ Las mismas skills para todos los agentes; se puede comparar el efecto de una skill entre backends.
+ Seguridad de la cadena de skills controlada.
− El exportador debe seguir los cambios de formato de cada agente (lo cubren los tests de F2.14).
