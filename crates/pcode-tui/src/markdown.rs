//! Markdown mínimo (títulos, listas, `código`, **negrita** y bloques de código) y ajuste de
//! líneas al ancho, sin dependencias.
use crate::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Trocea `text` en spans con `**negrita**` y `` `código` ``.
fn inline(text: &str, base: Style, th: &Theme) -> Vec<Span<'static>> {
    let mut out = vec![];
    let mut cur = String::new();
    let (mut bold, mut code) = (false, false);
    let style = |b: bool, c: bool| {
        let mut s = if c { th.code().patch(base) } else { base };
        if b {
            s = s.add_modifier(Modifier::BOLD);
        }
        s
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' {
            out.push(Span::styled(std::mem::take(&mut cur), style(bold, code)));
            code = !code;
        } else if ch == '*' && !code && chars.peek() == Some(&'*') {
            chars.next();
            out.push(Span::styled(std::mem::take(&mut cur), style(bold, code)));
            bold = !bold;
        } else {
            cur.push(ch);
        }
    }
    out.push(Span::styled(cur, style(bold, code)));
    out.retain(|s| !s.content.is_empty());
    out
}

/// Convierte Markdown en líneas con estilo, ya ajustadas a `width` columnas.
pub fn render_markdown_lines(text: &str, width: u16, th: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut in_code = false;
    for raw in text.lines() {
        if raw.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            let l = Line::from(Span::styled(format!("  {raw}"), th.code().patch(th.text())));
            lines.extend(wrap(l, width));
            continue;
        }
        let trimmed = raw.trim_start();
        let line = if let Some(h) = trimmed.strip_prefix('#') {
            Line::from(Span::styled(
                h.trim_start_matches('#').trim().to_string(),
                th.strong(),
            ))
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let indent = " ".repeat(raw.len() - trimmed.len());
            let mut spans = vec![Span::raw(format!("{indent}• "))];
            spans.extend(inline(item, th.text(), th));
            Line::from(spans)
        } else {
            Line::from(inline(raw, th.text(), th))
        };
        lines.extend(wrap(line, width));
    }
    lines
}

/// Parte una línea en varias de como mucho `width` caracteres, conservando los estilos.
/// Corta por espacios cuando puede.
pub fn wrap(line: Line<'static>, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let cells: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|s| {
            let st = line.style.patch(s.style);
            s.content.chars().map(move |c| (c, st)).collect::<Vec<_>>()
        })
        .collect();
    if cells.len() <= width {
        return vec![line];
    }
    let mut out = vec![];
    let mut start = 0;
    while start < cells.len() {
        let mut end = (start + width).min(cells.len());
        if end < cells.len()
            && let Some(sp) = cells[start..end].iter().rposition(|(c, _)| *c == ' ')
            && sp > 0
        {
            end = start + sp + 1;
        }
        out.push(to_line(&cells[start..end]));
        start = end;
    }
    out
}

fn to_line(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![];
    for (c, st) in cells {
        match spans.last_mut() {
            Some(last) if last.style == *st => last.content.to_mut().push(*c),
            _ => spans.push(Span::styled(c.to_string(), *st)),
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn markdown_titulos_listas_codigo_y_negrita() {
        let th = Theme::by_name("oscuro", false).unwrap();
        let ls = render_markdown_lines(
            "# Título\n- uno **dos**\n```\nlet x = 1;\n```\nusa `cargo`",
            80,
            &th,
        );
        let txt: Vec<String> = ls.iter().map(plain).collect();
        assert_eq!(
            txt,
            vec!["Título", "• uno dos", "  let x = 1;", "usa cargo"]
        );
        assert!(
            ls[1]
                .spans
                .iter()
                .any(|s| s.content == "dos" && s.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(ls[2].spans[0].style.bg.is_some(), "el código tiene fondo");
    }

    #[test]
    fn wrap_corta_por_espacios_y_conserva_todo_el_texto() {
        let l = Line::from("uno dos tres cuatro cinco");
        let w = wrap(l, 10);
        assert!(w.iter().all(|l| plain(l).chars().count() <= 10));
        let joined: String = w.iter().map(plain).collect();
        assert_eq!(joined, "uno dos tres cuatro cinco");
        assert_eq!(plain(&w[0]), "uno dos ");
    }
}
