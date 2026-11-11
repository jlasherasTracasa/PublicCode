//! Fragmentos (snippets) al estilo de VS Code: expandir marcadores y buscar por prefijo.

/// Un fragmento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// Prefijo que lo activa.
    pub prefix: String,
    /// Cuerpo con marcadores.
    pub body: String,
    /// Descripción.
    pub description: String,
}

/// Texto plano del cuerpo y dónde queda el cursor (índice de carácter): en `$1` o `${1:…}` (al
/// principio del texto por defecto), si no en `$0`, si no al final. `\$` es un `$` literal.
pub fn expand(body: &str) -> (String, usize) {
    let c: Vec<char> = body.chars().collect();
    let mut out: Vec<char> = vec![];
    let (mut first, mut zero) = (None, None);
    let mut i = 0;
    while i < c.len() {
        if c[i] == '\\' && c.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
        } else if c[i] == '$' && c.get(i + 1).is_some_and(|x| x.is_ascii_digit()) {
            let mut j = i + 1;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
            let n: String = c[i + 1..j].iter().collect();
            if n == "0" {
                zero.get_or_insert(out.len());
            } else if n == "1" {
                first.get_or_insert(out.len());
            }
            i = j;
        } else if c[i] == '$' && c.get(i + 1) == Some(&'{') {
            let mut j = i + 2;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
            let n: String = c[i + 2..j].iter().collect();
            if n == "1" {
                first.get_or_insert(out.len());
            } else if n == "0" {
                zero.get_or_insert(out.len());
            }
            let mut k = j;
            if c.get(k) == Some(&':') {
                k += 1;
            }
            let mut depth = 1;
            while k < c.len() {
                match c[k] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    ch => out.push(ch),
                }
                k += 1;
            }
            i = k + 1;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    let n = out.len();
    (out.into_iter().collect(), first.or(zero).unwrap_or(n))
}

/// Fragmentos cuyo prefijo empieza por `word` (vacío si `word` está vacía).
pub fn matching<'a>(snippets: &'a [Snippet], word: &str) -> Vec<&'a Snippet> {
    if word.is_empty() {
        return vec![];
    }
    snippets
        .iter()
        .filter(|s| s.prefix.starts_with(word))
        .collect()
}

/// La palabra justo antes del cursor (índice de carácter).
pub fn word_before(text: &str, cursor: usize) -> &str {
    let byte = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(b, _)| b);
    let start = text[..byte]
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(0, |(b, c)| b + c.len_utf8());
    &text[start..byte]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expande_marcadores() {
        assert_eq!(
            expand("print(${1:texto})$0"),
            ("print(texto)".to_string(), 6)
        );
        assert_eq!(
            expand("for ${1:i} in ${2:xs}:\n\t$0"),
            ("for i in xs:\n\t".to_string(), 4)
        );
    }

    #[test]
    fn cursor_en_el_primero() {
        assert_eq!(expand("a$0b$1c").1, 2);
        assert_eq!(expand("solo $0").1, 5);
        assert_eq!(expand("nada").1, 4);
    }

    #[test]
    fn escapes() {
        assert_eq!(expand("\\$HOME $1").0, "$HOME ");
    }

    #[test]
    fn coinciden_por_prefijo() {
        let s = vec![
            Snippet {
                prefix: "print".into(),
                body: String::new(),
                description: String::new(),
            },
            Snippet {
                prefix: "def".into(),
                body: String::new(),
                description: String::new(),
            },
        ];
        assert_eq!(matching(&s, "pr").len(), 1);
        assert!(matching(&s, "").is_empty());
        assert_eq!(word_before("x = pri", 7), "pri");
        assert_eq!(word_before("ñan", 3), "ñan");
    }
}
