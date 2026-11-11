//! Tokenizador de código y ranking BM25 en memoria (lógica pura).
use std::collections::HashMap;

/// Parámetros estándar de BM25.
pub const K1: f32 = 1.2;
pub const B: f32 = 0.75;

/// Tokens de un texto con código: separa snake_case y camelCase, en minúsculas.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in text.split(|c: char| !c.is_alphanumeric()) {
        if chunk.is_empty() {
            continue;
        }
        let words = split_camel(chunk);
        for w in &words {
            push_token(&mut out, w);
        }
        if words.len() > 1 {
            push_token(&mut out, chunk);
        }
    }
    out
}

/// Añade un token en minúsculas, descartando los de menos de 2 caracteres.
fn push_token(out: &mut Vec<String>, s: &str) {
    let lower = s.to_lowercase();
    if lower.chars().count() >= 2 {
        out.push(lower);
    }
}

/// Parte un trozo alfanumérico por los cambios de caja.
fn split_camel(chunk: &str) -> Vec<String> {
    let chars: Vec<char> = chunk.chars().collect();
    let mut words = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let cut =
                prev.is_lowercase() || prev.is_numeric() || (prev.is_uppercase() && next_lower);
            if cut && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Estadísticas de un documento indexado.
#[derive(Debug, Clone, Default)]
struct DocStats {
    /// Frecuencia de cada término en el documento.
    tf: HashMap<String, u32>,
    /// Longitud del documento en tokens.
    dl: usize,
}

/// Índice BM25 en memoria sobre una lista de documentos.
#[derive(Debug, Clone, Default)]
pub struct Bm25 {
    docs: Vec<DocStats>,
    /// Número de documentos que contienen cada término.
    df: HashMap<String, usize>,
    /// Longitud media de los documentos en tokens.
    avgdl: f32,
}

impl Bm25 {
    /// Indexa los documentos (el índice de cada uno es su posición en `docs`).
    pub fn new(docs: &[String]) -> Self {
        let mut stats = Vec::with_capacity(docs.len());
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total = 0usize;
        for doc in docs {
            let tokens = tokenize(doc);
            let mut tf: HashMap<String, u32> = HashMap::new();
            for t in &tokens {
                *tf.entry(t.clone()).or_insert(0) += 1;
            }
            for term in tf.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
            total += tokens.len();
            stats.push(DocStats {
                tf,
                dl: tokens.len(),
            });
        }
        let avgdl = if docs.is_empty() {
            0.0
        } else {
            total as f32 / docs.len() as f32
        };
        Self {
            docs: stats,
            df,
            avgdl,
        }
    }

    /// Número de documentos.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Si no hay documentos.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Los `k` documentos más relevantes: (índice, puntuación), de mayor a menor.
    pub fn search(&self, query: &str, k: usize) -> Vec<(usize, f32)> {
        if self.docs.is_empty() || self.avgdl == 0.0 {
            return Vec::new();
        }
        // Términos únicos de la consulta, en orden de aparición.
        let mut terms: Vec<String> = Vec::new();
        for t in tokenize(query) {
            if !terms.contains(&t) {
                terms.push(t);
            }
        }
        let n_docs = self.docs.len() as f32;
        let idfs: Vec<f32> = terms
            .iter()
            .map(|t| {
                let n = self.df.get(t).copied().unwrap_or(0) as f32;
                (1.0 + (n_docs - n + 0.5) / (n + 0.5)).ln()
            })
            .collect();

        let mut scored: Vec<(usize, f32)> = Vec::new();
        for (i, doc) in self.docs.iter().enumerate() {
            let norm = 1.0 - B + B * doc.dl as f32 / self.avgdl;
            let mut score = 0.0f32;
            for (t, idf) in terms.iter().zip(&idfs) {
                let tf = match doc.tf.get(t) {
                    Some(&tf) => tf as f32,
                    None => continue,
                };
                score += idf * tf * (K1 + 1.0) / (tf + K1 * norm);
            }
            if score > 0.0 {
                scored.push((i, score));
            }
        }
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        scored.truncate(k);
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokeniza_snake_y_camel() {
        assert_eq!(
            tokenize("parseBlame_v2 fooBar.rs"),
            vec![
                "parse",
                "blame",
                "parseblame",
                "v2",
                "foo",
                "bar",
                "foobar",
                "rs"
            ]
        );
    }

    #[test]
    fn tokeniza_siglas_y_descarta_cortos() {
        assert_eq!(
            tokenize("HTTPServer x"),
            vec!["http", "server", "httpserver"]
        );
    }

    #[test]
    fn bm25_encuentra_por_contenido() {
        let i = Bm25::new(&["alfa beta".into(), "beta gamma gamma delta".into()]);
        let r = i.search("gamma", 5);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, 1);
        assert!(r[0].1 > 0.0);
    }

    #[test]
    fn bm25_documento_corto_puntua_mas() {
        let i = Bm25::new(&["alfa beta".into(), "beta gamma gamma delta".into()]);
        let r = i.search("beta", 5);
        let orden: Vec<usize> = r.iter().map(|(idx, _)| *idx).collect();
        assert_eq!(orden, vec![0, 1]);
        assert!(r[0].1 > r[1].1);
    }

    #[test]
    fn bm25_sin_coincidencias_o_vacio() {
        let i = Bm25::new(&["alfa beta".into(), "beta gamma gamma delta".into()]);
        assert!(i.search("zeta", 5).is_empty());
        let vacio = Bm25::new(&[]);
        assert!(vacio.is_empty());
        assert_eq!(vacio.len(), 0);
        assert!(vacio.search("beta", 5).is_empty());
    }

    #[test]
    fn bm25_respeta_k() {
        let i = Bm25::new(&[
            "buscar uno".into(),
            "buscar dos".into(),
            "buscar tres buscar".into(),
        ]);
        assert_eq!(i.search("buscar", 2).len(), 2);
        assert_eq!(i.search("buscar", 10).len(), 3);
    }
}
