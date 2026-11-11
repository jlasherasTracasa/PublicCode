//! Registro de resultados de tools: cada resultado lleva su id `[rN]` en el propio texto, así que
//! compactar o soltar el historial no rompe los índices. El registro solo guarda el último id dado y
//! los ids fijados.
use pcode_models::{Message, Role};
use std::collections::BTreeSet;

/// Longitud máxima de la primera línea que se conserva al soltar un resultado.
const MAX_PRIMERA: usize = 80;

/// Id de un resultado de tool etiquetado (`[r7] …` o `[r7 soltado: …]`).
pub fn result_id(content: &str) -> Option<u32> {
    let rest = content.strip_prefix("[r")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let after = &rest[digits.len()..];
    if !(after.starts_with(']') || after.starts_with(' ')) {
        return None;
    }
    digits.parse().ok()
}

/// Registro de resultados de tools: último id dado y resultados fijados.
#[derive(Debug, Clone, Default)]
pub struct ContextLedger {
    last: u32,
    pinned: BTreeSet<u32>,
}

impl ContextLedger {
    /// Registro vacío (el primer id será 1).
    pub fn new() -> Self {
        Self::default()
    }

    /// Pone `last` al día con un historial ya etiquetado.
    pub fn sync(&mut self, messages: &[Message]) {
        let ids = messages
            .iter()
            .filter(|m| m.role == Role::Tool)
            .filter_map(|m| result_id(&m.content));
        for id in ids {
            self.last = self.last.max(id);
        }
    }

    /// Etiqueta un resultado nuevo: `[rN] contenido`.
    pub fn tag(&mut self, content: &str) -> String {
        self.last += 1;
        format!("[r{}] {content}", self.last)
    }

    /// Fija resultados; devuelve cuántos son nuevos.
    pub fn pin(&mut self, ids: &[u32]) -> usize {
        let mut nuevos = 0;
        for &id in ids {
            if (1..=self.last).contains(&id) && self.pinned.insert(id) {
                nuevos += 1;
            }
        }
        nuevos
    }

    /// Si el id está fijado.
    pub fn is_pinned(&self, id: u32) -> bool {
        self.pinned.contains(&id)
    }

    /// Si el mensaje es un resultado de tool fijado.
    pub fn message_pinned(&self, m: &Message) -> bool {
        m.role == Role::Tool && result_id(&m.content).is_some_and(|id| self.pinned.contains(&id))
    }

    /// Sustituye por un marcador los resultados indicados que no estén fijados.
    pub fn drop_results(&self, ids: &[u32], messages: &mut [Message]) -> usize {
        let mut cambiados = 0;
        for m in messages.iter_mut().filter(|m| m.role == Role::Tool) {
            for &id in ids {
                if self.pinned.contains(&id) {
                    continue;
                }
                let prefijo = format!("[r{id}] ");
                if let Some(resto) = m.content.strip_prefix(&prefijo) {
                    let primera: String = resto
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(MAX_PRIMERA)
                        .collect();
                    m.content = format!("[r{id} soltado: {primera}]");
                    cambiados += 1;
                    break;
                }
            }
        }
        cambiados
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiqueta_numera_desde_uno() {
        let mut l = ContextLedger::new();
        assert_eq!(l.tag("a"), "[r1] a");
        assert_eq!(l.tag("b"), "[r2] b");
    }

    #[test]
    fn id_de_resultado() {
        assert_eq!(result_id("[r7] hola"), Some(7));
        assert_eq!(result_id("[r7 soltado: hola]"), Some(7));
        assert_eq!(result_id("r12"), None);
        assert_eq!(result_id(""), None);
    }

    #[test]
    fn soltar_sustituye_por_marcador() {
        let mut l = ContextLedger::new();
        let mut ms = vec![Message::tool_result("c1", l.tag("  linea uno\nlinea dos"))];
        assert_eq!(ms[0].content, "[r1]   linea uno\nlinea dos");
        assert_eq!(l.drop_results(&[1], &mut ms), 1);
        assert_eq!(ms[0].content, "[r1 soltado: linea uno]");

        let largo = "x".repeat(200);
        let mut ms = vec![Message::tool_result("c2", l.tag(&largo))];
        assert_eq!(l.drop_results(&[2], &mut ms), 1);
        assert_eq!(ms[0].content, format!("[r2 soltado: {}]", "x".repeat(80)));
    }

    #[test]
    fn soltar_no_toca_fijados_ni_repite() {
        let mut l = ContextLedger::new();
        let mut ms = vec![
            Message::tool_result("c1", l.tag("uno")),
            Message::tool_result("c2", l.tag("dos")),
        ];
        assert_eq!(l.pin(&[1]), 1);
        assert_eq!(l.drop_results(&[1], &mut ms), 0);
        assert_eq!(l.drop_results(&[2], &mut ms), 1);
        assert_eq!(l.drop_results(&[2], &mut ms), 0);
    }

    #[test]
    fn fijar_ignora_ids_desconocidos() {
        let mut l = ContextLedger::new();
        l.tag("a");
        l.tag("b");
        assert_eq!(l.pin(&[2, 9]), 1);
        assert!(l.is_pinned(2));
        assert!(!l.is_pinned(9));
        assert!(!l.is_pinned(1));
    }

    #[test]
    fn sincroniza_con_el_historial() {
        let mut l = ContextLedger::new();
        l.sync(&[Message::tool_result("c", "[r5] x")]);
        assert_eq!(l.tag("y"), "[r6] y");
    }

    #[test]
    fn mensaje_fijado_solo_si_es_de_tool() {
        let mut l = ContextLedger::new();
        l.tag("x");
        l.pin(&[1]);
        let de_usuario = Message::user("[r1] x");
        let de_tool = Message::tool_result("c", "[r1] x");
        assert!(!l.message_pinned(&de_usuario));
        assert!(l.message_pinned(&de_tool));
    }
}
