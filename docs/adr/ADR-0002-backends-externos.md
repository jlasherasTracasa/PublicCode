# ADR-0002: agentes externos como backends, más un bucle propio para modelos

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

El objetivo es la mejor programación agéntica posible integrando Claude Code, GitHub Copilot, Gemini
y modelos propios (LiteLLM). Cada agente tiene puntos fuertes distintos y evoluciona más rápido de lo
que podríamos reimplementarlo. Al mismo tiempo, con modelos propios (GLM) necesitamos control total
del bucle: reparación de tool calls, compresión, caché y detector de atasco.

## Decisión

Dos clases de backend detrás del mismo `trait Backend`:

1. **Agentes completos** (Claude Code, Copilot CLI, Copilot coding agent, Gemini CLI y opencode),
   invocados **solo por sus vías oficiales** (modo no interactivo, SDK o API de GitHub) con las
   credenciales del usuario, dentro de mini sandboxes y con red solo hacia su API.
2. **Modelos** (LiteLLM/OpenAI-compatible, Anthropic, Gemini) con el bucle propio `pcode-core`.

PublicCode aporta lo común: tareas, política, sandbox, skills, tools por MCP, aceptación, relevo,
integración y evaluación.

## Alternativas

- **Solo bucle propio con muchos proveedores**: se pierden las capacidades nativas de cada agente
  (sus skills, sus subagentes y su ajuste fino del arnés).
- **Solo agentes externos**: sin control del bucle para modelos propios, que es donde más se gana
  con GLM.
- **Endpoints no oficiales para usar suscripciones**: descartado por términos de uso y por
  fragilidad.

## Consecuencias

+ Cada tarea va al mejor agente; se comparan en igualdad de condiciones en PublicBench.
+ El trabajo propio se concentra donde nadie más lo hace: la orquestación y la seguridad.
− Los adaptadores se rompen cuando cambian las CLIs. Mitigación: fixtures reales, tests de contrato
  y ACP donde exista.
− Cada agente externo necesita red hacia su API: proxy de salida con lista de dominios (F2.19).
