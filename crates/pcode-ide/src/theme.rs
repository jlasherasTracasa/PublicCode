//! Aspecto del IDE: paletas «Dark Modern» y «Light Modern» como las de VS Code, estilo de egui e
//! iconos de archivo por tipo. Sin imágenes: todo son colores y glifos (no gasta memoria).
use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Colores de un tema.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// Fondo del editor.
    pub editor: Color32,
    /// Barra lateral, barra de actividad, título y pestañas inactivas.
    pub side: Color32,
    /// Fondo de los cuadros de texto.
    pub input: Color32,
    /// Bordes entre zonas.
    pub border: Color32,
    /// Texto normal.
    pub fg: Color32,
    /// Texto secundario.
    pub fg_dim: Color32,
    /// Color de acento (foco, pestaña activa, botones).
    pub accent: Color32,
    /// Texto sobre el acento.
    pub on_accent: Color32,
    /// Fila bajo el ratón.
    pub hover: Color32,
    /// Fila seleccionada.
    pub active: Color32,
    /// Selección de texto.
    pub selection: Color32,
    /// Números de línea.
    pub line_number: Color32,
    /// Error.
    pub error: Color32,
    /// Aviso.
    pub warning: Color32,
    /// Git: modificado.
    pub git_modified: Color32,
    /// Git: nuevo o sin seguimiento.
    pub git_added: Color32,
    /// Git: borrado.
    pub git_deleted: Color32,
    /// Burbuja del usuario en el chat.
    pub bubble_user: Color32,
    /// Fondo de los bloques de código del chat.
    pub code_bg: Color32,
}

const fn hex(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// VS Code «Dark Modern».
pub const DARK: Palette = Palette {
    editor: hex(0x1f1f1f),
    side: hex(0x181818),
    input: hex(0x313131),
    border: hex(0x2b2b2b),
    fg: hex(0xcccccc),
    fg_dim: hex(0x9d9d9d),
    accent: hex(0x0078d4),
    on_accent: hex(0xffffff),
    hover: hex(0x2a2d2e),
    active: hex(0x37373d),
    selection: hex(0x264f78),
    line_number: hex(0x6e7681),
    error: hex(0xf14c4c),
    warning: hex(0xcca700),
    git_modified: hex(0xe2c08d),
    git_added: hex(0x73c991),
    git_deleted: hex(0xc74e39),
    bubble_user: hex(0x2b2d31),
    code_bg: hex(0x141414),
};

/// VS Code «Light Modern».
pub const LIGHT: Palette = Palette {
    editor: hex(0xffffff),
    side: hex(0xf8f8f8),
    input: hex(0xffffff),
    border: hex(0xe5e5e5),
    fg: hex(0x3b3b3b),
    fg_dim: hex(0x6e6e6e),
    accent: hex(0x005fb8),
    on_accent: hex(0xffffff),
    hover: hex(0xf2f2f2),
    active: hex(0xe4e6f1),
    selection: hex(0xadd6ff),
    line_number: hex(0x6e7681),
    error: hex(0xe51400),
    warning: hex(0xbf8803),
    git_modified: hex(0x895503),
    git_added: hex(0x587c0c),
    git_deleted: hex(0xad0707),
    bubble_user: hex(0xeef1f6),
    code_bg: hex(0xf3f3f3),
};

/// Paleta del tema.
pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// Aplica colores, espaciados y redondeos a egui.
pub fn apply(ctx: &egui::Context, dark: bool, font_size: f32) {
    let p = palette(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.side;
    v.window_fill = p.side;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.code_bg;
    v.hyperlink_color = p.accent;
    v.override_text_color = Some(p.fg);
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.fg);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.bg_fill = p.side;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.fg);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.widgets.inactive.weak_bg_fill = p.input;
    v.widgets.inactive.bg_fill = p.input;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.fg);
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.active.weak_bg_fill = p.active;
    v.widgets.active.bg_fill = p.active;
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
        use egui::{FontFamily::*, FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Small, FontId::new(11.0, Proportional)),
            (TextStyle::Body, FontId::new(13.0, Proportional)),
            (TextStyle::Button, FontId::new(13.0, Proportional)),
            (TextStyle::Heading, FontId::new(20.0, Proportional)),
            (TextStyle::Monospace, FontId::new(font_size, Monospace)),
        ]
        .into();
    });
}

/// Glifo y color del icono de un archivo según su nombre (como los iconos de VS Code).
pub fn file_icon(name: &str) -> (&'static str, Color32) {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match (lower.as_str(), ext) {
        ("cargo.toml" | "cargo.lock", _) => ("⚙", hex(0xdea584)),
        ("dockerfile", _) | (_, "dockerfile") => ("◈", hex(0x2496ed)),
        ("makefile", _) => ("⚙", hex(0x6d8086)),
        (n, _) if n.starts_with(".git") => ("◈", hex(0xf14c28)),
        (_, "rs") => ("◆", hex(0xdea584)),
        (_, "py" | "pyi") => ("◆", hex(0x4b8bbe)),
        (_, "js" | "mjs" | "cjs" | "jsx") => ("◆", hex(0xcbcb41)),
        (_, "ts" | "tsx" | "mts") => ("◆", hex(0x3178c6)),
        (_, "go") => ("◆", hex(0x00add8)),
        (_, "c" | "h" | "cc" | "cpp" | "hpp") => ("◆", hex(0x659ad2)),
        (_, "java" | "kt") => ("◆", hex(0xb07219)),
        (_, "md") => ("¶", hex(0x519aba)),
        (_, "json") => ("{}", hex(0xcbcb41)),
        (_, "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf") => ("⚙", hex(0x6d8086)),
        (_, "sh" | "bash" | "ps1") => ("$", hex(0x89e051)),
        (_, "html" | "htm") => ("◇", hex(0xe44d26)),
        (_, "css" | "scss") => ("#", hex(0x42a5f5)),
        (_, "svg" | "png" | "jpg" | "jpeg" | "gif" | "ico") => ("▣", hex(0xa074c4)),
        (_, "lock") => ("≡", hex(0x8b949e)),
        _ => ("≡", hex(0x8b949e)),
    }
}

/// Letra y color de la decoración de git de un archivo (`M`, `U`, `A`, `D`), como en VS Code.
pub fn git_decoration(index: &str, worktree: &str, p: &Palette) -> Option<(&'static str, Color32)> {
    let label = crate::git::status_label(index, worktree);
    Some(match label {
        "M" | "R" => ("M", p.git_modified),
        "?" => ("U", p.git_added),
        "A" => ("A", p.git_added),
        "D" => ("D", p.git_deleted),
        "U" => ("!", p.error),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iconos_por_tipo() {
        assert_eq!(file_icon("main.rs").0, "◆");
        assert_eq!(file_icon("Cargo.toml"), ("⚙", hex(0xdea584)));
        assert_eq!(file_icon("datos.JSON").0, "{}");
        assert_eq!(file_icon(".gitignore").0, "◈");
        assert_eq!(file_icon("sin_extension").0, "≡");
        // Solo glifos que existen en las fuentes del IDE (ⓘ salía como un cuadro).
        assert_eq!(file_icon("README.md").0, "¶");
    }

    #[test]
    fn decoraciones_de_git() {
        let p = &DARK;
        assert_eq!(git_decoration(" ", "M", p), Some(("M", p.git_modified)));
        assert_eq!(git_decoration("?", "?", p), Some(("U", p.git_added)));
        assert_eq!(git_decoration("D", " ", p).unwrap().0, "D");
        assert_eq!(git_decoration(" ", " ", p), None);
    }

    #[test]
    fn paletas_distintas_y_legibles() {
        assert_ne!(DARK.editor, LIGHT.editor);
        // Contraste mínimo entre el texto y el fondo del editor (luminancia relativa simple).
        let lum =
            |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        assert!(lum(DARK.fg) - lum(DARK.editor) > 120.0);
        assert!(lum(LIGHT.editor) - lum(LIGHT.fg) > 120.0);
    }
}
