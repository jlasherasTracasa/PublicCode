# Evaluación: SinapsisBench y métricas

«Mejor que opencode» tiene que ser un número. Este documento define cómo se mide.

## 1. Línea base

`tools/bench/linea_base.py --repo ../SO_IA` sobre el flujo actual (opencode + GLM-5.3-Flash +
`ci/orquestador.py`), commit `2ef8d59`:

| Métrica | Valor |
|---|---|
| Tareas | 84 (83 hechas, 1 bloqueada) |
| Aceptadas a la primera | 61 (**72,6 %**) |
| Distribución de intentos | 1: 61 · 2: 15 · 3: 6 · 4: 2 |
| Tareas con 3 o más intentos | 8 |
| Tareas `CI-arreglo-*` | 27 |
| Aceptación OK / FALLO en el log | 70 / 32 |
| Integraciones fallidas en el log | 17 |
| Reinicios del orquestador | 15 |
| Líneas de pegamento (`orquestador.py`, `.sh`, `agente.sh`, `integrar.sh`, `sync.sh`) | 456 |

Pendiente en F0.04: tokens y tiempo por tarea (no están en el log actual; se miden re-ejecutando el
smoke con opencode).

## 2. SinapsisBench

**Tareas**: las de SO_IA con criterio `acepta`, extraídas con `tools/bench/extraer_tareas.py`. Cada
una trae su commit base (estado del repo cuando pudo lanzarse) y el commit en que se aceptó.

**Conjuntos**

| Conjunto | Tamaño | Uso |
|---|---|---|
| `smoke` | 20 (10 aislado, 6 kernel, 2 cadenas de 2) | cada cambio del arnés; ~1 h con GLM |
| `completo` | todas las aceptadas (~80) | cada noche (F6.04) y en cada hito de fase |
| `cadenas` | secuencias dependientes | mide la degradación por cadena (*ChainSWE*) |

**Protocolo por tarea y backend**

1. Checkout de `base` en una sandbox limpia (N1 para `aislado`, N2 con `sinapsis-dev` para `kernel`).
2. Lanzar el backend con el texto de la tarea (el mismo `ci/tareas/<id>.md`).
3. Ejecutar `acepta` (con el tiempo máximo de SO_IA, 50 min).
4. Si falla y el flujo lo permite: reintento con el relevo del sistema evaluado (hasta 3).
5. Guardar resultado, intentos, tokens de entrada, salida y caché, tiempo de pared, diff, trayectoria
   y violaciones de política.

**Comparaciones**

* **Arnés**: mismo modelo (GLM-5.3-Flash) en opencode frente a PublicCode (`pcode-core`). Mide el
  valor del arnés.
* **Backends**: Claude Code, Copilot, Gemini, opencode y PublicCode con GLM, cada uno con su mejor
  configuración. Alimenta al router.
* **Ablaciones**: con y sin cada mecanismo (hook de parada, detector de atasco, compactación,
  memoria y skills), para que cada pieza demuestre lo que aporta.

## 3. Métricas

| Métrica | Definición | Objetivo |
|---|---|---|
| Aceptación a la primera | % de tareas con `acepta` en verde en el primer intento | ≥ 85 % (GLM) |
| Aceptación final | % en verde en ≤ 3 intentos | ≥ 97 % |
| Coste de relevo | tokens del intento n+1 dedicados a redescubrir lo del n (*Handoff Debt*) | −50 % frente a reintento sin relevo |
| Tokens por tarea aceptada | entrada + salida, y caché aparte | −30 % frente a la línea base |
| Tiempo de pared por tarea aceptada | — | −25 % |
| Violaciones de reglas | `tools/bench/reglas.py`: archivos fuera de `toca` y órdenes git prohibidas | 0 |
| Fallos de integración | merges que no se resuelven solos | 0 |
| CI fantasma | tareas `CI-arreglo` sin fallo real | 0 |
| Calidad de trayectoria | puntuación estructural (*Trajectory Quality Score*) | informativa |
| Fidelidad a la especificación | restricciones de la tarea respetadas al final (*SLUMP*) | informativa |

## 4. Benchmarks externos

Para credibilidad fuera de SO_IA: SWE-bench Verified o Terminal-Bench con el mismo modelo abierto en
opencode y en PublicCode. Se publica la configuración exacta y las trayectorias.

## 5. Seguridad (F7)

* Seguridad operativa en espacios de trabajo con estado, al estilo SABER: órdenes destructivas,
  borrados y fugas.
* Inyección indirecta por salidas de tools y contenido del repo: se mide el éxito **encubierto**
  (*Will the User Ever Know?*).
* Skills maliciosas o envenenadas (*SkillSafetyBench*, *SkillAttack*, *EvoSkill Injection*).

## 6. Informes

`pcode bench --conjunto smoke --backend X` (F3.13) genera `bench/resultados/<fecha>-<backend>.jsonl`
y una tabla Markdown. La regresión nocturna (F6.04) compara con el último verde y abre un issue si
algo empeora más de 3 puntos.
