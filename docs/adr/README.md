# Decisiones de arquitectura (ADR)

Formato y procedimiento: skill [`adr`](../../.claude/skills/adr/SKILL.md).

| ADR | Decisión | Estado |
|---|---|---|
| [0001](ADR-0001-rust.md) | Núcleo en Rust; prototipos en Python solo con la biblioteca estándar | aceptado |
| [0002](ADR-0002-backends-externos.md) | Agentes externos como backends por vías oficiales, más un bucle propio para modelos | aceptado |
| [0003](ADR-0003-mini-sandbox.md) | Mini sandbox N1 con namespaces de usuario y overlay CoW como aislamiento por defecto | aceptado |
| [0004](ADR-0004-skills-formato-unico.md) | Un único formato de skills (`SKILL.md`) exportado a cada agente, con cuarentena para las aprendidas | aceptado |
