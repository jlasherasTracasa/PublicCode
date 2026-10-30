# Arquitectura de PublicCode

Visión de conjunto y diagrama en el [README](../README.md#cómo-funciona). Aquí está cada subsistema:
responsabilidad, tipos clave, decisiones y tareas. Los nombres de crate son los de
[AGENTS.md](../AGENTS.md#estructura).

```
pcode-cli ─┬─ pcode-flow ─┬─ pcode-backends ─┬─ pcode-core ─┬─ pcode-models
           │              │                  │              ├─ pcode-tools
           │              │                  │              └─ pcode-policy
           │              │                  └─ pcode-sandbox
           │              └─ pcode-store
           ├─ pcode-skills
           ├─ pcode-mcp
           └─ pcode-config
```

## Configuración

`pcode-config` (F1.01). Un `pcode.toml` por proyecto, opcional. Claves en español, campos de Rust
en inglés. Los secretos no van en el archivo: `{env:VARIABLE}`. Secciones: `[proyecto]`,
`[proveedores.*]`, `[backends.*]`, `[roles]`, `[sandbox]` y `[politica]`. El esquema completo lo fija
F1.01; el ejemplo está en el README.

## Modelos

`pcode-models` (F1.02–F1.07, F1.22, F1.23).

* Tipos neutros: `Message`, `ToolCall`, `ToolSpec`, `ChatRequest`, `ChatResponse`, `Usage`,
  `StopReason`.
* `trait Provider { async fn chat(&ChatRequest) -> Result<ChatResponse, ModelError> }`.
* Proveedores: `OpenAiCompatProvider` (LiteLLM, vLLM, Ollama y cualquier modelo propio),
  `AnthropicProvider` y `GeminiProvider`. Cada uno tiene funciones **puras** de conversión, probadas
  sin red, y un cliente HTTP fino probado contra un servidor falso local.
* **Reparación de tool calls** (F1.07): los modelos abiertos devuelven JSON roto (vallas de código,
  comas finales, comillas simples, doble codificación); se repara antes de darlo por error.
* **Streaming** (F1.06): parser SSE incremental y acumulador de deltas.
* **Router** (F4.07): elige proveedor y modelo por rol (planificador, implementador, revisor,
  crítico, compactador) y por histórico del banco.

## Tools

`pcode-tools` (F1.08–F1.12).

* `trait Tool { spec(), effects(), call(args, ctx) }`. Los **efectos** (`Read`, `Write`, `Network`,
  `Exec`, `Irreversible`) son la base de la política: el motor no necesita conocer cada tool para
  decidir.
* El registro ordena por **privilegio**, así que el modelo ve primero las tools de menos privilegio
  (*When Lower Privileges Suffice*).
* Rutas siempre resueltas dentro de `root` (`resolve_in_root`).
* Nativas en F1: `leer`, `escribir`, `editar`, `buscar`, `listar` y `bash`. Llegan después
  `editar_simbolo` (AST, F4.04), `contexto` (F4.01), `preguntar` (F4.11) y `guion` (F4.12).
* Externas: cualquier servidor MCP (sus tools se envuelven con efectos declarados en la
  configuración) y tools WASM (N0) más adelante.

## Política

`pcode-policy` (F1.13–F1.15).

* `Capabilities`: rutas de lectura y escritura, órdenes permitidas y denegadas, dominios de red,
  tools ocultas, efectos denegados y si se pregunta por los irreversibles.
* `Policy` = pila de capas: la decisión es la más restrictiva de todas. Un subagente recibe
  `policy.child(caps)` y **no puede ampliar** lo que tenía su padre.
* Las tools no visibles **no se anuncian** al modelo (*Prompts Don't Protect*).
* Análisis de shell (F1.14): `bash -c '…'`, `$( … )`, `&&`, `sudo` y `env` se descomponen en órdenes
  simples y cada una se comprueba.
* Auditoría (F1.15): JSONL append-only con hash encadenado y **procedencia** de cada acción
  (*Trust Cascade*). `verify` detecta manipulaciones.
* Hooks: el de parada (F1.18) en F1; `antes_de_tool`, `despues_de_tool`, `al_compactar` y
  `al_terminar_subagente` en F2 como órdenes de shell configurables, igual que los hooks de
  Claude Code.

## Bucle

`pcode-core` (F1.16–F1.18, F1.21; F4.x).

1. Construye la petición con las tools visibles.
2. Ejecuta las llamadas pasando cada una por la política y la auditoría.
3. Comprime las salidas (F1.17).
4. Al terminar, el **hook de parada** ejecuta la aceptación y comprueba la lista de restricciones;
   si falla, el agente sigue con la salida comprimida (F1.18).
5. El **detector de atasco** (F1.21) da una pista ante la primera señal y para ante la segunda.
6. Presupuestos de pasos y tokens impuestos por el motor.

En F4 se añaden: contexto como tool, compactación que respeta los prefijos para no romper la caché,
estado del proyecto que sobrevive a la compactación, memoria de archivos por facetas y puerta de
incertidumbre.

## Almacenamiento

`pcode-store` (F1.19; F3.03). SQLite en `.pcode/pcode.db`: sesiones, eventos y, desde F3, el estado
de los flujos **por eventos**. El estado se reconstruye leyendo eventos, así que un reinicio no
pierde nada; esto sustituye a `estado.json` y a los hacks de recarga del orquestador de SO_IA.

## Sandbox

`pcode-sandbox` (F2.01–F2.05, F2.19, F2.20). Niveles N0–N3. Diseño completo en
[mini-sandboxes.md](mini-sandboxes.md). Interfaz:

```rust
pub struct SandboxSpec { pub root: PathBuf, pub level: Level, pub network: Network, pub writable: Vec<PathBuf>, pub timeout: Duration }
pub struct Sandbox { /* capa, estado */ }
impl Sandbox {
    pub fn create(spec: SandboxSpec) -> Result<Sandbox, SandboxError>;
    pub async fn run(&self, argv: &[String], env: &[(String, String)]) -> Result<ExitStatus, SandboxError>;
    pub fn changes(&self) -> Result<Vec<Change>, SandboxError>;
    pub fn apply(self) -> Result<Vec<Change>, SandboxError>;
    pub fn discard(self) -> Result<(), SandboxError>;
}
```

## Backends

`pcode-backends` (F2.06–F2.12). Detalle por agente en [backends.md](backends.md).

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> BackendCaps;               // ¿necesita red? ¿qué dominios? ¿lee AGENTS.md?
    async fn run(&self, job: &Job, sandbox: &Sandbox) -> Result<BackendResult, BackendError>;
}
pub struct Job { pub task: String, pub handoff: Option<Handoff>, pub skills: Vec<SkillRef>, pub accept: Option<String>, pub budget: Budget }
pub struct Handoff { pub tried: Vec<String>, pub failed_because: String, pub critical_step: Option<String>, pub files: Vec<String>, pub boundaries: Boundaries }
pub struct BackendResult { pub transcript: Vec<TranscriptItem>, pub usage: Option<Usage>, pub exit: i32, pub final_message: String }
```

`Handoff` es un objeto tipado, no texto libre: lleva las **fronteras** (capacidades, clase de datos,
audiencia) porque los resúmenes en texto libre las pierden (*Facts Without Rules*), y el **paso
crítico** del fallo anterior (*TRAJDEBUG*, *Handoff Debt*).

## Skills

`pcode-skills` (F2.13–F2.15, F2.17; F6.x).

* Formato: carpeta con `SKILL.md` (frontmatter `name`, `description`; opcionales `depende`,
  `no_usar_cuando` y una sección `## Contrato`) más scripts y recursos.
* Registro con índice ligero (solo nombre y descripción en contexto) y recuperación por
  dependencias (*Graph-of-Skills*).
* **Exportación**: una sola fuente, publicada en el sitio y formato de cada agente (F2.14).
* Ciclo de aprendizaje (F6): destilar → cuarentena → evals con y sin la skill → activar o retirar.

## Flujos

`pcode-flow` (F3.x).

* Tarea: Markdown con frontmatter o con las cabeceras HTML de SO_IA (compatibles).
* DAG con detección de ciclos; cola con paralelismo por modo (`kernel` = 1 a la vez; `aislado` = N).
* Aceptación en **dos etapas**: primero barata en N1 y después completa en N2 con la imagen del
  proyecto (*Dockerless*, *SWE-World*).
* Reintentos con informe de relevo y escalado de backend en el tercer intento.
* Cola de integración: aplicar el diff, rebase, **regenerar lockfiles** si chocan (`Cargo.lock`) y
  re-validar después del merge. Esto elimina los 17 fallos de integración de SO_IA.
* Clasificador de CI con fixtures (éxito, fallo real, cancelado, infraestructura). Esto elimina las
  tareas fantasma.
* Revisión cruzada: otro backend revisa el diff con la rúbrica del repo y comprueba la seguridad del
  parche (*Agentic Rubrics*, *When "Correct" Is Not Safe*).

## Memoria y contexto

`pcode-core` y `pcode-memory` (F4.x): memoria de archivos por facetas invalidada por hash (*Prism*),
memoria episódica **verificada** (solo experiencia con `acepta` en verde: *MemGuard*,
*Experience-Following*), mapa del repo con tree-sitter, búsqueda híbrida y un subagente bibliotecario
persistente (*Librarian*).

## MCP

`pcode-mcp` (F2.16). Servidor MCP por stdio que expone a **cualquier** agente las tools de PublicCode:
`sandbox_ejecutar`, `sandbox_cambios`, `aceptar`, `buscar_repo`, `memoria` y `papers`. Las tools se
filtran por la política del agente que se conecta. Esquemas mínimos, porque el coste de MCP está en
el esquema y el transporte (*ProMCP*).

## Interfaces

`pcode-cli` desde F1; demonio `pcoded` con API JSON-RPC más SSE (F5.01); TUI (F5.02); panel web
(F5.03); ACP para IDEs (F5.04); GitHub: PR, comentarios y checks (F5.05).

## Telemetría

`tracing` con exportación OpenTelemetry opcional: una traza por tarea, un span por paso, por tool y
por subagente, y tokens y coste por backend. SinapsisBench lee estas trazas.
