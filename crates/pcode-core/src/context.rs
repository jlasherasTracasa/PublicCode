//! Registro de resultados de tools: cada resultado lleva su id `[rN]` en el propio texto, así que
//! compactar o soltar el historial no rompe los índices. El registro solo guarda el último id dado y
//! los ids fijados.
use pcode_models::{Message, Role, ToolSpec};
use serde_json::{Value, json};
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

/// Nombre de la tool con la que el agente gestiona su contexto.
pub const CONTEXT_TOOL: &str = "contexto";

/// Especificación de la tool `contexto`.
pub fn context_spec() -> ToolSpec {
    ToolSpec {
        name: CONTEXT_TOOL.into(),
        description: "Gestiona tu contexto. Cada resultado de tool empieza por un id como [r7]. \
            op \"soltar\": sustituye por una línea los resultados de `ids` que ya no necesitas. \
            op \"fijar\": marca los resultados de `ids` para que la compactación no los resuma. \
            op \"lista\": qué resultados siguen en el contexto."
            .into(),
        parameters: json!({"type":"object","properties":{"op":{"type":"string","enum":["soltar","fijar","lista"]},"ids":{"type":"array","items":{"type":"integer"}}},"required":["op"]}),
    }
}

/// Mensaje cuando una operación necesita ids y no los trae.
fn falta_ids(op: &str) -> String {
    format!("faltan ids, p. ej. {{\"op\":\"{op}\",\"ids\":[3]}}")
}

impl ContextLedger {
    /// Ejecuta una llamada a `contexto` sobre el historial y devuelve el texto del resultado.
    pub fn handle(&mut self, args: &Value, messages: &mut [Message]) -> String {
        let ids: Vec<u32> = args
            .get("ids")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_u64)
                    .filter(|&n| n > 0)
                    .filter_map(|n| u32::try_from(n).ok())
                    .collect()
            })
            .unwrap_or_default();
        match args.get("op").and_then(Value::as_str) {
            Some("soltar") => {
                if ids.is_empty() {
                    return falta_ids("soltar");
                }
                let n = self.drop_results(&ids, messages);
                format!(
                    "soltados {n} de {} (los fijados y los ya soltados no se tocan)",
                    ids.len()
                )
            }
            Some("fijar") => {
                if ids.is_empty() {
                    return falta_ids("fijar");
                }
                self.pin(&ids);
                let fijados: Vec<String> = self.pinned.iter().map(|id| format!("r{id}")).collect();
                if fijados.is_empty() {
                    "fijados: ninguno".into()
                } else {
                    format!("fijados: {}", fijados.join(", "))
                }
            }
            Some("lista") => self.lista(messages),
            _ => "op no válida: usa \"soltar\", \"fijar\" o \"lista\"".into(),
        }
    }

    /// Una línea por resultado de tool que sigue en el contexto (los soltados no salen).
    fn lista(&self, messages: &[Message]) -> String {
        let lineas: Vec<String> = messages
            .iter()
            .filter(|m| m.role == Role::Tool)
            .filter_map(|m| {
                let id = result_id(&m.content)?;
                let prefijo = format!("[r{id}] ");
                let resto = m.content.strip_prefix(prefijo.as_str())?;
                let marca = if self.pinned.contains(&id) {
                    " (fijado)"
                } else {
                    ""
                };
                let n = m.content.chars().count();
                let primera: String = resto
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(60)
                    .collect();
                Some(format!("r{id}{marca} {n} caracteres: {primera}"))
            })
            .collect();
        if lineas.is_empty() {
            "no hay resultados de tools en el contexto".into()
        } else {
            lineas.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_spec_tiene_nombre_y_op_obligatoria() {
        let spec = context_spec();
        assert_eq!(spec.name, "contexto");
        let requeridos = spec.parameters["required"].as_array().unwrap();
        assert!(requeridos.iter().any(|v| v == "op"));
    }

    #[test]
    fn tool_soltar_y_fijar_como_en_el_ejemplo() {
        let mut ms = vec![
            Message::tool_result("c1", "[r1] uno"),
            Message::tool_result("c2", "[r2] dos\nmás"),
            Message::tool_result("c3", "[r3] tres"),
        ];
        let mut l = ContextLedger::new();
        l.sync(&ms);
        assert_eq!(
            l.handle(&json!({"op":"fijar","ids":[1]}), &mut ms),
            "fijados: r1"
        );
        assert_eq!(
            l.handle(&json!({"op":"soltar","ids":[1,2]}), &mut ms),
            "soltados 1 de 2 (los fijados y los ya soltados no se tocan)"
        );
        assert_eq!(
            l.handle(&json!({"op":"lista"}), &mut ms),
            "r1 (fijado) 8 caracteres: uno\nr3 9 caracteres: tres"
        );
        assert_eq!(
            l.handle(&json!({"op":"borrar"}), &mut ms),
            "op no válida: usa \"soltar\", \"fijar\" o \"lista\""
        );
    }

    #[test]
    fn tool_lista_omite_soltados_y_marca_fijados() {
        let mut ms = vec![
            Message::tool_result("c1", "[r1] uno"),
            Message::tool_result("c2", "[r2] dos"),
            Message::tool_result("c3", "[r3] tres"),
        ];
        let mut l = ContextLedger::new();
        l.sync(&ms);
        l.handle(&json!({"op":"fijar","ids":[1]}), &mut ms);
        l.handle(&json!({"op":"soltar","ids":[2]}), &mut ms);
        let lista = l.handle(&json!({"op":"lista"}), &mut ms);
        assert_eq!(
            lista,
            "r1 (fijado) 8 caracteres: uno\nr3 9 caracteres: tres"
        );
        assert!(!lista.contains("r2"));
    }

    #[test]
    fn tool_sin_ids_explica_el_formato() {
        let mut l = ContextLedger::new();
        let texto = l.handle(&json!({"op":"soltar"}), &mut []);
        assert!(texto.starts_with("faltan ids"));
    }

    #[test]
    fn tool_op_desconocida() {
        let mut l = ContextLedger::new();
        let esperado = "op no válida: usa \"soltar\", \"fijar\" o \"lista\"";
        assert_eq!(l.handle(&json!({"op":"borrar"}), &mut []), esperado);
        assert_eq!(l.handle(&json!({}), &mut []), esperado);
    }

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
