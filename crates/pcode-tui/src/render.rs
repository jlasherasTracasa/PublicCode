//! Dibujo de la TUI: transcripción, entrada, barra de estado y ventana de aprobación.
use crate::app::{App, Entry, ToolState};
use crate::i18n::t;
use crate::markdown::{render_markdown_lines, wrap};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

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
                // Tarjeta: fondo a todo el ancho, una línea de margen arriba y abajo y una barra
                // de acento a la izquierda.
                let card = th.bubble();
                let pad = |l: Line<'static>| -> Line<'static> {
                    let used = l.width() as u16;
                    let mut l = l;
                    l.spans.push(Span::styled(
                        " ".repeat(width.saturating_sub(used) as usize),
                        card,
                    ));
                    l
                };
                let bar = Span::styled(
                    "▌",
                    card.patch(th.strong()).bg(if th.use_color {
                        th.bubble_bg
                    } else {
                        ratatui::style::Color::Reset
                    }),
                );
                out.push(pad(Line::from(vec![bar.clone()])));
                for l in text.lines() {
                    for w in wrap(
                        Line::from(Span::styled(l.to_string(), card)),
                        width.saturating_sub(3).max(1),
                    ) {
                        let mut spans = vec![bar.clone(), Span::styled(" ", card)];
                        spans.extend(w.spans);
                        out.push(pad(Line::from(spans)));
                    }
                }
                out.push(pad(Line::from(vec![bar])));
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
            Entry::Welcome => {
                // Tarjeta con borde redondeado: logo, nombre, modelo y consejos.
                let w = width.clamp(20, 72) as usize;
                let inner = w.saturating_sub(4);
                let border = th.strong();
                let row = |spans: Vec<Span<'static>>| -> Line<'static> {
                    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                    let mut v = vec![Span::styled("│ ", border)];
                    v.extend(spans);
                    v.push(Span::raw(" ".repeat(inner.saturating_sub(used))));
                    v.push(Span::styled(" │", border));
                    Line::from(v)
                };
                out.push(Line::from(Span::styled(
                    format!("╭{}╮", "─".repeat(w - 2)),
                    border,
                )));
                out.push(row(vec![
                    Span::styled(
                        " >_ ",
                        th.bar().add_modifier(ratatui::style::Modifier::BOLD),
                    ),
                    Span::styled("  PublicCode", th.strong()),
                    Span::styled(
                        format!("  ·  {}", app.model)
                            .chars()
                            .take(inner.saturating_sub(16))
                            .collect::<String>(),
                        th.dimmed(),
                    ),
                ]));
                out.push(row(vec![]));
                for l in t(app.lang, "welcome")
                    .lines()
                    .chain(t(app.lang, "tips").lines())
                {
                    let cut: String = l.chars().take(inner).collect();
                    out.push(row(vec![Span::styled(cut, th.text())]));
                }
                out.push(Line::from(Span::styled(
                    format!("╰{}╯", "─".repeat(w - 2)),
                    border,
                )));
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
    let [header, body, input_area, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(input_h),
        Constraint::Length(1),
    ])
    .areas(area);

    // Barra superior: logo, nombre y modelo a la izquierda; estado a la derecha.
    let state = if app.busy {
        format!("{} {}", SPINNER[app.spinner % 4], t(app.lang, "working"))
    } else {
        t(app.lang, "ready").to_string()
    };
    let left = format!(" >_ PublicCode  ·  {}", app.model);
    let mode = if app.plan {
        format!(" ◆ {} ", t(app.lang, "plan"))
    } else {
        String::new()
    };
    let right = format!("{state} ");
    let gap = (header.width as usize)
        .saturating_sub(left.chars().count() + mode.chars().count() + 1 + right.chars().count());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, th.bar().add_modifier(ratatui::style::Modifier::BOLD)),
            Span::styled(" ", th.bar()),
            Span::styled(
                mode,
                if th.use_color {
                    ratatui::style::Style::new()
                        .bg(th.warn)
                        .fg(ratatui::style::Color::Black)
                        .add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    th.bar().add_modifier(ratatui::style::Modifier::BOLD)
                },
            ),
            Span::styled(" ".repeat(gap), th.bar()),
            Span::styled(right, th.bar()),
        ]))
        .style(th.bar()),
        header,
    );

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
    let accent = if app.busy { th.warning() } else { th.strong() };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(title, accent))
        .border_style(accent);
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

    // Sugerencias de `@archivo`, encima de la entrada.
    if !app.suggestions.is_empty() && app.approval.is_none() {
        let n = app.suggestions.len() as u16;
        let h = (n + 2).min(input_area.y.saturating_sub(body.y));
        let w = body.width.saturating_sub(2).clamp(10, 70);
        if h >= 3 {
            let r = Rect::new(input_area.x + 1, input_area.y - h, w, h);
            let items: Vec<Line> = app
                .suggestions
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let st = if i == app.suggestion {
                        th.bubble()
                    } else {
                        th.text()
                    };
                    let sym = if i == app.suggestion { "› " } else { "  " };
                    Line::from(Span::styled(format!("{sym}{f}"), st))
                })
                .collect();
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(items).block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .title(Span::styled(" @ ", th.strong()))
                        .border_style(th.strong()),
                ),
                r,
            );
        }
    }

    // Barra de estado: tokens y atajos.
    let left = format!(
        " ↑{} ↓{} {} ",
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
            Paragraph::new(lines)
                .block(
                    Block::bordered()
                        .border_type(BorderType::Rounded)
                        .title(Span::styled(t(app.lang, "approval"), th.warning()))
                        .border_style(th.warning()),
                )
                .style(if th.use_color {
                    ratatui::style::Style::new()
                        .bg(th.bubble_bg)
                        .fg(th.bubble_fg)
                } else {
                    ratatui::style::Style::new()
                }),
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
        assert!(
            s[0].starts_with(" >_ PublicCode  ·  claude-x") && s[0].ends_with("✓ listo"),
            "{}",
            s[0]
        );
        assert_eq!(s[1], "▌", "la tarjeta del usuario tiene margen arriba");
        assert_eq!(s[2], "▌ arregla el test");
        assert_eq!(s[3], "▌");
        assert_eq!(s[5], "Miro el test.");
        assert_eq!(s[7], "✓ bash cargo test");
        assert_eq!(s[8], "  ⎿ l1");
        assert_eq!(s[10], "    l3");
        assert_eq!(s[11], "    … +2 líneas");
        assert!(s[20].starts_with("╭ Mensaje"), "{}", s[20]);
        assert!(s[22].starts_with("╰"), "{}", s[22]);
        assert!(s[23].starts_with(" ↑0 ↓0 tokens"), "{}", s[23]);
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
        assert_eq!(s[12], "    l5", "con Ctrl+O se ve toda la salida");
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
        assert!(s[18].starts_with("╭"), "{}", s[18]);
        assert!(
            s[19].starts_with("│uno  ") && s[19].ends_with('│'),
            "{}",
            s[19]
        );
        assert!(s[21].starts_with("│tres "), "{}", s[21]);
    }

    fn css(c: ratatui::style::Color, default: &str) -> String {
        use ratatui::style::Color as C;
        match c {
            C::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
            C::Reset => default.into(),
            C::Black => "#000".into(),
            C::White => "#fff".into(),
            C::Red | C::LightRed => "#ff6b6b".into(),
            C::Green | C::LightGreen => "#6dd58c".into(),
            C::Yellow | C::LightYellow => "#f5c451".into(),
            C::Blue => "#0b57d0".into(),
            C::Cyan => "#5fd7ff".into(),
            C::DarkGray => "#777".into(),
            C::Gray => "#aaa".into(),
            _ => default.into(),
        }
    }

    /// Vuelca la pantalla a HTML con sus colores (para ver la TUI sin terminal).
    fn to_html(app: &App, w: u16, h: u16, bg: &str, fg: &str) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(app, f)).unwrap();
        let buf = term.backend().buffer().clone();
        let mut out = format!(
            "<pre style=\"margin:0;padding:12px;background:{bg};color:{fg};font:15px/1.35 'DejaVu Sans Mono',monospace\">"
        );
        for y in 0..h {
            for x in 0..w {
                let c = &buf[(x, y)];
                let st = c.style();
                let bold = if st.add_modifier.contains(ratatui::style::Modifier::BOLD) {
                    "font-weight:bold;"
                } else {
                    ""
                };
                let sym = c.symbol().replace('&', "&amp;").replace('<', "&lt;");
                out.push_str(&format!(
                    "<span style=\"background:{};color:{};{bold}\">{sym}</span>",
                    css(st.bg.unwrap_or(ratatui::style::Color::Reset), "transparent"),
                    css(st.fg.unwrap_or(ratatui::style::Color::Reset), fg)
                ));
            }
            out.push('\n');
        }
        out.push_str("</pre>");
        out
    }

    #[test]
    fn render_bienvenida_en_tarjeta() {
        let a = App::new(Lang::Es, Theme::by_name("azul", false).unwrap(), "claude-x");
        let s = screen(&a, 80, 24);
        assert!(s[1].starts_with("╭──") && s[1].ends_with('╮'), "{}", s[1]);
        assert!(
            s[2].contains(">_") && s[2].contains("PublicCode") && s[2].contains("claude-x"),
            "{}",
            s[2]
        );
        assert!(s.iter().any(|l| l.contains("@archivo adjunta")));
        let w = s[1].chars().count();
        assert!(
            s[1..8]
                .iter()
                .filter(|l| l.starts_with('│'))
                .all(|l| l.chars().count() == w),
            "bordes alineados"
        );
    }

    #[test]
    fn render_modo_plan_y_sugerencias() {
        let mut a = App::new(Lang::Es, Theme::by_name("azul", false).unwrap(), "m");
        a.plan = true;
        a.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
        a.input = "mira @ma".into();
        a.suggestions = vec!["src/main.rs".into()];
        let s = screen(&a, 80, 24);
        assert!(s[0].contains("◆ PLAN"), "{}", s[0]);
        let all = s.join("\n");
        assert!(all.contains("› src/main.rs"), "{all}");
        assert!(all.contains("╭ @ "));
    }

    #[test]
    fn render_vista_previa_html() {
        let mut a = sample();
        a.theme = Theme::by_name("azul", false).unwrap();
        a.model = "claude-sonnet-5-5".into();
        a.update(Event::Agent(AgentEvent::TextDelta(
            "Listo: he corregido `parse()` y el test pasa.\n\n- **1 archivo** cambiado\n- `cargo test` en verde".into(),
        )));
        a.usage.input_tokens = 3120;
        a.usage.output_tokens = 410;
        a.input = "ahora documenta el módulo".into();
        let html = to_html(&a, 100, 30, "#0b1626", "#e3ecfb");
        assert!(
            html.contains("rgb(11,87,208)"),
            "la barra usa el azul del tema"
        );
        let mut b = a.clone();
        b.update(Event::Approval {
            action: "bash {\"command\":\"git push origin main\"}".into(),
            reason: "orden con efectos difíciles de deshacer: git push origin main".into(),
        });
        let mut c = a.clone();
        c.theme = Theme::by_name("claro", false).unwrap();
        let mut ini = App::new(
            Lang::Es,
            Theme::by_name("azul", false).unwrap(),
            "claude-sonnet-5-5",
        );
        ini.input = "explica @src/main.rs".into();
        ini.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
        if let Ok(dir) = std::env::var("PCODE_TUI_PREVIEW") {
            for (name, app, bg, fg) in [
                ("azul", &a, "#0b1626", "#e3ecfb"),
                ("aprobacion", &b, "#0b1626", "#e3ecfb"),
                ("claro", &c, "#f7faff", "#0b1b33"),
                ("inicio", &ini, "#0b1626", "#e3ecfb"),
            ] {
                std::fs::write(
                    format!("{dir}/tui-{name}.html"),
                    format!("<!doctype html><meta charset=utf-8><body style=\"margin:0;background:{bg}\">{}", to_html(app, 100, 30, bg, fg)),
                )
                .unwrap();
            }
        }
    }
}
