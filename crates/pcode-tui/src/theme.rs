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
    /// Si se usan colores.
    pub use_color: bool,
}

impl Theme {
    /// Tema por nombre: `oscuro`, `claro` o `alto-contraste`.
    pub fn by_name(name: &str, no_color: bool) -> Option<Theme> {
        let mut t = match name {
            "oscuro" | "dark" => Theme {
                fg: Color::Reset,
                dim: Color::DarkGray,
                accent: Color::Cyan,
                ok: Color::Green,
                err: Color::Red,
                warn: Color::Yellow,
                code_bg: Color::Rgb(40, 44, 52),
                use_color: true,
            },
            "claro" | "light" => Theme {
                fg: Color::Reset,
                dim: Color::Gray,
                accent: Color::Blue,
                ok: Color::Green,
                err: Color::Red,
                warn: Color::Magenta,
                code_bg: Color::Rgb(235, 235, 235),
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
                use_color: true,
            },
            _ => return None,
        };
        t.use_color = !no_color;
        Some(t)
    }

    /// Tema por defecto: `PCODE_TEMA` o `oscuro`, respetando `NO_COLOR`.
    pub fn from_env() -> Theme {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        std::env::var("PCODE_TEMA")
            .ok()
            .and_then(|n| Theme::by_name(&n, no_color))
            .unwrap_or_else(|| Theme::by_name("oscuro", no_color).unwrap())
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
    }
}
