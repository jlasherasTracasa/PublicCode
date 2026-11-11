//! Buscar y reemplazar dentro de un archivo.
use std::ops::Range;

/// Rangos de bytes de cada aparición de `query`, sin solaparse y de izquierda a derecha.
pub fn find_all(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if query.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    if case_sensitive {
        let mut from = 0;
        while let Some(i) = text[from..].find(query) {
            out.push(from + i..from + i + query.len());
            from += i + query.len();
        }
        return out;
    }
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        let mut k = 0;
        while j < chars.len() && k < q.len() {
            let mut low = chars[j].1.to_lowercase();
            let ok = low.all(|c| {
                let hit = q.get(k) == Some(&c);
                k += 1;
                hit
            });
            if !ok {
                break;
            }
            j += 1;
        }
        if k == q.len() && j > i {
            let end = chars.get(j).map_or(text.len(), |c| c.0);
            out.push(chars[i].0..end);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Texto con cada aparición sustituida y cuántas hubo.
pub fn replace_all(
    text: &str,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
) -> (String, usize) {
    let hits = find_all(text, query, case_sensitive);
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for r in &hits {
        out.push_str(&text[pos..r.start]);
        out.push_str(replacement);
        pos = r.end;
    }
    out.push_str(&text[pos..]);
    (out, hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busca_todas_sin_solaparse() {
        assert_eq!(find_all("aaaa", "aa", true), vec![0..2, 2..4]);
        assert_eq!(find_all("a-b-a", "a", true), vec![0..1, 4..5]);
    }

    #[test]
    fn busca_sin_distinguir_mayusculas_con_utf8() {
        let t = "Ñandú ñANDÚ";
        let r = find_all(t, "ñandú", false);
        assert_eq!(r.len(), 2);
        assert_eq!(&t[r[1].clone()], "ñANDÚ");
        assert_eq!(find_all("Hola HOLA", "hola", true), vec![]);
    }

    #[test]
    fn reemplaza_y_cuenta() {
        assert_eq!(
            replace_all("uno dos uno", "uno", "1", true),
            ("1 dos 1".to_string(), 2)
        );
        assert_eq!(replace_all("x", "y", "z", true), ("x".to_string(), 0));
    }

    #[test]
    fn consulta_vacia() {
        assert!(find_all("abc", "", true).is_empty());
    }
}
