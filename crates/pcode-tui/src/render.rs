//! Dibujo de la TUI: transcripción, entrada, barra de estado y ventana de aprobación.
use crate::app::{App, Entry, ToolState};
use crate::i18n::t;
use crate::markdown::{render_markdown_lines, wrap};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];
/// Líneas de salida de una tool que se muestran plegadas.
const FOLDED_LINES: usize = 3;
/// Líneas de salida de una tool que se muestran desplegadas.
const EXPANDED_LINES: usize = 40;

/// Líneas de la transcripción, ya ajustadas a `width`.
pub fn transcript_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let th = &app.theme;
    let mut out: Vec<Line<'static>> = vec![];
    for e in &app.entries {
        match e {
            Entry::User(text) => {
                for (i, l) in text.lines().enumerate() {
                    let prefix = if i == 0 { "› " } else { "  " };
                    out.extend(wrap(
                        Line::from(Span::styled(format!("{prefix}{l}"), th.strong())),
                        width,
                    ));
                }
            }
            Entry::Assistant { text, .. } => out.extend(render_markdown_lines(text, width, th)),
            Entry::Tool {
                name,
                brief,
                state,
                output,
            } => {
                let (sym, st) = match state {
                    ToolState::Running => (SPINNER[app.spinner % 4], th.warning()),
                    ToolState::Ok => ("✓", th.good()),
                    ToolState::Err => ("✗", th.bad()),
                };
                out.extend(wrap(
                    Line::from(vec![
                        Span::styled(format!("{sym} {name} "), st),
                        Span::styled(brief.clone(), th.dimmed()),
                    ]),
                    width,
                ));
                if *state != ToolState::Running {
                    let lines: Vec<&str> = output.lines().collect();
                    let max = if app.expand_tools || *state == ToolState::Err {
                        EXPANDED_LINES
                    } else {
                        FOLDED_LINES
                    };
                    let style = if *state == ToolState::Err {
                        th.bad()
                    } else {
                        th.dimmed()
                    };
                    if lines.is_empty() {
                        out.push(Line::from(Span::styled(
                            format!("  ⎿ {}", t(app.lang, "no_output")),
                            th.dimmed(),
                        )));
                    }
                    for (i, l) in lines.iter().take(max).enumerate() {
                        let p = if i == 0 { "  ⎿ " } else { "    " };
                        out.extend(wrap(
                            Line::from(Span::styled(format!("{p}{l}"), style)),
                            width,
                        ));
                    }
                    if lines.len() > max {
                        out.push(Line::from(Span::styled(
                            format!("    … +{} {}", lines.len() - max, t(app.lang, "lines")),
                            th.dimmed(),
                        )));
                    }
                }
            }
            Entry::Info(text) => {
                for l in text.lines() {
                    out.extend(wrap(
                        Line::from(Span::styled(l.to_string(), th.dimmed())),
                        width,
                    ));
                }
            }
            Entry::Error(text) => {
                out.extend(wrap(
                    Line::from(Span::styled(text.clone(), th.bad())),
                    width,
                ));
            }
        }
        out.push(Line::default());
    }
    out
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Dibuja toda la TUI. No tiene efectos fuera del `Frame`.
pub fn render(app: &App, f: &mut Frame) {
    let th = &app.theme;
    let area = f.area();
    let inner_w = area.width.saturating_sub(2).max(1);
    let input_lines: Vec<Line> = app
        .input
        .split('\n')
        .flat_map(|l| wrap(Line::from(l.to_string()), inner_w))
        .collect();
    let input_h = (input_lines.len() as u16).clamp(1, 6) + 2;
    let [body, input_area, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(input_h),
        Constraint::Length(1),
    ])
    .areas(area);

    // Transcripción: se pinta desde el final, desplazada `scroll` líneas hacia arriba.
    let lines = transcript_lines(app, body.width.max(1));
    let total = lines.len() as u16;
    let max_scroll = total.saturating_sub(body.height);
    let offset = max_scroll.saturating_sub(app.scroll.min(max_scroll));
    f.render_widget(Paragraph::new(lines).scroll((offset, 0)), body);

    // Entrada.
    let title = if app.busy {
        format!(
            " {} {}",
            SPINNER[app.spinner % 4],
            t(app.lang, "busy").trim_start_matches(" ⏳ ")
        )
    } else {
        t(app.lang, "input").to_string()
    };
    let block = Block::bordered()
        .title(Span::styled(
            title,
            if app.busy { th.warning() } else { th.dimmed() },
        ))
        .border_style(th.dimmed());
    let visible = (input_h - 2) as usize;
    let skip = input_lines.len().saturating_sub(visible);
    let shown: Vec<Line> = input_lines.into_iter().skip(skip).collect();
    let (cx, cy) = (
        shown.last().map(|l| l.width() as u16).unwrap_or(0),
        shown.len().saturating_sub(1) as u16,
    );
    f.render_widget(Paragraph::new(shown).block(block), input_area);
    if app.approval.is_none() {
        f.set_cursor_position((
            (input_area.x + 1 + cx).min(input_area.right().saturating_sub(2)),
            input_area.y + 1 + cy,
        ));
    }

    // Barra de estado.
    let state = if app.busy {
        format!("{} {}", SPINNER[app.spinner % 4], t(app.lang, "working"))
    } else {
        t(app.lang, "ready").to_string()
    };
    let left = format!(
        " {state} · {} · ↑{} ↓{} {} ",
        app.model,
        app.usage.input_tokens,
        app.usage.output_tokens,
        t(app.lang, "tokens")
    );
    let right = t(app.lang, "hint");
    let pad =
        (status.width as usize).saturating_sub(left.chars().count() + right.chars().count() + 1);
    let bar = if pad > 0 {
        format!("{left}{}{right} ", " ".repeat(pad))
    } else {
        left
    };
    f.render_widget(Paragraph::new(Span::styled(bar, th.dimmed())), status);

    // Aprobación.
    if let Some((action, reason)) = &app.approval {
        let w = body.width.saturating_sub(4).clamp(20, 76);
        let text_w = w.saturating_sub(2);
        let mut lines = vec![Line::from(Span::styled(reason.clone(), th.warning()))];
        lines.extend(wrap(Line::from(action.clone()), text_w).into_iter().take(6));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            t(app.lang, "approval_keys"),
            th.strong(),
        )));
        let h = lines.len() as u16 + 2;
        let r = centered(body, w, h);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .title(Span::styled(t(app.lang, "approval"), th.warning()))
                    .border_style(th.warning()),
            ),
            r,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Event;
    use crate::i18n::Lang;
    use crate::theme::Theme;
    use pcode_core::AgentEvent;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn screen(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(app, f)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    fn sample() -> App {
        let mut a = App::new(
            Lang::Es,
            Theme::by_name("oscuro", true).unwrap(),
            "claude-x",
        );
        a.entries.clear();
        a.entries.push(Entry::User("arregla el test".into()));
        a.update(Event::Agent(AgentEvent::TextDelta(
            "Miro **el** test.".into(),
        )));
        a.update(Event::Agent(AgentEvent::ToolCall {
            name: "bash".into(),
            arguments: serde_json::json!({"command": "cargo test"}),
        }));
        a.update(Event::Agent(AgentEvent::ToolResult {
            name: "bash".into(),
            is_error: false,
            content: "l1\nl2\nl3\nl4\nl5".into(),
        }));
        a
    }

    #[test]
    fn render_80x24_transcripcion_entrada_y_estado() {
        let s = screen(&sample(), 80, 24);
        assert_eq!(s[0], "› arregla el test");
        assert_eq!(s[2], "Miro el test.");
        assert_eq!(s[4], "✓ bash cargo test");
        assert_eq!(s[5], "  ⎿ l1");
        assert_eq!(s[7], "    l3");
        assert_eq!(s[8], "    … +2 líneas");
        assert!(s[20].starts_with("┌ Mensaje"), "{}", s[20]);
        assert!(
            s[23].starts_with(" ✓ listo · claude-x · ↑0 ↓0 tokens"),
            "{}",
            s[23]
        );
    }

    #[test]
    fn render_120x40_despliega_salidas_y_muestra_aprobacion() {
        let mut a = sample();
        a.expand_tools = true;
        a.update(Event::Approval {
            action: "bash git push --force".into(),
            reason: "acción irreversible".into(),
        });
        let s = screen(&a, 120, 40);
        assert_eq!(s[9], "    l5", "con Ctrl+O se ve toda la salida");
        let all = s.join("\n");
        assert!(all.contains("⚠ Requiere aprobación"));
        assert!(all.contains("bash git push --force"));
        assert!(all.contains("[s] sí"));
        assert!(
            s[39].contains("PgUp/PgDn"),
            "a 120 columnas cabe la ayuda: {}",
            s[39]
        );
    }

    #[test]
    fn render_sigue_el_final_y_scroll_sube() {
        let mut a = App::new(Lang::Es, Theme::by_name("oscuro", true).unwrap(), "m");
        a.entries.clear();
        for i in 0..50 {
            a.entries.push(Entry::Info(format!("linea {i}")));
        }
        let s = screen(&a, 80, 24);
        assert_eq!(s[19], "", "tras cada entrada hay una línea en blanco");
        assert_eq!(s[18], "linea 49");
        a.scroll = 10;
        let s = screen(&a, 80, 24);
        assert_eq!(s[18], "linea 44");
    }

    #[test]
    fn render_entrada_multilinea_crece() {
        let mut a = App::new(Lang::Es, Theme::by_name("oscuro", true).unwrap(), "m");
        a.input = "uno\ndos\ntres".into();
        let s = screen(&a, 80, 24);
        assert!(s[18].starts_with("┌"), "{}", s[18]);
        assert!(
            s[19].starts_with("│uno  ") && s[19].ends_with('│'),
            "{}",
            s[19]
        );
        assert!(s[21].starts_with("│tres "), "{}", s[21]);
    }
}
