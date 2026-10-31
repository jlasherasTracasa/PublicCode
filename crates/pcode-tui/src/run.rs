//! Bucle real de la TUI: terminal (modo raw y pantalla alternativa), teclado en un hilo, el agente
//! en una tarea y las aprobaciones por canal. Todo llega a [`App::update`] como [`Event`].
use crate::app::{App, Command, Event};
use crate::i18n::{Lang, t};
use crate::render::render;
use crate::theme::Theme;
use pcode_core::{Agent, Approver, Outcome, RunReport};
use pcode_models::Message;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyEventKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc, oneshot};

/// Opciones de arranque.
pub struct TuiOptions {
    /// Modelo (solo para mostrarlo).
    pub model: String,
    /// Historial con el que empezar (p. ej. `--continuar`).
    pub history: Vec<Message>,
    /// Aprobar sin preguntar.
    pub approve_all: bool,
    /// Idioma.
    pub lang: Lang,
    /// Tema.
    pub theme: Theme,
    /// Avisos que se muestran al empezar (servidores MCP que fallaron…).
    pub notices: Vec<String>,
    /// Raíz del proyecto (para `@archivo` y `!orden`).
    pub root: std::path::PathBuf,
}

/// Archivos del proyecto: con git, los versionados y los nuevos no ignorados; si no, un recorrido
/// que salta `.git`, `target` y `node_modules` (como mucho 20 000).
pub fn project_files(root: &std::path::Path) -> Vec<String> {
    let git = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output();
    if let Ok(o) = git
        && o.status.success()
    {
        let mut v: Vec<String> = String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect();
        v.sort();
        v.dedup();
        return v;
    }
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            match e.file_type() {
                Ok(t) if t.is_dir() => {
                    if !matches!(name.as_str(), ".git" | "target" | "node_modules" | ".pcode") {
                        stack.push(e.path());
                    }
                }
                Ok(t) if t.is_file() => {
                    if let Ok(r) = e.path().strip_prefix(root) {
                        out.push(r.to_string_lossy().replace('\\', "/"));
                    }
                    if out.len() >= 20_000 {
                        return out;
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// Añade al mensaje el contenido de los `@archivo` mencionados (hasta 100 KB cada uno) y las
/// salidas de `!orden` pendientes.
pub fn expand_message(root: &std::path::Path, text: &str, shell: &[String]) -> String {
    let mut out = text.to_string();
    for (cmd_out, i) in shell.iter().zip(1..) {
        if i == 1 {
            out.push_str("\n\n# Salidas de órdenes que ejecuté\n");
        }
        out.push_str(cmd_out);
        out.push('\n');
    }
    let mut seen = std::collections::BTreeSet::new();
    for m in crate::app::mentions(text) {
        if !seen.insert(m.clone()) {
            continue;
        }
        let Ok(p) = pcode_tools::resolve_in_root(root, &m) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let mut body = String::from_utf8_lossy(&bytes).into_owned();
        if body.len() > 100_000 {
            let mut cut = 100_000;
            while !body.is_char_boundary(cut) {
                cut -= 1;
            }
            body.truncate(cut);
            body.push_str("\n[… recortado]");
        }
        out.push_str(&format!("\n\n# Archivo {m}\n```\n{body}\n```"));
    }
    out
}

/// Se llama al terminar cada vuelta con el mensaje del usuario y la transcripción completa
/// (para guardarla). Con `None` se empieza una conversación nueva.
pub type OnTurn = Box<dyn FnMut(Option<(&str, &[Message])>) + Send>;

enum Msg {
    Ui(Event),
    Turn(String, RunReport),
    Compacted(Vec<Message>),
    Shell(String, String, bool),
}

/// Aprobador que pregunta en la TUI.
struct TuiApprover {
    tx: mpsc::UnboundedSender<Msg>,
    pending: Arc<std::sync::Mutex<Option<oneshot::Sender<bool>>>>,
    always: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Approver for TuiApprover {
    async fn ask(&self, action: &str, reason: &str) -> bool {
        if self.always.load(Ordering::Relaxed) {
            return true;
        }
        let (tx, rx) = oneshot::channel();
        *self.pending.lock().unwrap() = Some(tx);
        let _ = self.tx.send(Msg::Ui(Event::Approval {
            action: action.to_string(),
            reason: reason.to_string(),
        }));
        rx.await.unwrap_or(false)
    }
}

fn problem(lang: Lang, r: &RunReport) -> Option<String> {
    match &r.outcome {
        Outcome::Done { .. } => None,
        Outcome::Failed(m) => Some(format!("✗ {m}")),
        Outcome::BudgetExhausted => Some(t(lang, "budget").into()),
        Outcome::Stuck(_) => Some(t(lang, "stuck").into()),
    }
}

/// Arranca la TUI con `agent` hasta que el usuario sale. Restaura la terminal al salir y también
/// si hay un pánico.
pub async fn run_tui(agent: Agent, opts: TuiOptions, mut on_turn: OnTurn) -> std::io::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let pending = Arc::new(std::sync::Mutex::new(None));
    let always = Arc::new(AtomicBool::new(opts.approve_all));
    let obs_tx = tx.clone();
    let agent = agent
        .with_observer(Arc::new(move |e| {
            let _ = obs_tx.send(Msg::Ui(Event::Agent(e.clone())));
        }))
        .with_approver(Arc::new(TuiApprover {
            tx: tx.clone(),
            pending: pending.clone(),
            always: always.clone(),
        }));
    let agent = Arc::new(Mutex::new(agent));

    let mut app = App::new(opts.lang, opts.theme, opts.model);
    for n in opts.notices {
        app.entries.push(crate::Entry::Info(format!("· {n}")));
    }
    let mut history = opts.history;
    let root = opts.root.clone();
    let mut shell_context: Vec<String> = vec![];
    {
        let (tx, root) = (tx.clone(), root.clone());
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Msg::Ui(Event::Files(project_files(&root))));
        });
    }
    if history.len() > 1 {
        for m in &history {
            match m.role {
                pcode_models::Role::User => app.entries.push(crate::Entry::User(m.content.clone())),
                pcode_models::Role::Assistant if !m.content.is_empty() => {
                    app.entries.push(crate::Entry::Assistant {
                        text: m.content.clone(),
                        done: true,
                    })
                }
                _ => {}
            }
        }
    }

    // Teclado y ticks en un hilo: `poll` con tiempo máximo y una bandera para parar.
    let stop = Arc::new(AtomicBool::new(false));
    let (key_tx, key_stop) = (tx.clone(), stop.clone());
    let input = std::thread::spawn(move || {
        while !key_stop.load(Ordering::Relaxed) {
            match event::poll(Duration::from_millis(120)) {
                Ok(true) => match event::read() {
                    Ok(TermEvent::Key(k)) if k.kind != KeyEventKind::Release => {
                        let _ = key_tx.send(Msg::Ui(Event::Key(k)));
                    }
                    Ok(TermEvent::Paste(s)) => {
                        for c in s.chars() {
                            let code = if c == '\n' {
                                event::KeyCode::Enter
                            } else {
                                event::KeyCode::Char(c)
                            };
                            let mods = if c == '\n' {
                                event::KeyModifiers::ALT
                            } else {
                                event::KeyModifiers::NONE
                            };
                            let _ =
                                key_tx.send(Msg::Ui(Event::Key(event::KeyEvent::new(code, mods))));
                        }
                    }
                    _ => {}
                },
                Ok(false) => {
                    let _ = key_tx.send(Msg::Ui(Event::Tick));
                }
                Err(_) => break,
            }
        }
    });

    let mut term = ratatui::init();
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::EnableBracketedPaste);
    let mut turn: Option<tokio::task::JoinHandle<()>> = None;
    let lang = app.lang;
    let result: std::io::Result<()> = async {
        term.draw(|f| render(&app, f))?;
        'outer: while let Some(msg) = rx.recv().await {
            let ev = match msg {
                Msg::Ui(ev) => ev,
                Msg::Turn(user, report) => {
                    turn = None;
                    let ev = Event::TurnDone {
                        problem: problem(lang, &report),
                        usage: report.usage,
                    };
                    history = report.transcript;
                    on_turn(Some((&user, &history)));
                    ev
                }
                Msg::Shell(cmd, output, ok) => {
                    let cut: String = output.chars().take(20_000).collect();
                    shell_context.push(format!("$ {cmd}\n```\n{cut}\n```"));
                    Event::ShellDone {
                        command: cmd,
                        output,
                        ok,
                    }
                }
                Msg::Compacted(h) => {
                    turn = None;
                    if h.len() == history.len() {
                        app.entries
                            .push(crate::Entry::Info(t(lang, "nothing_to_compact").into()));
                    } else {
                        history = h;
                        on_turn(None);
                        on_turn(Some(("(compactada)", &history)));
                    }
                    Event::TurnDone {
                        problem: None,
                        usage: Default::default(),
                    }
                }
            };
            for cmd in app.update(ev) {
                match cmd {
                    Command::Quit => break 'outer,
                    Command::Send(text) => {
                        let full = expand_message(&root, &text, &shell_context);
                        shell_context.clear();
                        let (agent, tx, h) = (agent.clone(), tx.clone(), history.clone());
                        turn = Some(tokio::spawn(async move {
                            let r = agent.lock().await.run_turn(h, &full).await;
                            let _ = tx.send(Msg::Turn(text, r));
                        }));
                    }
                    Command::SetPlan(on) => {
                        if let Ok(mut a) = agent.try_lock() {
                            a.set_read_only(on);
                        }
                    }
                    Command::Shell(cmd) => {
                        let (tx, root) = (tx.clone(), root.clone());
                        tokio::spawn(async move {
                            let mut c = pcode_tools::shell_command(&cmd);
                            c.current_dir(&root)
                                .stdin(std::process::Stdio::null())
                                .kill_on_drop(true);
                            let (output, ok) =
                                match tokio::time::timeout(Duration::from_secs(300), c.output())
                                    .await
                                {
                                    Ok(Ok(o)) => (
                                        format!(
                                            "{}{}",
                                            String::from_utf8_lossy(&o.stdout),
                                            String::from_utf8_lossy(&o.stderr)
                                        ),
                                        o.status.success(),
                                    ),
                                    Ok(Err(e)) => (e.to_string(), false),
                                    Err(_) => ("tiempo máximo (300 s) agotado".into(), false),
                                };
                            let _ = tx.send(Msg::Shell(cmd, output, ok));
                        });
                    }
                    Command::Cancel => {
                        if let Some(h) = turn.take() {
                            h.abort();
                        }
                        pending.lock().unwrap().take();
                        app.update(Event::Cancelled);
                    }
                    Command::Approve { allow, remember } => {
                        if remember {
                            always.store(true, Ordering::Relaxed);
                        }
                        if let Some(s) = pending.lock().unwrap().take() {
                            let _ = s.send(allow);
                        }
                    }
                    Command::SetModel(m) => {
                        if let Ok(mut a) = agent.try_lock() {
                            a.set_model(m);
                        }
                    }
                    Command::Compact => {
                        let (agent, tx, h) = (agent.clone(), tx.clone(), history.clone());
                        turn = Some(tokio::spawn(async move {
                            let out = agent.lock().await.compact(h).await;
                            let _ = tx.send(Msg::Compacted(out));
                        }));
                    }
                    Command::New => {
                        history.clear();
                        on_turn(None);
                    }
                }
            }
            term.draw(|f| render(&app, f))?;
        }
        Ok(())
    }
    .await;

    if let Some(h) = turn.take() {
        h.abort();
    }
    stop.store(true, Ordering::Relaxed);
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::DisableBracketedPaste);
    ratatui::restore();
    let _ = input.join();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expandir_adjunta_menciones_y_salidas() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/a.rs"), "fn a() {}").unwrap();
        let out = expand_message(
            d.path(),
            "revisa @src/a.rs y @no-existe.rs",
            &["$ cargo test\n```\nok\n```".into()],
        );
        assert!(out.starts_with("revisa @src/a.rs"));
        assert!(out.contains("# Archivo src/a.rs\n```\nfn a() {}"));
        assert!(out.contains("$ cargo test"));
        assert!(!out.contains("# Archivo no-existe"));
        assert!(!expand_message(d.path(), "@../../etc/passwd", &[]).contains("root:"));
    }

    #[test]
    fn archivos_sin_git_saltan_target() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("target")).unwrap();
        std::fs::write(d.path().join("target/x"), "").unwrap();
        std::fs::write(d.path().join("a.txt"), "").unwrap();
        assert_eq!(project_files(d.path()), vec!["a.txt"]);
    }
}
