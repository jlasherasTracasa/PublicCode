//! Árbol de sesiones y subagentes para la barra lateral, como lógica pura.
//! Las sesiones guardadas del proyecto van primero; bajo la actual cuelgan sus subagentes.
//! No hay E/S aquí: quien llama pasa los datos ya leídos.
use crate::app::{Entry, ToolState};

/// Una conversación guardada del proyecto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    /// Identificador en la base de sesiones.
    pub id: i64,
    /// Título (el primer mensaje, recortado).
    pub title: String,
    /// Mensajes guardados.
    pub messages: usize,
    /// Es la conversación abierta.
    pub current: bool,
}

/// Un subagente lanzado en la conversación actual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentItem {
    /// Qué se le encargó (resumen de los argumentos).
    pub brief: String,
    /// Estado.
    pub state: ToolState,
}

/// Tools que lanzan un subagente.
pub const SUBAGENT_TOOLS: [&str; 3] = ["subagente", "agente", "task"];

/// Subagentes de la transcripción, en orden.
pub fn subagents_of(entries: &[Entry]) -> Vec<SubagentItem> {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Tool {
                name, brief, state, ..
            } if SUBAGENT_TOOLS.contains(&name.as_str()) => Some(SubagentItem {
                brief: brief.clone(),
                state: *state,
            }),
            _ => None,
        })
        .collect()
}

/// Filas del árbol, de `width` caracteres como mucho.
pub fn tree_rows(sessions: &[SessionItem], subagents: &[SubagentItem], width: u16) -> Vec<String> {
    let w = width as usize;
    let mut out = Vec::new();
    let any_current = sessions.iter().any(|s| s.current);
    for s in sessions {
        let mark = if s.current { "●" } else { "○" };
        let title = if s.title.is_empty() {
            "—"
        } else {
            s.title.as_str()
        };
        out.push(fit(&format!(" {mark} {title} ({})", s.messages), w));
        if s.current {
            out.extend(subagents.iter().map(|sub| fit(&subagent_row(sub), w)));
        }
    }
    if !any_current {
        out.extend(subagents.iter().map(|sub| fit(&subagent_row(sub), w)));
    }
    out
}

/// Fila de un subagente, con el símbolo de su estado.
fn subagent_row(sub: &SubagentItem) -> String {
    let symbol = match sub.state {
        ToolState::Running => "⏳",
        ToolState::Ok => "✓",
        ToolState::Err => "✗",
    };
    format!("   {symbol} {}", sub.brief)
}

/// Recorta `s` a `w` caracteres; si se recorta, el último es `…`.
fn fit(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        return s.to_string();
    }
    if w == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(w - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: i64, title: &str, messages: usize, current: bool) -> SessionItem {
        SessionItem {
            id,
            title: title.to_string(),
            messages,
            current,
        }
    }

    fn sub(brief: &str, state: ToolState) -> SubagentItem {
        SubagentItem {
            brief: brief.to_string(),
            state,
        }
    }

    #[test]
    fn arbol_anida_subagentes_bajo_la_sesion_actual() {
        let sessions = [
            session(2, "arregla el test", 6, true),
            session(1, "añade login", 12, false),
        ];
        let subagents = [
            sub("busca usos de foo", ToolState::Ok),
            sub("revisa el diff", ToolState::Running),
        ];
        let rows = tree_rows(&sessions, &subagents, 29);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0], " ● arregla el test (6)");
        assert_eq!(rows[1], "   ✓ busca usos de foo");
        assert_eq!(rows[2], "   ⏳ revisa el diff");
        assert_eq!(rows[3], " ○ añade login (12)");
    }

    #[test]
    fn arbol_recorta_filas_largas() {
        let sessions = [session(1, "un título larguísimo que no cabe", 3, false)];
        let rows = tree_rows(&sessions, &[], 12);
        assert_eq!(rows, vec![" ○ un títul…".to_string()]);
        assert_eq!(rows[0].chars().count(), 12);
    }

    #[test]
    fn arbol_sin_sesion_actual_pone_subagentes_al_final() {
        let sessions = [session(1, "uno", 1, false), session(2, "dos", 2, false)];
        let subagents = [sub("revisa", ToolState::Running)];
        let rows = tree_rows(&sessions, &subagents, 40);
        assert_eq!(rows.len(), 3);
        assert!(rows[2].starts_with("   "));
        assert_eq!(rows[2], "   ⏳ revisa");
    }

    #[test]
    fn subagentes_salen_de_las_tools_del_catalogo() {
        let entries = [
            Entry::Tool {
                name: "bash".into(),
                brief: "ls".into(),
                state: ToolState::Ok,
                output: String::new(),
            },
            Entry::Tool {
                name: "subagente".into(),
                brief: "busca".into(),
                state: ToolState::Running,
                output: String::new(),
            },
            Entry::User("hola".into()),
            Entry::Tool {
                name: "task".into(),
                brief: "revisa".into(),
                state: ToolState::Err,
                output: String::new(),
            },
        ];
        let found = subagents_of(&entries);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], sub("busca", ToolState::Running));
        assert_eq!(found[1], sub("revisa", ToolState::Err));
    }

    #[test]
    fn arbol_titulo_vacio_es_raya() {
        let sessions = [session(1, "", 0, false)];
        assert_eq!(tree_rows(&sessions, &[], 20), vec![" ○ — (0)".to_string()]);
    }
}
