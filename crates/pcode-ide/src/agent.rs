//! Tarjeta de cambios de un turno y texto en streaming del agente.
use serde_json::Value;
use std::collections::HashMap;

/// Un archivo cambiado en el turno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Ruta.
    pub path: String,
    /// Líneas añadidas.
    pub added: u32,
    /// Líneas quitadas.
    pub removed: u32,
    /// Archivo nuevo.
    pub created: bool,
    /// Archivo borrado.
    pub deleted: bool,
}

/// Resumen de un turno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCard {
    /// Sesión.
    pub session: i64,
    /// Cambios.
    pub changes: Vec<Change>,
}

impl TurnCard {
    /// Lee un evento `turnFinished`.
    pub fn from_event(ev: &Value) -> Option<TurnCard> {
        if ev["tipo"] != "turnFinished" {
            return None;
        }
        Some(TurnCard {
            session: ev["session"].as_i64()?,
            changes: ev["changes"]
                .as_array()?
                .iter()
                .map(|c| Change {
                    path: c["path"].as_str().unwrap_or_default().to_string(),
                    added: c["added"].as_u64().unwrap_or(0) as u32,
                    removed: c["removed"].as_u64().unwrap_or(0) as u32,
                    created: c["created"].as_bool().unwrap_or(false),
                    deleted: c["deleted"].as_bool().unwrap_or(false),
                })
                .collect(),
        })
    }

    /// `✎ 2 archivos · src/a.rs +3 −1 · b.rs nuevo +12`.
    pub fn summary(&self) -> String {
        let n = self.changes.len();
        let mut s = format!("✎ {n} {}", if n == 1 { "archivo" } else { "archivos" });
        for c in &self.changes {
            let tag = if c.deleted {
                " borrado".to_string()
            } else if c.created {
                format!(" nuevo +{}", c.added)
            } else {
                format!(" +{} −{}", c.added, c.removed)
            };
            s.push_str(&format!(" · {}{tag}", c.path));
        }
        s
    }
}

/// Texto que llega por trozos hasta que se completa el mensaje.
#[derive(Debug, Default)]
pub struct Stream {
    /// Sesión → texto parcial.
    pub text: HashMap<i64, String>,
}

impl Stream {
    /// Procesa `assistantDelta` (acumula) y `transcript` del asistente (cierra).
    pub fn apply(&mut self, ev: &Value) -> bool {
        match (ev["tipo"].as_str(), ev["session"].as_i64()) {
            (Some("assistantDelta"), Some(s)) => {
                self.text
                    .entry(s)
                    .or_default()
                    .push_str(ev["text"].as_str().unwrap_or_default());
                true
            }
            (Some("transcript"), Some(s)) if ev["entry"]["tipo"] == "assistant" => {
                self.text.remove(&s).is_some()
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tarjeta_de_un_turno() {
        let ev = json!({"tipo":"turnFinished","session":1,"changes":[
            {"path":"src/a.rs","added":3,"removed":1,"created":false,"deleted":false},
            {"path":"b.rs","added":12,"removed":0,"created":true,"deleted":false}]});
        let c = TurnCard::from_event(&ev).unwrap();
        assert_eq!(
            c.summary(),
            "✎ 2 archivos · src/a.rs +3 −1 · b.rs nuevo +12"
        );
        assert!(TurnCard::from_event(&json!({"tipo":"otro"})).is_none());
    }

    #[test]
    fn tarjeta_singular_y_borrados() {
        let ev = json!({"tipo":"turnFinished","session":2,"changes":[{"path":"x","added":0,"removed":4,"created":false,"deleted":true}]});
        assert_eq!(
            TurnCard::from_event(&ev).unwrap().summary(),
            "✎ 1 archivo · x borrado"
        );
    }

    #[test]
    fn streaming_se_acumula_y_se_cierra() {
        let mut s = Stream::default();
        s.apply(&json!({"tipo":"assistantDelta","session":1,"text":"ho"}));
        s.apply(&json!({"tipo":"assistantDelta","session":1,"text":"la"}));
        assert_eq!(s.text[&1], "hola");
        assert!(s.apply(
            &json!({"tipo":"transcript","session":1,"entry":{"tipo":"assistant","text":"hola"}})
        ));
        assert!(s.text.is_empty());
    }
}
