//! Detector de atasco y pistas por fallo típico.
use pcode_models::ToolCall;
use pcode_tools::ToolOutput;

/// Señal de atasco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StuckSignal {
    /// Misma llamada repetida.
    RepeatedCall(String),
    /// Mismo error repetido.
    RepeatedError(String),
    /// Pasos sin escribir.
    NoProgress(u32),
}

/// Detector con ventana simple.
pub struct StuckDetector {
    max_repeats: u32,
    max_no_write: u32,
    last_call: Option<String>,
    call_n: u32,
    last_err: Option<String>,
    err_n: u32,
    since_write: u32,
}

impl StuckDetector {
    /// Nuevo detector.
    pub fn new(max_repeats: u32, max_steps_without_write: u32) -> Self {
        StuckDetector {
            max_repeats,
            max_no_write: max_steps_without_write,
            last_call: None,
            call_n: 0,
            last_err: None,
            err_n: 0,
            since_write: 0,
        }
    }
    /// Observa una llamada y su resultado.
    pub fn observe(
        &mut self,
        call: &ToolCall,
        wrote: bool,
        out: &ToolOutput,
    ) -> Option<StuckSignal> {
        let key = format!("{}:{}", call.name, call.arguments);
        if self.last_call.as_deref() == Some(&key) {
            self.call_n += 1;
        } else {
            self.last_call = Some(key.clone());
            self.call_n = 1;
        }
        if out.is_error {
            let e: String = out.content.chars().take(200).collect();
            if self.last_err.as_deref() == Some(&e) {
                self.err_n += 1;
            } else {
                self.last_err = Some(e);
                self.err_n = 1;
            }
        } else {
            self.last_err = None;
            self.err_n = 0;
        }
        if wrote && !out.is_error {
            self.since_write = 0;
        } else {
            self.since_write += 1;
        }
        if self.call_n >= self.max_repeats {
            self.call_n = 0;
            return Some(StuckSignal::RepeatedCall(key));
        }
        if self.err_n >= self.max_repeats {
            self.err_n = 0;
            return Some(StuckSignal::RepeatedError(
                self.last_err.clone().unwrap_or_default(),
            ));
        }
        if self.since_write >= self.max_no_write {
            self.since_write = 0;
            return Some(StuckSignal::NoProgress(self.max_no_write));
        }
        None
    }
}

/// Pista por fallo típico.
pub fn hint_for(output: &str) -> Option<&'static str> {
    let t = [
        (
            "no se encontró el texto a reemplazar",
            "Vuelve a leer el archivo con `leer` antes de editar.",
        ),
        ("rustfmt", "Ejecuta `cargo fmt --all`."),
        ("cargo fmt", "Ejecuta `cargo fmt --all`."),
        (
            "unresolved import",
            "Revisa el `use` y que el módulo esté declarado en lib.rs.",
        ),
        (
            "denegado por la política",
            "Esa acción no está permitida; busca otra forma o explica el bloqueo en el informe.",
        ),
    ];
    t.iter().find(|(k, _)| output.contains(k)).map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(n: &str, a: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: n.into(),
            arguments: a,
        }
    }
    #[test]
    fn stuck_llamada_repetida() {
        let mut d = StuckDetector::new(3, 99);
        let c = call("leer", serde_json::json!({"path":"a"}));
        let o = ToolOutput::ok("x");
        assert!(d.observe(&c, false, &o).is_none());
        assert!(d.observe(&c, false, &o).is_none());
        assert!(matches!(
            d.observe(&c, false, &o),
            Some(StuckSignal::RepeatedCall(_))
        ));
    }
    #[test]
    fn stuck_error_repetido() {
        let mut d = StuckDetector::new(2, 99);
        let o = ToolOutput::error("boom");
        assert!(
            d.observe(&call("a", serde_json::json!(1)), false, &o)
                .is_none()
        );
        assert!(matches!(
            d.observe(&call("b", serde_json::json!(2)), false, &o),
            Some(StuckSignal::RepeatedError(_))
        ));
    }
    #[test]
    fn stuck_sin_progreso() {
        let mut d = StuckDetector::new(99, 2);
        let o = ToolOutput::ok("");
        assert!(
            d.observe(&call("a", serde_json::json!(1)), false, &o)
                .is_none()
        );
        assert!(matches!(
            d.observe(&call("b", serde_json::json!(2)), false, &o),
            Some(StuckSignal::NoProgress(2))
        ));
    }
    #[test]
    fn stuck_se_reinicia_al_escribir() {
        let mut d = StuckDetector::new(99, 2);
        let o = ToolOutput::ok("");
        d.observe(&call("a", serde_json::json!(1)), false, &o);
        d.observe(&call("w", serde_json::json!(2)), true, &o);
        assert!(
            d.observe(&call("c", serde_json::json!(3)), false, &o)
                .is_none()
        );
    }
    #[test]
    fn hint_editar() {
        assert!(
            hint_for("no se encontró el texto a reemplazar en x")
                .unwrap()
                .contains("leer")
        );
        assert!(hint_for("nada").is_none());
    }
}
