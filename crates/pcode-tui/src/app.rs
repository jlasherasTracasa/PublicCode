//! Estado de la TUI y su actualización pura: `update(Event) -> Vec<Command>`, sin E/S.
use crate::i18n::{Lang, t};
use crate::theme::Theme;
use pcode_core::AgentEvent;
use pcode_models::Usage;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Estado de una llamada a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    /// En marcha.
    Running,
    /// Terminó bien.
    Ok,
    /// Falló o se denegó.
    Err,
}

/// Una entrada de la transcripción.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// Mensaje del usuario.
    User(String),
    /// Texto del asistente (`done` falso mientras llega por streaming).
    Assistant {
        /// Texto.
        text: String,
        /// Completo.
        done: bool,
    },
    /// Llamada a una tool.
    Tool {
        /// Nombre.
        name: String,
        /// Resumen de los argumentos.
        brief: String,
        /// Estado.
        state: ToolState,
        /// Salida.
        output: String,
    },
    /// Tarjeta de bienvenida (logo, modelo y consejos).
    Welcome,
    /// Aviso informativo.
    Info(String),
    /// Error.
    Error(String),
}

/// Lo que le pasa a la TUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Tecla.
    Key(KeyEvent),
    /// Evento del agente.
    Agent(AgentEvent),
    /// El agente pide aprobación.
    Approval {
        /// Acción.
        action: String,
        /// Motivo.
        reason: String,
    },
    /// Terminó la vuelta del agente.
    TurnDone {
        /// Error o motivo de parada anómala, si lo hubo.
        problem: Option<String>,
        /// Tokens de la vuelta.
        usage: Usage,
    },
    /// Se interrumpió la vuelta.
    Cancelled,
    /// Pasa el tiempo (anima el indicador).
    Tick,
    /// Lista de archivos del proyecto (para `@archivo`).
    Files(Vec<String>),
    /// Terminó una `!orden`.
    ShellDone {
        /// Orden.
        command: String,
        /// Salida.
        output: String,
        /// Si salió con 0.
        ok: bool,
    },
}

/// Lo que la TUI pide hacer fuera.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Mandar un mensaje al agente.
    Send(String),
    /// Interrumpir la vuelta en marcha.
    Cancel,
    /// Responder a la aprobación pendiente.
    Approve {
        /// Permitir.
        allow: bool,
        /// No volver a preguntar.
        remember: bool,
    },
    /// Cambiar de modelo.
    SetModel(String),
    /// Empezar una conversación nueva.
    New,
    /// Resumir la conversación para liberar contexto.
    Compact,
    /// Activar o desactivar el modo plan (solo lectura).
    SetPlan(bool),
    /// Ejecutar una orden de shell (`!orden`).
    Shell(String),
    /// Deshacer el último turno del agente (o rehacerlo con `redo`).
    Undo {
        /// Rehacer en lugar de deshacer.
        redo: bool,
    },
    /// Salir.
    Quit,
}

/// Todo el estado de la TUI.
#[derive(Debug, Clone)]
pub struct App {
    /// Idioma.
    pub lang: Lang,
    /// Tema.
    pub theme: Theme,
    /// Transcripción.
    pub entries: Vec<Entry>,
    /// Texto que se está escribiendo.
    pub input: String,
    /// Mensajes enviados (para ↑/↓).
    pub history: Vec<String>,
    /// Posición al recorrer el historial.
    pub history_pos: Option<usize>,
    /// Líneas desplazadas hacia arriba desde el final (0 = sigue lo último).
    pub scroll: u16,
    /// El agente está trabajando.
    pub busy: bool,
    /// Aprobación pendiente (acción, motivo).
    pub approval: Option<(String, String)>,
    /// Mostrar entera la salida de las tools.
    pub expand_tools: bool,
    /// Modelo actual.
    pub model: String,
    /// Tokens acumulados.
    pub usage: Usage,
    /// Tokens que ocupa el contexto en la última petición.
    pub context_used: u64,
    /// Límite a partir del que se compacta (0 = sin medidor).
    pub context_limit: u64,
    /// Fotograma del indicador de actividad.
    pub spinner: usize,
    /// Modo plan (el agente solo lee y propone).
    pub plan: bool,
    /// Archivos del proyecto, para `@archivo`.
    pub files: Vec<String>,
    /// Sugerencias de `@archivo` para lo que se está escribiendo.
    pub suggestions: Vec<String>,
    /// Sugerencia seleccionada.
    pub suggestion: usize,
}

/// Tokens en corto: `950`, `3.1k`, `1.2M`.
pub fn human_tokens(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1_000.0).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

impl App {
    /// Porcentaje del contexto ocupado respecto al límite de compactación (0 si no hay límite).
    pub fn context_percent(&self) -> u64 {
        (self.context_used * 100)
            .checked_div(self.context_limit)
            .unwrap_or(0)
            .min(999)
    }
}

/// Puntuación difusa de `pattern` en `candidate` (subsecuencia, sin distinguir mayúsculas), con
/// bonus por inicio de segmento (`/`, `.`, `_`, `-`) y por coincidencias seguidas; `None` si no casa.
pub fn fuzzy_score(pattern: &str, candidate: &str) -> Option<i32> {
    let c: Vec<char> = candidate.to_lowercase().chars().collect();
    let mut score = 0i32;
    let (mut i, mut run) = (0usize, 0i32);
    for pc in pattern.to_lowercase().chars() {
        let j = (i..c.len()).find(|&j| c[j] == pc)?;
        run = if j == i && i > 0 { run + 1 } else { 0 };
        let start = j == 0 || matches!(c[j - 1], '/' | '.' | '_' | '-');
        score += 10 + run * 6 + if start { 8 } else { 0 } - (j - i) as i32;
        i = j + 1;
    }
    let name_start = candidate.rfind('/').map(|x| x + 1).unwrap_or(0);
    if candidate[name_start..]
        .to_lowercase()
        .starts_with(&pattern.to_lowercase())
    {
        score += 25;
    }
    Some(score - c.len() as i32 / 8)
}

/// Mejores `n` archivos para `pattern`.
pub fn best_matches(files: &[String], pattern: &str, n: usize) -> Vec<String> {
    let mut v: Vec<(i32, &String)> = files
        .iter()
        .filter_map(|f| fuzzy_score(pattern, f).map(|s| (s, f)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.len().cmp(&b.1.len())));
    v.into_iter().take(n).map(|x| x.1.clone()).collect()
}

/// Rutas mencionadas con `@ruta` en un mensaje.
pub fn mentions(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter_map(|w| w.strip_prefix('@'))
        .map(|w| {
            w.trim_end_matches([',', '.', ';', ':', ')', '?', '!'])
                .to_string()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Resumen de los argumentos de una tool en una línea.
pub fn brief(args: &serde_json::Value) -> String {
    let s = ["command", "path", "pattern", "query"]
        .iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_else(|| args.to_string());
    let first = s.lines().next().unwrap_or_default();
    let cut: String = first.chars().take(80).collect();
    if cut.len() < s.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

impl App {
    /// Estado inicial.
    pub fn new(lang: Lang, theme: Theme, model: impl Into<String>) -> App {
        App {
            lang,
            theme,
            entries: vec![Entry::Welcome],
            input: String::new(),
            history: vec![],
            history_pos: None,
            scroll: 0,
            busy: false,
            approval: None,
            expand_tools: false,
            model: model.into(),
            usage: Usage::default(),
            context_used: 0,
            context_limit: 0,
            spinner: 0,
            plan: false,
            files: vec![],
            suggestions: vec![],
            suggestion: 0,
        }
    }

    /// `@token` que se está escribiendo al final de la entrada.
    fn mention_token(&self) -> Option<&str> {
        let start = self
            .input
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.input[start..].strip_prefix('@')
    }

    fn refresh_suggestions(&mut self) {
        self.suggestion = 0;
        self.suggestions = match self.mention_token() {
            Some(q) if !self.files.is_empty() => best_matches(&self.files, q, 8),
            _ => vec![],
        };
    }

    fn complete_mention(&mut self) {
        if let Some(path) = self.suggestions.get(self.suggestion).cloned() {
            let start = self
                .input
                .rfind(char::is_whitespace)
                .map(|i| i + 1)
                .unwrap_or(0);
            self.input.truncate(start);
            self.input.push('@');
            self.input.push_str(&path);
            self.input.push(' ');
        }
        self.suggestions.clear();
    }

    /// Aplica un evento y devuelve lo que hay que hacer fuera.
    pub fn update(&mut self, ev: Event) -> Vec<Command> {
        match ev {
            Event::Key(k) => self.key(k),
            Event::Agent(e) => {
                self.agent(e);
                vec![]
            }
            Event::Approval { action, reason } => {
                self.approval = Some((action, reason));
                vec![]
            }
            Event::TurnDone { problem, usage } => {
                self.busy = false;
                self.usage.add(&usage);
                self.finish_streaming();
                if let Some(p) = problem {
                    self.entries.push(Entry::Error(p));
                }
                vec![]
            }
            Event::Cancelled => {
                self.busy = false;
                self.approval = None;
                self.finish_streaming();
                self.entries
                    .push(Entry::Error(t(self.lang, "interrupted").into()));
                vec![]
            }
            Event::Tick => {
                self.spinner = self.spinner.wrapping_add(1);
                vec![]
            }
            Event::Files(f) => {
                self.files = f;
                vec![]
            }
            Event::ShellDone {
                command,
                output,
                ok,
            } => {
                self.entries.push(Entry::Tool {
                    name: "!".into(),
                    brief: command,
                    state: if ok { ToolState::Ok } else { ToolState::Err },
                    output,
                });
                self.entries
                    .push(Entry::Info(t(self.lang, "shell_attached").into()));
                vec![]
            }
        }
    }

    fn finish_streaming(&mut self) {
        for e in self.entries.iter_mut() {
            if let Entry::Assistant { done, .. } = e {
                *done = true;
            }
            if let Entry::Tool { state, .. } = e
                && *state == ToolState::Running
            {
                *state = ToolState::Err;
            }
        }
    }

    fn agent(&mut self, e: AgentEvent) {
        match e {
            AgentEvent::Info(m) => self.entries.push(Entry::Info(format!("· {m}"))),
            AgentEvent::TextDelta(d) => match self.entries.last_mut() {
                Some(Entry::Assistant { text, done: false }) => text.push_str(&d),
                _ => self.entries.push(Entry::Assistant {
                    text: d,
                    done: false,
                }),
            },
            AgentEvent::Text(full) => match self.entries.last_mut() {
                Some(Entry::Assistant { text, done }) if !*done => {
                    *text = full;
                    *done = true;
                }
                _ => self.entries.push(Entry::Assistant {
                    text: full,
                    done: true,
                }),
            },
            AgentEvent::ToolCall { name, arguments } => {
                if let Some(Entry::Assistant { done, .. }) = self.entries.last_mut() {
                    *done = true;
                }
                self.entries.push(Entry::Tool {
                    name,
                    brief: brief(&arguments),
                    state: ToolState::Running,
                    output: String::new(),
                });
            }
            AgentEvent::ToolResult {
                is_error, content, ..
            } => {
                if let Some(Entry::Tool { state, output, .. }) =
                    self.entries.iter_mut().rev().find(|e| {
                        matches!(
                            e,
                            Entry::Tool {
                                state: ToolState::Running,
                                ..
                            }
                        )
                    })
                {
                    *state = if is_error {
                        ToolState::Err
                    } else {
                        ToolState::Ok
                    };
                    *output = content;
                }
            }
        }
    }

    fn key(&mut self, k: KeyEvent) -> Vec<Command> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(_pending) = &self.approval {
            let answer = match k.code {
                KeyCode::Char('s' | 'S' | 'y' | 'Y') => Some((true, false)),
                KeyCode::Char('t' | 'T' | 'a' | 'A') => Some((true, true)),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => Some((false, false)),
                _ => None,
            };
            return match answer {
                Some((allow, remember)) => {
                    self.approval = None;
                    vec![Command::Approve { allow, remember }]
                }
                None => vec![],
            };
        }
        if !self.suggestions.is_empty() {
            match k.code {
                KeyCode::Up => {
                    self.suggestion = self.suggestion.saturating_sub(1);
                    return vec![];
                }
                KeyCode::Down => {
                    self.suggestion = (self.suggestion + 1).min(self.suggestions.len() - 1);
                    return vec![];
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.complete_mention();
                    return vec![];
                }
                KeyCode::Esc => {
                    self.suggestions.clear();
                    return vec![];
                }
                _ => {}
            }
        }
        let cmds = self.key_main(k, ctrl);
        if matches!(k.code, KeyCode::Char(_) | KeyCode::Backspace) {
            self.refresh_suggestions();
        }
        cmds
    }

    fn key_main(&mut self, k: KeyEvent, ctrl: bool) -> Vec<Command> {
        match k.code {
            KeyCode::BackTab => {
                self.plan = !self.plan;
                self.entries.push(Entry::Info(
                    t(self.lang, if self.plan { "plan_on" } else { "plan_off" }).into(),
                ));
                vec![Command::SetPlan(self.plan)]
            }
            KeyCode::Char('c') if ctrl => {
                if self.busy {
                    vec![Command::Cancel]
                } else if !self.input.is_empty() {
                    self.input.clear();
                    vec![]
                } else {
                    vec![Command::Quit]
                }
            }
            KeyCode::Char('d') if ctrl && self.input.is_empty() => vec![Command::Quit],
            KeyCode::Char('o') if ctrl => {
                self.expand_tools = !self.expand_tools;
                vec![]
            }
            KeyCode::Char('u') if ctrl => {
                self.input.clear();
                vec![]
            }
            KeyCode::Esc if self.busy => vec![Command::Cancel],
            KeyCode::Enter
                if k.modifiers.contains(KeyModifiers::ALT)
                    || k.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                self.input.push('\n');
                vec![]
            }
            KeyCode::Enter if self.input.ends_with('\\') => {
                self.input.pop();
                self.input.push('\n');
                vec![]
            }
            KeyCode::Enter => self.submit(),
            KeyCode::Backspace => {
                self.input.pop();
                vec![]
            }
            KeyCode::Up if !self.input.contains('\n') => {
                self.browse(true);
                vec![]
            }
            KeyCode::Down if !self.input.contains('\n') => {
                self.browse(false);
                vec![]
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(10);
                vec![]
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_sub(10);
                vec![]
            }
            KeyCode::Tab => {
                self.input.push_str("    ");
                vec![]
            }
            KeyCode::Char(c) if !ctrl => {
                self.input.push(c);
                self.history_pos = None;
                vec![]
            }
            _ => vec![],
        }
    }

    fn browse(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        let pos = match (self.history_pos, up) {
            (None, true) => Some(last),
            (None, false) => None,
            (Some(p), true) => Some(p.saturating_sub(1)),
            (Some(p), false) if p < last => Some(p + 1),
            (Some(_), false) => None,
        };
        self.history_pos = pos;
        self.input = pos.map(|p| self.history[p].clone()).unwrap_or_default();
    }

    fn submit(&mut self) -> Vec<Command> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return vec![];
        }
        self.input.clear();
        self.history_pos = None;
        if self.history.last() != Some(&text) {
            self.history.push(text.clone());
        }
        self.scroll = 0;
        if let Some(cmd) = text.strip_prefix('/') {
            let (name, arg) = cmd.split_once(' ').unwrap_or((cmd, ""));
            let arg = arg.trim();
            return match name {
                "salir" | "quit" | "exit" => vec![Command::Quit],
                "ayuda" | "help" => {
                    self.entries.push(Entry::Info(t(self.lang, "help").into()));
                    vec![]
                }
                "nuevo" | "new" => {
                    self.entries = vec![Entry::Info(t(self.lang, "new").into())];
                    self.usage = Usage::default();
                    vec![Command::New]
                }
                "modelo" | "model" if arg.is_empty() => {
                    self.entries.push(Entry::Info(format!(
                        "{}: {}",
                        t(self.lang, "model"),
                        self.model
                    )));
                    vec![]
                }
                "modelo" | "model" => {
                    self.model = arg.to_string();
                    self.entries.push(Entry::Info(format!(
                        "{}: {}",
                        t(self.lang, "model"),
                        self.model
                    )));
                    vec![Command::SetModel(arg.to_string())]
                }
                "plan" => {
                    self.plan = true;
                    self.entries
                        .push(Entry::Info(t(self.lang, "plan_on").into()));
                    vec![Command::SetPlan(true)]
                }
                "construir" | "build" | "normal" => {
                    self.plan = false;
                    self.entries
                        .push(Entry::Info(t(self.lang, "plan_off").into()));
                    vec![Command::SetPlan(false)]
                }
                "deshacer" | "undo" | "rehacer" | "redo" if self.busy => {
                    self.entries.push(Entry::Info(t(self.lang, "wait").into()));
                    vec![]
                }
                "deshacer" | "undo" => vec![Command::Undo { redo: false }],
                "rehacer" | "redo" => vec![Command::Undo { redo: true }],
                "compactar" | "compact" if !self.busy => {
                    self.busy = true;
                    vec![Command::Compact]
                }
                "uso" | "usage" => {
                    self.entries.push(Entry::Info(format!(
                        "{} ↑{} ({} caché) ↓{}",
                        t(self.lang, "tokens"),
                        self.usage.input_tokens,
                        self.usage.cached_input_tokens,
                        self.usage.output_tokens
                    )));
                    if self.context_limit > 0 {
                        self.entries.push(Entry::Info(format!(
                            "{}: {} / {} ({} %)",
                            t(self.lang, "context"),
                            human_tokens(self.context_used),
                            human_tokens(self.context_limit),
                            self.context_percent()
                        )));
                    }
                    vec![]
                }
                _ => {
                    self.entries
                        .push(Entry::Info(t(self.lang, "unknown").into()));
                    vec![]
                }
            };
        }
        if self.busy {
            self.entries.push(Entry::Info(t(self.lang, "wait").into()));
            return vec![];
        }
        if let Some(cmd) = text.strip_prefix('!') {
            let cmd = cmd.trim().to_string();
            if cmd.is_empty() {
                return vec![];
            }
            return vec![Command::Shell(cmd)];
        }
        self.entries.push(Entry::User(text.clone()));
        self.busy = true;
        vec![Command::Send(text)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn app() -> App {
        App::new(Lang::Es, Theme::by_name("oscuro", true).unwrap(), "m")
    }
    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
    fn ctrl(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }
    fn typed(a: &mut App, s: &str) {
        for c in s.chars() {
            a.update(key(KeyCode::Char(c)));
        }
    }

    #[test]
    fn chat_intro_envia_y_marca_ocupado() {
        let mut a = app();
        typed(&mut a, "hola");
        assert_eq!(
            a.update(key(KeyCode::Enter)),
            vec![Command::Send("hola".into())]
        );
        assert!(a.busy && a.input.is_empty());
        assert_eq!(a.entries.last(), Some(&Entry::User("hola".into())));
        // Mientras trabaja no se manda otro mensaje.
        typed(&mut a, "otro");
        assert!(a.update(key(KeyCode::Enter)).is_empty());
    }

    #[test]
    fn chat_alt_intro_y_barra_final_hacen_nueva_linea() {
        let mut a = app();
        typed(&mut a, "a");
        a.update(Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)));
        typed(&mut a, "b\\");
        a.update(key(KeyCode::Enter));
        typed(&mut a, "c");
        assert_eq!(a.input, "a\nb\nc");
    }

    #[test]
    fn chat_streaming_une_trozos_y_tools_cambian_de_estado() {
        let mut a = app();
        a.update(Event::Agent(AgentEvent::TextDelta("Ho".into())));
        a.update(Event::Agent(AgentEvent::TextDelta("la".into())));
        assert_eq!(
            a.entries.last(),
            Some(&Entry::Assistant {
                text: "Hola".into(),
                done: false
            })
        );
        a.update(Event::Agent(AgentEvent::Text("Hola".into())));
        a.update(Event::Agent(AgentEvent::ToolCall {
            name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
        }));
        assert!(
            matches!(a.entries.last(), Some(Entry::Tool { state: ToolState::Running, brief, .. }) if brief == "ls")
        );
        a.update(Event::Agent(AgentEvent::ToolResult {
            name: "bash".into(),
            is_error: true,
            content: "no".into(),
        }));
        assert!(matches!(
            a.entries.last(),
            Some(Entry::Tool {
                state: ToolState::Err,
                ..
            })
        ));
        // Un texto nuevo tras la tool empieza otra entrada.
        a.update(Event::Agent(AgentEvent::TextDelta("Fin".into())));
        assert!(matches!(a.entries.last(), Some(Entry::Assistant { text, .. }) if text == "Fin"));
    }

    #[test]
    fn chat_aprobacion_bloquea_el_teclado_hasta_responder() {
        let mut a = app();
        a.update(Event::Approval {
            action: "bash rm".into(),
            reason: "irreversible".into(),
        });
        assert!(a.update(key(KeyCode::Char('x'))).is_empty());
        assert!(a.input.is_empty());
        assert_eq!(
            a.update(key(KeyCode::Char('t'))),
            vec![Command::Approve {
                allow: true,
                remember: true
            }]
        );
        assert!(a.approval.is_none());
        a.update(Event::Approval {
            action: "x".into(),
            reason: "y".into(),
        });
        assert_eq!(
            a.update(key(KeyCode::Char('n'))),
            vec![Command::Approve {
                allow: false,
                remember: false
            }]
        );
    }

    #[test]
    fn chat_esc_y_ctrl_c_cancelan_o_salen() {
        let mut a = app();
        assert_eq!(a.update(ctrl('c')), vec![Command::Quit]);
        typed(&mut a, "x");
        a.update(key(KeyCode::Enter));
        assert_eq!(a.update(key(KeyCode::Esc)), vec![Command::Cancel]);
        assert_eq!(a.update(ctrl('c')), vec![Command::Cancel]);
        a.update(Event::Cancelled);
        assert!(!a.busy);
        assert!(matches!(a.entries.last(), Some(Entry::Error(_))));
    }

    #[test]
    fn tokens_en_corto_y_porcentaje_de_contexto() {
        assert_eq!(human_tokens(950), "950");
        assert_eq!(human_tokens(3_120), "3.1k");
        assert_eq!(human_tokens(160_000), "160k");
        assert_eq!(human_tokens(1_250_000), "1.2M");
        let mut a = app();
        assert_eq!(a.context_percent(), 0);
        a.context_limit = 160_000;
        a.context_used = 40_000;
        assert_eq!(a.context_percent(), 25);
        typed(&mut a, "/uso");
        a.update(key(KeyCode::Enter));
        assert!(
            matches!(a.entries.last(), Some(Entry::Info(t)) if t.contains("40k / 160k (25 %)"))
        );
    }

    #[test]
    fn deshacer_y_rehacer() {
        let mut a = app();
        a.busy = true;
        typed(&mut a, "/deshacer");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![], "ocupado: espera");
        a.busy = false;
        typed(&mut a, "/deshacer");
        assert_eq!(
            a.update(key(KeyCode::Enter)),
            vec![Command::Undo { redo: false }]
        );
        typed(&mut a, "/rehacer");
        assert_eq!(
            a.update(key(KeyCode::Enter)),
            vec![Command::Undo { redo: true }]
        );
    }

    #[test]
    fn chat_ordenes_y_historial() {
        let mut a = app();
        typed(&mut a, "/modelo otro");
        assert_eq!(
            a.update(key(KeyCode::Enter)),
            vec![Command::SetModel("otro".into())]
        );
        assert_eq!(a.model, "otro");
        typed(&mut a, "/nuevo");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![Command::New]);
        assert_eq!(a.entries.len(), 1);
        typed(&mut a, "/compactar");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![Command::Compact]);
        a.update(Event::TurnDone {
            problem: None,
            usage: Usage::default(),
        });
        typed(&mut a, "/salir");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![Command::Quit]);
        a.update(key(KeyCode::Up));
        assert_eq!(a.input, "/salir");
        a.update(key(KeyCode::Up));
        assert_eq!(a.input, "/compactar");
        a.update(key(KeyCode::Down));
        a.update(key(KeyCode::Down));
        assert_eq!(a.input, "");
        let _ = KeyEventKind::Press;
    }

    #[test]
    fn fuzzy_prefiere_nombre_e_inicio_de_segmento() {
        assert!(fuzzy_score("mrs", "src/main.rs").is_some());
        assert!(fuzzy_score("xyz", "src/main.rs").is_none());
        let files: Vec<String> = ["docs/manual.md", "src/main.rs", "crates/a/src/domain.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(best_matches(&files, "main", 3)[0], "src/main.rs");
        assert_eq!(
            mentions("mira @src/main.rs, y @README.md."),
            vec!["src/main.rs", "README.md"]
        );
    }

    #[test]
    fn chat_mencion_sugiere_y_tab_completa() {
        let mut a = app();
        a.update(Event::Files(vec![
            "src/main.rs".into(),
            "src/lib.rs".into(),
            "README.md".into(),
        ]));
        typed(&mut a, "explica @lib");
        assert_eq!(a.suggestions, vec!["src/lib.rs"]);
        a.update(key(KeyCode::Tab));
        assert_eq!(a.input, "explica @src/lib.rs ");
        assert!(a.suggestions.is_empty());
        typed(&mut a, "y @");
        assert_eq!(a.suggestions.len(), 3);
        a.update(key(KeyCode::Down));
        a.update(key(KeyCode::Enter));
        assert!(a.input.ends_with("@src/lib.rs "), "{}", a.input);
    }

    #[test]
    fn chat_modo_plan_y_orden_de_shell() {
        let mut a = app();
        assert_eq!(
            a.update(Event::Key(KeyEvent::new(
                KeyCode::BackTab,
                KeyModifiers::SHIFT
            ))),
            vec![Command::SetPlan(true)]
        );
        assert!(a.plan);
        typed(&mut a, "/construir");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![Command::SetPlan(false)]);
        typed(&mut a, "!cargo test");
        assert_eq!(
            a.update(key(KeyCode::Enter)),
            vec![Command::Shell("cargo test".into())]
        );
        assert!(!a.busy);
        a.update(Event::ShellDone {
            command: "cargo test".into(),
            output: "ok".into(),
            ok: true,
        });
        assert!(
            a.entries.iter().any(
                |e| matches!(e, Entry::Tool { name, state: ToolState::Ok, .. } if name == "!")
            )
        );
    }

    #[test]
    fn chat_turno_termina_con_problema_y_suma_tokens() {
        let mut a = app();
        a.busy = true;
        a.update(Event::TurnDone {
            problem: Some("✗ fallo".into()),
            usage: Usage {
                input_tokens: 5,
                output_tokens: 2,
                cached_input_tokens: 1,
            },
        });
        assert!(!a.busy);
        assert_eq!(a.usage.input_tokens, 5);
        assert_eq!(a.entries.last(), Some(&Entry::Error("✗ fallo".into())));
    }
}
