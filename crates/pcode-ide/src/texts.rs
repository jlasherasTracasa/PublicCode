//! Textos de la interfaz en español e inglés (según `LANG`); nada de literales sueltos en la UI.

/// Todos los textos visibles del IDE.
#[derive(Debug)]
pub struct Texts {
    /// «PublicCode».
    pub window_title: &'static str,
    /// «Explorador».
    pub explorer: &'static str,
    /// «Buscar».
    pub search: &'static str,
    /// «Control de código fuente».
    pub git: &'static str,
    /// «Ajustes».
    pub settings: &'static str,
    /// «Agente».
    pub agent: &'static str,
    /// «Problemas».
    pub problems: &'static str,
    /// «Salida».
    pub output: &'static str,
    /// «Terminal».
    pub terminal: &'static str,
    /// «Archivo».
    pub menu_file: &'static str,
    /// «Editar».
    pub menu_edit: &'static str,
    /// «Ver».
    pub menu_view: &'static str,
    /// «Ir».
    pub menu_go: &'static str,
    /// «Terminal».
    pub menu_terminal: &'static str,
    /// «Ayuda».
    pub menu_help: &'static str,
    /// «Guardar».
    pub save: &'static str,
    /// «Guardar todo».
    pub save_all: &'static str,
    /// «Cerrar editor».
    pub close_editor: &'static str,
    /// «Salir».
    pub quit: &'static str,
    /// «Buscar».
    pub find: &'static str,
    /// «Reemplazar».
    pub replace: &'static str,
    /// «Reemplazar todo».
    pub replace_all: &'static str,
    /// «Coincidir mayúsculas».
    pub match_case: &'static str,
    /// «Paleta de comandos…».
    pub command_palette: &'static str,
    /// «Ir a archivo…».
    pub quick_open: &'static str,
    /// «Ir a la línea…».
    pub go_to_line: &'static str,
    /// «Ir a la definición».
    pub go_to_definition: &'static str,
    /// «Mostrar información».
    pub show_hover: &'static str,
    /// «Barra lateral».
    pub toggle_sidebar: &'static str,
    /// «Panel».
    pub toggle_panel: &'static str,
    /// «Agente (barra secundaria)».
    pub toggle_agent: &'static str,
    /// «Nueva orden en la terminal».
    pub new_terminal: &'static str,
    /// «Cambiar tema claro/oscuro».
    pub theme_toggle: &'static str,
    /// «Aumentar letra del editor».
    pub zoom_in: &'static str,
    /// «Reducir letra del editor».
    pub zoom_out: &'static str,
    /// «Acerca de PublicCode».
    pub about: &'static str,
    /// «IDE ligero con agentes. Sin navegador: egui sobre OpenGL.».
    pub about_text: &'static str,
    /// «Escribe el nombre de un comando».
    pub type_command: &'static str,
    /// «Busca archivos por nombre».
    pub type_file: &'static str,
    /// «Sin resultados».
    pub no_results: &'static str,
    /// «resultados».
    pub results: &'static str,
    /// «archivos».
    pub in_files: &'static str,
    /// «Mensaje (Ctrl+Intro para confirmar)».
    pub commit_hint: &'static str,
    /// «✓ Confirmar».
    pub commit: &'static str,
    /// «Cambios preparados».
    pub staged: &'static str,
    /// «Cambios».
    pub changes: &'static str,
    /// «No hay cambios.».
    pub no_changes: &'static str,
    /// «Preparar».
    pub stage: &'static str,
    /// «Quitar».
    pub unstage: &'static str,
    /// «Actualizar».
    pub refresh: &'static str,
    /// «Nueva conversación».
    pub new_chat: &'static str,
    /// «Pide algo al agente… (Ctrl+Intro envía, @archivo adjunta)».
    pub agent_hint: &'static str,
    /// «Pregunta, pide un cambio o un test. El agente ve tu proyecto, trabaja con aprobaciones y cada turno se puede deshacer.».
    pub agent_empty: &'static str,
    /// «El agente está trabajando…».
    pub agent_working: &'static str,
    /// «Enviar».
    pub send: &'static str,
    /// «Detener».
    pub stop: &'static str,
    /// «Tú».
    pub you: &'static str,
    /// «Pide aprobación».
    pub approval: &'static str,
    /// «Permitir».
    pub allow: &'static str,
    /// «Denegar».
    pub deny: &'static str,
    /// «↶ Deshacer el turno».
    pub undo_turn: &'static str,
    /// «↷ Rehacer».
    pub redo: &'static str,
    /// «Editor ligero con agentes».
    pub welcome_sub: &'static str,
    /// «Inicio».
    pub start: &'static str,
    /// «Recientes».
    pub recent: &'static str,
    /// «Atajos».
    pub help_title: &'static str,
    /// «No se han detectado problemas en el área de trabajo.».
    pub no_problems: &'static str,
    /// «Ejecutar».
    pub run: &'static str,
    /// «Orden (Intro ejecuta)».
    pub terminal_hint: &'static str,
    /// «Guardado».
    pub saved: &'static str,
    /// «sin guardar».
    pub unsaved: &'static str,
    /// «Archivo binario: no se puede editar».
    pub binary: &'static str,
    /// «conectado».
    pub connected: &'static str,
    /// «reconectando…».
    pub reconnecting: &'static str,
    /// «MB».
    pub memory: &'static str,
    /// «Tamaño de letra del editor».
    pub font_size: &'static str,
    /// «Tema».
    pub theme: &'static str,
    /// «Oscuro».
    pub dark: &'static str,
    /// «Claro».
    pub light: &'static str,
    /// «Servidores de lenguaje».
    pub lsp_servers: &'static str,
    /// «Ninguno arrancado: se arrancan al abrir un archivo.».
    pub lsp_none: &'static str,
    /// «Hay cambios sin guardar. Pulsa Ctrl+Q otra vez para salir sin guardar.».
    pub quit_unsaved: &'static str,
    /// «Sin información en esta posición».
    pub hover_none: &'static str,
    /// «No se encontró la definición».
    pub definition_none: &'static str,
    /// «Para abrir otra carpeta: pcode ide <ruta>».
    pub open_folder_hint: &'static str,
    /// «Abre un archivo del explorador o con Ctrl+P».
    pub no_file: &'static str,
    /// «Ctrl+P abrir · Ctrl+Mayús+P comandos · Ctrl+B barra · Ctrl+J panel · Ctrl+I agente».
    pub shortcuts: &'static str,
}

/// Español.
pub const ES: Texts = Texts {
    window_title: "PublicCode",
    explorer: "Explorador",
    search: "Buscar",
    git: "Control de código fuente",
    settings: "Ajustes",
    agent: "Agente",
    problems: "Problemas",
    output: "Salida",
    terminal: "Terminal",
    menu_file: "Archivo",
    menu_edit: "Editar",
    menu_view: "Ver",
    menu_go: "Ir",
    menu_terminal: "Terminal",
    menu_help: "Ayuda",
    save: "Guardar",
    save_all: "Guardar todo",
    close_editor: "Cerrar editor",
    quit: "Salir",
    find: "Buscar",
    replace: "Reemplazar",
    replace_all: "Reemplazar todo",
    match_case: "Coincidir mayúsculas",
    command_palette: "Paleta de comandos…",
    quick_open: "Ir a archivo…",
    go_to_line: "Ir a la línea…",
    go_to_definition: "Ir a la definición",
    show_hover: "Mostrar información",
    toggle_sidebar: "Barra lateral",
    toggle_panel: "Panel",
    toggle_agent: "Agente (barra secundaria)",
    new_terminal: "Nueva orden en la terminal",
    theme_toggle: "Cambiar tema claro/oscuro",
    zoom_in: "Aumentar letra del editor",
    zoom_out: "Reducir letra del editor",
    about: "Acerca de PublicCode",
    about_text: "IDE ligero con agentes. Sin navegador: egui sobre OpenGL.",
    type_command: "Escribe el nombre de un comando",
    type_file: "Busca archivos por nombre",
    no_results: "Sin resultados",
    results: "resultados",
    in_files: "archivos",
    commit_hint: "Mensaje (Ctrl+Intro para confirmar)",
    commit: "✓ Confirmar",
    staged: "Cambios preparados",
    changes: "Cambios",
    no_changes: "No hay cambios.",
    stage: "Preparar",
    unstage: "Quitar",
    refresh: "Actualizar",
    new_chat: "Nueva conversación",
    agent_hint: "Pide algo al agente… (Ctrl+Intro envía, @archivo adjunta)",
    agent_empty: "Pregunta, pide un cambio o un test. El agente ve tu proyecto, trabaja con aprobaciones y cada turno se puede deshacer.",
    agent_working: "El agente está trabajando…",
    send: "Enviar",
    stop: "Detener",
    you: "Tú",
    approval: "Pide aprobación",
    allow: "Permitir",
    deny: "Denegar",
    undo_turn: "↶ Deshacer el turno",
    redo: "↷ Rehacer",
    welcome_sub: "Editor ligero con agentes",
    start: "Inicio",
    recent: "Recientes",
    help_title: "Atajos",
    no_problems: "No se han detectado problemas en el área de trabajo.",
    run: "Ejecutar",
    terminal_hint: "Orden (Intro ejecuta)",
    saved: "Guardado",
    unsaved: "sin guardar",
    binary: "Archivo binario: no se puede editar",
    connected: "conectado",
    reconnecting: "reconectando…",
    memory: "MB",
    font_size: "Tamaño de letra del editor",
    theme: "Tema",
    dark: "Oscuro",
    light: "Claro",
    lsp_servers: "Servidores de lenguaje",
    lsp_none: "Ninguno arrancado: se arrancan al abrir un archivo.",
    quit_unsaved: "Hay cambios sin guardar. Pulsa Ctrl+Q otra vez para salir sin guardar.",
    hover_none: "Sin información en esta posición",
    definition_none: "No se encontró la definición",
    open_folder_hint: "Para abrir otra carpeta: pcode ide <ruta>",
    no_file: "Abre un archivo del explorador o con Ctrl+P",
    shortcuts: "Ctrl+P abrir · Ctrl+Mayús+P comandos · Ctrl+B barra · Ctrl+J panel · Ctrl+I agente",
};

/// Inglés.
pub const EN: Texts = Texts {
    window_title: "PublicCode",
    explorer: "Explorer",
    search: "Search",
    git: "Source Control",
    settings: "Settings",
    agent: "Agent",
    problems: "Problems",
    output: "Output",
    terminal: "Terminal",
    menu_file: "File",
    menu_edit: "Edit",
    menu_view: "View",
    menu_go: "Go",
    menu_terminal: "Terminal",
    menu_help: "Help",
    save: "Save",
    save_all: "Save All",
    close_editor: "Close Editor",
    quit: "Exit",
    find: "Find",
    replace: "Replace",
    replace_all: "Replace All",
    match_case: "Match Case",
    command_palette: "Command Palette…",
    quick_open: "Go to File…",
    go_to_line: "Go to Line…",
    go_to_definition: "Go to Definition",
    show_hover: "Show Hover",
    toggle_sidebar: "Primary Side Bar",
    toggle_panel: "Panel",
    toggle_agent: "Agent (Secondary Side Bar)",
    new_terminal: "Run in Terminal",
    theme_toggle: "Toggle Light/Dark Theme",
    zoom_in: "Increase Editor Font",
    zoom_out: "Decrease Editor Font",
    about: "About PublicCode",
    about_text: "Lightweight agentic IDE. No browser: egui on OpenGL.",
    type_command: "Type the name of a command",
    type_file: "Search files by name",
    no_results: "No results",
    results: "results",
    in_files: "files",
    commit_hint: "Message (Ctrl+Enter to commit)",
    commit: "✓ Commit",
    staged: "Staged Changes",
    changes: "Changes",
    no_changes: "No changes.",
    stage: "Stage",
    unstage: "Unstage",
    refresh: "Refresh",
    new_chat: "New Chat",
    agent_hint: "Ask the agent… (Ctrl+Enter sends)",
    agent_empty: "Ask a question, request a change or a test. The agent sees your project, works with approvals and every turn can be undone.",
    agent_working: "The agent is working…",
    send: "Send",
    stop: "Stop",
    you: "You",
    approval: "Needs approval",
    allow: "Allow",
    deny: "Deny",
    undo_turn: "↶ Undo Turn",
    redo: "↷ Redo",
    welcome_sub: "Lightweight editor with agents",
    start: "Start",
    recent: "Recent",
    help_title: "Shortcuts",
    no_problems: "No problems have been detected in the workspace.",
    run: "Run",
    terminal_hint: "Command (Enter runs)",
    saved: "Saved",
    unsaved: "unsaved",
    binary: "Binary file: cannot be edited",
    connected: "connected",
    reconnecting: "reconnecting…",
    memory: "MB",
    font_size: "Editor font size",
    theme: "Theme",
    dark: "Dark",
    light: "Light",
    lsp_servers: "Language servers",
    lsp_none: "None running: they start when you open a file.",
    quit_unsaved: "Unsaved changes. Press Ctrl+Q again to quit without saving.",
    hover_none: "No information here",
    definition_none: "No definition found",
    open_folder_hint: "To open another folder: pcode ide <path>",
    no_file: "Open a file from the explorer or with Ctrl+P",
    shortcuts: "Ctrl+P open · Ctrl+Shift+P commands · Ctrl+B side bar · Ctrl+J panel · Ctrl+I agent",
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
