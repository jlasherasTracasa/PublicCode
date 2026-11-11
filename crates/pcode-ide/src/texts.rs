//! Textos de la interfaz en español e inglés (según `LANG`); nada de literales sueltos en la UI.

/// Todos los textos visibles del IDE.
#[derive(Debug)]
pub struct Texts {
    /// Título de la ventana.
    pub window_title: &'static str,
    /// Explorador.
    pub explorer: &'static str,
    /// Agente.
    pub agent: &'static str,
    /// Buscar en el proyecto.
    pub search: &'static str,
    /// Git.
    pub git: &'static str,
    /// Abrir archivo (Ctrl+P).
    pub quick_open: &'static str,
    /// Sin archivos abiertos.
    pub no_file: &'static str,
    /// Guardado.
    pub saved: &'static str,
    /// Cambios sin guardar.
    pub unsaved: &'static str,
    /// Enviar al agente.
    pub send: &'static str,
    /// Pista del cuadro del agente.
    pub agent_hint: &'static str,
    /// Permitir una aprobación.
    pub allow: &'static str,
    /// Denegar una aprobación.
    pub deny: &'static str,
    /// Pide aprobación.
    pub approval: &'static str,
    /// Deshacer el turno del agente.
    pub undo_turn: &'static str,
    /// Tema claro/oscuro.
    pub theme: &'static str,
    /// Memoria del IDE.
    pub memory: &'static str,
    /// Sin cambios en git.
    pub git_clean: &'static str,
    /// Resultados de la búsqueda.
    pub results: &'static str,
    /// Cerrar la pestaña.
    pub close: &'static str,
    /// Conectado al demonio.
    pub connected: &'static str,
    /// Reconectando con el demonio.
    pub reconnecting: &'static str,
    /// Archivo binario.
    pub binary: &'static str,
    /// Hay cambios sin guardar al cerrar.
    pub quit_unsaved: &'static str,
    /// Problemas (diagnósticos).
    pub problems: &'static str,
    /// Atajos.
    pub shortcuts: &'static str,
    /// Texto de la interfaz.
    pub find: &'static str,
    /// Texto de la interfaz.
    pub replace: &'static str,
    /// Texto de la interfaz.
    pub replace_all: &'static str,
    /// Texto de la interfaz.
    pub match_case: &'static str,
    /// Texto de la interfaz.
    pub go_to_line: &'static str,
    /// Texto de la interfaz.
    pub terminal: &'static str,
    /// Texto de la interfaz.
    pub stop: &'static str,
    /// Texto de la interfaz.
    pub run: &'static str,
    /// Texto de la interfaz.
    pub redo: &'static str,
    /// Texto de la interfaz.
    pub of: &'static str,
}

/// Español.
pub const ES: Texts = Texts {
    window_title: "PublicCode IDE",
    explorer: "Explorador",
    agent: "Agente",
    search: "Buscar",
    git: "Git",
    quick_open: "Abrir archivo…",
    no_file: "Abre un archivo del explorador o con Ctrl+P",
    saved: "guardado",
    unsaved: "sin guardar",
    send: "Enviar",
    agent_hint: "Pide algo al agente (Ctrl+Intro envía)",
    allow: "Permitir",
    deny: "Denegar",
    approval: "Pide aprobación",
    undo_turn: "↶ Deshacer el turno",
    theme: "Tema",
    memory: "memoria",
    git_clean: "sin cambios",
    results: "resultados",
    close: "Cerrar",
    connected: "conectado",
    reconnecting: "reconectando…",
    binary: "archivo binario: no se puede editar",
    quit_unsaved: "Hay cambios sin guardar: Ctrl+S para guardar o Ctrl+Q otra vez para salir",
    problems: "Problemas",
    shortcuts: "Ctrl+P abrir · Ctrl+S guardar · Ctrl+W cerrar · Ctrl+Mayús+F buscar · Ctrl+I agente",
    find: "Buscar",
    replace: "Reemplazar por",
    replace_all: "Reemplazar todo",
    match_case: "Aa",
    go_to_line: "Ir a la línea",
    terminal: "Terminal",
    stop: "Detener",
    run: "Ejecutar",
    redo: "↷ Rehacer",
    of: "de",
};

/// Inglés.
pub const EN: Texts = Texts {
    window_title: "PublicCode IDE",
    explorer: "Explorer",
    agent: "Agent",
    search: "Search",
    git: "Git",
    quick_open: "Open file…",
    no_file: "Open a file from the explorer or with Ctrl+P",
    saved: "saved",
    unsaved: "unsaved",
    send: "Send",
    agent_hint: "Ask the agent (Ctrl+Enter sends)",
    allow: "Allow",
    deny: "Deny",
    approval: "Needs approval",
    undo_turn: "↶ Undo turn",
    theme: "Theme",
    memory: "memory",
    git_clean: "clean",
    results: "results",
    close: "Close",
    connected: "connected",
    reconnecting: "reconnecting…",
    binary: "binary file: cannot be edited",
    quit_unsaved: "Unsaved changes: Ctrl+S to save or Ctrl+Q again to quit",
    problems: "Problems",
    shortcuts: "Ctrl+P open · Ctrl+S save · Ctrl+W close · Ctrl+Shift+F search · Ctrl+I agent",
    find: "Find",
    replace: "Replace with",
    replace_all: "Replace all",
    match_case: "Aa",
    go_to_line: "Go to line",
    terminal: "Terminal",
    stop: "Stop",
    run: "Run",
    redo: "↷ Redo",
    of: "of",
};

/// Textos para un valor de `LANG` (`es_ES.UTF-8` → español; lo demás que empiece por `en`, inglés).
pub fn for_locale(lang: &str) -> &'static Texts {
    if lang.to_ascii_lowercase().starts_with("en") {
        &EN
    } else {
        &ES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idioma_segun_lang() {
        assert_eq!(for_locale("es_ES.UTF-8").explorer, "Explorador");
        assert_eq!(for_locale("en_US.UTF-8").explorer, "Explorer");
        assert_eq!(
            for_locale("").explorer,
            "Explorador",
            "por defecto, español"
        );
    }
}
