//! Textos de la TUI en español e inglés: ningún literal visible fuera de estas tablas.

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Español.
    Es,
    /// Inglés.
    En,
}

impl Lang {
    /// Idioma según `LANG`/`LC_ALL` (inglés si empiezan por `en`; si no, español).
    pub fn from_env() -> Lang {
        let v = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        if v.starts_with("en") {
            Lang::En
        } else {
            Lang::Es
        }
    }
}

const ES: &[(&str, &str)] = &[
    ("input", " Mensaje · Intro envía · Alt+Intro nueva línea "),
    ("busy", " ⏳ Trabajando · Esc interrumpe "),
    ("ready", "✓ listo"),
    ("working", "trabajando"),
    (
        "hint",
        "PgUp/PgDn desplaza · Ctrl+O salidas · /ayuda · Ctrl+C sale",
    ),
    ("tokens", "tokens"),
    ("approval", " ⚠ Requiere aprobación "),
    (
        "approval_keys",
        "[s] sí   [t] sí, y no volver a preguntar   [n] no",
    ),
    ("interrupted", "✗ interrumpido"),
    ("lines", "líneas"),
    (
        "nothing_to_compact",
        "No hay bastante conversación para compactar.",
    ),
    (
        "plan_on",
        "◆ Modo plan: el agente solo lee y propone. Mayús+Tab o /construir para volver.",
    ),
    (
        "plan_off",
        "▶ Modo normal: el agente puede cambiar archivos y ejecutar órdenes.",
    ),
    (
        "shell_attached",
        "· La salida irá como contexto en tu próximo mensaje.",
    ),
    ("plan", "PLAN"),
    (
        "tips",
        "@archivo adjunta un archivo · !orden ejecuta en el proyecto\nMayús+Tab modo plan · Ctrl+O salidas · /ayuda todas las órdenes",
    ),
    ("no_output", "(sin salida)"),
    ("wait", "El agente sigue trabajando: espera o pulsa Esc."),
    ("new", "Conversación nueva."),
    ("model", "Modelo"),
    ("unknown", "Comando desconocido. Prueba /ayuda."),
    (
        "budget",
        "✗ presupuesto del mensaje agotado: escribe «sigue» para continuar",
    ),
    ("stuck", "✗ atascado: prueba a darle otra pista"),
    ("welcome", "Escribe lo que quieres hacer en este proyecto."),
    ("context", "contexto"),
    ("remembered", "🧠 Guardado en la memoria del proyecto:"),
    ("forgotten", "Olvidado:"),
    ("memory_title", "Memoria del proyecto"),
    (
        "memory_empty",
        "La memoria del proyecto está vacía. Añade notas con /recuerda <texto>.",
    ),
    (
        "memory_usage",
        "Uso: /recuerda <texto> · /memoria · /olvida <número>",
    ),
    ("hint_short", "/ayuda · Ctrl+C sale"),
    ("undo_none", "No hay ningún turno del agente que deshacer."),
    ("redo_none", "No hay nada que rehacer."),
    ("undone", "↶ Deshecho"),
    ("redone", "↷ Rehecho"),
    ("files", "archivos"),
    ("new_file", "nuevo"),
    ("deleted_file", "borrado"),
    ("undo_hint", "/deshacer lo revierte"),
    (
        "help",
        "/ayuda  esta ayuda\n/nuevo  olvida el historial\n/modelo [m]  muestra o cambia el modelo\n/uso  tokens gastados\n/compactar  resume la conversación para liberar contexto\n/deshacer · /rehacer  revierte o vuelve a aplicar el último turno del agente\n/recuerda <texto> · /memoria · /olvida <n>  memoria del proyecto (el agente la lee en cada turno)\n/plan · /construir  modo plan (solo lectura) o normal (Mayús+Tab)\n@archivo  adjunta un archivo (Tab completa) · !orden  ejecuta y adjunta la salida\n/salir  termina\nIntro envía · Alt+Intro o \\ al final: nueva línea · ↑/↓ historial\nEsc interrumpe al agente · PgUp/PgDn desplaza · Ctrl+O despliega las salidas de las tools",
    ),
];

const EN: &[(&str, &str)] = &[
    ("input", " Message · Enter sends · Alt+Enter new line "),
    ("busy", " ⏳ Working · Esc interrupts "),
    ("ready", "✓ ready"),
    ("working", "working"),
    (
        "hint",
        "PgUp/PgDn scroll · Ctrl+O outputs · /help · Ctrl+C quits",
    ),
    ("tokens", "tokens"),
    ("approval", " ⚠ Approval required "),
    (
        "approval_keys",
        "[y] yes   [a] yes, don't ask again   [n] no",
    ),
    ("interrupted", "✗ interrupted"),
    ("lines", "lines"),
    ("nothing_to_compact", "Not enough conversation to compact."),
    (
        "plan_on",
        "◆ Plan mode: the agent only reads and proposes. Shift+Tab or /build to go back.",
    ),
    (
        "plan_off",
        "▶ Normal mode: the agent can change files and run commands.",
    ),
    (
        "shell_attached",
        "· The output will be sent as context with your next message.",
    ),
    ("plan", "PLAN"),
    (
        "tips",
        "@file attaches a file · !command runs it in the project\nShift+Tab plan mode · Ctrl+O outputs · /help all commands",
    ),
    ("no_output", "(no output)"),
    ("wait", "The agent is still working: wait or press Esc."),
    ("new", "New conversation."),
    ("model", "Model"),
    ("unknown", "Unknown command. Try /help."),
    (
        "budget",
        "✗ message budget exhausted: type “continue” to go on",
    ),
    ("stuck", "✗ stuck: try giving it another hint"),
    ("welcome", "Type what you want to do in this project."),
    ("context", "context"),
    ("remembered", "🧠 Saved to the project memory:"),
    ("forgotten", "Forgotten:"),
    ("memory_title", "Project memory"),
    (
        "memory_empty",
        "The project memory is empty. Add notes with /remember <text>.",
    ),
    (
        "memory_usage",
        "Usage: /remember <text> · /memory · /forget <number>",
    ),
    ("hint_short", "/help · Ctrl+C quits"),
    ("undo_none", "There is no agent turn to undo."),
    ("redo_none", "Nothing to redo."),
    ("undone", "↶ Undone"),
    ("redone", "↷ Redone"),
    ("files", "files"),
    ("new_file", "new"),
    ("deleted_file", "deleted"),
    ("undo_hint", "/undo reverts it"),
    (
        "help",
        "/help  this help\n/new  forget the history\n/model [m]  show or change the model\n/usage  tokens spent\n/compact  summarise the conversation to free context\n/undo · /redo  revert or reapply the agent's last turn\n/remember <text> · /memory · /forget <n>  project memory (read by the agent every turn)\n/plan · /build  plan (read-only) or normal mode (Shift+Tab)\n@file  attach a file (Tab completes) · !command  run it and attach the output\n/quit  exit\nEnter sends · Alt+Enter or a trailing \\: new line · ↑/↓ history\nEsc interrupts the agent · PgUp/PgDn scroll · Ctrl+O expands tool outputs",
    ),
];

/// Texto de `key` en `lang` (la propia clave si falta, para que se note en las pruebas).
pub fn t(lang: Lang, key: &str) -> &'static str {
    let table = match lang {
        Lang::Es => ES,
        Lang::En => EN,
    };
    table
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("??")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i18n_las_dos_tablas_tienen_las_mismas_claves() {
        fn ks(t: &[(&'static str, &'static str)]) -> Vec<&'static str> {
            t.iter().map(|(k, _)| *k).collect()
        }
        assert_eq!(ks(ES), ks(EN));
        for (k, _) in ES {
            assert_ne!(t(Lang::En, k), "??");
        }
    }
}
