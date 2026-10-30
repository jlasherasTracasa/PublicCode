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
    /// Fotograma del indicador de actividad.
    pub spinner: usize,
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
            entries: vec![Entry::Info(t(lang, "welcome").into())],
            input: String::new(),
            history: vec![],
            history_pos: None,
            scroll: 0,
            busy: false,
            approval: None,
            expand_tools: false,
            model: model.into(),
            usage: Usage::default(),
            spinner: 0,
        }
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
        match k.code {
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
                "uso" | "usage" => {
                    self.entries.push(Entry::Info(format!(
                        "{} ↑{} ({} caché) ↓{}",
                        t(self.lang, "tokens"),
                        self.usage.input_tokens,
                        self.usage.cached_input_tokens,
                        self.usage.output_tokens
                    )));
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
        typed(&mut a, "/salir");
        assert_eq!(a.update(key(KeyCode::Enter)), vec![Command::Quit]);
        a.update(key(KeyCode::Up));
        assert_eq!(a.input, "/salir");
        a.update(key(KeyCode::Up));
        assert_eq!(a.input, "/nuevo");
        a.update(key(KeyCode::Down));
        a.update(key(KeyCode::Down));
        assert_eq!(a.input, "");
        let _ = KeyEventKind::Press;
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
