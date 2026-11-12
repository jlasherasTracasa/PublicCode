//! Temas de color. Con `NO_COLOR` no hay colores, pero se mantienen símbolos y negrita.
use ratatui::style::{Color, Modifier, Style};

/// Paleta de la TUI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Texto.
    pub fg: Color,
    /// Texto secundario.
    pub dim: Color,
    /// Acento (usuario, títulos).
    pub accent: Color,
    /// Correcto.
    pub ok: Color,
    /// Error.
    pub err: Color,
    /// Aviso.
    pub warn: Color,
    /// Fondo de los bloques de código.
    pub code_bg: Color,
    /// Fondo de las barras superior e inferior.
    pub bar_bg: Color,
    /// Texto de las barras.
    pub bar_fg: Color,
    /// Fondo de las tarjetas de los mensajes del usuario.
    pub bubble_bg: Color,
    /// Texto de esas tarjetas.
    pub bubble_fg: Color,
    /// Si se usan colores.
    pub use_color: bool,
}

impl Theme {
    /// Nombres de los temas, en el orden en que se recorren.
    pub const NAMES: [&'static str; 5] = ["azul", "oscuro", "claro", "alto-contraste", "opencode"];

    /// El tema siguiente a este en `NAMES` (el primero tras el último o si no se reconoce).
    pub fn next(&self) -> Theme {
        let no_color = !self.use_color;
        let pos = Self::NAMES
            .iter()
            .position(|n| Theme::by_name(n, no_color) == Some(*self));
        let idx = match pos {
            Some(i) => (i + 1) % Self::NAMES.len(),
            None => 0,
        };
        Theme::by_name(Self::NAMES[idx], no_color).unwrap()
    }

    /// Tema por nombre: `azul` (por defecto), `oscuro`, `claro`, `alto-contraste` u `opencode`.
    pub fn by_name(name: &str, no_color: bool) -> Option<Theme> {
        let mut t = match name {
            "azul" | "blue" => Theme {
                fg: Color::Reset,
                dim: Color::Rgb(159, 179, 209),
                accent: Color::Rgb(138, 180, 248),
                ok: Color::Rgb(109, 213, 140),
                err: Color::Rgb(255, 180, 171),
                warn: Color::Rgb(245, 196, 81),
                code_bg: Color::Rgb(24, 43, 70),
                bar_bg: Color::Rgb(11, 87, 208),
                bar_fg: Color::Rgb(255, 255, 255),
                bubble_bg: Color::Rgb(31, 58, 95),
                bubble_fg: Color::Rgb(211, 227, 253),
                use_color: true,
            },
            "oscuro" | "dark" => Theme {
                fg: Color::Reset,
                dim: Color::DarkGray,
                accent: Color::Cyan,
                ok: Color::Green,
                err: Color::Red,
                warn: Color::Yellow,
                code_bg: Color::Rgb(40, 44, 52),
                bar_bg: Color::Rgb(40, 44, 52),
                bar_fg: Color::Rgb(220, 223, 228),
                bubble_bg: Color::Rgb(50, 56, 66),
                bubble_fg: Color::Rgb(230, 237, 243),
                use_color: true,
            },
            "claro" | "light" => Theme {
                fg: Color::Reset,
                dim: Color::Gray,
                accent: Color::Rgb(11, 87, 208),
                ok: Color::Green,
                err: Color::Red,
                warn: Color::Magenta,
                code_bg: Color::Rgb(234, 242, 253),
                bar_bg: Color::Rgb(11, 87, 208),
                bar_fg: Color::Rgb(255, 255, 255),
                bubble_bg: Color::Rgb(211, 227, 253),
                bubble_fg: Color::Rgb(4, 30, 73),
                use_color: true,
            },
            "alto-contraste" | "high-contrast" => Theme {
                fg: Color::White,
                dim: Color::White,
                accent: Color::LightYellow,
                ok: Color::LightGreen,
                err: Color::LightRed,
                warn: Color::LightYellow,
                code_bg: Color::Black,
                bar_bg: Color::White,
                bar_fg: Color::Black,
                bubble_bg: Color::Black,
                bubble_fg: Color::LightYellow,
                use_color: true,
            },
            "opencode" => Theme {
                fg: Color::Reset,
                dim: Color::Rgb(128, 128, 128),
                accent: Color::Rgb(250, 178, 131),
                ok: Color::Rgb(127, 216, 143),
                err: Color::Rgb(224, 108, 117),
                warn: Color::Rgb(245, 167, 66),
                code_bg: Color::Rgb(20, 20, 20),
                bar_bg: Color::Rgb(20, 20, 20),
                bar_fg: Color::Rgb(238, 238, 238),
                bubble_bg: Color::Rgb(30, 30, 30),
                bubble_fg: Color::Rgb(238, 238, 238),
                use_color: true,
            },
            _ => return None,
        };
        t.use_color = !no_color;
        Some(t)
    }

    /// Tema por defecto: `PCODE_TEMA` o `azul`, respetando `NO_COLOR`.
    pub fn from_env() -> Theme {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        std::env::var("PCODE_TEMA")
            .ok()
            .and_then(|n| Theme::by_name(&n, no_color))
            .unwrap_or_else(|| Theme::by_name("azul", no_color).unwrap())
    }

    fn color(&self, c: Color) -> Style {
        if self.use_color {
            Style::new().fg(c)
        } else {
            Style::new()
        }
    }
    /// Texto normal.
    pub fn text(&self) -> Style {
        self.color(self.fg)
    }
    /// Texto secundario.
    pub fn dimmed(&self) -> Style {
        if self.use_color {
            Style::new().fg(self.dim)
        } else {
            Style::new().add_modifier(Modifier::DIM)
        }
    }
    /// Acento en negrita.
    pub fn strong(&self) -> Style {
        self.color(self.accent).add_modifier(Modifier::BOLD)
    }
    /// Correcto.
    pub fn good(&self) -> Style {
        self.color(self.ok)
    }
    /// Error.
    pub fn bad(&self) -> Style {
        self.color(self.err)
    }
    /// Aviso en negrita.
    pub fn warning(&self) -> Style {
        self.color(self.warn).add_modifier(Modifier::BOLD)
    }
    /// Barras superior e inferior (sin color: inverso, para que se distingan igual).
    pub fn bar(&self) -> Style {
        if self.use_color {
            Style::new().bg(self.bar_bg).fg(self.bar_fg)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }
    /// Tarjeta de un mensaje del usuario.
    pub fn bubble(&self) -> Style {
        if self.use_color {
            Style::new().bg(self.bubble_bg).fg(self.bubble_fg)
        } else {
            Style::new().add_modifier(Modifier::BOLD)
        }
    }
    /// Código.
    pub fn code(&self) -> Style {
        if self.use_color {
            Style::new().bg(self.code_bg)
        } else {
            Style::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_no_color_quita_colores_pero_no_negrita() {
        let t = Theme::by_name("oscuro", true).unwrap();
        assert_eq!(t.strong().fg, None);
        assert!(t.strong().add_modifier.contains(Modifier::BOLD));
        assert!(Theme::by_name("inexistente", false).is_none());
        assert!(Theme::by_name("alto-contraste", false).unwrap().use_color);
        assert_eq!(
            Theme::by_name("azul", false).unwrap().bar().bg,
            Some(Color::Rgb(11, 87, 208))
        );
        assert!(t.bar().add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn theme_opencode_existe_y_tiene_su_acento() {
        let t = Theme::by_name("opencode", false).unwrap();
        assert_eq!(t.accent, Color::Rgb(250, 178, 131));
    }

    #[test]
    fn theme_todos_los_nombres_existen() {
        for n in Theme::NAMES {
            assert!(Theme::by_name(n, false).is_some(), "falta el tema {n}");
        }
    }

    #[test]
    fn theme_next_recorre_y_vuelve_al_primero() {
        let azul = Theme::by_name("azul", false).unwrap();
        let oscuro = Theme::by_name("oscuro", false).unwrap();
        assert_eq!(azul.next(), oscuro);
        let opencode = Theme::by_name("opencode", false).unwrap();
        assert_eq!(opencode.next(), azul);
        let sin_color = Theme::by_name("opencode", true).unwrap().next();
        assert_eq!(sin_color, Theme::by_name("azul", true).unwrap());
        assert!(!sin_color.use_color);
    }
}
