//! Interfaz del IDE con la estructura de VS Code: barra de título con menús y «command center»,
//! barra de actividad, barra lateral, pestañas con migas de pan, panel inferior, agente como barra
//! secundaria y barra de estado. Las peticiones van en hilos y vuelven por un canal; se repinta solo
//! cuando llega algo (0 % de CPU en reposo).
use crate::agent::{Stream, TurnCard};
use crate::find::{find_all, replace_all};
use crate::git::{DiffLine, diff_lines, is_staged};
use crate::highlight::layout_job;
use crate::lsp_pos::{from_lsp, to_lsp};
use crate::problems::Problems;
use crate::rpc::Client;
use crate::settings::IdeSettings;
use crate::state::{Entry, Tabs, Tree, fuzzy_score, line_to_byte, own_rss_mb, quick_open};
use crate::terminal::Terminal;
use crate::texts::Texts;
use crate::theme::{self, Palette, file_icon, git_decoration};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, Modifiers, RichText, Sense, Stroke, vec2,
};
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Resultado de una petición o evento que llega al hilo de la interfaz.
pub enum Msg {
    /// Contenido de una carpeta.
    Dir(String, Vec<Entry>),
    /// Archivo leído (ruta, texto, es binario).
    File(String, String, bool),
    /// Lista de archivos del proyecto.
    Files(Vec<String>),
    /// Resultados de búsqueda: (ruta, línea, texto).
    Hits(Vec<(String, u32, String)>),
    /// Rama y archivos de git: (ruta, índice, árbol).
    Git(String, Vec<GitEntry>),
    /// Archivo guardado.
    Saved(String, String),
    /// Diff de un archivo.
    Diff(String, String),
    /// Sesión del agente creada.
    Session(i64),
    /// Orden de la terminal lanzada.
    TermStarted(u64, String),
    /// Estado de los servidores de lenguaje.
    Lsp(Vec<(String, String)>),
    /// Definición: (ruta, línea, carácter) desde 0.
    Definition(Vec<(String, u32, u32)>),
    /// Información del LSP.
    Hover(String),
    /// Evento del demonio (Null = conexión perdida).
    Event(Value),
    /// Aviso (texto, es error).
    Toast(String, bool),
}

/// Un archivo con cambios en git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitEntry {
    /// Ruta relativa.
    pub path: String,
    /// Estado en el índice.
    pub index: String,
    /// Estado en el árbol de trabajo.
    pub worktree: String,
}

/// Vista de la barra lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Explorador.
    Explorer,
    /// Buscar en el proyecto.
    Search,
    /// Control de código fuente.
    Git,
    /// Ajustes.
    Settings,
}

/// Pestaña del panel inferior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bottom {
    /// Problemas.
    Problems,
    /// Salida (diffs, información).
    Output,
    /// Terminal.
    Terminal,
}

/// Órdenes de la paleta y de los menús.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Ir a archivo.
    QuickOpen,
    /// Guardar.
    Save,
    /// Guardar todo.
    SaveAll,
    /// Cerrar editor.
    CloseEditor,
    /// Buscar en el archivo.
    Find,
    /// Reemplazar en el archivo.
    Replace,
    /// Ir a la línea.
    GoToLine,
    /// Ir a la definición.
    GoToDefinition,
    /// Información del símbolo.
    ShowHover,
    /// Mostrar u ocultar la barra lateral.
    ToggleSidebar,
    /// Mostrar u ocultar el panel.
    TogglePanel,
    /// Mostrar u ocultar el agente.
    ToggleAgent,
    /// Terminal.
    Terminal,
    /// Explorador.
    Explorer,
    /// Buscar en el proyecto.
    Search,
    /// Git.
    Git,
    /// Ajustes.
    Settings,
    /// Problemas.
    Problems,
    /// Tema.
    ToggleTheme,
    /// Letra más grande.
    ZoomIn,
    /// Letra más pequeña.
    ZoomOut,
    /// Nueva conversación.
    NewChat,
    /// Deshacer el turno del agente.
    UndoTurn,
    /// Salir.
    Quit,
}

impl Command {
    /// Todas, en el orden de la paleta.
    pub const ALL: [Command; 24] = [
        Command::QuickOpen,
        Command::Save,
        Command::SaveAll,
        Command::CloseEditor,
        Command::Find,
        Command::Replace,
        Command::GoToLine,
        Command::GoToDefinition,
        Command::ShowHover,
        Command::ToggleSidebar,
        Command::TogglePanel,
        Command::ToggleAgent,
        Command::Terminal,
        Command::Explorer,
        Command::Search,
        Command::Git,
        Command::Settings,
        Command::Problems,
        Command::ToggleTheme,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::NewChat,
        Command::UndoTurn,
        Command::Quit,
    ];

    /// Texto y atajo.
    pub fn label(self, t: &Texts) -> (String, &'static str) {
        let (a, b): (&str, &str) = match self {
            Command::QuickOpen => (t.quick_open, "Ctrl+P"),
            Command::Save => (t.save, "Ctrl+S"),
            Command::SaveAll => (t.save_all, "Ctrl+Alt+S"),
            Command::CloseEditor => (t.close_editor, "Ctrl+W"),
            Command::Find => (t.find, "Ctrl+F"),
            Command::Replace => (t.replace, "Ctrl+H"),
            Command::GoToLine => (t.go_to_line, "Ctrl+G"),
            Command::GoToDefinition => (t.go_to_definition, "F12"),
            Command::ShowHover => (t.show_hover, "Ctrl+K"),
            Command::ToggleSidebar => (t.toggle_sidebar, "Ctrl+B"),
            Command::TogglePanel => (t.toggle_panel, "Ctrl+J"),
            Command::ToggleAgent => (t.toggle_agent, "Ctrl+Alt+B"),
            Command::Terminal => (t.terminal, "Ctrl+`"),
            Command::Explorer => (t.explorer, "Ctrl+Mayús+E"),
            Command::Search => (t.search, "Ctrl+Mayús+F"),
            Command::Git => (t.git, "Ctrl+Mayús+G"),
            Command::Settings => (t.settings, "Ctrl+,"),
            Command::Problems => (t.problems, "Ctrl+Mayús+M"),
            Command::ToggleTheme => (t.theme_toggle, ""),
            Command::ZoomIn => (t.zoom_in, "Ctrl+="),
            Command::ZoomOut => (t.zoom_out, "Ctrl+-"),
            Command::NewChat => (t.new_chat, "Ctrl+I"),
            Command::UndoTurn => (t.undo_turn, "Ctrl+Alt+Z"),
            Command::Quit => (t.quit, "Ctrl+Q"),
        };
        (a.to_string(), b)
    }
}

/// Índices de `labels` que coinciden con `query` (difuso), de mejor a peor.
pub fn palette_matches(query: &str, labels: &[String]) -> Vec<usize> {
    let mut v: Vec<(i32, usize)> = labels
        .iter()
        .enumerate()
        .filter_map(|(i, l)| fuzzy_score(query, l).map(|s| (s, i)))
        .collect();
    if query.is_empty() {
        return (0..labels.len()).collect();
    }
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, i)| i).collect()
}

/// Agrupa resultados de búsqueda por archivo, en orden de aparición.
pub fn group_hits(hits: &[(String, u32, String)]) -> Vec<(String, Vec<(u32, String)>)> {
    let mut out: Vec<(String, Vec<(u32, String)>)> = vec![];
    for (p, l, t) in hits {
        match out.iter_mut().find(|(q, _)| q == p) {
            Some((_, v)) => v.push((*l, t.clone())),
            None => out.push((p.clone(), vec![(*l, t.clone())])),
        }
    }
    out
}

/// Migas de pan de una ruta: `crates › pcode-ide › src › app.rs`.
pub fn breadcrumbs(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Un mensaje del chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMsg {
    /// Del usuario (si no, del agente o del sistema).
    pub user: bool,
    /// Texto.
    pub text: String,
}

/// Una aprobación pendiente.
#[derive(Debug, Clone)]
struct Approval {
    id: u64,
    action: String,
    reason: String,
}

/// Un aviso flotante.
struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

/// Selector (Ctrl+P o paleta).
#[derive(Default)]
struct Picker {
    query: String,
    selected: usize,
}

/// Fuentes del sistema que traen los símbolos que usa la interfaz (▸ ● ⎇ ✗ ⚠ ✎ ↶).
pub const FALLBACK_FONTS: &[(&str, &str)] = &[
    ("dejavu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    (
        "dejavu-mono",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ),
    ("segoe-symbol", "C:\\Windows\\Fonts\\seguisym.ttf"),
];

/// Fuentes: Ubuntu/Noto de egui y, como respaldo, las de [`FALLBACK_FONTS`] que existan.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, path) in FALLBACK_FONTS {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts.font_data.insert(
            (*name).to_string(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push((*name).to_string());
        }
    }
    ctx.set_fonts(fonts);
}

/// Salto pendiente al abrir un archivo: ruta, línea (desde 1) y posición LSP (línea, columna).
type PendingGoto = (String, Option<u32>, Option<(u32, u32)>);

/// El IDE.
pub struct Ide {
    client: Arc<Client>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    t: &'static Texts,
    settings: IdeSettings,
    project: String,
    tabs: Tabs,
    tree: Tree,
    files: Vec<String>,
    branch: String,
    git: Vec<GitEntry>,
    commit_msg: String,
    side: Side,
    show_side: bool,
    show_panel: bool,
    bottom: Bottom,
    quick: Option<Picker>,
    palette: Option<Picker>,
    search_query: String,
    search_case: bool,
    hits: Vec<(String, u32, String)>,
    searching: bool,
    session: Option<i64>,
    chat: Vec<ChatMsg>,
    busy: bool,
    prompt: String,
    focus_prompt: bool,
    approvals: Vec<Approval>,
    stream: Stream,
    cards: Vec<TurnCard>,
    problems: Problems,
    term: Terminal,
    term_cmd: String,
    output: (String, String),
    find: Option<(String, String, bool, bool)>,
    goto: Option<String>,
    pending_cursor: Option<usize>,
    pending_goto: Option<PendingGoto>,
    cursor: (u32, u32),
    cursor_char: usize,
    hover: Option<String>,
    toasts: Vec<Toast>,
    lsp: Vec<(String, String)>,
    connected: bool,
    quit_armed: bool,
    recent_files: Vec<String>,
    last_sync: Option<(String, Instant)>,
}

impl Ide {
    /// Crea el IDE y pide lo primero que se ve.
    pub fn new(client: Client, ctx: &egui::Context, t: &'static Texts) -> Ide {
        let (tx, rx) = mpsc::channel();
        let client = Arc::new(client);
        let settings = IdeSettings::load();
        install_fonts(ctx);
        theme::apply(ctx, settings.dark, settings.font_size);
        let ide = Ide {
            client: client.clone(),
            tx: tx.clone(),
            rx,
            t,
            show_side: true,
            show_panel: false,
            side: Side::Explorer,
            bottom: Bottom::Terminal,
            settings,
            project: String::new(),
            tabs: Tabs::default(),
            tree: Tree::default(),
            files: vec![],
            branch: String::new(),
            git: vec![],
            commit_msg: String::new(),
            quick: None,
            palette: None,
            search_query: String::new(),
            search_case: false,
            hits: vec![],
            searching: false,
            session: None,
            chat: vec![],
            busy: false,
            prompt: String::new(),
            focus_prompt: false,
            approvals: vec![],
            stream: Stream::default(),
            cards: vec![],
            problems: Problems::default(),
            term: Terminal::default(),
            term_cmd: String::new(),
            output: (String::new(), String::new()),
            find: None,
            goto: None,
            pending_cursor: None,
            pending_goto: None,
            cursor: (0, 0),
            cursor_char: 0,
            hover: None,
            toasts: vec![],
            lsp: vec![],
            connected: true,
            quit_armed: false,
            recent_files: vec![],
            last_sync: None,
        };
        ide.list_dir("");
        ide.spawn(|c| {
            let repo = c
                .call("server.info", json!({}))
                .ok()
                .and_then(|v| v["repo"].as_str().map(String::from))
                .unwrap_or_default();
            Msg::Event(json!({"tipo": "__repo", "repo": repo}))
        });
        ide.spawn(|c| match c.call("fs.files", json!({})) {
            Ok(v) => Msg::Files(
                v.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
            ),
            Err(e) => Msg::Toast(e, true),
        });
        ide.refresh_git();
        let (c2, tx2, ctx2) = (client, tx, ctx.clone());
        std::thread::spawn(move || {
            crate::events::listen(&c2, |ev| {
                let ok = tx2.send(Msg::Event(ev)).is_ok();
                ctx2.request_repaint();
                ok
            })
        });
        ide
    }

    fn pal(&self) -> &'static Palette {
        theme::palette(self.settings.dark)
    }

    fn spawn(&self, f: impl FnOnce(&Client) -> Msg + Send + 'static) {
        let (c, tx) = (self.client.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(f(&c));
        });
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        self.toasts.push(Toast {
            text,
            error,
            until: Instant::now() + Duration::from_secs(if error { 8 } else { 3 }),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn list_dir(&self, dir: &str) {
        let d = dir.to_string();
        self.spawn(move |c| match c.call("fs.list", json!({"path": d})) {
            Ok(v) => Msg::Dir(d, serde_json::from_value(v).unwrap_or_default()),
            Err(e) => Msg::Toast(e, true),
        });
    }

    fn open_file(&mut self, path: &str) {
        if self.tabs.list.iter().any(|t| t.path == path) {
            self.tabs.open(path, String::new());
            return;
        }
        let p = path.to_string();
        self.spawn(move |c| match c.call("fs.read", json!({"path": p})) {
            Ok(v) => Msg::File(
                p,
                v["content"].as_str().unwrap_or_default().to_string(),
                v["binary"].as_bool().unwrap_or(false),
            ),
            Err(e) => Msg::Toast(e, true),
        });
    }

    /// Abre `path` y coloca el cursor en una línea (1 = primera) o una posición LSP.
    fn open_at(&mut self, path: &str, line: Option<u32>, lsp: Option<(u32, u32)>) {
        if let Some(i) = self.tabs.list.iter().position(|t| t.path == path) {
            self.tabs.active = i;
            self.apply_goto(line, lsp);
        } else {
            self.pending_goto = Some((path.to_string(), line, lsp));
            self.open_file(path);
        }
    }

    fn apply_goto(&mut self, line: Option<u32>, lsp: Option<(u32, u32)>) {
        let Some(tab) = self.tabs.current() else {
            return;
        };
        self.pending_cursor = match (line, lsp) {
            (_, Some((l, c))) => Some(from_lsp(&tab.text, l, c)),
            (Some(l), None) => {
                let b = line_to_byte(&tab.text, l as usize);
                Some(tab.text[..b].chars().count())
            }
            _ => None,
        };
    }

    fn refresh_git(&self) {
        self.spawn(|c| match c.call("git.status", json!({})) {
            Ok(v) => Msg::Git(
                v["branch"].as_str().unwrap_or_default().to_string(),
                v["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| GitEntry {
                        path: f["path"].as_str().unwrap_or_default().to_string(),
                        index: f["index"].as_str().unwrap_or(" ").to_string(),
                        worktree: f["worktree"].as_str().unwrap_or(" ").to_string(),
                    })
                    .collect(),
            ),
            Err(_) => Msg::Git(String::new(), vec![]),
        });
    }

    fn save_tab(&self, i: usize) {
        if let Some(t) = self.tabs.list.get(i) {
            let (p, text) = (t.path.clone(), t.text.clone());
            self.spawn(
                move |c| match c.call("fs.write", json!({"path": p, "content": text})) {
                    Ok(_) => {
                        let _ = c.call("lsp.saved", json!({"path": p, "content": text}));
                        Msg::Saved(p, text)
                    }
                    Err(e) => Msg::Toast(e, true),
                },
            );
        }
    }

    fn search(&mut self) {
        if self.search_query.trim().is_empty() {
            return;
        }
        self.searching = true;
        let (q, case) = (self.search_query.clone(), self.search_case);
        self.spawn(move |c| {
            match c.call("search.text", json!({"query": q, "caseSensitive": case})) {
                Ok(v) => Msg::Hits(
                    v.as_array()
                        .into_iter()
                        .flatten()
                        .take(2000)
                        .map(|h| {
                            (
                                h["path"].as_str().unwrap_or_default().to_string(),
                                h["line"].as_u64().unwrap_or(1) as u32,
                                h["text"].as_str().unwrap_or_default().trim().to_string(),
                            )
                        })
                        .collect(),
                ),
                Err(e) => Msg::Toast(e, true),
            }
        });
    }

    fn git_action(&self, method: &'static str, paths: Vec<String>) {
        self.spawn(move |c| match c.call(method, json!({"paths": paths})) {
            Ok(_) => Msg::Event(json!({"tipo": "filesChanged"})),
            Err(e) => Msg::Toast(e, true),
        });
    }

    fn commit(&mut self) {
        let msg = self.commit_msg.trim().to_string();
        if msg.is_empty() {
            return;
        }
        self.commit_msg.clear();
        self.spawn(
            move |c| match c.call("git.commit", json!({"message": msg})) {
                Ok(_) => Msg::Toast(format!("✓ {msg}"), false),
                Err(e) => Msg::Toast(e, true),
            },
        );
        self.refresh_git();
    }

    fn show_diff(&self, path: &str, staged: bool) {
        let p = path.to_string();
        self.spawn(
            move |c| match c.call("git.diff", json!({"path": p, "staged": staged})) {
                Ok(v) => Msg::Diff(
                    p,
                    v.as_str()
                        .map(String::from)
                        .unwrap_or_else(|| v.to_string()),
                ),
                Err(e) => Msg::Toast(e, true),
            },
        );
    }

    fn send_prompt(&mut self) {
        let text = self.prompt.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.prompt.clear();
        self.chat.push(ChatMsg {
            user: true,
            text: text.clone(),
        });
        self.busy = true;
        match self.session {
            Some(s) => self.spawn(move |c| {
                match c.call("session.send", json!({"session": s, "text": text})) {
                    Ok(_) => Msg::Toast(String::new(), false),
                    Err(e) => Msg::Toast(e, true),
                }
            }),
            None => self.spawn(
                move |c| match c.call("session.create", json!({"task": text})) {
                    Ok(v) => Msg::Session(v["id"].as_i64().unwrap_or(-1)),
                    Err(e) => Msg::Toast(e, true),
                },
            ),
        }
    }

    fn run_terminal(&mut self) {
        let cmd = std::mem::take(&mut self.term_cmd);
        if cmd.trim().is_empty() {
            return;
        }
        self.spawn(
            move |c| match c.call("terminal.run", json!({"command": cmd})) {
                Ok(v) => Msg::TermStarted(v["id"].as_u64().or(v.as_u64()).unwrap_or(0), cmd),
                Err(e) => Msg::Toast(e, true),
            },
        );
    }

    fn lsp_request(&mut self, definition: bool) {
        let Some(tab) = self.tabs.current() else {
            return;
        };
        let (line, character) = to_lsp(&tab.text, self.cursor_char);
        let (p, text) = (tab.path.clone(), tab.text.clone());
        let t = self.t;
        self.spawn(move |c| {
            let _ = c.call("lsp.sync", json!({"path": p, "content": text}));
            let args = json!({"path": p, "line": line, "character": character});
            if definition {
                match c.call("lsp.definition", args) {
                    Ok(v) => Msg::Definition(
                        v.as_array()
                            .into_iter()
                            .flatten()
                            .map(|l| {
                                (
                                    l["path"].as_str().unwrap_or_default().to_string(),
                                    l["line"].as_u64().unwrap_or(0) as u32,
                                    l["character"].as_u64().unwrap_or(0) as u32,
                                )
                            })
                            .collect(),
                    ),
                    Err(e) => Msg::Toast(e, true),
                }
            } else {
                match c.call("lsp.hover", args) {
                    Ok(v) => Msg::Hover(v.as_str().unwrap_or(t.hover_none).to_string()),
                    Err(e) => Msg::Toast(e, true),
                }
            }
        });
    }

    fn fire(&self, method: &'static str) {
        self.spawn(move |c| match c.call(method, json!({})) {
            Ok(_) => Msg::Event(json!({"tipo": "filesChanged"})),
            Err(e) => Msg::Toast(e, true),
        });
    }

    fn set_theme(&mut self, ctx: &egui::Context) {
        theme::apply(ctx, self.settings.dark, self.settings.font_size);
        self.settings.save();
    }

    fn run(&mut self, cmd: Command, ctx: &egui::Context) {
        match cmd {
            Command::QuickOpen => self.quick = Some(Picker::default()),
            Command::Save => self.save_tab(self.tabs.active),
            Command::SaveAll => {
                for i in 0..self.tabs.list.len() {
                    if self.tabs.list[i].dirty() {
                        self.save_tab(i);
                    }
                }
            }
            Command::CloseEditor => {
                let i = self.tabs.active;
                self.tabs.close(i);
            }
            Command::Find => self.find = Some((String::new(), String::new(), false, false)),
            Command::Replace => self.find = Some((String::new(), String::new(), false, true)),
            Command::GoToLine => self.goto = Some(String::new()),
            Command::GoToDefinition => self.lsp_request(true),
            Command::ShowHover => self.lsp_request(false),
            Command::ToggleSidebar => self.show_side = !self.show_side,
            Command::TogglePanel => self.show_panel = !self.show_panel,
            Command::ToggleAgent => {
                self.settings.show_agent = !self.settings.show_agent;
                self.settings.save();
            }
            Command::Terminal => {
                self.show_panel = true;
                self.bottom = Bottom::Terminal;
            }
            Command::Explorer | Command::Search | Command::Git | Command::Settings => {
                self.side = match cmd {
                    Command::Explorer => Side::Explorer,
                    Command::Search => Side::Search,
                    Command::Git => Side::Git,
                    _ => Side::Settings,
                };
                self.show_side = true;
                if self.side == Side::Settings {
                    self.spawn(|c| match c.call("lsp.status", json!({})) {
                        Ok(v) => Msg::Lsp(serde_json::from_value(v).unwrap_or_default()),
                        Err(e) => Msg::Toast(e, true),
                    });
                }
            }
            Command::Problems => {
                self.show_panel = true;
                self.bottom = Bottom::Problems;
            }
            Command::ToggleTheme => {
                self.settings.dark = !self.settings.dark;
                self.set_theme(ctx);
            }
            Command::ZoomIn | Command::ZoomOut => {
                let d = if cmd == Command::ZoomIn { 1.0 } else { -1.0 };
                self.settings.font_size = (self.settings.font_size + d).clamp(8.0, 32.0);
                self.set_theme(ctx);
            }
            Command::NewChat => {
                self.settings.show_agent = true;
                if !self.busy {
                    self.session = None;
                    self.chat.clear();
                    self.cards.clear();
                }
                self.focus_prompt = true;
            }
            Command::UndoTurn => self.fire("checkpoint.undo"),
            Command::Quit => {
                if self.tabs.any_dirty() && !self.quit_armed {
                    self.quit_armed = true;
                    self.toast(self.t.quit_unsaved, true);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn on_event(&mut self, ev: Value) {
        if ev.is_null() {
            self.connected = false;
            return;
        }
        self.connected = true;
        if self.problems.apply(&ev) || self.term.apply(&ev) {
            return;
        }
        if self.stream.apply(&ev) && ev["tipo"] == "assistantDelta" {
            return;
        }
        if let Some(c) = TurnCard::from_event(&ev) {
            if !c.changes.is_empty() {
                self.cards.push(c);
            }
            return;
        }
        let mine = ev["session"].as_i64().is_some() && ev["session"].as_i64() == self.session;
        match ev["tipo"].as_str() {
            Some("__repo") => {
                let r = ev["repo"].as_str().unwrap_or_default();
                self.project = r
                    .rsplit(['/', '\\'])
                    .find(|s| !s.is_empty())
                    .unwrap_or(r)
                    .to_string();
            }
            Some("filesChanged") => {
                for d in self.tree.invalidate() {
                    self.list_dir(&d);
                }
                self.refresh_git();
            }
            Some("transcript") if mine => {
                let e = &ev["entry"];
                if e["tipo"] != "user"
                    && let Some(t) = e["text"].as_str()
                    && !t.trim().is_empty()
                {
                    self.chat.push(ChatMsg {
                        user: false,
                        text: t.to_string(),
                    });
                }
            }
            Some("sessionUpdated") if ev["session"]["id"].as_i64() == self.session => {
                let st = ev["session"]["state"].as_str().unwrap_or_default();
                self.busy = matches!(st, "running" | "waitingApproval");
            }
            Some("turnFinished") if mine => self.busy = false,
            Some("approvalRequested") => {
                let a = &ev["approval"];
                self.approvals.push(Approval {
                    id: a["id"].as_u64().unwrap_or(0),
                    action: a["action"].as_str().unwrap_or_default().to_string(),
                    reason: a["reason"].as_str().unwrap_or_default().to_string(),
                });
                self.settings.show_agent = true;
            }
            Some("approvalResolved") => {
                let id = ev["id"].as_u64();
                self.approvals.retain(|a| Some(a.id) != id);
            }
            _ => {}
        }
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Dir(d, e) => self.tree.set(&d, e),
            Msg::File(p, text, bin) => {
                if bin {
                    self.toast(self.t.binary, true);
                    return;
                }
                self.tabs.open(&p, text.clone());
                self.recent_files.retain(|x| x != &p);
                self.recent_files.insert(0, p.clone());
                self.recent_files.truncate(8);
                if let Some((gp, line, lsp)) = self.pending_goto.take() {
                    if gp == p {
                        self.apply_goto(line, lsp);
                    } else {
                        self.pending_goto = Some((gp, line, lsp));
                    }
                }
                self.spawn(move |c| {
                    let _ = c.call("lsp.sync", json!({"path": p, "content": text}));
                    Msg::Toast(String::new(), false)
                });
            }
            Msg::Files(f) => self.files = f,
            Msg::Hits(h) => {
                self.hits = h;
                self.searching = false;
            }
            Msg::Git(b, f) => (self.branch, self.git) = (b, f),
            Msg::Saved(p, text) => {
                self.tabs.mark_saved(&p, &text);
                self.refresh_git();
            }
            Msg::Diff(p, d) => {
                self.output = (p, d);
                self.show_panel = true;
                self.bottom = Bottom::Output;
            }
            Msg::Session(id) => self.session = Some(id),
            Msg::TermStarted(id, cmd) => self.term.start(id, &cmd),
            Msg::Lsp(l) => self.lsp = l,
            Msg::Definition(v) => match v.first() {
                Some((p, l, c)) => {
                    let p = p.clone();
                    self.open_at(&p, None, Some((*l, *c)));
                }
                None => self.toast(self.t.definition_none, false),
            },
            Msg::Hover(h) => self.hover = Some(h),
            Msg::Event(ev) => self.on_event(ev),
            Msg::Toast(t, e) => self.toast(t, e),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        let ca = Modifiers::COMMAND | Modifiers::ALT;
        let press = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        let mut cmds = vec![];
        // Primero las que llevan Mayús o Alt (Ctrl+Mayús+P también «es» Ctrl+P para egui).
        for (m, k, c) in [
            (cs, Key::P, Command::QuickOpen),
            (cs, Key::E, Command::Explorer),
            (cs, Key::F, Command::Search),
            (cs, Key::G, Command::Git),
            (cs, Key::M, Command::Problems),
            (ca, Key::B, Command::ToggleAgent),
            (ca, Key::S, Command::SaveAll),
            (ca, Key::Z, Command::UndoTurn),
        ] {
            if press(m, k) {
                cmds.push(if k == Key::P { None } else { Some(c) });
            }
        }
        for (k, c) in [
            (Key::P, Command::QuickOpen),
            (Key::S, Command::Save),
            (Key::W, Command::CloseEditor),
            (Key::F, Command::Find),
            (Key::H, Command::Replace),
            (Key::G, Command::GoToLine),
            (Key::K, Command::ShowHover),
            (Key::B, Command::ToggleSidebar),
            (Key::J, Command::TogglePanel),
            (Key::Backtick, Command::Terminal),
            (Key::Comma, Command::Settings),
            (Key::I, Command::NewChat),
            (Key::Plus, Command::ZoomIn),
            (Key::Equals, Command::ZoomIn),
            (Key::Minus, Command::ZoomOut),
            (Key::Q, Command::Quit),
        ] {
            if press(Modifiers::COMMAND, k) {
                cmds.push(Some(c));
            }
        }
        if press(Modifiers::NONE, Key::F12) {
            cmds.push(Some(Command::GoToDefinition));
        }
        if press(Modifiers::COMMAND, Key::Tab) && !self.tabs.list.is_empty() {
            self.tabs.active = (self.tabs.active + 1) % self.tabs.list.len();
        }
        for c in cmds {
            match c {
                None => self.palette = Some(Picker::default()),
                Some(c) => self.run(c, ctx),
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.hover = None;
        }
    }

    // ───────────────────────────── zonas ─────────────────────────────

    fn ui_title(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let mut cmd = None;
        // El rectángulo entero de la barra: el menú se queda con el ancho libre, así que el centro
        // y los conmutadores se colocan en posiciones absolutas con `ui.put`.
        let full = ui.max_rect();
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new(">_").strong().color(p.accent).size(15.0));
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(t.menu_file, |ui| {
                    for c in [
                        Command::QuickOpen,
                        Command::Save,
                        Command::SaveAll,
                        Command::CloseEditor,
                        Command::Quit,
                    ] {
                        menu_item(ui, t, c, &mut cmd);
                    }
                    ui.separator();
                    ui.label(RichText::new(t.open_folder_hint).small().color(p.fg_dim));
                });
                ui.menu_button(t.menu_edit, |ui| {
                    for c in [Command::Find, Command::Replace, Command::UndoTurn] {
                        menu_item(ui, t, c, &mut cmd);
                    }
                });
                ui.menu_button(t.menu_view, |ui| {
                    for c in [
                        Command::Explorer,
                        Command::Search,
                        Command::Git,
                        Command::Problems,
                        Command::ToggleSidebar,
                        Command::TogglePanel,
                        Command::ToggleAgent,
                        Command::ToggleTheme,
                        Command::ZoomIn,
                        Command::ZoomOut,
                    ] {
                        menu_item(ui, t, c, &mut cmd);
                    }
                    if ui.button(t.command_palette).clicked() {
                        self.palette = Some(Picker::default());
                        ui.close();
                    }
                });
                ui.menu_button(t.menu_go, |ui| {
                    for c in [
                        Command::QuickOpen,
                        Command::GoToLine,
                        Command::GoToDefinition,
                        Command::ShowHover,
                    ] {
                        menu_item(ui, t, c, &mut cmd);
                    }
                });
                ui.menu_button(t.menu_terminal, |ui| {
                    menu_item(ui, t, Command::Terminal, &mut cmd)
                });
                ui.menu_button(t.menu_help, |ui| {
                    ui.label(RichText::new(t.about).strong());
                    ui.label(format!(
                        "pcode-ide {} · {}",
                        env!("CARGO_PKG_VERSION"),
                        t.about_text
                    ));
                    ui.separator();
                    ui.label(RichText::new(t.shortcuts).small().color(p.fg_dim));
                });
            });
            // «Command center» en el centro, como en VS Code.
            let label = if self.project.is_empty() {
                "PublicCode".to_string()
            } else {
                self.project.clone()
            };
            let (center, toggles) = title_rects(full);
            let b = egui::Button::new(RichText::new(format!("⚲  {label}")).color(p.fg_dim))
                .fill(p.input)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(6);
            if ui
                .put(center, b)
                .on_hover_text(format!("{} (Ctrl+P)", t.quick_open))
                .clicked()
            {
                self.quick = Some(Picker::default());
            }
            let items = [
                (
                    self.show_side,
                    "◧",
                    t.toggle_sidebar,
                    Command::ToggleSidebar,
                ),
                (self.show_panel, "⬓", t.toggle_panel, Command::TogglePanel),
                (
                    self.settings.show_agent,
                    "◨",
                    t.toggle_agent,
                    Command::ToggleAgent,
                ),
            ];
            for ((on, glyph, tip, c), rect) in items.into_iter().zip(toggles) {
                let color = if on { p.fg } else { p.fg_dim };
                let b =
                    egui::Button::new(RichText::new(glyph).color(color).size(15.0)).frame(false);
                if ui.put(rect, b).on_hover_text(tip).clicked() {
                    cmd = Some(c);
                }
            }
        });
        if let Some(c) = cmd {
            let ctx = ui.ctx().clone();
            self.run(c, &ctx);
        }
    }

    fn ui_activity(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        ui.add_space(4.0);
        let mut pick = None;
        for (side, glyph, tip) in [
            (Side::Explorer, "❐", t.explorer),
            (Side::Search, "⚲", t.search),
            (Side::Git, "⎇", t.git),
        ] {
            let active = self.show_side && self.side == side;
            let badge = (side == Side::Git && !self.git.is_empty()).then_some(self.git.len());
            if activity_button(ui, p, glyph, active, badge)
                .on_hover_text(tip)
                .clicked()
            {
                pick = Some(side);
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            ui.add_space(4.0);
            let active = self.show_side && self.side == Side::Settings;
            if activity_button(ui, p, "⚙", active, None)
                .on_hover_text(t.settings)
                .clicked()
            {
                pick = Some(Side::Settings);
            }
            let agent = self.settings.show_agent;
            if activity_button(ui, p, "✦", agent, None)
                .on_hover_text(t.agent)
                .clicked()
            {
                self.settings.show_agent = !agent;
                self.settings.save();
            }
        });
        if let Some(s) = pick {
            if self.show_side && self.side == s {
                self.show_side = false;
            } else {
                let ctx = ui.ctx().clone();
                self.run(
                    match s {
                        Side::Explorer => Command::Explorer,
                        Side::Search => Command::Search,
                        Side::Git => Command::Git,
                        Side::Settings => Command::Settings,
                    },
                    &ctx,
                );
            }
        }
    }

    fn ui_side(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let title = match self.side {
            Side::Explorer => self.t.explorer,
            Side::Search => self.t.search,
            Side::Git => self.t.git,
            Side::Settings => self.t.settings,
        };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new(title.to_uppercase())
                    .size(11.0)
                    .color(p.fg_dim),
            );
        });
        ui.add_space(4.0);
        match self.side {
            Side::Explorer => self.ui_explorer(ui),
            Side::Search => self.ui_search(ui),
            Side::Git => self.ui_git(ui),
            Side::Settings => self.ui_settings(ui),
        }
    }

    fn ui_explorer(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let name = if self.project.is_empty() {
            "PROYECTO".to_string()
        } else {
            self.project.to_uppercase()
        };
        section_header(ui, p, &format!("▾ {name}"));
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                self.tree_rows(ui, "", 0);
            });
    }

    fn tree_rows(&mut self, ui: &mut egui::Ui, dir: &str, depth: usize) {
        let p = self.pal();
        let entries = self.tree.children.get(dir).cloned().unwrap_or_default();
        let current = self.tabs.current().map(|t| t.path.clone());
        for e in entries {
            let indent = 8.0 + depth as f32 * 12.0;
            if e.dir {
                let open = self.tree.expanded.contains(&e.path);
                let dirty = self
                    .git
                    .iter()
                    .any(|g| g.path.starts_with(&format!("{}/", e.path)));
                let deco = dirty.then_some(("●", p.git_modified));
                let chevron = if open { "▾" } else { "▸" };
                let color = if dirty { p.git_modified } else { p.fg };
                if row(
                    ui,
                    p,
                    indent,
                    (chevron, p.fg_dim),
                    &e.name,
                    color,
                    false,
                    deco,
                )
                .clicked()
                    && self.tree.toggle(&e.path)
                {
                    self.list_dir(&e.path);
                }
                if open {
                    self.tree_rows(ui, &e.path, depth + 1);
                }
            } else {
                let g = self.git.iter().find(|g| g.path == e.path);
                let deco = g.and_then(|g| git_decoration(&g.index, &g.worktree, p));
                let color = deco.map_or(p.fg, |d| d.1);
                let selected = current.as_deref() == Some(e.path.as_str());
                if row(
                    ui,
                    p,
                    indent + 12.0,
                    file_icon(&e.name),
                    &e.name,
                    color,
                    selected,
                    deco,
                )
                .clicked()
                {
                    self.open_file(&e.path);
                }
            }
        }
    }

    fn ui_search(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(t.search)
                    .desired_width(ui.available_width() - 44.0),
            );
            if self.side == Side::Search && self.hits.is_empty() && self.search_query.is_empty() {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.search();
            }
            let c = if self.search_case { p.accent } else { p.fg_dim };
            if ui
                .add(egui::Button::new(RichText::new("Aa").color(c)).frame(false))
                .on_hover_text(t.match_case)
                .clicked()
            {
                self.search_case = !self.search_case;
                self.search();
            }
        });
        ui.add_space(4.0);
        if self.searching {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.spinner();
            });
        } else if !self.search_query.is_empty() && !self.hits.is_empty() {
            let groups = group_hits(&self.hits);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "{} {} · {} {}",
                        self.hits.len(),
                        t.results,
                        groups.len(),
                        t.in_files
                    ))
                    .small()
                    .color(p.fg_dim),
                );
            });
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    for (path, lines) in groups {
                        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                        let id = ui.make_persistent_id(("busqueda", &path));
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            id,
                            true,
                        )
                        .show_header(ui, |ui| {
                            let (g, c) = file_icon(&name);
                            ui.label(RichText::new(g).color(c));
                            ui.label(RichText::new(&name).color(p.fg));
                            ui.label(RichText::new(&path).small().color(p.fg_dim));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                pill(ui, p, &lines.len().to_string());
                            });
                        })
                        .body(|ui| {
                            for (l, text) in lines {
                                let shown: String = text.chars().take(120).collect();
                                if row(
                                    ui,
                                    p,
                                    20.0,
                                    ("", p.fg_dim),
                                    &format!("{l}  {shown}"),
                                    p.fg,
                                    false,
                                    None,
                                )
                                .clicked()
                                {
                                    self.open_at(&path, Some(l), None);
                                }
                            }
                        });
                    }
                });
        } else if !self.search_query.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(t.no_results).color(p.fg_dim));
            });
        }
    }

    fn ui_git(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.commit_msg)
                    .hint_text(t.commit_hint)
                    .desired_rows(2)
                    .desired_width(ui.available_width() - 8.0),
            );
        });
        let send = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let enabled =
                !self.commit_msg.trim().is_empty() && self.git.iter().any(|g| is_staged(&g.index));
            let b = egui::Button::new(RichText::new(t.commit).color(p.on_accent))
                .fill(p.accent)
                .min_size(vec2(ui.available_width() - 8.0, 26.0));
            if (ui.add_enabled(enabled, b).clicked() || (send && enabled)) && enabled {
                self.commit();
            }
        });
        ui.add_space(6.0);
        if self.git.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(t.no_changes).color(p.fg_dim));
            });
            return;
        }
        let staged: Vec<GitEntry> = self
            .git
            .iter()
            .filter(|g| is_staged(&g.index))
            .cloned()
            .collect();
        let changes: Vec<GitEntry> = self
            .git
            .iter()
            .filter(|g| g.worktree.trim() != "" || g.index == "?")
            .cloned()
            .collect();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (title, list, staged_list) in
                    [(t.staged, &staged, true), (t.changes, &changes, false)]
                {
                    if list.is_empty() {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(title).strong().size(12.0));
                        pill(ui, p, &list.len().to_string());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (g, tip, m) = if staged_list {
                                ("−", t.unstage, "git.unstage")
                            } else {
                                ("+", t.stage, "git.stage")
                            };
                            if ui.small_button(g).on_hover_text(tip).clicked() {
                                self.git_action(m, list.iter().map(|g| g.path.clone()).collect());
                            }
                        });
                    });
                    for g in list {
                        let name = g.path.rsplit('/').next().unwrap_or(&g.path).to_string();
                        let (idx, wt) = if staged_list {
                            (g.index.as_str(), " ")
                        } else {
                            (" ", g.worktree.as_str())
                        };
                        let (idx, wt) = if g.index == "?" {
                            ("?", "?")
                        } else {
                            (idx, wt)
                        };
                        let deco = git_decoration(idx, wt, p);
                        let resp = row(
                            ui,
                            p,
                            16.0,
                            file_icon(&name),
                            &name,
                            deco.map_or(p.fg, |d| d.1),
                            false,
                            deco,
                        );
                        if resp.clicked() {
                            self.show_diff(&g.path, staged_list);
                        }
                        if resp.double_clicked() {
                            self.open_file(&g.path);
                        }
                        resp.context_menu(|ui| {
                            let (label, m) = if staged_list {
                                (t.unstage, "git.unstage")
                            } else {
                                (t.stage, "git.stage")
                            };
                            if ui.button(label).clicked() {
                                self.git_action(m, vec![g.path.clone()]);
                                ui.close();
                            }
                        });
                    }
                    ui.add_space(6.0);
                }
            });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let ctx = ui.ctx().clone();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.label(RichText::new(t.theme).strong());
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.settings.dark, t.dark).clicked()
                        && !self.settings.dark
                    {
                        self.run(Command::ToggleTheme, &ctx);
                    }
                    if ui.selectable_label(!self.settings.dark, t.light).clicked()
                        && self.settings.dark
                    {
                        self.run(Command::ToggleTheme, &ctx);
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new(t.font_size).strong());
                let mut size = self.settings.font_size;
                if ui
                    .add(egui::Slider::new(&mut size, 8.0..=32.0).step_by(1.0))
                    .changed()
                {
                    self.settings.font_size = size;
                    self.set_theme(&ctx);
                }
                ui.add_space(10.0);
                ui.label(RichText::new(t.lsp_servers).strong());
                if self.lsp.is_empty() {
                    ui.label(RichText::new(t.lsp_none).color(p.fg_dim));
                }
                for (g, st) in &self.lsp {
                    let ok = st == "activo";
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(if ok { "●" } else { "○" }).color(if ok {
                            p.git_added
                        } else {
                            p.fg_dim
                        }));
                        ui.label(RichText::new(g).strong());
                        ui.label(RichText::new(st).color(p.fg_dim));
                    });
                }
                ui.add_space(10.0);
                if let Some(mb) = own_rss_mb() {
                    ui.label(
                        RichText::new(format!("pcode-ide · {mb:.0} {}", t.memory)).color(p.fg_dim),
                    );
                }
            });
    }

    fn ui_tabs(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let mut close = None;
        let mut activate = None;
        egui::ScrollArea::horizontal()
            .id_salt("pestanas")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, tab) in self.tabs.list.iter().enumerate() {
                        let active = i == self.tabs.active;
                        let fill = if active { p.editor } else { p.side };
                        let resp = egui::Frame::new()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (g, c) = file_icon(tab.title());
                                    ui.label(RichText::new(g).color(c));
                                    let color = if active { p.fg } else { p.fg_dim };
                                    let name = ui.add(
                                        egui::Label::new(RichText::new(tab.title()).color(color))
                                            .sense(Sense::click()),
                                    );
                                    if name.clicked() {
                                        activate = Some(i);
                                    }
                                    if name.middle_clicked() {
                                        close = Some(i);
                                    }
                                    let x = if tab.dirty() { "●" } else { "×" };
                                    if ui
                                        .add(
                                            egui::Button::new(RichText::new(x).color(p.fg_dim))
                                                .frame(false),
                                        )
                                        .clicked()
                                    {
                                        close = Some(i);
                                    }
                                });
                            })
                            .response;
                        if active {
                            let r = resp.rect;
                            ui.painter().line_segment(
                                [r.left_top(), r.right_top()],
                                Stroke::new(2.0, p.accent),
                            );
                        }
                        let r = resp.rect;
                        ui.painter().line_segment(
                            [r.right_top(), r.right_bottom()],
                            Stroke::new(1.0, p.border),
                        );
                    }
                });
            });
        if let Some(i) = activate {
            self.tabs.active = i;
        }
        if let Some(i) = close {
            self.tabs.close(i);
        }
    }

    fn ui_editor(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let dark = self.settings.dark;
        if self.tabs.list.is_empty() {
            self.ui_welcome(ui);
            return;
        }
        self.ui_tabs(ui);
        // Migas de pan.
        if let Some(tab) = self.tabs.current() {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let parts = breadcrumbs(&tab.path);
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        ui.label(RichText::new("›").color(p.fg_dim));
                    }
                    let last = i + 1 == parts.len();
                    if last {
                        let (g, c) = file_icon(part);
                        ui.label(RichText::new(g).color(c).small());
                    }
                    ui.label(
                        RichText::new(*part)
                            .color(if last { p.fg } else { p.fg_dim })
                            .small(),
                    );
                }
            });
        }
        // Barra de buscar/reemplazar e ir a línea (arriba a la derecha, como en VS Code).
        if self.find.is_some() || self.goto.is_some() {
            self.ui_find_bar(ui);
        }
        let size = self.settings.font_size;
        let pending = self.pending_cursor.take();
        let Some(tab) = self.tabs.current_mut() else {
            return;
        };
        let lang = tab.lang;
        let font = FontId::monospace(size);
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, w: f32| {
            let mut job = layout_job(buf.as_str(), lang, dark, font.clone());
            job.wrap.max_width = w;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let lines = tab.text.split('\n').count();
        let width = lines.to_string().len().max(3);
        let cur_line = self.cursor.0 as usize + 1;
        let numbers: String = (1..=lines).map(|i| format!("{i:>width$}\n")).collect();
        let mut new_cursor = None;
        egui::Frame::new().fill(p.editor).show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("editor")
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.add_space(8.0);
                        // Números de línea; el de la línea actual, más claro.
                        let mut job = egui::text::LayoutJob::default();
                        for (i, l) in numbers.lines().enumerate() {
                            let c = if i + 1 == cur_line {
                                p.fg
                            } else {
                                p.line_number
                            };
                            job.append(
                                &format!("{l}\n"),
                                0.0,
                                egui::TextFormat {
                                    font_id: FontId::monospace(size),
                                    color: c,
                                    ..Default::default()
                                },
                            );
                        }
                        ui.label(job);
                        ui.add_space(12.0);
                        let mut out = egui::TextEdit::multiline(&mut tab.text)
                            .font(FontId::monospace(size))
                            .frame(egui::Frame::NONE)
                            .background_color(p.editor)
                            .desired_width(f32::INFINITY)
                            .desired_rows(40)
                            .lock_focus(true)
                            .layouter(&mut layouter)
                            .show(ui);
                        if let Some(i) = pending {
                            let c = egui::text::CCursor::new(i);
                            out.state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(c)));
                            out.state.clone().store(ui.ctx(), out.response.id);
                            out.response.request_focus();
                            // Que la línea se vea.
                            let pos = out.galley.pos_from_cursor(c);
                            ui.scroll_to_rect(
                                pos.translate(out.galley_pos.to_vec2()).expand(60.0),
                                Some(Align::Center),
                            );
                        }
                        if let Some(r) = out.cursor_range {
                            new_cursor = Some(r.primary.index);
                        }
                    });
                });
        });
        if let Some(ci) = new_cursor {
            self.cursor_char = ci.into();
            if let Some(tab) = self.tabs.current() {
                self.cursor = to_lsp(&tab.text, self.cursor_char);
                // Sincroniza con el LSP como mucho una vez por segundo mientras se escribe.
                let due = match &self.last_sync {
                    Some((p, at)) => p != &tab.path || at.elapsed() > Duration::from_secs(1),
                    None => true,
                };
                if due && tab.dirty() {
                    let (pp, text) = (tab.path.clone(), tab.text.clone());
                    self.last_sync = Some((pp.clone(), Instant::now()));
                    self.spawn(move |c| {
                        let _ = c.call("lsp.sync", json!({"path": pp, "content": text}));
                        Msg::Toast(String::new(), false)
                    });
                }
            }
        }
        if let Some(h) = self.hover.clone() {
            let mut open = true;
            egui::Window::new(t.show_hover)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(420.0)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            chat_text(ui, p, &h);
                        });
                });
            if !open {
                self.hover = None;
            }
        }
    }

    fn ui_find_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let mut close_find = false;
        let mut replace_now = false;
        let mut goto_done: Option<Option<usize>> = None;
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.add_space(16.0);
            egui::Frame::new()
                .fill(p.side)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(4)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.vertical(|ui| {
                        if let Some((q, r, case, show_r)) = self.find.as_mut() {
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(q)
                                        .hint_text(t.find)
                                        .desired_width(220.0),
                                );
                                if q.is_empty() {
                                    resp.request_focus();
                                }
                                let c = if *case { p.accent } else { p.fg_dim };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Aa").color(c))
                                            .frame(false),
                                    )
                                    .on_hover_text(t.match_case)
                                    .clicked()
                                {
                                    *case = !*case;
                                }
                                let n = self
                                    .tabs
                                    .current()
                                    .map_or(0, |tab| find_all(&tab.text, q, *case).len());
                                ui.label(
                                    RichText::new(if q.is_empty() {
                                        String::new()
                                    } else if n == 0 {
                                        t.no_results.to_string()
                                    } else {
                                        format!("{n} {}", t.results)
                                    })
                                    .small()
                                    .color(p.fg_dim),
                                );
                                if ui.add(egui::Button::new("×").frame(false)).clicked() {
                                    close_find = true;
                                }
                            });
                            if *show_r {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(r)
                                            .hint_text(t.replace)
                                            .desired_width(220.0),
                                    );
                                    if ui.button(t.replace_all).clicked() {
                                        replace_now = true;
                                    }
                                });
                            }
                        }
                        if let Some(g) = self.goto.as_mut() {
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(g)
                                        .hint_text(t.go_to_line)
                                        .desired_width(220.0),
                                );
                                resp.request_focus();
                                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    goto_done = Some(g.trim().parse().ok());
                                }
                            });
                        }
                    });
                });
        });
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            close_find = true;
            goto_done = goto_done.or(Some(None));
        }
        if replace_now
            && let Some((q, r, case, _)) = self.find.clone()
            && let Some(tab) = self.tabs.current_mut()
        {
            let (text, n) = replace_all(&tab.text, &q, &r, case);
            tab.text = text;
            self.toast(format!("{n} ✓"), false);
        }
        if close_find {
            self.find = None;
        }
        if let Some(n) = goto_done {
            self.goto = None;
            if let Some(n) = n {
                self.apply_goto(Some(n as u32), None);
            }
        }
    }

    fn ui_welcome(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let ctx = ui.ctx().clone();
        egui::Frame::new().fill(p.editor).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.18).max(20.0));
                ui.label(RichText::new(">_").size(64.0).color(p.accent).strong());
                ui.label(RichText::new("PublicCode").size(30.0).color(p.fg));
                ui.label(RichText::new(t.welcome_sub).size(15.0).color(p.fg_dim));
                ui.add_space(28.0);
            });
            // Dos columnas de ancho fijo centradas, las dos alineadas a la izquierda.
            let margin = welcome_margin(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(WELCOME_COL);
                    ui.label(RichText::new(t.start).size(16.0).color(p.fg));
                    ui.add_space(6.0);
                    for c in [
                        Command::QuickOpen,
                        Command::NewChat,
                        Command::Terminal,
                        Command::Search,
                        Command::Git,
                    ] {
                        let (label, keys) = c.label(t);
                        let b =
                            egui::Button::new(RichText::new(label).color(p.accent)).frame(false);
                        if ui.add(b).on_hover_text(keys).clicked() {
                            self.run(c, &ctx);
                        }
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new(t.command_palette).color(p.accent))
                                .frame(false),
                        )
                        .clicked()
                    {
                        self.palette = Some(Picker::default());
                    }
                });
                ui.add_space(WELCOME_GAP);
                ui.vertical(|ui| {
                    ui.set_width(WELCOME_COL);
                    ui.label(RichText::new(t.recent).size(16.0).color(p.fg));
                    ui.add_space(6.0);
                    let recent: Vec<String> = if self.recent_files.is_empty() {
                        self.git.iter().map(|g| g.path.clone()).take(6).collect()
                    } else {
                        self.recent_files.clone()
                    };
                    for f in recent {
                        let name = f.rsplit('/').next().unwrap_or(&f).to_string();
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(&name).color(p.accent))
                                        .frame(false),
                                )
                                .clicked()
                            {
                                self.open_file(&f);
                            }
                            ui.label(RichText::new(&f).small().color(p.fg_dim));
                        });
                    }
                });
            });
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(t.help_title).size(14.0).color(p.fg));
                ui.add_space(4.0);
                for c in [
                    Command::QuickOpen,
                    Command::Save,
                    Command::ToggleSidebar,
                    Command::TogglePanel,
                    Command::NewChat,
                    Command::GoToDefinition,
                ] {
                    let (label, keys) = c.label(t);
                    ui.horizontal(|ui| {
                        let w = ui.available_width();
                        ui.add_space((w / 2.0 - 160.0).max(0.0));
                        ui.add_sized(
                            [180.0, 20.0],
                            egui::Label::new(RichText::new(label).color(p.fg_dim)),
                        );
                        keycap(ui, p, keys);
                    });
                }
            });
        });
    }

    fn ui_panel(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let (e, w) = self.problems.counts();
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            for (b, label) in [
                (
                    Bottom::Problems,
                    format!(
                        "{}{}",
                        t.problems.to_uppercase(),
                        if e + w > 0 {
                            format!(" {}", e + w)
                        } else {
                            String::new()
                        }
                    ),
                ),
                (Bottom::Output, t.output.to_uppercase()),
                (Bottom::Terminal, t.terminal.to_uppercase()),
            ] {
                let active = self.bottom == b;
                let r = ui.add(
                    egui::Label::new(RichText::new(label).size(11.0).color(if active {
                        p.fg
                    } else {
                        p.fg_dim
                    }))
                    .sense(Sense::click()),
                );
                if active {
                    let rr = r.rect;
                    ui.painter().line_segment(
                        [
                            rr.left_bottom() + vec2(0.0, 3.0),
                            rr.right_bottom() + vec2(0.0, 3.0),
                        ],
                        Stroke::new(1.0, p.accent),
                    );
                }
                if r.clicked() {
                    self.bottom = b;
                }
                ui.add_space(14.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(RichText::new("×").color(p.fg_dim)).frame(false))
                    .clicked()
                {
                    self.show_panel = false;
                }
                if self.bottom == Bottom::Terminal
                    && let Some(id) = self.term.running
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!("■ {}", t.stop)).color(p.error),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    self.spawn(move |c| match c.call("terminal.kill", json!({"id": id})) {
                        Ok(_) => Msg::Toast(String::new(), false),
                        Err(e) => Msg::Toast(e, true),
                    });
                }
            });
        });
        ui.add_space(4.0);
        match self.bottom {
            Bottom::Problems => {
                let list = self.problems.sorted();
                if list.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(t.no_problems).color(p.fg_dim));
                    });
                }
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for pr in list {
                            let (g, c) = if pr.severity == 1 {
                                ("✗", p.error)
                            } else {
                                ("⚠", p.warning)
                            };
                            let text = format!("{}  {}:{}", pr.message, pr.path, pr.line + 1);
                            if row(ui, p, 12.0, (g, c), &text, p.fg, false, None).clicked() {
                                self.open_at(&pr.path, None, Some((pr.line, pr.character)));
                            }
                        }
                    });
            }
            Bottom::Output => {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        if !self.output.0.is_empty() {
                            ui.label(RichText::new(&self.output.0).strong());
                        }
                        for (k, line) in diff_lines(&self.output.1) {
                            let c = match k {
                                DiffLine::Added => p.git_added,
                                DiffLine::Removed => p.git_deleted,
                                DiffLine::Hunk => p.accent,
                                DiffLine::Header => p.fg_dim,
                                DiffLine::Context => p.fg,
                            };
                            ui.label(RichText::new(line).monospace().color(c));
                        }
                    });
            }
            Bottom::Terminal => {
                let h = ui.available_height() - 30.0;
                egui::ScrollArea::vertical()
                    .max_height(h.max(40.0))
                    .stick_to_bottom(true)
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(self.term.text()).monospace()).wrap(),
                        );
                    });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❯").color(p.git_added).monospace());
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.term_cmd)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(t.terminal_hint)
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY),
                    );
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.run_terminal();
                        r.request_focus();
                    }
                });
            }
        }
    }

    fn ui_agent(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("✦ {}", t.agent.to_uppercase()))
                    .size(11.0)
                    .color(p.fg_dim),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                if ui
                    .add(egui::Button::new(RichText::new("✎").color(p.fg_dim)).frame(false))
                    .on_hover_text(t.new_chat)
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.run(Command::NewChat, &ctx);
                }
            });
        });
        ui.separator();
        // Caja de texto abajo.
        let input_h = 96.0;
        let avail = ui.available_height() - input_h;
        egui::ScrollArea::vertical()
            .max_height(avail.max(80.0))
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_space(4.0);
                if self.chat.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("✦").size(32.0).color(p.accent));
                        ui.add_space(6.0);
                        ui.label(RichText::new(t.agent_empty).color(p.fg_dim));
                    });
                }
                for m in &self.chat {
                    if m.user {
                        ui.with_layout(Layout::top_down(Align::Max), |ui| {
                            egui::Frame::new()
                                .fill(p.bubble_user)
                                .corner_radius(8)
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.set_max_width(ui.available_width() * 0.85);
                                    ui.add(
                                        egui::Label::new(RichText::new(&m.text).color(p.fg)).wrap(),
                                    );
                                });
                        });
                    } else {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("✦").color(p.accent));
                            ui.vertical(|ui| chat_text(ui, p, &m.text));
                        });
                    }
                    ui.add_space(8.0);
                }
                if let Some(s) = self.session
                    && let Some(partial) = self.stream.text.get(&s)
                {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new("✦").color(p.accent));
                        ui.vertical(|ui| chat_text(ui, p, &format!("{partial}▍")));
                    });
                }
                let mut undo = None;
                for c in &self.cards {
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(c.summary()).small()).wrap());
                            ui.horizontal(|ui| {
                                if ui.small_button(t.undo_turn).clicked() {
                                    undo = Some("checkpoint.undo");
                                }
                                if ui.small_button(t.redo).clicked() {
                                    undo = Some("checkpoint.redo");
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
                if let Some(m) = undo {
                    self.fire(m);
                }
                let mut decide = None;
                for a in &self.approvals {
                    egui::Frame::new()
                        .fill(p.bubble_user)
                        .stroke(Stroke::new(1.0, p.warning))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("⚠ {}", t.approval))
                                    .color(p.warning)
                                    .strong(),
                            );
                            ui.label(RichText::new(&a.action).monospace());
                            if !a.reason.is_empty() {
                                ui.label(RichText::new(&a.reason).small().color(p.fg_dim));
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(t.allow).color(p.on_accent),
                                        )
                                        .fill(p.accent),
                                    )
                                    .clicked()
                                {
                                    decide = Some((a.id, true));
                                }
                                if ui.button(t.deny).clicked() {
                                    decide = Some((a.id, false));
                                }
                            });
                        });
                }
                if let Some((id, allowed)) = decide {
                    self.approvals.retain(|a| a.id != id);
                    self.spawn(move |c| {
                        match c.call("approvals.decide", json!({"id": id, "allowed": allowed})) {
                            Ok(_) => Msg::Toast(String::new(), false),
                            Err(e) => Msg::Toast(e, true),
                        }
                    });
                }
                if self.busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t.agent_working).color(p.fg_dim).italics());
                    });
                }
            });
        egui::Frame::new()
            .fill(p.input)
            .stroke(Stroke::new(
                1.0,
                if self.focus_prompt {
                    p.accent
                } else {
                    p.border
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::same(6))
            .outer_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .hint_text(t.agent_hint)
                        .frame(egui::Frame::NONE)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if self.focus_prompt {
                    r.request_focus();
                    self.focus_prompt = false;
                }
                let send = r.has_focus()
                    && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Ctrl+↵").small().color(p.fg_dim));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.busy {
                            if ui
                                .add(egui::Button::new(RichText::new("■").color(p.error)))
                                .on_hover_text(t.stop)
                                .clicked()
                                && let Some(s) = self.session
                            {
                                self.spawn(move |c| {
                                    match c.call("session.cancel", json!({"session": s})) {
                                        Ok(_) => Msg::Toast(String::new(), false),
                                        Err(e) => Msg::Toast(e, true),
                                    }
                                });
                            }
                        } else {
                            let b = egui::Button::new(RichText::new("➤").color(p.on_accent))
                                .fill(p.accent);
                            if ui
                                .add_enabled(!self.prompt.trim().is_empty(), b)
                                .on_hover_text(t.send)
                                .clicked()
                                || send
                            {
                                self.send_prompt();
                            }
                        }
                    });
                });
            });
    }

    fn ui_status(&mut self, ui: &mut egui::Ui) {
        let p = self.pal();
        let t = self.t;
        let ctx = ui.ctx().clone();
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            // Indicador de conexión a la izquierda, como el de remoto de VS Code.
            let (txt, fill) = if self.connected {
                ("⇆ pcoded".to_string(), p.accent)
            } else {
                (format!("⚠ {}", t.reconnecting), p.error)
            };
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    ui.label(RichText::new(txt).color(p.on_accent).small());
                });
            if !self.branch.is_empty()
                && ui
                    .add(
                        egui::Label::new(RichText::new(format!("⎇ {}", self.branch)).small())
                            .sense(Sense::click()),
                    )
                    .clicked()
            {
                self.run(Command::Git, &ctx);
            }
            let (e, w) = self.problems.counts();
            if ui
                .add(
                    egui::Label::new(RichText::new(format!("✗ {e}  ⚠ {w}")).small())
                        .sense(Sense::click()),
                )
                .clicked()
            {
                self.run(Command::Problems, &ctx);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(mb) = own_rss_mb() {
                    ui.label(
                        RichText::new(format!("{mb:.0} {}", t.memory))
                            .small()
                            .color(p.fg_dim),
                    );
                }
                if let Some(tab) = self.tabs.current() {
                    ui.label(RichText::new(format!("{:?}", tab.lang)).small());
                    ui.label(RichText::new("UTF-8").small());
                    ui.label(
                        RichText::new(format!(
                            "Ln {}, Col {}",
                            self.cursor.0 + 1,
                            self.cursor.1 + 1
                        ))
                        .small(),
                    );
                }
            });
        });
    }

    fn ui_picker(&mut self, ctx: &egui::Context, palette: bool) {
        let p = self.pal();
        let t = self.t;
        let Some(mut pk) = (if palette {
            self.palette.take()
        } else {
            self.quick.take()
        }) else {
            return;
        };
        let labels: Vec<String>;
        let details: Vec<String>;
        let order: Vec<usize>;
        if palette {
            let all: Vec<(String, &str)> = Command::ALL.iter().map(|c| c.label(t)).collect();
            labels = all.iter().map(|(l, _)| l.clone()).collect();
            details = all.iter().map(|(_, k)| k.to_string()).collect();
            order = palette_matches(&pk.query, &labels)
                .into_iter()
                .take(14)
                .collect();
        } else {
            let found = quick_open(&pk.query, &self.files, 14);
            labels = found.clone();
            details = found;
            order = (0..labels.len()).collect();
        }
        let mut chosen = None;
        let mut keep = true;
        let width = 600.0_f32.min(ctx.content_rect().width() - 40.0);
        egui::Area::new(egui::Id::unique(("selector", palette)))
            .anchor(Align2::CENTER_TOP, vec2(0.0, 42.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(p.side)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(8)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(90),
                    })
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let hint = if palette { t.type_command } else { t.type_file };
                        let prefix = if palette { "> " } else { "" };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(prefix).color(p.fg_dim));
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut pk.query)
                                    .hint_text(hint)
                                    .desired_width(f32::INFINITY),
                            );
                            r.request_focus();
                            if r.changed() {
                                pk.selected = 0;
                            }
                        });
                        ui.add_space(4.0);
                        if order.is_empty() {
                            ui.label(RichText::new(t.no_results).color(p.fg_dim));
                        }
                        for (row_i, &i) in order.iter().enumerate() {
                            let sel = row_i == pk.selected;
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
                            if sel || resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    4,
                                    if sel { p.selection } else { p.hover },
                                );
                            }
                            let y = rect.center().y;
                            let mut x = rect.left() + 8.0;
                            if !palette {
                                let name = labels[i].rsplit('/').next().unwrap_or(&labels[i]);
                                let (g, c) = file_icon(name);
                                ui.painter().text(
                                    egui::pos2(x, y),
                                    Align2::LEFT_CENTER,
                                    g,
                                    FontId::proportional(13.0),
                                    c,
                                );
                                x += 20.0;
                                let r = ui.painter().text(
                                    egui::pos2(x, y),
                                    Align2::LEFT_CENTER,
                                    name,
                                    FontId::proportional(13.0),
                                    p.fg,
                                );
                                ui.painter().text(
                                    egui::pos2(r.right() + 8.0, y),
                                    Align2::LEFT_CENTER,
                                    &details[i],
                                    FontId::proportional(11.0),
                                    p.fg_dim,
                                );
                            } else {
                                ui.painter().text(
                                    egui::pos2(x, y),
                                    Align2::LEFT_CENTER,
                                    &labels[i],
                                    FontId::proportional(13.0),
                                    p.fg,
                                );
                                if !details[i].is_empty() {
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 8.0, y),
                                        Align2::RIGHT_CENTER,
                                        &details[i],
                                        FontId::proportional(11.0),
                                        p.fg_dim,
                                    );
                                }
                            }
                            if resp.clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
            });
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) && !order.is_empty() {
                pk.selected = (pk.selected + 1) % order.len();
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) && !order.is_empty() {
                pk.selected = (pk.selected + order.len() - 1) % order.len();
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                chosen = order.get(pk.selected).copied();
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                keep = false;
            }
        });
        match chosen {
            Some(i) if palette => {
                let c = Command::ALL[i];
                self.run(c, ctx);
            }
            Some(i) => {
                let f = labels[i].clone();
                self.open_file(&f);
            }
            None if keep => {
                if palette {
                    self.palette = Some(pk);
                } else {
                    self.quick = Some(pk);
                }
            }
            None => {}
        }
    }

    fn ui_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let p = self.pal();
        egui::Area::new(egui::Id::unique("avisos"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-14.0, -34.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    egui::Frame::new()
                        .fill(p.side)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(6)
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 4],
                            blur: 12,
                            spread: 0,
                            color: Color32::from_black_alpha(80),
                        })
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.horizontal(|ui| {
                                let (g, c) = if t.error {
                                    ("✗", p.error)
                                } else {
                                    ("●", p.accent)
                                };
                                ui.label(RichText::new(g).color(c));
                                ui.add(egui::Label::new(&t.text).wrap());
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        let next = self.toasts.iter().map(|t| t.until).min().unwrap_or(now);
        ctx.request_repaint_after(next.saturating_duration_since(now));
    }
}

/// Un elemento de menú con su atajo a la derecha.
/// Ancho de cada columna de la bienvenida y separación entre las dos.
const WELCOME_COL: f32 = 260.0;
const WELCOME_GAP: f32 = 20.0;

/// Margen izquierdo que centra las dos columnas de la bienvenida en `width` (0 si no caben).
fn welcome_margin(width: f32) -> f32 {
    ((width - 2.0 * WELCOME_COL - WELCOME_GAP) / 2.0).max(0.0)
}

/// Dónde van, dentro de la barra de título `full`, el «command center» (centrado, 420 px como
/// mucho) y los tres conmutadores de paneles (26×22, pegados a la derecha, de izquierda a derecha).
fn title_rects(full: egui::Rect) -> (egui::Rect, [egui::Rect; 3]) {
    const TOGGLE: f32 = 26.0;
    const H: f32 = 22.0;
    let y = full.center().y;
    let toggles = [2.0, 1.0, 0.0].map(|i: f32| {
        let right = full.right() - 6.0 - i * TOGGLE;
        egui::Rect::from_min_max(
            egui::pos2(right - TOGGLE, y - H / 2.0),
            egui::pos2(right, y + H / 2.0),
        )
    });
    // El centro no pisa los conmutadores ni, a la izquierda, un hueco simétrico para el menú.
    let w = (2.0 * (toggles[0].left() - 8.0 - full.center().x)).clamp(120.0, 420.0);
    (
        egui::Rect::from_center_size(full.center(), vec2(w, H)),
        toggles,
    )
}

fn menu_item(ui: &mut egui::Ui, t: &Texts, c: Command, out: &mut Option<Command>) {
    let (label, keys) = c.label(t);
    if ui
        .add(egui::Button::new(label).shortcut_text(keys))
        .clicked()
    {
        *out = Some(c);
        ui.close();
    }
}

/// Botón de la barra de actividad: glifo grande, barra de acento a la izquierda si está activo y
/// un globo con un número (p. ej. cambios de git).
fn activity_button(
    ui: &mut egui::Ui,
    p: &Palette,
    glyph: &str,
    active: bool,
    badge: Option<usize>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(48.0, 44.0), Sense::click());
    let c = if active || resp.hovered() {
        p.fg
    } else {
        p.fg_dim
    };
    if active {
        ui.painter().line_segment(
            [rect.left_top(), rect.left_bottom()],
            Stroke::new(2.0, p.accent),
        );
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(20.0),
        c,
    );
    if let Some(n) = badge {
        let center = rect.center() + vec2(10.0, 9.0);
        ui.painter().circle_filled(center, 8.0, p.accent);
        let s = if n > 99 {
            "99+".to_string()
        } else {
            n.to_string()
        };
        ui.painter().text(
            center,
            Align2::CENTER_CENTER,
            s,
            FontId::proportional(9.0),
            p.on_accent,
        );
    }
    resp
}

/// Fila de lista a lo ancho (explorador, git, búsqueda): resaltado al pasar, icono, texto y, a la
/// derecha, una decoración (p. ej. la letra de git).
#[allow(clippy::too_many_arguments)] // una fila de lista: cada dato es una parte visible
fn row(
    ui: &mut egui::Ui,
    p: &Palette,
    indent: f32,
    icon: (&str, Color32),
    text: &str,
    color: Color32,
    selected: bool,
    deco: Option<(&str, Color32)>,
) -> egui::Response {
    let w = ui.available_width().max(60.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 0, p.active);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 0, p.hover);
    }
    let y = rect.center().y;
    let mut x = rect.left() + indent;
    if !icon.0.is_empty() {
        ui.painter().text(
            egui::pos2(x, y),
            Align2::LEFT_CENTER,
            icon.0,
            FontId::proportional(12.0),
            icon.1,
        );
        x += 18.0;
    }
    let right = rect.right() - if deco.is_some() { 24.0 } else { 6.0 };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(13.0), color);
    let clip =
        egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(right, rect.bottom()));
    ui.painter().with_clip_rect(clip).galley(
        egui::pos2(x, y - galley.size().y / 2.0),
        galley,
        color,
    );
    if let Some((d, c)) = deco {
        ui.painter().text(
            egui::pos2(rect.right() - 12.0, y),
            Align2::CENTER_CENTER,
            d,
            FontId::proportional(11.0),
            c,
        );
    }
    resp
}

/// Cabecera de sección de la barra lateral.
fn section_header(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(text).size(11.0).strong().color(p.fg));
    });
}

/// Globo con un número.
fn pill(ui: &mut egui::Ui, p: &Palette, text: &str) {
    egui::Frame::new()
        .fill(p.active)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(6, 0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(p.fg));
        });
}

/// Tecla con aspecto de tecla.
fn keycap(ui: &mut egui::Ui, p: &Palette, keys: &str) {
    for (i, k) in keys.split('+').enumerate() {
        if i > 0 {
            ui.label(RichText::new("+").small().color(p.fg_dim));
        }
        egui::Frame::new()
            .fill(p.input)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(3)
            .inner_margin(egui::Margin::symmetric(5, 1))
            .show(ui, |ui| {
                ui.label(RichText::new(k).small().color(p.fg));
            });
    }
}

/// Texto del agente con bloques de código (```) en monoespaciada sobre fondo propio.
fn chat_text(ui: &mut egui::Ui, p: &Palette, text: &str) {
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 1 {
            let code = part.split_once('\n').map_or(part, |(first, rest)| {
                if first.trim().chars().all(|c| c.is_ascii_alphanumeric()) {
                    rest
                } else {
                    part
                }
            });
            egui::Frame::new()
                .fill(p.code_bg)
                .corner_radius(4)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(code.trim_end()).monospace()).wrap());
                });
        } else if !part.trim().is_empty() {
            ui.add(egui::Label::new(RichText::new(part.trim()).color(p.fg)).wrap());
        }
    }
}

impl eframe::App for Ide {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(m) = self.rx.try_recv() {
            self.on_msg(m);
        }
        if self.quick.is_none() && self.palette.is_none() {
            self.shortcuts(&ctx);
        }
        let p = self.pal();
        let bar = |fill| {
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(6, 0))
        };
        egui::Panel::top("titulo")
            .exact_size(34.0)
            .frame(bar(p.side))
            .show(ui, |ui| self.ui_title(ui));
        egui::Panel::bottom("estado")
            .exact_size(22.0)
            .frame(bar(p.side))
            .show(ui, |ui| self.ui_status(ui));
        egui::Panel::left("actividad")
            .exact_size(48.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(p.side))
            .show(ui, |ui| self.ui_activity(ui));
        if self.show_side {
            let w = self.settings.explorer_width;
            let r = egui::Panel::left("lateral")
                .default_size(w)
                .size_range(160.0..=600.0)
                .frame(egui::Frame::new().fill(p.side))
                .show(ui, |ui| self.ui_side(ui));
            let nw = r.response.rect.width().round();
            if (nw - w).abs() > 2.0 && !ctx.input(|i| i.pointer.any_down()) {
                self.settings.explorer_width = nw;
                self.settings.save();
            }
        }
        if self.settings.show_agent {
            egui::Panel::right("agente")
                .default_size(360.0)
                .size_range(260.0..=700.0)
                .frame(egui::Frame::new().fill(p.side))
                .show(ui, |ui| self.ui_agent(ui));
        }
        if self.show_panel {
            egui::Panel::bottom("panel")
                .default_size(200.0)
                .size_range(80.0..=600.0)
                .frame(
                    egui::Frame::new()
                        .fill(p.editor)
                        .inner_margin(egui::Margin::symmetric(6, 4)),
                )
                .show(ui, |ui| self.ui_panel(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::Frame::new()
                .fill(p.side)
                .show(ui, |ui| self.ui_editor(ui));
        });
        self.ui_picker(&ctx, false);
        self.ui_picker(&ctx, true);
        self.ui_toasts(&ctx);
        if self.busy || self.term.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barra_de_titulo_centro_y_conmutadores_a_la_derecha() {
        let full = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1100.0, 30.0));
        let (center, toggles) = title_rects(full);
        assert_eq!(center.center(), full.center());
        assert_eq!(center.width(), 420.0);
        // Tres botones de 26×22, de izquierda a derecha, pegados al borde y sin solaparse.
        assert!(
            toggles
                .iter()
                .all(|r| r.width() == 26.0 && r.height() == 22.0)
        );
        assert!(toggles[0].right() <= toggles[1].left() && toggles[1].right() <= toggles[2].left());
        assert_eq!(toggles[2].right(), full.right() - 6.0);
        assert!(center.right() < toggles[0].left());
        // En una ventana estrecha el centro encoge y sigue sin pisar los conmutadores.
        let (c, t) = title_rects(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            vec2(500.0, 30.0),
        ));
        assert!(c.width() < 420.0 && c.right() < t[0].left());
    }

    #[test]
    fn bienvenida_centrada() {
        assert_eq!(welcome_margin(1000.0), 230.0);
        assert_eq!(welcome_margin(540.0), 0.0);
        assert_eq!(welcome_margin(300.0), 0.0);
    }

    #[test]
    fn paleta_filtra_comandos() {
        let t = &crate::texts::ES;
        let labels: Vec<String> = Command::ALL.iter().map(|c| c.label(t).0).collect();
        let m = palette_matches("guardar", &labels);
        assert_eq!(Command::ALL[m[0]], Command::Save);
        assert_eq!(palette_matches("", &labels).len(), Command::ALL.len());
        assert!(palette_matches("zzzz", &labels).is_empty());
        // Todos los comandos tienen texto en los dos idiomas.
        for c in Command::ALL {
            assert!(!c.label(&crate::texts::EN).0.is_empty());
        }
    }

    #[test]
    fn busqueda_agrupada_por_archivo() {
        let hits = vec![
            ("a.rs".to_string(), 3, "x".to_string()),
            ("b.rs".to_string(), 1, "y".to_string()),
            ("a.rs".to_string(), 9, "z".to_string()),
        ];
        let g = group_hits(&hits);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].0, "a.rs");
        assert_eq!(g[0].1, vec![(3, "x".to_string()), (9, "z".to_string())]);
    }

    #[test]
    fn migas_de_pan() {
        assert_eq!(
            breadcrumbs("crates/pcode-ide/src/app.rs"),
            ["crates", "pcode-ide", "src", "app.rs"]
        );
        assert_eq!(breadcrumbs("README.md"), ["README.md"]);
    }
}
