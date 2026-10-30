//! Compresión de salidas de compilador y tests antes de devolverlas al modelo.
use pcode_tools::head_tail;
use std::collections::HashSet;

fn is_summary(l: &str) -> bool {
    l.contains("test result:")
        || l.contains("could not compile")
        || l.starts_with("failures:")
        || l.starts_with("---- ")
        || l.starts_with("exit=")
}

type Block<'a> = Option<(bool, Vec<&'a str>)>;

fn flush<'a>(c: &mut Block<'a>, e: &mut Vec<Vec<&'a str>>, w: &mut Vec<Vec<&'a str>>) {
    if let Some((is_err, b)) = c.take() {
        if is_err { e.push(b) } else { w.push(b) }
    }
}

/// Conserva diagnósticos de rustc (sin duplicados) y líneas de resumen.
pub fn compress_cargo(text: &str, max_errors: usize) -> String {
    let mut errors: Vec<Vec<&str>> = vec![];
    let mut warnings: Vec<Vec<&str>> = vec![];
    let mut summary: Vec<&str> = vec![];
    let mut cur: Block = None;
    for l in text.lines() {
        if l.starts_with("error") && !l.contains("could not compile") {
            flush(&mut cur, &mut errors, &mut warnings);
            cur = Some((true, vec![l]));
        } else if l.starts_with("warning") && !l.contains("generated") {
            flush(&mut cur, &mut errors, &mut warnings);
            cur = Some((false, vec![l]));
        } else if l.trim().is_empty() {
            flush(&mut cur, &mut errors, &mut warnings);
        } else if let Some((_, b)) = cur.as_mut() {
            b.push(l);
        }
        if is_summary(l) {
            summary.push(l);
        }
    }
    flush(&mut cur, &mut errors, &mut warnings);
    let pool = if errors.is_empty() { warnings } else { errors };
    let mut seen = HashSet::new();
    let mut out: Vec<String> = vec![];
    for b in pool {
        if seen.insert(b[0].to_string()) && seen.len() <= max_errors {
            out.push(b.join("\n"));
        }
    }
    let mut res = out.join("\n\n");
    if !summary.is_empty() {
        res.push_str("\n\n");
        res.push_str(&summary.join("\n"));
    }
    res.trim().to_string()
}

/// Comprime la salida según la orden.
pub fn compress_tool_output(command: &str, text: &str) -> String {
    if command.trim_start().starts_with("cargo") {
        compress_cargo(text, 5)
    } else {
        head_tail(text, 150, 150)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ERR: &str = "error[E0425]: no value x\n --> a.rs:1\n\nerror[E0425]: no value x\n --> a.rs:9\n\nerror[E0308]: mismatched\n --> b.rs:2\n\nerror: could not compile `x` (lib) due to 3 errors\n";
    #[test]
    fn compress_cargo_deduplica() {
        let r = compress_cargo(ERR, 5);
        assert_eq!(r.matches("no value x").count(), 1);
        assert!(r.contains("mismatched"));
    }
    #[test]
    fn compress_cargo_limita_errores() {
        let r = compress_cargo(ERR, 1);
        assert!(r.contains("no value x") && !r.contains("mismatched"));
    }
    #[test]
    fn compress_cargo_conserva_resumen_de_tests() {
        let t = "---- t1 stdout ----\nboom\n\nfailures:\n    t1\n\ntest result: FAILED. 0 passed; 1 failed\n";
        let r = compress_cargo(t, 5);
        assert!(r.contains("---- t1 stdout ----") && r.contains("test result: FAILED"));
    }
    #[test]
    fn compress_cargo_warnings_solo_sin_errores() {
        let w = "warning: unused x\n --> a.rs\n\n";
        assert!(compress_cargo(w, 5).contains("unused x"));
        let both = format!("{w}error: bad\n\n");
        assert!(!compress_cargo(&both, 5).contains("unused x"));
    }
    #[test]
    fn compress_otros_usa_head_tail() {
        let t: String = (0..400).map(|i| format!("{i}\n")).collect();
        assert!(compress_tool_output("ls", &t).contains("líneas omitidas"));
    }
}
