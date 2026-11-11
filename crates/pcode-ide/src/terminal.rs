//! Salida de la terminal integrada, con memoria acotada.
use serde_json::Value;
use std::collections::VecDeque;

/// Líneas que se conservan como máximo.
pub const MAX_LINES: usize = 2000;

/// Salida de la última orden.
#[derive(Debug, Default)]
pub struct Terminal {
    /// Líneas (la última puede estar abierta).
    pub lines: VecDeque<String>,
    /// Orden en marcha.
    pub running: Option<u64>,
    open_line: bool,
}

impl Terminal {
    fn push_line(&mut self, l: String) {
        self.lines.push_back(l);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    /// Anota la orden lanzada.
    pub fn start(&mut self, id: u64, command: &str) {
        self.open_line = false;
        self.push_line(format!("$ {command}"));
        self.running = Some(id);
    }

    /// Procesa `terminalOutput` y `terminalExit` de la orden en marcha.
    pub fn apply(&mut self, ev: &Value) -> bool {
        if ev["id"].as_u64() != self.running || self.running.is_none() {
            return false;
        }
        match ev["tipo"].as_str() {
            Some("terminalOutput") => {
                let text = ev["text"].as_str().unwrap_or_default();
                let ends = text.ends_with('\n');
                let mut parts: Vec<&str> = text.split('\n').collect();
                if ends {
                    parts.pop();
                }
                for (i, part) in parts.iter().enumerate() {
                    match self.lines.back_mut() {
                        Some(l) if i == 0 && self.open_line => l.push_str(part),
                        _ => self.push_line(part.to_string()),
                    }
                }
                if !parts.is_empty() {
                    self.open_line = !ends;
                }
                true
            }
            Some("terminalExit") => {
                let msg = match ev["code"].as_i64() {
                    Some(c) => format!("[salió con {c}]"),
                    None => "[detenida]".to_string(),
                };
                self.open_line = false;
                self.push_line(msg);
                self.running = None;
                true
            }
            _ => false,
        }
    }

    /// Todo el texto.
    pub fn text(&self) -> String {
        self.lines.iter().cloned().collect::<Vec<_>>().join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn out(id: u64, t: &str) -> Value {
        json!({"tipo":"terminalOutput","id":id,"text":t})
    }

    #[test]
    fn salida_partida_en_varios_eventos() {
        let mut t = Terminal::default();
        t.start(1, "ls");
        t.apply(&out(1, "ho"));
        t.apply(&out(1, "la\nsegunda\n"));
        t.apply(&out(1, "tercera"));
        assert_eq!(t.text(), "$ ls\nhola\nsegunda\ntercera");
    }

    #[test]
    fn salida_de_otra_orden_se_ignora() {
        let mut t = Terminal::default();
        t.start(1, "x");
        assert!(!t.apply(&out(2, "no")));
        assert_eq!(t.text(), "$ x");
    }

    #[test]
    fn fin_con_codigo() {
        let mut t = Terminal::default();
        t.start(3, "x");
        t.apply(&json!({"tipo":"terminalExit","id":3,"code":2}));
        assert_eq!(t.running, None);
        assert!(t.text().ends_with("[salió con 2]"));
    }

    #[test]
    fn memoria_acotada() {
        let mut t = Terminal::default();
        t.start(1, "x");
        for i in 0..MAX_LINES + 500 {
            t.apply(&out(1, &format!("l{i}\n")));
        }
        assert_eq!(t.lines.len(), MAX_LINES);
        assert!(t.text().ends_with(&format!("l{}", MAX_LINES + 499)));
    }
}
