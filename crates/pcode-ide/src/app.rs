//! Interfaz del IDE. Las peticiones van en hilos y vuelven por un canal; se repinta solo cuando
//! llega algo (sin sondeo en reposo).
use crate::agent::{Stream, TurnCard};
use crate::find::{find_all, replace_all};
use crate::highlight::layout_job;
use crate::problems::Problems;
use crate::rpc::Client;
use crate::settings::IdeSettings;
use crate::state::{Entry, Tabs, Tree, gutter, line_to_byte, own_rss_mb, quick_open};
use crate::terminal::Terminal;
use crate::texts::Texts;
use eframe::egui;
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};

/// Resultado de una petición o evento que llega al hilo de la interfaz.
pub enum Msg {
    /// Contenido de una carpeta.
    Dir(String, Vec<Entry>),
    /// Archivo leído (ruta, texto, es binario).
    File(String, String, bool),
    /// Lista de archivos del proyecto.
    Files(Vec<String>),
    /// Resultados de búsqueda (ruta:línea texto).
    Hits(Vec<String>),
    /// Rama y archivos cambiados de git.
    Git(String, Vec<(String, String)>),
    /// Archivo guardado.
    Saved(String, String),
    /// Evento del demonio (Null = conexión perdida).
    Event(Value),
    /// Aviso o error.
    Note(String),
}

/// El IDE.
pub struct Ide {
    client: Arc<Client>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    t: &'static Texts,
    tabs: Tabs,
    tree: Tree,
    files: Vec<String>,
    open_box: Option<String>,
    search: String,
    hits: Vec<String>,
    branch: String,
    git: Vec<(String, String)>,
    session: Option<i64>,
    chat: Vec<String>,
    prompt: String,
    note: String,
    connected: bool,
    dark: bool,
    settings: IdeSettings,
    problems: Problems,
    term: Terminal,
    term_cmd: String,
    show_term: bool,
    stream: Stream,
    cards: Vec<TurnCard>,
    find: Option<(String, String, bool)>,
    goto: Option<String>,
    pending_cursor: Option<usize>,
}

impl Ide {
    /// Crea el IDE y pide lo primero que se ve.
    pub fn new(client: Client, ctx: &egui::Context, t: &'static Texts) -> Ide {
        let (tx, rx) = mpsc::channel();
        let client = Arc::new(client);
        let settings = IdeSettings::load();
        let ide = Ide {
            client: client.clone(),
            tx: tx.clone(),
            rx,
            t,
            tabs: Tabs::default(),
            tree: Tree::default(),
            files: vec![],
            open_box: None,
            search: String::new(),
            hits: vec![],
            branch: String::new(),
            git: vec![],
            session: None,
            chat: vec![],
            prompt: String::new(),
            note: String::new(),
            connected: true,
            dark: settings.dark,
            settings,
            problems: Problems::default(),
            term: Terminal::default(),
            term_cmd: String::new(),
            show_term: false,
            stream: Stream::default(),
            cards: vec![],
            find: None,
            goto: None,
            pending_cursor: None,
        };
        ctx.set_visuals(if ide.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        ide.list_dir("");
        ide.spawn(|c| match c.call("fs.files", json!({})) {
            Ok(v) => Msg::Files(
                v.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
            ),
            Err(e) => Msg::Note(e),
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

    fn spawn(&self, f: impl FnOnce(&Client) -> Msg + Send + 'static) {
        let (c, tx) = (self.client.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(f(&c));
        });
    }

    fn list_dir(&self, dir: &str) {
        let d = dir.to_string();
        self.spawn(move |c| match c.call("fs.list", json!({"path": d})) {
            Ok(v) => Msg::Dir(d, serde_json::from_value(v).unwrap_or_default()),
            Err(e) => Msg::Note(e),
        });
    }

    fn open_file(&self, path: &str) {
        let p = path.to_string();
        self.spawn(move |c| match c.call("fs.read", json!({"path": p})) {
            Ok(v) => Msg::File(
                p,
                v["content"].as_str().unwrap_or_default().to_string(),
                v["binary"].as_bool().unwrap_or(false),
            ),
            Err(e) => Msg::Note(e),
        });
    }

    fn refresh_git(&self) {
        self.spawn(|c| match c.call("git.status", json!({})) {
            Ok(v) => Msg::Git(
                v["branch"].as_str().unwrap_or_default().to_string(),
                v["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| {
                        (
                            f["path"].as_str().unwrap_or_default().to_string(),
                            format!(
                                "{}{}",
                                f["index"].as_str().unwrap_or(" "),
                                f["worktree"].as_str().unwrap_or(" ")
                            ),
                        )
                    })
                    .collect(),
            ),
            Err(e) => Msg::Note(e),
        });
    }

    fn save(&mut self) {
        if let Some(t) = self.tabs.current() {
            let (p, text) = (t.path.clone(), t.text.clone());
            self.spawn(
                move |c| match c.call("fs.write", json!({"path": p, "content": text})) {
                    Ok(_) => Msg::Saved(p, text),
                    Err(e) => Msg::Note(e),
                },
            );
        }
    }

    fn send_prompt(&mut self) {
        let text = std::mem::take(&mut self.prompt);
        if text.trim().is_empty() {
            return;
        }
        self.chat.push(format!("» {text}"));
        match self.session {
            Some(s) => self.spawn(move |c| {
                match c.call("session.send", json!({"session": s, "text": text})) {
                    Ok(_) => Msg::Note(String::new()),
                    Err(e) => Msg::Note(e),
                }
            }),
            None => {
                let c = self.client.clone();
                match c.call("session.create", json!({"task": text})) {
                    Ok(v) => self.session = v["id"].as_i64(),
                    Err(e) => self.note = e,
                }
            }
        }
    }

    fn run_terminal(&mut self) {
        let cmd = std::mem::take(&mut self.term_cmd);
        if cmd.trim().is_empty() {
            return;
        }
        match self.client.call("terminal.run", json!({"command": cmd})) {
            Ok(v) => self
                .term
                .start(v.as_u64().or(v["id"].as_u64()).unwrap_or(0), &cmd),
            Err(e) => self.note = e,
        }
    }

    fn fire(&self, method: &'static str) {
        self.spawn(move |c| match c.call(method, json!({})) {
            Ok(_) => Msg::Note(String::new()),
            Err(e) => Msg::Note(e),
        });
    }

    fn on_event(&mut self, ev: Value) {
        if ev.is_null() {
            self.connected = false;
            return;
        }
        self.connected = true;
        if self.problems.apply(&ev) || self.term.apply(&ev) || self.stream.apply(&ev) {
            return;
        }
        if let Some(c) = TurnCard::from_event(&ev) {
            self.cards.push(c);
            return;
        }
        match ev["tipo"].as_str() {
            Some("filesChanged") => {
                for d in self.tree.invalidate() {
                    self.list_dir(&d);
                }
                self.refresh_git();
            }
            Some("transcript") if ev["session"].as_i64() == self.session => {
                if let Some(t) = ev["entry"]["text"].as_str() {
                    let who = if ev["entry"]["tipo"] == "assistant" {
                        "✦"
                    } else {
                        "·"
                    };
                    if ev["entry"]["tipo"] != "user" {
                        self.chat.push(format!("{who} {t}"));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Dir(d, e) => self.tree.set(&d, e),
            Msg::File(p, text, bin) => {
                if bin {
                    self.note = self.t.binary.into();
                } else {
                    self.tabs.open(&p, text);
                }
            }
            Msg::Files(f) => self.files = f,
            Msg::Hits(h) => self.hits = h,
            Msg::Git(b, f) => (self.branch, self.git) = (b, f),
            Msg::Saved(p, text) => {
                self.tabs.mark_saved(&p, &text);
                self.refresh_git();
            }
            Msg::Event(ev) => self.on_event(ev),
            Msg::Note(n) => self.note = n,
        }
    }

    fn tree_ui(&mut self, ui: &mut egui::Ui, dir: &str) {
        let entries = self.tree.children.get(dir).cloned().unwrap_or_default();
        for e in entries {
            if e.dir {
                let open = self.tree.expanded.contains(&e.path);
                if ui
                    .selectable_label(
                        false,
                        format!("{} {}", if open { "▾" } else { "▸" }, e.name),
                    )
                    .clicked()
                    && self.tree.toggle(&e.path)
                {
                    self.list_dir(&e.path);
                }
                if open {
                    ui.indent(&e.path, |ui| self.tree_ui(ui, &e.path));
                }
            } else if ui.selectable_label(false, &e.name).clicked() {
                self.open_file(&e.path);
            }
        }
    }
}

impl eframe::App for Ide {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(m) = self.rx.try_recv() {
            self.on_msg(m);
        }
        let ctrl = |k| ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, k));
        if ctrl(egui::Key::S) {
            self.save();
        }
        if ctrl(egui::Key::P) {
            self.open_box = Some(String::new());
        }
        if ctrl(egui::Key::F) || ctrl(egui::Key::H) {
            self.find
                .get_or_insert_with(|| (String::new(), String::new(), false));
        }
        if ctrl(egui::Key::G) {
            self.goto.get_or_insert_with(String::new);
        }
        if ctrl(egui::Key::Backtick) {
            self.show_term = !self.show_term;
        }
        if ctrl(egui::Key::Plus) || ctrl(egui::Key::Equals) {
            self.settings.font_size = (self.settings.font_size + 1.0).min(32.0);
            self.settings.save();
        }
        if ctrl(egui::Key::Minus) {
            self.settings.font_size = (self.settings.font_size - 1.0).max(8.0);
            self.settings.save();
        }
        if ctrl(egui::Key::W) {
            let i = self.tabs.active;
            self.tabs.close(i);
        }
        let t = self.t;
        egui::Panel::bottom("estado").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("⎇ {}", self.branch));
                ui.label(if self.connected {
                    format!("● {}", t.connected)
                } else {
                    format!("○ {}", t.reconnecting)
                });
                if let Some(mb) = own_rss_mb() {
                    ui.label(format!("{mb:.0} MB {}", t.memory));
                }
                let (e, w) = self.problems.counts();
                ui.label(format!("✗ {e} ⚠ {w}"));
                ui.label(&self.note);
            });
        });
        egui::Panel::left("explorador")
            .default_size(240.0)
            .show(ui, |ui| {
                ui.heading(t.explorer);
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.search);
                    if ui.button(t.search).clicked() && !self.search.is_empty() {
                        let q = self.search.clone();
                        self.spawn(move |c| match c.call("search.text", json!({"query": q})) {
                            Ok(v) => Msg::Hits(
                                v.as_array()
                                    .into_iter()
                                    .flatten()
                                    .take(200)
                                    .map(|h| {
                                        format!(
                                            "{}:{} {}",
                                            h["path"].as_str().unwrap_or(""),
                                            h["line"],
                                            h["text"].as_str().unwrap_or("").trim()
                                        )
                                    })
                                    .collect(),
                            ),
                            Err(e) => Msg::Note(e),
                        });
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if !self.hits.is_empty() {
                        ui.label(format!("{} {}", self.hits.len(), t.results));
                        for h in self.hits.clone() {
                            if ui.small_button(&h).clicked()
                                && let Some(p) = h.split(':').next()
                            {
                                self.open_file(p);
                            }
                        }
                        ui.separator();
                    }
                    self.tree_ui(ui, "");
                    ui.separator();
                    ui.label(t.git);
                    if self.git.is_empty() {
                        ui.weak(t.git_clean);
                    }
                    for (p, st) in self.git.clone() {
                        if ui.small_button(format!("{st} {p}")).clicked() {
                            self.open_file(&p);
                        }
                    }
                });
            });
        egui::Panel::right("agente")
            .default_size(340.0)
            .show(ui, |ui| {
                ui.heading(t.agent);
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(ui.available_height() - 90.0)
                    .show(ui, |ui| {
                        for l in &self.chat {
                            ui.add(egui::Label::new(l).wrap());
                        }
                        if let Some(sid) = self.session
                            && let Some(partial) = self.stream.text.get(&sid)
                        {
                            ui.add(egui::Label::new(format!("✦ {partial}▌")).wrap());
                        }
                        let mut undo = None;
                        for c in &self.cards {
                            ui.add(egui::Label::new(c.summary()).wrap());
                            ui.horizontal(|ui| {
                                if ui.small_button(t.undo_turn).clicked() {
                                    undo = Some("checkpoint.undo");
                                }
                                if ui.small_button(t.redo).clicked() {
                                    undo = Some("checkpoint.redo");
                                }
                            });
                        }
                        if let Some(m) = undo {
                            self.fire(m);
                        }
                    });
                ui.add(
                    egui::TextEdit::multiline(&mut self.prompt)
                        .hint_text(t.agent_hint)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                let enter =
                    ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
                if ui.button(t.send).clicked() || enter {
                    self.send_prompt();
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut close = None;
                for (i, tab) in self.tabs.list.iter().enumerate() {
                    let label = format!("{}{}", tab.title(), if tab.dirty() { " ●" } else { "" });
                    if ui.selectable_label(i == self.tabs.active, label).clicked() {
                        self.tabs.active = i;
                    }
                    if ui.small_button("×").on_hover_text(t.close).clicked() {
                        close = Some(i);
                    }
                }
                if let Some(i) = close {
                    self.tabs.close(i);
                }
            });
            let dark = self.dark;
            if let Some((q, r, case)) = self.find.as_mut() {
                let mut close = false;
                let mut do_replace = false;
                ui.horizontal(|ui| {
                    ui.label(t.find);
                    ui.text_edit_singleline(q);
                    ui.label(t.replace);
                    ui.text_edit_singleline(r);
                    ui.toggle_value(case, t.match_case);
                    if ui.button(t.replace_all).clicked() {
                        do_replace = true;
                    }
                    if ui.button("×").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        close = true;
                    }
                });
                if let Some(tab) = self.tabs.current_mut() {
                    let n = find_all(&tab.text, q, *case).len();
                    ui.weak(format!("{n} {}", t.results));
                    if do_replace {
                        tab.text = replace_all(&tab.text, q, r, *case).0;
                    }
                }
                if close {
                    self.find = None;
                }
            }
            if let Some(g) = self.goto.as_mut() {
                let mut done = None;
                ui.horizontal(|ui| {
                    ui.label(t.go_to_line);
                    let r = ui.text_edit_singleline(g);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        done = Some(g.trim().parse::<usize>().ok());
                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        done = Some(None);
                    }
                });
                if let Some(n) = done {
                    self.goto = None;
                    if let (Some(n), Some(tab)) = (n, self.tabs.current()) {
                        let byte = line_to_byte(&tab.text, n);
                        self.pending_cursor = Some(tab.text[..byte].chars().count());
                    }
                }
            }
            let size = self.settings.font_size;
            let pending = self.pending_cursor.take();
            match self.tabs.current_mut() {
                None => {
                    ui.weak(t.no_file);
                    ui.weak(t.shortcuts);
                }
                Some(tab) => {
                    let lang = tab.lang;
                    let font = egui::FontId::monospace(size);
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, w: f32| {
                        let mut job = layout_job(buf.as_str(), lang, dark, font.clone());
                        job.wrap.max_width = w;
                        ui.fonts_mut(|f| f.layout_job(job))
                    };
                    let numbers = gutter(&tab.text);
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(
                                egui::RichText::new(numbers)
                                    .font(egui::FontId::monospace(size))
                                    .color(ui.visuals().weak_text_color()),
                            );
                            let mut out = egui::TextEdit::multiline(&mut tab.text)
                                .font(egui::FontId::monospace(size))
                                .desired_width(f32::INFINITY)
                                .desired_rows(40)
                                .layouter(&mut layouter)
                                .show(ui);
                            if let Some(i) = pending {
                                let c = egui::text::CCursor::new(i);
                                out.state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(c)));
                                out.state.store(ui.ctx(), out.response.id);
                                out.response.request_focus();
                            }
                        });
                    });
                }
            }
        });
        if self.show_term {
            egui::Panel::bottom("terminal")
                .default_size(180.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(t.terminal);
                        let r = ui.text_edit_singleline(&mut self.term_cmd);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.run_terminal();
                        }
                        if ui.button(t.run).clicked() {
                            self.run_terminal();
                        }
                        if let Some(id) = self.term.running
                            && ui.button(t.stop).clicked()
                        {
                            let c = self.client.clone();
                            std::thread::spawn(move || {
                                let _ = c.call("terminal.kill", json!({"id": id}));
                            });
                        }
                    });
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(self.term.text()).monospace())
                                    .wrap(),
                            );
                        });
                });
        }
        if let Some(mut q) = self.open_box.take() {
            let mut keep = true;
            let mut picked = None;
            egui::Window::new(t.quick_open)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
                .collapsible(false)
                .show(&ctx, |ui| {
                    let r = ui.text_edit_singleline(&mut q);
                    r.request_focus();
                    let found = quick_open(&q, &self.files, 12);
                    for f in &found {
                        if ui.selectable_label(false, f).clicked() {
                            picked = Some(f.clone());
                        }
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        picked = found.first().cloned();
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        keep = false;
                    }
                });
            match picked {
                Some(p) => self.open_file(&p),
                None if keep => self.open_box = Some(q),
                None => {}
            }
        }
    }
}
