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
}

/// Se llama al terminar cada vuelta con el mensaje del usuario y la transcripción completa
/// (para guardarla). Con `None` se empieza una conversación nueva.
pub type OnTurn = Box<dyn FnMut(Option<(&str, &[Message])>) + Send>;

enum Msg {
    Ui(Event),
    Turn(String, RunReport),
    Compacted(Vec<Message>),
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
                        let (agent, tx, h) = (agent.clone(), tx.clone(), history.clone());
                        turn = Some(tokio::spawn(async move {
                            let r = agent.lock().await.run_turn(h, &text).await;
                            let _ = tx.send(Msg::Turn(text, r));
                        }));
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
