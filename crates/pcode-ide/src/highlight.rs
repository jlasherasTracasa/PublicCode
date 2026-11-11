//! Resaltado de sintaxis ligero (sin gramáticas ni dependencias): palabras clave, cadenas,
//! comentarios y números. Se calcula solo cuando cambia el texto (ver `app::Highlighter`).
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId};
use std::ops::Range;

/// Lenguaje de un archivo (para el resaltado).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// Rust.
    Rust,
    /// Python.
    Python,
    /// JavaScript y TypeScript.
    JsTs,
    /// C, C++, Go y Java (comentarios `//` y `/* */`).
    CLike,
    /// TOML, YAML, shell y archivos con comentarios `#`.
    Hash,
    /// Texto sin resaltar.
    Plain,
}

/// Lenguaje según la extensión.
pub fn lang_of(path: &str) -> Lang {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("rs") => Lang::Rust,
        Some("py" | "pyi") => Lang::Python,
        Some("js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts") => Lang::JsTs,
        Some("c" | "h" | "cc" | "cpp" | "hpp" | "go" | "java" | "kt" | "cs") => Lang::CLike,
        Some("toml" | "yaml" | "yml" | "sh" | "bash" | "ini" | "cfg" | "conf") => Lang::Hash,
        _ if name == "Makefile" || name == "Dockerfile" => Lang::Hash,
        _ => Lang::Plain,
    }
}

/// Tipo de cada trozo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Texto normal.
    Plain,
    /// Palabra clave.
    Keyword,
    /// Cadena.
    Str,
    /// Comentario.
    Comment,
    /// Número.
    Number,
}

fn keywords(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        Lang::Python => &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
            "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return",
            "True", "try", "while", "with", "yield",
        ],
        Lang::JsTs => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "from",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "interface",
            "let",
            "new",
            "null",
            "return",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "type",
            "typeof",
            "undefined",
            "var",
            "void",
            "while",
            "yield",
        ],
        Lang::CLike => &[
            "break",
            "case",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "do",
            "double",
            "else",
            "enum",
            "false",
            "float",
            "for",
            "func",
            "go",
            "if",
            "import",
            "int",
            "long",
            "namespace",
            "new",
            "nil",
            "package",
            "private",
            "public",
            "return",
            "static",
            "struct",
            "switch",
            "true",
            "type",
            "var",
            "void",
            "while",
        ],
        Lang::Hash => &[
            "true", "false", "if", "then", "fi", "for", "do", "done", "export",
        ],
        Lang::Plain => &[],
    }
}

/// Trozos que cubren todo `text`, en orden, con su tipo. Los rangos son de bytes y siempre caen en
/// límites de carácter.
pub fn tokens(text: &str, lang: Lang) -> Vec<(Range<usize>, Kind)> {
    let mut out: Vec<(Range<usize>, Kind)> = vec![];
    let push = |out: &mut Vec<(Range<usize>, Kind)>, r: Range<usize>, k: Kind| {
        if r.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, lk)) if *lk == k && last.end == r.start => last.end = r.end,
            _ => out.push((r, k)),
        }
    };
    if lang == Lang::Plain {
        push(&mut out, 0..text.len(), Kind::Plain);
        return out;
    }
    let slash = matches!(lang, Lang::Rust | Lang::JsTs | Lang::CLike);
    let hash = matches!(lang, Lang::Python | Lang::Hash);
    let kw = keywords(lang);
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let rest = &text[i..];
        let c = rest.chars().next().unwrap_or(' ');
        let end_of_line = |from: usize| text[from..].find('\n').map_or(text.len(), |n| from + n);
        if (slash && rest.starts_with("//")) || (hash && c == '#') {
            let e = end_of_line(i);
            push(&mut out, i..e, Kind::Comment);
            i = e;
        } else if slash && rest.starts_with("/*") {
            let e = rest[2..].find("*/").map_or(text.len(), |n| i + 2 + n + 2);
            push(&mut out, i..e, Kind::Comment);
            i = e;
        } else if c == '"' || (c == '\'' && lang != Lang::Rust) || (c == '`' && lang == Lang::JsTs)
        {
            // Hasta la comilla de cierre sin escapar (o el final de la línea, salvo en `…`).
            let mut j = i + 1;
            let mut esc = false;
            let mut e = text.len();
            for (k, ch) in text[j..].char_indices() {
                if esc {
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if ch == c {
                    e = j + k + 1;
                    break;
                } else if ch == '\n' && c != '`' && lang != Lang::Python {
                    e = j + k;
                    break;
                }
            }
            j = e;
            push(&mut out, i..j, Kind::Str);
            i = j;
        } else if c.is_ascii_digit() {
            let e = rest
                .char_indices()
                .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.'))
                .map_or(text.len(), |(n, _)| i + n);
            push(&mut out, i..e, Kind::Number);
            i = e;
        } else if c.is_alphabetic() || c == '_' {
            let e = rest
                .char_indices()
                .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
                .map_or(text.len(), |(n, _)| i + n);
            let k = if kw.contains(&&text[i..e]) {
                Kind::Keyword
            } else {
                Kind::Plain
            };
            push(&mut out, i..e, k);
            i = e;
        } else {
            let e = i + c.len_utf8();
            push(&mut out, i..e, Kind::Plain);
            i = e;
        }
    }
    out
}

/// Color de cada tipo, como los temas Dark+ y Light+ de VS Code.
pub fn color(kind: Kind, dark: bool) -> Color32 {
    let rgb = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    rgb(match (kind, dark) {
        (Kind::Plain, true) => 0xd4d4d4,
        (Kind::Plain, false) => 0x1f1f1f,
        (Kind::Keyword, true) => 0x569cd6,
        (Kind::Keyword, false) => 0x0000ff,
        (Kind::Str, true) => 0xce9178,
        (Kind::Str, false) => 0xa31515,
        (Kind::Comment, true) => 0x6a9955,
        (Kind::Comment, false) => 0x008000,
        (Kind::Number, true) => 0xb5cea8,
        (Kind::Number, false) => 0x098658,
    })
}

/// Texto resaltado listo para el editor.
pub fn layout_job(text: &str, lang: Lang, dark: bool, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (r, k) in tokens(text, lang) {
        job.append(
            &text[r],
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: color(k, dark),
                ..Default::default()
            },
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, lang: Lang) -> Vec<(&str, Kind)> {
        tokens(text, lang)
            .into_iter()
            .filter(|(_, k)| *k != Kind::Plain)
            .map(|(r, k)| (&text[r], k))
            .collect()
    }

    #[test]
    fn rust_basico() {
        let src = "fn main() { let x = 42; // hola\n println!(\"a\\\"b\"); }";
        assert_eq!(
            kinds(src, Lang::Rust),
            vec![
                ("fn", Kind::Keyword),
                ("let", Kind::Keyword),
                ("42", Kind::Number),
                ("// hola", Kind::Comment),
                ("\"a\\\"b\"", Kind::Str),
            ]
        );
        // Las vidas ('a) no abren cadenas en Rust.
        assert!(
            kinds("fn f<'a>(x: &'a str) {}", Lang::Rust)
                .iter()
                .all(|(_, k)| *k != Kind::Str)
        );
    }

    #[test]
    fn python_y_toml() {
        assert_eq!(
            kinds("def f():\n    return 'ñandú'  # fin", Lang::Python),
            vec![
                ("def", Kind::Keyword),
                ("return", Kind::Keyword),
                ("'ñandú'", Kind::Str),
                ("# fin", Kind::Comment)
            ]
        );
        assert_eq!(
            kinds("[a]\nb = true # c", Lang::Hash),
            vec![("true", Kind::Keyword), ("# c", Kind::Comment)]
        );
    }

    #[test]
    fn cubre_todo_el_texto_y_respeta_utf8() {
        for (src, lang) in [
            ("/* sin cerrar ñ", Lang::CLike),
            ("\"sin cerrar\nsigue", Lang::JsTs),
            ("áé 1.5e3 `x${y}`", Lang::JsTs),
            ("texto plano", Lang::Plain),
            ("", Lang::Rust),
        ] {
            let t = tokens(src, lang);
            let mut pos = 0;
            for (r, _) in &t {
                assert_eq!(r.start, pos, "{src:?}");
                assert!(src.is_char_boundary(r.end));
                pos = r.end;
            }
            assert_eq!(pos, src.len(), "{src:?}");
        }
    }

    #[test]
    fn lenguaje_por_extension() {
        assert_eq!(lang_of("src/main.rs"), Lang::Rust);
        assert_eq!(lang_of("a/b.PY"), Lang::Python);
        assert_eq!(lang_of("web/app.tsx"), Lang::JsTs);
        assert_eq!(lang_of("Makefile"), Lang::Hash);
        assert_eq!(lang_of("README"), Lang::Plain);
        assert_eq!(lang_of("notas.md"), Lang::Plain);
    }
}
