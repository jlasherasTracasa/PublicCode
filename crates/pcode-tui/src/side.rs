//! Barra lateral de la TUI: contenido (sesión, contexto y archivos tocados) como lógica pura.
//! Todo sale de [`App`]; no hay E/S aquí. Dibujarla en pantalla es cosa de `render`.
use crate::app::{App, human_tokens};
use crate::i18n::t;
use pcode_core::FileChange;
use ratatui::text::Line;

/// Ancho de la barra lateral, con su borde.
pub const SIDE_WIDTH: u16 = 30;

/// Si la barra lateral se dibuja: está pedida y la terminal mide al menos 70 columnas.
pub fn side_visible(show: bool, width: u16) -> bool {
    show && width >= 70
}

/// Acumula en `all` los cambios de un turno (sustituye por ruta o añade).
pub fn merge_changes(all: &mut Vec<FileChange>, turn: &[FileChange]) {
    for c in turn {
        if let Some(slot) = all.iter_mut().find(|a| a.path == c.path) {
            *slot = c.clone();
        } else {
            all.push(c.clone());
        }
    }
}

/// Recorta `s` a `max` caracteres (por carácter, no por byte).
fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Fila de un archivo tocado, de `width` caracteres como mucho.
pub fn change_row(c: &FileChange, width: u16) -> String {
    let symbol = if c.created {
        'A'
    } else if c.deleted {
        'D'
    } else {
        'M'
    };
    let mut tail = String::new();
    if c.added > 0 {
        tail.push_str(&format!(" +{}", c.added));
    }
    if c.removed > 0 {
        tail.push_str(&format!(" \u{2212}{}", c.removed));
    }
    let head = format!(" {symbol} ");
    let width = width as usize;
    let fixed = head.chars().count() + tail.chars().count();
    let path: Vec<char> = c.path.chars().collect();
    let row = if fixed + path.len() <= width {
        format!("{head}{}{tail}", c.path)
    } else {
        // Recorta la ruta por la izquierda y pone « … » delante hasta que quepa.
        let keep = width.saturating_sub(fixed + 1);
        let suffix: String = path[path.len().saturating_sub(keep)..].iter().collect();
        format!("{head}\u{2026}{suffix}{tail}")
    };
    clip(&row, width)
}

/// Líneas de la barra lateral para un ancho interior `width`.
pub fn side_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let th = &app.theme;
    let lang = app.lang;
    let w = width as usize;
    let mut out: Vec<Line<'static>> = vec![];

    out.push(Line::styled(
        format!(" {}", t(lang, "side_session")),
        th.strong(),
    ));
    let title = if app.title.is_empty() {
        t(lang, "untitled").to_string()
    } else {
        app.title.clone()
    };
    out.push(Line::styled(
        format!(" {}", clip(&title, w.saturating_sub(1))),
        th.text(),
    ));
    out.push(Line::raw(""));

    out.push(Line::styled(
        format!(" {}", t(lang, "side_context")),
        th.strong(),
    ));
    if app.context_limit > 0 {
        out.push(Line::styled(
            format!(
                " {} / {} ({} %)",
                human_tokens(app.context_used),
                human_tokens(app.context_limit),
                app.context_percent()
            ),
            th.text(),
        ));
    }
    out.push(Line::styled(
        format!(
            " \u{2191}{} \u{2193}{} {}",
            human_tokens(app.usage.input_tokens),
            human_tokens(app.usage.output_tokens),
            t(lang, "tokens")
        ),
        th.text(),
    ));
    out.push(Line::raw(""));

    let n = app.changes.len();
    out.push(Line::styled(
        format!(" {} ({n})", t(lang, "side_changes")),
        th.strong(),
    ));
    if n == 0 {
        out.push(Line::styled(format!(" {}", t(lang, "none")), th.dimmed()));
    } else {
        for c in app.changes.iter().take(10) {
            let style = if c.created {
                th.good()
            } else if c.deleted {
                th.bad()
            } else {
                th.text()
            };
            out.push(Line::styled(change_row(c, width), style));
        }
        if n > 10 {
            out.push(Line::styled(format!(" \u{2026} +{}", n - 10), th.dimmed()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::theme::Theme;

    fn app() -> App {
        App::new(Lang::Es, Theme::by_name("oscuro", true).unwrap(), "m")
    }

    fn fc(path: &str, added: usize, removed: usize, created: bool, deleted: bool) -> FileChange {
        FileChange {
            path: path.into(),
            added,
            removed,
            created,
            deleted,
        }
    }

    fn texts(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn fila_de_cambio_con_simbolo_y_contadores() {
        assert_eq!(
            change_row(&fc("src/a.rs", 3, 1, false, false), 29),
            " M src/a.rs +3 \u{2212}1"
        );
        assert_eq!(change_row(&fc("b.rs", 2, 0, true, false), 29), " A b.rs +2");
        assert!(change_row(&fc("c.rs", 0, 4, false, true), 29).starts_with(" D "));
    }

    #[test]
    fn fila_larga_recorta_la_ruta_por_la_izquierda() {
        let row = change_row(
            &fc("crates/pcode-tui/src/render.rs", 10, 2, false, false),
            20,
        );
        assert_eq!(row.chars().count(), 20);
        assert!(row.contains('\u{2026}'));
        assert!(row.ends_with(" +10 \u{2212}2"));
    }

    #[test]
    fn merge_sustituye_por_ruta_y_anade() {
        let mut all = vec![fc("a.rs", 1, 0, false, false)];
        let turn = vec![
            fc("a.rs", 5, 2, false, false),
            fc("b.rs", 1, 0, true, false),
        ];
        merge_changes(&mut all, &turn);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], fc("a.rs", 5, 2, false, false));
        assert_eq!(all[1].path, "b.rs");
    }

    #[test]
    fn lineas_sin_datos() {
        let lines = side_lines(&app(), 29);
        assert_eq!(
            texts(&lines),
            vec![
                " Sesión",
                " (sin título)",
                "",
                " Contexto",
                " ↑0 ↓0 tokens",
                "",
                " Cambios (0)",
                " (ninguno)",
            ]
        );
    }

    #[test]
    fn lineas_con_contexto_y_mas_de_diez_cambios() {
        let mut a = app();
        a.context_limit = 160_000;
        a.context_used = 40_000;
        a.changes = (0..12)
            .map(|i| fc(&format!("f{i}.rs"), 1, 0, false, false))
            .collect();
        let lines = texts(&side_lines(&a, 29));
        assert!(lines.iter().any(|l| l == " 40k / 160k (25 %)"));
        assert_eq!(lines.len(), 19);
        assert_eq!(lines[18], " … +2");
        assert_eq!(lines.iter().filter(|l| l.starts_with(" M f")).count(), 10);
    }

    #[test]
    fn visible_solo_si_se_pide_y_cabe() {
        assert!(side_visible(true, 80));
        assert!(!side_visible(true, 69));
        assert!(!side_visible(false, 200));
    }
}
