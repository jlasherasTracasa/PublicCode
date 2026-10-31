//! `pcode chat` (y `pcode` sin argumentos en una terminal): conversación con el agente.
use crate::provider::{build_provider, pick_model, system_prompt};
use anyhow::Context;
use pcode_config::Config;
use pcode_core::{Agent, AgentConfig, AgentEvent, Approver, Budget, Outcome};
use pcode_models::{Message, Role, Usage};
use pcode_policy::{AuditLog, Policy};
use pcode_store::Store;
use pcode_tools::ToolContext;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Argumentos de `pcode chat`.
#[derive(Debug, clap::Args)]
pub struct ChatArgs {
    /// Primer mensaje (si falta, se pide en el prompt; si stdin no es una terminal, se leen
    /// los mensajes de stdin, uno por línea).
    pub prompt: Vec<String>,
    /// Modelo.
    #[arg(long = "modelo")]
    pub model: Option<String>,
    /// Proveedor de `pcode.toml`.
    #[arg(long = "proveedor")]
    pub provider: Option<String>,
    /// Retoma la última conversación de este directorio.
    #[arg(long = "continuar")]
    pub resume: bool,
    /// Aprueba sin preguntar las acciones que la política pide confirmar (las denegadas siguen
    /// denegadas).
    #[arg(long = "aprobar-todo")]
    pub approve_all: bool,
    /// Pasos máximos por mensaje.
    #[arg(long = "max-pasos", default_value_t = 100)]
    pub max_steps: u32,
    /// Archivo de configuración.
    #[arg(long = "config", default_value = "pcode.toml")]
    pub config: PathBuf,
    /// `texto` (por defecto) o `jsonl`: protocolo de líneas JSON por stdin y stdout para que otro
    /// programa (el demonio, un IDE) maneje la conversación.
    #[arg(long = "formato", default_value = "texto")]
    pub format: String,
}

impl ChatArgs {
    /// Valores por defecto (los mismos que daría `pcode chat` sin opciones).
    pub fn default_args() -> ChatArgs {
        ChatArgs {
            prompt: vec![],
            model: None,
            provider: None,
            resume: false,
            approve_all: false,
            max_steps: 100,
            config: PathBuf::from("pcode.toml"),
            format: "texto".into(),
        }
    }
}

/// Estilo de salida: colores solo en una terminal y sin `NO_COLOR`; los símbolos van siempre.
#[derive(Clone, Copy)]
struct Style(bool);

impl Style {
    fn detect() -> Style {
        Style(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
    }
    fn dim(self, s: &str) -> String {
        if self.0 {
            format!("\x1b[2m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn red(self, s: &str) -> String {
        if self.0 {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(self, s: &str) -> String {
        if self.0 {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

fn shorten(s: &str, max: usize) -> String {
    let one: String = s.lines().next().unwrap_or("").chars().take(max).collect();
    if s.lines().count() > 1 || s.chars().count() > max {
        format!("{one}…")
    } else {
        one
    }
}

/// Resumen de los argumentos de una tool para una línea.
fn brief(args: &serde_json::Value) -> String {
    for k in ["command", "path", "pattern", "query"] {
        if let Some(v) = args.get(k).and_then(|v| v.as_str()) {
            return shorten(v, 100);
        }
    }
    shorten(&args.to_string(), 100)
}

/// Muestra un evento del agente.
fn render(st: Style, e: &AgentEvent) {
    match e {
        AgentEvent::TextDelta(t) => {
            print!("{t}");
            let _ = std::io::stdout().flush();
        }
        AgentEvent::Text(_) => println!("\n"),
        AgentEvent::Info(m) => println!("{}", st.dim(&format!("· {m}"))),
        AgentEvent::ToolCall { name, arguments } => {
            println!(
                "{} {}",
                st.bold(&format!("⏺ {name}")),
                st.dim(&brief(arguments))
            );
        }
        AgentEvent::ToolResult {
            is_error, content, ..
        } => {
            let lines: Vec<&str> = content.lines().collect();
            let mut shown: Vec<String> = lines.iter().take(3).map(|l| shorten(l, 110)).collect();
            if lines.len() > 3 {
                shown.push(format!("… (+{} líneas)", lines.len() - 3));
            }
            if shown.is_empty() {
                shown.push("(sin salida)".into());
            }
            let head = if *is_error { "✗" } else { "⎿" };
            let text = format!("  {head} {}", shown.join("\n    "));
            println!(
                "{}",
                if *is_error {
                    st.red(&text)
                } else {
                    st.dim(&text)
                }
            );
        }
    }
}

fn read_line() -> Option<String> {
    let mut s = String::new();
    match std::io::stdin().read_line(&mut s) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(s.trim_end_matches(['\r', '\n']).to_string()),
    }
}

async fn read_line_async() -> Option<String> {
    tokio::task::spawn_blocking(read_line).await.ok().flatten()
}

/// Pregunta en la terminal antes de una acción que la política marca como `Ask`.
struct TerminalApprover {
    always: AtomicBool,
    interactive: bool,
    style: Style,
}

#[async_trait::async_trait]
impl Approver for TerminalApprover {
    async fn ask(&self, action: &str, reason: &str) -> bool {
        if self.always.load(Ordering::Relaxed) {
            return true;
        }
        if !self.interactive {
            return false;
        }
        println!(
            "{}",
            self.style
                .bold(&format!("⚠ {reason}: {}", shorten(action, 160)))
        );
        print!("  ¿Permitir? [s = sí, t = sí y no volver a preguntar, N = no] ");
        let _ = std::io::stdout().flush();
        match read_line_async().await.as_deref().map(str::trim) {
            Some("s") | Some("S") | Some("y") => true,
            Some("t") | Some("T") => {
                self.always.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }
}

const AYUDA: &str = "Comandos:\n  /ayuda          esta ayuda\n  /nuevo          empieza otra conversación (olvida el historial)\n  /modelo [m]     muestra o cambia el modelo\n  /uso            tokens gastados en esta sesión\n  /compactar      resume la conversación para liberar contexto\n  /plan           modo plan: solo lee y propone (/construir vuelve)\n  @archivo        adjunta un archivo al mensaje\n  !orden          ejecuta una orden y adjunta su salida al próximo mensaje\n  /salir          termina (también Ctrl-D)\nUna línea que acaba en \\ continúa en la siguiente. Ctrl-C interrumpe al agente.";

/// Persiste en SQLite los mensajes nuevos de cada vuelta.
struct Persist {
    store: Store,
    session: Option<i64>,
    saved: usize,
    model: String,
}

impl Persist {
    fn save(&mut self, title: &str, transcript: &[Message]) {
        let sid = match self.session {
            Some(s) => s,
            None => match self.store.create_session(&self.model, &shorten(title, 80)) {
                Ok(s) => {
                    self.session = Some(s);
                    s
                }
                Err(e) => {
                    tracing::warn!("no se pudo guardar la sesión: {e}");
                    return;
                }
            },
        };
        for m in transcript.iter().skip(self.saved) {
            if let Ok(v) = serde_json::to_value(m)
                && let Err(e) = self.store.append_event(sid, "mensaje", &v)
            {
                tracing::warn!("no se pudo guardar un mensaje: {e}");
            }
        }
        self.saved = transcript.len();
    }
}

/// Mensajes de la última sesión del directorio.
fn load_last(store: &Store) -> Option<(i64, Vec<Message>)> {
    let row = store.sessions(1).ok()?.into_iter().next()?;
    let msgs: Vec<Message> = store
        .events(row.id)
        .ok()?
        .into_iter()
        .filter(|e| e.kind == "mensaje")
        .filter_map(|e| serde_json::from_value(e.payload).ok())
        .collect();
    Some((row.id, msgs))
}

/// Lo común a `pcode chat` y `pcode tui`: agente (sin observador ni aprobador), modelo,
/// historial (si `--continuar`) y persistencia.
struct Prepared {
    agent: Agent,
    model: String,
    history: Vec<Message>,
    persist: Persist,
    resumed: Option<(i64, usize)>,
    theme: Option<String>,
    /// Avisos al arrancar (servidores MCP que no se pudieron usar…).
    notices: Vec<String>,
}

async fn prepare(a: &ChatArgs) -> anyhow::Result<Prepared> {
    let (cfg, _) = Config::load_layered(&a.config)?;
    let sel = build_provider(&cfg, a.provider.as_deref())?;
    let model = pick_model(a.model.as_deref(), &cfg, &sel);
    let root = std::env::current_dir()?;
    let sys = system_prompt(&cfg, &root);
    crate::run::ensure_state_dir(&root).context("no se pudo crear .pcode")?;
    let audit = AuditLog::open(&root.join(".pcode/audit.jsonl"))?;
    let store = Store::open(&root.join(".pcode/pcode.db"))?;
    let mut persist = Persist {
        store,
        session: None,
        saved: 0,
        model: model.clone(),
    };
    let mut history: Vec<Message> = vec![];
    let mut resumed = None;
    if a.resume
        && let Some((sid, mut msgs)) = load_last(&persist.store)
        && !msgs.is_empty()
    {
        if msgs.first().is_some_and(|m| m.role == Role::System) {
            msgs[0] = Message::system(sys.clone());
        }
        persist.session = Some(sid);
        persist.saved = msgs.len();
        resumed = Some((sid, msgs.len()));
        history = msgs;
    }
    let mut tools = pcode_tools::native_registry();
    let (extra, mut notices) = crate::provider::mcp_tools(&cfg).await;
    let n_mcp = extra.len();
    for t in extra {
        if let Err(e) = tools.register(t) {
            notices.push(format!("MCP: {e}"));
        }
    }
    if n_mcp > 0 {
        notices.push(format!("{n_mcp} tools de MCP disponibles"));
    }
    let agent = Agent::new(
        sel.provider.clone(),
        tools,
        interactive_policy(&a.config),
        Some(audit),
        AgentConfig {
            agent_id: "chat".into(),
            model: model.clone(),
            system_prompt: sys,
            budget: Budget {
                max_steps: a.max_steps,
                max_context_tokens: cfg
                    .preferences
                    .max_context_tokens
                    .unwrap_or(sel.context_limit),
                ..Default::default()
            },
            stop_hook: None,
        },
        ToolContext {
            root,
            timeout: Duration::from_secs(600),
        },
    );
    Ok(Prepared {
        agent,
        model,
        history,
        persist,
        resumed,
        theme: cfg.preferences.theme.clone(),
        notices,
    })
}

/// Política del uso interactivo: la capa segura por defecto, la `[politica]` global del usuario y
/// la del proyecto (cada capa solo puede endurecer la anterior).
fn interactive_policy(project: &std::path::Path) -> Policy {
    let mut p = Policy::new(pcode_policy::Capabilities::interactive_defaults());
    if let Some(user) = pcode_config::user_config_path().filter(|u| u.is_file()) {
        p = p.child(crate::run::load_capabilities(&user));
    }
    p.child(crate::run::load_capabilities(project))
}

/// `pcode tui`: la interfaz a pantalla completa.
pub async fn tui(a: ChatArgs) -> anyhow::Result<i32> {
    let p = prepare(&a).await?;
    let mut persist = p.persist;
    let on_turn: pcode_tui::OnTurn = Box::new(move |turn| match turn {
        Some((user, transcript)) => persist.save(user, transcript),
        None => {
            if let Some(sid) = persist.session.take() {
                let _ = persist.store.finish_session(sid, "chat");
            }
            persist.saved = 0;
        }
    });
    pcode_tui::run_tui(
        p.agent,
        pcode_tui::TuiOptions {
            model: p.model,
            history: p.history,
            approve_all: a.approve_all,
            lang: pcode_tui::Lang::from_env(),
            theme: p
                .theme
                .as_deref()
                .and_then(|t| pcode_tui::Theme::by_name(t, std::env::var_os("NO_COLOR").is_some()))
                .unwrap_or_else(pcode_tui::Theme::from_env),
            notices: p.notices,
            root: std::env::current_dir()?,
        },
        on_turn,
    )
    .await?;
    Ok(0)
}

/// Escribe una línea JSON en stdout (protocolo `--formato jsonl`).
fn emit_json(v: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// Traduce un evento del agente a una línea del protocolo `jsonl`.
fn agent_json(e: &AgentEvent) -> serde_json::Value {
    use serde_json::json;
    match e {
        AgentEvent::TextDelta(t) => json!({"tipo": "delta", "texto": t}),
        AgentEvent::Text(t) => json!({"tipo": "texto", "texto": t}),
        AgentEvent::Info(t) => json!({"tipo": "info", "texto": t}),
        AgentEvent::ToolCall { name, arguments } => {
            json!({"tipo": "tool", "nombre": name, "argumentos": arguments})
        }
        AgentEvent::ToolResult {
            name,
            is_error,
            content,
        } => {
            let cut: String = content.chars().take(8000).collect();
            json!({"tipo": "resultado", "nombre": name, "error": is_error, "contenido": cut})
        }
    }
}

/// Aprobaciones por el protocolo `jsonl`: emite `{"tipo":"aprobacion",…}` y espera en stdin
/// `{"aprobacion": id, "permitir": bool}`.
struct JsonApprover {
    next: std::sync::atomic::AtomicU64,
    answers: Arc<std::sync::Mutex<Answers>>,
    always: AtomicBool,
}

/// Respuestas de aprobación: las que esperan respuesta y las que llegaron antes de preguntarse
/// (un cliente rápido o un guion pueden adelantarse).
#[derive(Default)]
struct Answers {
    waiting: std::collections::HashMap<u64, tokio::sync::oneshot::Sender<bool>>,
    early: std::collections::HashMap<u64, bool>,
}

#[async_trait::async_trait]
impl Approver for JsonApprover {
    async fn ask(&self, action: &str, reason: &str) -> bool {
        if self.always.load(Ordering::Relaxed) {
            return true;
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut a = self.answers.lock().unwrap();
            match a.early.remove(&id) {
                Some(allow) => {
                    let _ = tx.send(allow);
                }
                None => {
                    a.waiting.insert(id, tx);
                }
            }
        }
        emit_json(
            serde_json::json!({"tipo": "aprobacion", "id": id, "accion": action, "motivo": reason}),
        );
        // Sin respuesta en 30 minutos, se deniega.
        matches!(
            tokio::time::timeout(Duration::from_secs(1800), rx).await,
            Ok(Ok(true))
        )
    }
}

/// `pcode chat --formato jsonl`: cada línea de stdin es un mensaje (`{"mensaje": "…"}` o texto) o
/// una respuesta de aprobación; cada vuelta termina con `{"tipo":"fin",…}`.
async fn chat_jsonl(a: ChatArgs) -> anyhow::Result<i32> {
    let Prepared {
        agent,
        mut history,
        mut persist,
        notices,
        ..
    } = prepare(&a).await?;
    for n in notices {
        emit_json(serde_json::json!({"tipo": "info", "texto": n}));
    }
    let answers = Arc::new(std::sync::Mutex::new(Answers::default()));
    let approver = Arc::new(JsonApprover {
        next: std::sync::atomic::AtomicU64::new(1),
        answers: answers.clone(),
        always: AtomicBool::new(a.approve_all),
    });
    let mut agent = agent
        .with_approver(approver)
        .with_observer(Arc::new(|e| emit_json(agent_json(e))));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // Lector de stdin: reparte mensajes y respuestas de aprobación. Termina con EOF.
    std::thread::spawn(move || {
        while let Some(line) = read_line() {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(v) if v.get("aprobacion").is_some() => {
                    let id = v["aprobacion"].as_u64().unwrap_or(0);
                    let allow = v["permitir"].as_bool().unwrap_or(false);
                    let mut a = answers.lock().unwrap();
                    match a.waiting.remove(&id) {
                        Some(s) => {
                            let _ = s.send(allow);
                        }
                        None => {
                            a.early.insert(id, allow);
                        }
                    }
                }
                Ok(v) if v.get("mensaje").is_some() => {
                    let _ = tx.send(v["mensaje"].as_str().unwrap_or_default().to_string());
                }
                _ => {
                    let _ = tx.send(line);
                }
            }
        }
    });
    if !a.prompt.is_empty() {
        let first = a.prompt.join(" ");
        let r = agent.run_turn(history.clone(), &first).await;
        history = finish_json_turn(&first, r, &mut persist);
    }
    let root = std::env::current_dir()?;
    while let Some(msg) = rx.recv().await {
        let full = pcode_tui::run::expand_message(&root, &msg, &[]);
        let r = agent.run_turn(history.clone(), &full).await;
        history = finish_json_turn(&msg, r, &mut persist);
    }
    if let Some(sid) = persist.session {
        let _ = persist.store.finish_session(sid, "chat");
    }
    Ok(0)
}

fn finish_json_turn(user: &str, r: pcode_core::RunReport, persist: &mut Persist) -> Vec<Message> {
    let problem = match &r.outcome {
        Outcome::Done { .. } => None,
        Outcome::Failed(m) => Some(m.clone()),
        Outcome::BudgetExhausted => Some("presupuesto del mensaje agotado".to_string()),
        Outcome::Stuck(m) => Some(format!("atascado: {m}")),
    };
    emit_json(serde_json::json!({
        "tipo": "fin",
        "problema": problem,
        "entrada": r.usage.input_tokens,
        "salida": r.usage.output_tokens,
    }));
    persist.save(user, &r.transcript);
    r.transcript
}

/// Ejecuta el chat y devuelve el código de salida.
pub async fn chat(a: ChatArgs) -> anyhow::Result<i32> {
    if a.format == "jsonl" {
        return chat_jsonl(a).await;
    }
    let style = Style::detect();
    let interactive = std::io::stdin().is_terminal();
    let Prepared {
        agent,
        model,
        mut history,
        mut persist,
        resumed,
        notices,
        ..
    } = prepare(&a).await?;
    for n in &notices {
        eprintln!("{}", style.dim(&format!("· {n}")));
    }
    if a.resume {
        match resumed {
            Some((sid, n)) => println!(
                "{}",
                style.dim(&format!("Retomando la sesión {sid} ({n} mensajes)"))
            ),
            None => println!(
                "{}",
                style.dim("No hay conversación previa: empiezo una nueva.")
            ),
        }
    }
    let approver = Arc::new(TerminalApprover {
        always: AtomicBool::new(a.approve_all),
        interactive,
        style,
    });
    let observer_style = style;
    let mut agent = agent
        .with_approver(approver)
        .with_observer(Arc::new(move |e| render(observer_style, e)));

    if interactive {
        println!(
            "{}",
            style.bold(&format!(
                "PublicCode · {model} · /ayuda para ver los comandos"
            ))
        );
    }
    let mut total = Usage::default();
    let mut current_model = model;
    let mut first = if a.prompt.is_empty() {
        None
    } else {
        Some(a.prompt.join(" "))
    };
    let mut exit_code = 0;
    let root = std::env::current_dir()?;
    let mut shell_context: Vec<String> = vec![];
    loop {
        let input = match first.take() {
            Some(p) => p,
            None => {
                if interactive {
                    print!("{} ", style.bold("›"));
                    let _ = std::io::stdout().flush();
                }
                let Some(mut line) = read_line_async().await else {
                    break;
                };
                while line.ends_with('\\') {
                    line.pop();
                    line.push('\n');
                    let Some(next) = read_line_async().await else {
                        break;
                    };
                    line.push_str(&next);
                }
                line
            }
        };
        let input = input.trim().to_string();
        if input.is_empty() {
            continue;
        }
        if let Some(cmd) = input.strip_prefix('!') {
            let cmd = cmd.trim();
            let out = pcode_tools::shell_command(cmd)
                .current_dir(&root)
                .stdin(std::process::Stdio::null())
                .output()
                .await;
            let text = match out {
                Ok(o) => format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ),
                Err(e) => e.to_string(),
            };
            print!("{text}");
            println!(
                "{}",
                style.dim("· La salida irá como contexto en tu próximo mensaje.")
            );
            let cut: String = text.chars().take(20_000).collect();
            shell_context.push(format!("$ {cmd}\n```\n{cut}\n```"));
            continue;
        }
        if let Some(cmd) = input.strip_prefix('/') {
            let (name, arg) = cmd.split_once(' ').unwrap_or((cmd, ""));
            match name {
                "salir" | "exit" | "quit" => break,
                "ayuda" | "help" => println!("{AYUDA}"),
                "nuevo" | "new" => {
                    history.clear();
                    persist.session = None;
                    persist.saved = 0;
                    println!("{}", style.dim("Conversación nueva."));
                }
                "modelo" | "model" => {
                    if arg.trim().is_empty() {
                        println!("{current_model}");
                    } else {
                        current_model = arg.trim().to_string();
                        agent.set_model(current_model.clone());
                        println!("{}", style.dim(&format!("Modelo: {current_model}")));
                    }
                }
                "plan" => {
                    agent.set_read_only(true);
                    println!(
                        "{}",
                        style.bold(
                            "◆ Modo plan: el agente solo lee y propone. /construir para volver."
                        )
                    );
                }
                "construir" | "build" | "normal" => {
                    agent.set_read_only(false);
                    println!("{}", style.dim("▶ Modo normal."));
                }
                "compactar" | "compact" => {
                    let before = history.len();
                    history = agent.compact(history).await;
                    if history.len() == before {
                        println!(
                            "{}",
                            style.dim("No hay bastante conversación para compactar.")
                        );
                    }
                    persist.saved = 0;
                    persist.session = None;
                    persist.save("(compactada)", &history);
                }
                "uso" | "usage" => println!(
                    "{} tokens de entrada ({} de caché) · {} de salida",
                    total.input_tokens, total.cached_input_tokens, total.output_tokens
                ),
                _ => println!("Comando desconocido: /{name}. Prueba /ayuda."),
            }
            continue;
        }
        let full = pcode_tui::run::expand_message(&root, &input, &shell_context);
        shell_context.clear();
        let turn = agent.run_turn(history.clone(), &full);
        let report = tokio::select! {
            r = turn => r,
            _ = tokio::signal::ctrl_c() => {
                println!("{}", style.red("\n✗ interrumpido"));
                continue;
            }
        };
        total.add(&report.usage);
        match &report.outcome {
            Outcome::Done { .. } => {}
            Outcome::Failed(m) => {
                println!("{}", style.red(&format!("✗ {m}")));
                exit_code = 2;
            }
            Outcome::BudgetExhausted => {
                println!(
                    "{}",
                    style.red("✗ presupuesto del mensaje agotado: escribe «sigue» para continuar")
                );
                exit_code = 3;
            }
            Outcome::Stuck(m) => {
                println!(
                    "{}",
                    style.red(&format!("✗ atascado ({m}): prueba a darle otra pista"))
                );
                exit_code = 4;
            }
        }
        if !matches!(report.outcome, Outcome::Failed(_)) {
            exit_code = if matches!(report.outcome, Outcome::Done { .. }) {
                0
            } else {
                exit_code
            };
        }
        history = report.transcript;
        persist.save(&input, &history);
    }
    if let Some(sid) = persist.session {
        let _ = persist.store.finish_session(sid, "chat");
    }
    Ok(exit_code)
}
