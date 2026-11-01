//! `pcode spec`: desarrollo guiado por especificaciones. La persona escribe `specs/<nombre>/spec.md`
//! y PublicCode planifica (diseño y tareas), implementa tarea a tarea hasta que pasa la comprobación
//! de cada una y verifica todos los criterios. La lógica determinista está en `pcode-spec`.
use crate::provider::{build_provider, pick_model, system_prompt};
use pcode_config::Config;
use pcode_core::{Agent, AgentConfig, AgentEvent, Budget, Outcome, StopHook};
use pcode_models::{ChatRequest, Message};
use pcode_policy::{AuditLog, Policy};
use pcode_spec::prompts::{self, Replan};
use pcode_spec::status::{self as st, PLAN_FILE, SPECS_DIR, TASKS_FILE};
use pcode_spec::tasks::{self, TaskList};
use pcode_spec::{Phase, Verdict};
use pcode_tools::{ToolContext, ToolRegistry};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Modelo y configuración (comunes a los pasos que usan el modelo).
#[derive(Debug, Clone, clap::Args)]
pub struct ModelArgs {
    /// Modelo.
    #[arg(long = "modelo")]
    pub model: Option<String>,
    /// Proveedor de `[proveedores]`.
    #[arg(long = "proveedor")]
    pub provider: Option<String>,
    /// Configuración del proyecto.
    #[arg(long = "config", default_value = "pcode.toml")]
    pub config: PathBuf,
}

/// Subcomandos de `pcode spec`.
#[derive(Debug, clap::Subcommand)]
pub enum SpecCmd {
    /// Lista las especificaciones del proyecto y en qué fase está cada una.
    Lista {
        /// `texto` o `json`.
        #[arg(long = "formato", default_value = "texto")]
        format: String,
    },
    /// Crea `specs/<nombre>/spec.md` (con `--desde`, el agente redacta un borrador).
    Nueva {
        /// Nombre (minúsculas, números, - y _).
        nombre: String,
        /// Título (por defecto, el nombre).
        #[arg(long = "titulo")]
        title: Option<String>,
        /// Descripción en una o dos frases para que el agente redacte el borrador.
        #[arg(long = "desde")]
        from: Option<String>,
        #[command(flatten)]
        m: ModelArgs,
    },
    /// Requisitos, tareas, trazabilidad, cambios desde el plan y última verificación.
    Estado {
        /// Nombre.
        nombre: String,
        /// `texto` o `json`.
        #[arg(long = "formato", default_value = "texto")]
        format: String,
    },
    /// Genera `plan.md` y `tareas.md`. Si ya hay plan, solo replanifica lo que cambió.
    Planificar {
        /// Nombre.
        nombre: String,
        /// Rehacer el plan entero aunque la especificación no haya cambiado.
        #[arg(long = "desde-cero")]
        fresh: bool,
        #[command(flatten)]
        m: ModelArgs,
    },
    /// Implementa las tareas pendientes, cada una hasta que pasa su `acepta`, y verifica.
    Implementar {
        /// Nombre.
        nombre: String,
        /// Solo esta tarea (aunque ya esté hecha).
        #[arg(long = "tarea")]
        task: Option<String>,
        /// Agente externo (`copilot`, `claude-code`…) en vez del propio.
        #[arg(long = "backend")]
        backend: Option<String>,
        /// Intentos por tarea.
        #[arg(long = "intentos", default_value_t = 2)]
        attempts: u32,
        /// Pasos máximos del agente por intento.
        #[arg(long = "max-pasos", default_value_t = 60)]
        max_steps: u32,
        /// No verificar la especificación al terminar.
        #[arg(long = "sin-verificar")]
        no_verify: bool,
        #[command(flatten)]
        m: ModelArgs,
    },
    /// Ejecuta todas las comprobaciones y da el veredicto de cada requisito.
    Verificar {
        /// Nombre.
        nombre: String,
        /// `texto` o `json`.
        #[arg(long = "formato", default_value = "texto")]
        format: String,
        /// Segundos máximos por orden.
        #[arg(long = "tiempo", default_value_t = 900)]
        timeout: u64,
    },
}

const PHASE_TEXT: [(Phase, &str, &str); 5] = [
    (Phase::Plan, "planificar", "pcode spec planificar"),
    (Phase::Replan, "replanificar", "pcode spec planificar"),
    (Phase::Implement, "implementar", "pcode spec implementar"),
    (Phase::Verify, "verificar", "pcode spec verificar"),
    (Phase::Done, "hecha", ""),
];

fn phase_text(p: Phase) -> (&'static str, &'static str) {
    PHASE_TEXT
        .iter()
        .find(|(x, _, _)| *x == p)
        .map(|(_, a, b)| (*a, *b))
        .unwrap_or(("", ""))
}

fn verdict_sym(v: Verdict) -> &'static str {
    match v {
        Verdict::Ok => "✓",
        Verdict::Failed => "✗",
        Verdict::NotRun => "⏳",
        Verdict::NoChecks => "○",
    }
}

/// Las últimas `n` líneas de un texto.
fn tail(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Ejecuta una orden de comprobación en `root` con tiempo máximo: (ok, código, final de la salida, ms).
pub async fn run_check(
    root: &Path,
    cmd: &str,
    timeout: Duration,
) -> (bool, Option<i32>, String, u64) {
    let t0 = Instant::now();
    let mut c = pcode_tools::shell_command(cmd);
    c.current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let ms = |t0: Instant| t0.elapsed().as_millis() as u64;
    let child = match c.spawn() {
        Ok(c) => c,
        Err(e) => return (false, None, format!("no se pudo lanzar: {e}"), ms(t0)),
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            (
                out.status.success(),
                out.status.code(),
                tail(text.trim_end(), 25),
                ms(t0),
            )
        }
        Ok(Err(e)) => (false, None, e.to_string(), ms(t0)),
        Err(_) => (
            false,
            None,
            format!("sin terminar en {} s", timeout.as_secs()),
            ms(t0),
        ),
    }
}

fn describe_call(name: &str, args: &serde_json::Value) -> String {
    let arg = ["path", "command", "pattern", "query", "ruta", "orden"]
        .iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("");
    let arg: String = arg.lines().next().unwrap_or("").chars().take(90).collect();
    format!("  · {name} {arg}").trim_end().to_string()
}

fn observer() -> pcode_core::Observer {
    Arc::new(|e: &AgentEvent| {
        if let AgentEvent::ToolCall { name, arguments } = e {
            println!("{}", describe_call(name, arguments));
        }
    })
}

/// Registro con las tools de solo lectura (para planificar sin tocar nada).
pub fn readonly_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    let tools: Vec<Arc<dyn pcode_tools::Tool>> = vec![
        Arc::new(pcode_tools::ReadFile),
        Arc::new(pcode_tools::Grep),
        Arc::new(pcode_tools::Glob),
        Arc::new(pcode_tools::Outline),
    ];
    for t in tools {
        r.register(t).expect("nombres únicos");
    }
    r
}

fn title_from(name: &str) -> String {
    let s = name.replace(['-', '_'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Ejecuta `pcode spec …` y devuelve el código de salida.
pub async fn spec_cmd(c: Option<SpecCmd>) -> anyhow::Result<i32> {
    let root = std::env::current_dir()?;
    match c.unwrap_or(SpecCmd::Lista {
        format: "texto".into(),
    }) {
        SpecCmd::Lista { format } => Ok(lista(&root, &format)),
        SpecCmd::Nueva {
            nombre,
            title,
            from,
            m,
        } => nueva(&root, &nombre, title, from, &m).await,
        SpecCmd::Estado { nombre, format } => estado(&root, &nombre, &format),
        SpecCmd::Planificar { nombre, fresh, m } => planificar(&root, &nombre, fresh, &m).await,
        SpecCmd::Implementar {
            nombre,
            task,
            backend,
            attempts,
            max_steps,
            no_verify,
            m,
        } => {
            let code = implementar(
                &root,
                &nombre,
                task.as_deref(),
                backend.as_deref(),
                attempts.max(1),
                max_steps,
                &m,
            )
            .await?;
            if code != 0 || no_verify {
                return Ok(code);
            }
            println!();
            verificar(&root, &nombre, "texto", Duration::from_secs(900)).await
        }
        SpecCmd::Verificar {
            nombre,
            format,
            timeout,
        } => verificar(&root, &nombre, &format, Duration::from_secs(timeout.max(1))).await,
    }
}

fn lista(root: &Path, format: &str) -> i32 {
    let l = st::list(root);
    if format == "json" {
        println!("{}", serde_json::to_string(&l).unwrap_or_default());
        return 0;
    }
    if l.is_empty() {
        println!(
            "No hay especificaciones en {SPECS_DIR}/. Crea una: pcode spec nueva <nombre> [--desde \"lo que quieres\"]"
        );
        return 0;
    }
    for s in &l {
        match (&s.phase, &s.error) {
            (Some(p), _) => println!(
                "{:<22} {:<13} {} de {} requisitos verificados · {} de {} tareas · {}",
                s.name,
                phase_text(*p).0,
                s.verified,
                s.requirements,
                s.tasks_done,
                s.tasks_total,
                s.title
            ),
            (None, Some(e)) => println!(
                "{:<22} ✗ spec.md: {}",
                s.name,
                e.lines().next().unwrap_or("")
            ),
            _ => {}
        }
    }
    0
}

async fn nueva(
    root: &Path,
    name: &str,
    title: Option<String>,
    from: Option<String>,
    m: &ModelArgs,
) -> anyhow::Result<i32> {
    let title = title.unwrap_or_else(|| title_from(name));
    let mut content = pcode_spec::template(&title);
    if let Some(desc) = from.filter(|d| !d.trim().is_empty()) {
        if !st::valid_name(name) {
            anyhow::bail!("nombre no válido: «{name}» (minúsculas, números, - y _)");
        }
        let (cfg, _) = Config::load_layered(&m.config)?;
        let sel = build_provider(&cfg, m.provider.as_deref())?;
        let model = pick_model(m.model.as_deref(), &cfg, &sel);
        println!("✦ Redactando el borrador de «{name}»…");
        let mut messages = vec![
            Message::system(format!(
                "{}\n\n{}",
                system_prompt(&cfg, root),
                prompts::WRITER_SYSTEM
            )),
            Message::user(prompts::writer_prompt(name, &desc)),
        ];
        let mut drafted = None;
        for _ in 0..2 {
            let req = ChatRequest {
                model: model.clone(),
                messages: messages.clone(),
                tools: vec![],
                max_tokens: Some(6000),
                ..Default::default()
            };
            let r = sel.provider.chat_stream(&req, &|_| {}).await?;
            let text = prompts::strip_fence(&r.message.content);
            match pcode_spec::parse_spec(name, &text) {
                Ok(_) => {
                    drafted = Some(text);
                    break;
                }
                Err(e) => {
                    messages.push(Message::assistant(r.message.content.clone()));
                    messages.push(Message::user(format!(
                        "No se puede usar todavía: {e}\nVuelve a escribir spec.md completo con el formato indicado."
                    )));
                }
            }
        }
        content = match drafted {
            Some(t) => t,
            None => {
                println!(
                    "⚠ El borrador no tenía el formato esperado: se crea la plantilla con tu descripción."
                );
                content.replace(
                    "Qué se quiere conseguir, para quién y por qué. Dos o tres frases.",
                    desc.trim(),
                )
            }
        };
    }
    let p = st::create(root, name, &content).map_err(anyhow::Error::msg)?;
    let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
    println!("Creada {}", rel.replace('\\', "/"));
    println!("Escribe (o revisa) los requisitos y después: pcode spec planificar {name}");
    Ok(0)
}

fn estado(root: &Path, name: &str, format: &str) -> anyhow::Result<i32> {
    let s = st::status(root, name).map_err(anyhow::Error::msg)?;
    if format == "json" {
        println!("{}", serde_json::to_string(&s)?);
        return Ok(0);
    }
    let (fase, siguiente) = phase_text(s.phase);
    println!("{}  ({} · fase: {fase})", s.title, s.dir);
    println!();
    println!("Requisitos");
    for r in &s.requirements {
        let tareas: Vec<String> = r
            .tasks
            .iter()
            .map(|t| {
                let done = s.tasks.iter().any(|x| &x.id == t && x.done);
                format!("{t}{}", if done { "✓" } else { "" })
            })
            .collect();
        println!(
            "  {} {:<6} {:<40} {}{}",
            verdict_sym(r.verdict),
            r.id,
            r.title,
            if tareas.is_empty() {
                "sin tareas".to_string()
            } else {
                tareas.join(" ")
            },
            if r.changed { "  ⚠ cambió" } else { "" }
        );
    }
    if !s.tasks.is_empty() {
        println!();
        println!("Tareas {} de {}", s.tasks_done, s.tasks.len());
        for t in &s.tasks {
            println!(
                "  [{}] {:<4} {}  ({}){}",
                if t.done { "x" } else { " " },
                t.id,
                t.title,
                t.requirements.join(", "),
                t.accept
                    .as_ref()
                    .map(|a| format!("  acepta: {a}"))
                    .unwrap_or_default()
            );
        }
    }
    for w in &s.warnings {
        println!("⚠ {w}");
    }
    if !s.drift.is_empty() {
        println!();
        println!(
            "⚠ La especificación cambió desde el plan: cambiados {:?}, nuevos {:?}, eliminados {:?}",
            s.drift.changed, s.drift.added, s.drift.removed
        );
    }
    for p in &s.problems {
        println!("✗ plan: {p}");
    }
    for q in &s.open_questions {
        println!("? {q}");
    }
    if !siguiente.is_empty() {
        println!();
        println!("Siguiente paso: {siguiente} {name}");
    }
    Ok(0)
}

/// Un agente para un paso. El proveedor se construye una vez por orden y se comparte entre
/// intentos.
#[allow(clippy::too_many_arguments)] // Cada argumento es una pieza distinta del agente.
fn agent_for(
    cfg: &Config,
    root: &Path,
    m: &ModelArgs,
    sel: &crate::provider::Selected,
    tools: ToolRegistry,
    system: &str,
    max_steps: u32,
    hook: Option<StopHook>,
) -> anyhow::Result<Agent> {
    let model = pick_model(m.model.as_deref(), cfg, sel);
    crate::run::ensure_state_dir(root)?;
    let audit = AuditLog::open(&root.join(".pcode/audit.jsonl"))?;
    Ok(Agent::new(
        sel.provider.clone(),
        tools,
        Policy::new(crate::run::load_capabilities(&m.config)),
        Some(audit),
        AgentConfig {
            agent_id: "spec".into(),
            model,
            system_prompt: format!("{}\n\n{system}", system_prompt(cfg, root)),
            budget: Budget {
                max_steps,
                max_context_tokens: cfg
                    .preferences
                    .max_context_tokens
                    .unwrap_or(sel.context_limit),
                ..Default::default()
            },
            stop_hook: hook,
        },
        ToolContext {
            root: root.to_path_buf(),
            timeout: Duration::from_secs(600),
        },
    )
    .with_observer(observer()))
}

async fn planificar(root: &Path, name: &str, fresh: bool, m: &ModelArgs) -> anyhow::Result<i32> {
    let l = st::load(root, name).map_err(anyhow::Error::msg)?;
    for w in &l.spec.warnings {
        println!("⚠ {w}");
    }
    let d = pcode_spec::drift(&l.spec, &l.tasks);
    let has_plan = !l.tasks.tasks.is_empty();
    if has_plan && !fresh && d.is_empty() && pcode_spec::validate_plan(&l.spec, &l.tasks).is_empty()
    {
        println!(
            "El plan está al día con la especificación: no hay nada que replanificar (--desde-cero lo rehace)."
        );
        return Ok(0);
    }
    let (cfg, _) = Config::load_layered(&m.config)?;
    let sel = build_provider(&cfg, m.provider.as_deref())?;
    let replan = (has_plan && !fresh).then(|| Replan {
        plan: l.plan_text.as_deref().unwrap_or(""),
        tasks: l.tasks_text.as_deref().unwrap_or(""),
        drift: &d,
    });
    let old = if replan.is_some() {
        l.tasks.clone()
    } else {
        TaskList::default()
    };
    match &replan {
        Some(_) => println!(
            "✦ Replanificando «{}»: cambiados {:?}, nuevos {:?}, eliminados {:?}",
            l.spec.title, d.changed, d.added, d.removed
        ),
        None => println!(
            "✦ Planificando «{}» ({} requisitos)…",
            l.spec.title,
            l.spec.requirements.len()
        ),
    }
    let mut repair: Option<(String, Vec<String>)> = None;
    // Un intento y, si el plan no pasa la validación, una reparación con los problemas (PaveDSL).
    for _ in 0..2 {
        let prompt = prompts::planner_prompt(
            name,
            &l.spec_text,
            replan,
            repair.as_ref().map(|(t, p)| (t.as_str(), p.as_slice())),
        );
        let mut agent = agent_for(
            &cfg,
            root,
            m,
            &sel,
            readonly_registry(),
            prompts::PLANNER_SYSTEM,
            30,
            None,
        )?;
        let report = agent.run(&prompt).await;
        let text = match report.outcome {
            Outcome::Done { final_message } => final_message,
            Outcome::Failed(e) | Outcome::Stuck(e) => anyhow::bail!("el planificador falló: {e}"),
            Outcome::BudgetExhausted => {
                anyhow::bail!("el planificador agotó sus pasos sin terminar el plan")
            }
        };
        let problems = match prompts::parse_planner_output(&text) {
            Err(e) => vec![e],
            Ok((plan, tareas)) => {
                let tareas = prompts::merge_done(&old, &tareas, &d);
                let parsed = tasks::parse_tasks(&tareas);
                let problems = pcode_spec::validate_plan(&l.spec, &parsed);
                if problems.is_empty() {
                    let dir = st::dir(root, name);
                    std::fs::write(dir.join(PLAN_FILE), plan)?;
                    std::fs::write(dir.join(TASKS_FILE), tasks::stamp(&tareas, &l.spec))?;
                    println!(
                        "✓ {SPECS_DIR}/{name}/{PLAN_FILE} y {TASKS_FILE}: {} tareas ({} ya hechas)",
                        parsed.tasks.len(),
                        parsed.done()
                    );
                    for t in &parsed.tasks {
                        println!(
                            "  [{}] {} {}  ({})",
                            if t.done { "x" } else { " " },
                            t.id,
                            t.title,
                            t.requirements.join(", ")
                        );
                    }
                    let q = st::open_questions(&std::fs::read_to_string(dir.join(PLAN_FILE))?);
                    if !q.is_empty() {
                        println!(
                            "Preguntas abiertas (resuélvelas en spec.md antes de implementar):"
                        );
                        for x in q {
                            println!("  ? {x}");
                        }
                    }
                    println!("Revisa el plan y después: pcode spec implementar {name}");
                    return Ok(0);
                }
                problems
            }
        };
        println!("⚠ El plan necesita correcciones:");
        for p in &problems {
            println!("  - {p}");
        }
        repair = Some((text, problems));
    }
    println!("✗ No se pudo obtener un plan válido: revisa spec.md o vuelve a intentarlo.");
    Ok(2)
}

async fn implementar(
    root: &Path,
    name: &str,
    only: Option<&str>,
    backend: Option<&str>,
    attempts: u32,
    max_steps: u32,
    m: &ModelArgs,
) -> anyhow::Result<i32> {
    let l = st::load(root, name).map_err(anyhow::Error::msg)?;
    if l.tasks.tasks.is_empty() {
        println!("No hay plan todavía: pcode spec planificar {name}");
        return Ok(2);
    }
    let problems = pcode_spec::validate_plan(&l.spec, &l.tasks);
    if !problems.is_empty() {
        println!("✗ El plan tiene problemas (pcode spec planificar {name} los corrige):");
        for p in problems {
            println!("  - {p}");
        }
        return Ok(2);
    }
    let d = pcode_spec::drift(&l.spec, &l.tasks);
    if !d.is_empty() {
        println!(
            "⚠ La especificación cambió desde el plan ({}): replanifica antes con pcode spec planificar {name}",
            d.changed
                .iter()
                .chain(&d.added)
                .chain(&d.removed)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Ok(2);
    }
    let q = l
        .plan_text
        .as_deref()
        .map(st::open_questions)
        .unwrap_or_default();
    if !q.is_empty() {
        println!("⚠ El plan tiene preguntas abiertas; se sigue con lo que dice la especificación:");
        for x in &q {
            println!("  ? {x}");
        }
    }
    let order: Vec<pcode_spec::SpecTask> = match only {
        Some(id) => vec![
            l.tasks
                .get(id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no existe la tarea {id} en {TASKS_FILE}"))?,
        ],
        None => tasks::pending_in_order(&l.tasks)
            .into_iter()
            .cloned()
            .collect(),
    };
    if order.is_empty() {
        println!("Todas las tareas están hechas.");
        return Ok(0);
    }
    let (cfg, _) = Config::load_layered(&m.config)?;
    let sel = match backend {
        None => Some(build_provider(&cfg, m.provider.as_deref())?),
        Some(_) => None,
    };
    let tasks_path = st::dir(root, name).join(TASKS_FILE);
    for (i, t) in order.iter().enumerate() {
        let accept = t.accept.clone().unwrap_or_default();
        println!(
            "▶ {} · {}  ({})  [{} de {}]",
            t.id,
            t.title,
            t.requirements.join(", "),
            i + 1,
            order.len()
        );
        let mut failure: Option<String> = None;
        let mut ok = false;
        for attempt in 1..=attempts {
            if attempt > 1 {
                println!("  ↻ intento {attempt} de {attempts}");
            }
            let prompt =
                prompts::task_prompt(name, &l.spec, t, l.plan_text.as_deref(), failure.as_deref());
            match backend {
                Some(b) => {
                    crate::backend_run::run_with_backend(
                        &cfg,
                        b,
                        &prompt,
                        Some(&accept),
                        m.model.as_deref(),
                        crate::backend_run::Apply::Always,
                    )
                    .await?;
                }
                None => {
                    let hook = StopHook {
                        command: accept.clone(),
                        max_rounds: 3,
                        timeout: Duration::from_secs(900),
                        checklist: vec![],
                    };
                    let mut agent = agent_for(
                        &cfg,
                        root,
                        m,
                        sel.as_ref().expect("proveedor del agente propio"),
                        pcode_tools::native_registry(),
                        prompts::IMPLEMENTER_SYSTEM,
                        max_steps,
                        Some(hook),
                    )?
                    .with_checkpoints();
                    let report = agent.run(&prompt).await;
                    match report.outcome {
                        Outcome::Done { final_message } => {
                            let msg = final_message.trim();
                            if !msg.is_empty() {
                                println!("{msg}");
                            }
                        }
                        Outcome::Failed(e) | Outcome::Stuck(e) => println!("  ⚠ el agente: {e}"),
                        Outcome::BudgetExhausted => println!("  ⚠ el agente agotó sus pasos"),
                    }
                }
            }
            let (pass, code, out, ms) = run_check(root, &accept, Duration::from_secs(900)).await;
            if pass {
                let text = std::fs::read_to_string(&tasks_path)?;
                if let Some(s) = tasks::set_done(&text, &t.id, true) {
                    std::fs::write(&tasks_path, s)?;
                }
                println!(
                    "✓ {} hecha · `{accept}` en verde ({:.1} s)",
                    t.id,
                    ms as f64 / 1000.0
                );
                ok = true;
                break;
            }
            println!(
                "✗ {}: `{accept}` falló{}",
                t.id,
                code.map(|c| format!(" (código {c})")).unwrap_or_default()
            );
            for line in tail(&out, 8).lines() {
                println!("    {line}");
            }
            failure = Some(format!("$ {accept}\n{out}"));
        }
        if !ok {
            println!(
                "Parado en {}: corrige la especificación o el plan, o reinténtalo con pcode spec implementar {name} --tarea {}",
                t.id, t.id
            );
            return Ok(2);
        }
    }
    println!("✓ {} tareas hechas", order.len());
    Ok(0)
}

async fn verificar(
    root: &Path,
    name: &str,
    format: &str,
    timeout: Duration,
) -> anyhow::Result<i32> {
    let l = st::load(root, name).map_err(anyhow::Error::msg)?;
    let checks = st::checks(&l.spec, &l.tasks);
    let json = format == "json";
    if checks.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({"ok": false, "error": "sin comprobaciones"})
            );
        } else {
            println!(
                "No hay ninguna comprobación: añade «- acepta: `orden`» a los requisitos o planifica con pcode spec planificar {name}"
            );
        }
        return Ok(2);
    }
    if !json {
        println!("Verificando «{}» ({} órdenes)", l.spec.title, checks.len());
    }
    let mut results = vec![];
    for (cmd, sources) in checks {
        let (ok, code, out, millis) = run_check(root, &cmd, timeout).await;
        if !json {
            println!(
                "  {} {cmd}  ({}) {:.1} s",
                if ok { "✓" } else { "✗" },
                sources.join(", "),
                millis as f64 / 1000.0
            );
            if !ok {
                println!(
                    "      código {}",
                    code.map(|c| c.to_string()).unwrap_or("—".into())
                );
                for line in tail(&out, 6).lines() {
                    println!("      {line}");
                }
            }
        }
        results.push(st::CheckResult {
            command: cmd,
            sources,
            ok,
            code,
            tail: out,
            millis,
        });
    }
    let v = st::Verification {
        time: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        results,
    };
    st::save_verification(root, name, &v)?;
    let verd = st::verdicts(&l.spec, &l.tasks, Some(&v));
    let ok_n = verd.values().filter(|v| **v == Verdict::Ok).count();
    let all = ok_n == verd.len();
    if json {
        println!(
            "{}",
            serde_json::json!({"ok": all, "verificados": ok_n, "requisitos": verd.len(), "veredictos": verd, "resultados": v.results})
        );
    } else {
        println!();
        for r in &l.spec.requirements {
            let v = verd.get(&r.id).copied().unwrap_or(Verdict::NoChecks);
            println!(
                "  {} {:<6} {}{}",
                verdict_sym(v),
                r.id,
                r.title,
                if v == Verdict::NoChecks {
                    "  (sin ninguna orden que lo compruebe)"
                } else {
                    ""
                }
            );
        }
        println!(
            "{} {ok_n} de {} requisitos verificados",
            if all { "✓" } else { "✗" },
            verd.len()
        );
    }
    Ok(if all { 0 } else { 2 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titulo_desde_el_nombre_y_llamadas() {
        assert_eq!(title_from("exportar-csv"), "Exportar csv");
        assert_eq!(title_from("api_v2"), "Api v2");
        assert_eq!(
            describe_call(
                "escribir",
                &serde_json::json!({"path": "src/a.rs", "content": "x"})
            ),
            "  · escribir src/a.rs"
        );
        assert_eq!(
            describe_call("bash", &serde_json::json!({"command": "cargo test\nmás"})),
            "  · bash cargo test"
        );
        let r = readonly_registry();
        let names: Vec<String> = r
            .specs_filtered(|_, _| true)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n == "escribir" || n == "editar" || n == "bash"),
            "{names:?}"
        );
        assert!(names.iter().any(|n| n == "esquema"));
    }

    #[tokio::test]
    async fn comprobacion_con_tiempo_maximo() {
        let d = tempfile::tempdir().unwrap();
        let (ok, code, out, _) = run_check(d.path(), "echo hola", Duration::from_secs(20)).await;
        assert!(ok && code == Some(0) && out.contains("hola"));
        let (ok, code, _, _) = run_check(d.path(), "exit 3", Duration::from_secs(20)).await;
        assert!(!ok && code == Some(3));
        #[cfg(unix)]
        {
            let (ok, code, out, _) =
                run_check(d.path(), "sleep 5", Duration::from_millis(300)).await;
            assert!(!ok && code.is_none() && out.contains("sin terminar"));
        }
    }
}
