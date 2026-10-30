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

const AYUDA: &str = "Comandos:\n  /ayuda          esta ayuda\n  /nuevo          empieza otra conversación (olvida el historial)\n  /modelo [m]     muestra o cambia el modelo\n  /uso            tokens gastados en esta sesión\n  /salir          termina (también Ctrl-D)\nUna línea que acaba en \\ continúa en la siguiente. Ctrl-C interrumpe al agente.";

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

/// Ejecuta el chat y devuelve el código de salida.
pub async fn chat(a: ChatArgs) -> anyhow::Result<i32> {
    let cfg = Config::load(&a.config)?;
    let sel = build_provider(&cfg, a.provider.as_deref())?;
    let model = pick_model(a.model.as_deref(), &sel);
    let root = std::env::current_dir()?;
    let style = Style::detect();
    let interactive = std::io::stdin().is_terminal();
    let sys = system_prompt(&cfg, &root);

    std::fs::create_dir_all(root.join(".pcode")).context("no se pudo crear .pcode")?;
    let audit = AuditLog::open(&root.join(".pcode/audit.jsonl"))?;
    let store = Store::open(&root.join(".pcode/pcode.db"))?;
    let mut persist = Persist {
        store,
        session: None,
        saved: 0,
        model: model.clone(),
    };
    let mut history: Vec<Message> = vec![];
    if a.resume {
        match load_last(&persist.store) {
            Some((sid, mut msgs)) if !msgs.is_empty() => {
                if msgs.first().is_some_and(|m| m.role == Role::System) {
                    msgs[0] = Message::system(sys.clone());
                }
                println!(
                    "{}",
                    style.dim(&format!(
                        "Retomando la sesión {sid} ({} mensajes)",
                        msgs.len()
                    ))
                );
                persist.session = Some(sid);
                persist.saved = msgs.len();
                history = msgs;
            }
            _ => println!(
                "{}",
                style.dim("No hay conversación previa: empiezo una nueva.")
            ),
        }
    }

    let caps = crate::run::load_capabilities(&a.config);
    let approver = Arc::new(TerminalApprover {
        always: AtomicBool::new(a.approve_all),
        interactive,
        style,
    });
    let observer_style = style;
    let mut agent = Agent::new(
        sel.provider.clone(),
        pcode_tools::native_registry(),
        Policy::new(caps),
        Some(audit),
        AgentConfig {
            agent_id: "chat".into(),
            model: model.clone(),
            system_prompt: sys,
            budget: Budget {
                max_steps: a.max_steps,
                ..Default::default()
            },
            stop_hook: None,
        },
        ToolContext {
            root,
            timeout: Duration::from_secs(600),
        },
    )
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
                "uso" | "usage" => println!(
                    "{} tokens de entrada ({} de caché) · {} de salida",
                    total.input_tokens, total.cached_input_tokens, total.output_tokens
                ),
                _ => println!("Comando desconocido: /{name}. Prueba /ayuda."),
            }
            continue;
        }
        let turn = agent.run_turn(history.clone(), &input);
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
