//! Utilidades del panel de git.

/// Línea de un diff unificado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLine {
    /// Línea añadida.
    Added,
    /// Línea quitada.
    Removed,
    /// Cabecera de bloque.
    Hunk,
    /// Cabecera de archivo.
    Header,
    /// Contexto.
    Context,
}

/// Letra que se ve para un estado de `git status --porcelain`.
pub fn status_label(index: &str, worktree: &str) -> &'static str {
    let both = format!("{index}{worktree}");
    if matches!(both.as_str(), "UU" | "AA" | "DD") {
        return "U";
    }
    if both == "??" {
        return "?";
    }
    let c = if index.trim().is_empty() {
        worktree
    } else {
        index
    };
    match c.trim() {
        "M" => "M",
        "A" => "A",
        "D" => "D",
        "R" => "R",
        _ => "·",
    }
}

/// Si el archivo tiene cambios preparados.
pub fn is_staged(index: &str) -> bool {
    !index.trim().is_empty() && index != "?"
}

/// Clasifica cada línea de un diff.
pub fn diff_lines(diff: &str) -> Vec<(DiffLine, &str)> {
    diff.lines()
        .map(|l| {
            let k = if l.starts_with("diff ")
                || l.starts_with("index ")
                || l.starts_with("+++")
                || l.starts_with("---")
            {
                DiffLine::Header
            } else if l.starts_with("@@") {
                DiffLine::Hunk
            } else if l.starts_with('+') {
                DiffLine::Added
            } else if l.starts_with('-') {
                DiffLine::Removed
            } else {
                DiffLine::Context
            };
            (k, l)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etiquetas_de_estado() {
        assert_eq!(status_label(" ", "M"), "M");
        assert_eq!(status_label("A", " "), "A");
        assert_eq!(status_label("?", "?"), "?");
        assert_eq!(status_label("U", "U"), "U");
        assert_eq!(status_label("D", " "), "D");
    }

    #[test]
    fn preparado_o_no() {
        assert!(is_staged("M"));
        assert!(!is_staged(" "));
        assert!(!is_staged("?"));
    }

    #[test]
    fn clasifica_un_diff() {
        let d = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n c";
        let k: Vec<DiffLine> = diff_lines(d).into_iter().map(|x| x.0).collect();
        use DiffLine::*;
        assert_eq!(k, [Header, Header, Header, Hunk, Removed, Added, Context]);
    }
}
