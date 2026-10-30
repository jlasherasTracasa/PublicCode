# Evaluación: PublicBench y métricas

«Mejor que opencode» tiene que ser un número. Este documento define cómo se mide.

## 1. Qué se compara

* **Arnés**: el mismo modelo (p. ej. GLM-5.3-Flash por LiteLLM) en opencode y en PublicCode. Mide el
  valor del arnés: política, hook de parada, detector de atasco, compresión y relevo.
* **Backends**: Claude Code, Copilot, Gemini, opencode y PublicCode con un modelo propio, cada uno con
  su mejor configuración. Alimenta al router.
* **Ablaciones**: con y sin cada mecanismo, para que cada pieza demuestre lo que aporta.

La línea base no se asume: se **mide en F0** ejecutando el banco con opencode antes de activar nada de
PublicCode, y todos los objetivos se expresan como mejora respecto a esa medida.

## 2. PublicBench

**Tareas**: las del propio plan (`tareas/`), cada una con su criterio `acepta`, su commit base (estado
del repo antes de la tarea) y el commit en que se aceptó. Se extraen de `informes/` y del historial de
git (tarea F0.04).

**Conjuntos**

| Conjunto | Tamaño | Uso |
|---|---|---|
| `smoke` | 20 (12 de una crate, 4 de varias, 2 cadenas de 2) | cada cambio del arnés; ~1 h con un modelo rápido |
| `completo` | todas las tareas hechas | cada noche (F6.04) y en cada hito de fase |
| `cadenas` | secuencias dependientes | mide la degradación por cadena (*ChainSWE*) |
| `seguridad` | escenarios de F7.02 | inyección, envenenamiento y operaciones destructivas |

**Protocolo por tarea y backend**

1. Checkout de `base` en una sandbox limpia (N1, o N2 si la tarea necesita un toolchain especial).
2. Lanzar el backend con el texto de la tarea.
3. Ejecutar `acepta` con el tiempo máximo de la tarea.
4. Si falla y el flujo lo permite: reintento con el relevo del sistema evaluado (hasta 3).
5. Guardar resultado, intentos, tokens de entrada, salida y caché, tiempo de pared, diff, trayectoria
   y violaciones de política.

## 3. Métricas

| Métrica | Definición | Objetivo |
|---|---|---|
| Aceptación a la primera | % de tareas con `acepta` en verde en el primer intento | +12 puntos sobre la línea base medida |
| Aceptación final | % en verde en ≤ 3 intentos | ≥ 97 % |
| Coste de relevo | tokens del intento n+1 dedicados a redescubrir lo del n (*Handoff Debt*) | −50 % frente a reintento sin relevo |
| Tokens por tarea aceptada | entrada + salida, y caché aparte | −30 % frente a la línea base |
| Tiempo de pared por tarea aceptada | — | −25 % |
| Violaciones de reglas | archivos fuera de `toca` y órdenes git prohibidas | 0 |
| Fallos de integración | merges que no se resuelven solos | 0 |
| Falsos rojos de CI | tareas de arreglo sin fallo real | 0 |
| Calidad de trayectoria | puntuación estructural (*Trajectory Quality Score*) | informativa |
| Fidelidad a la especificación | restricciones de la tarea respetadas al final (*SLUMP*) | informativa |

## 4. Benchmarks externos

Para credibilidad fuera del proyecto: SWE-bench Verified o Terminal-Bench con el mismo modelo abierto
en opencode y en PublicCode. Se publica la configuración exacta y las trayectorias.

## 5. Seguridad (F7)

* Seguridad operativa en espacios de trabajo con estado, al estilo SABER: órdenes destructivas,
  borrados y fugas.
* Inyección indirecta por salidas de tools y contenido del repo: se mide el éxito **encubierto**
  (*Will the User Ever Know?*).
* Skills maliciosas o envenenadas (*SkillSafetyBench*, *SkillAttack*, *EvoSkill Injection*).

## 6. Informes

`pcode bench --conjunto smoke --backend X` (F3.13) genera `bench/resultados/<fecha>-<backend>.jsonl` y
una tabla Markdown. La regresión nocturna (F6.04) compara con el último verde y abre un issue si algo
empeora más de 3 puntos.
