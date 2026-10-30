//! Descomposición de líneas de shell en órdenes simples.

const MAX_DEPTH: usize = 4;
const PREFIXES: &[&str] = &["sudo", "nohup", "time", "command", "exec"];

/// Órdenes simples que ejecutaría la línea (error si está demasiado anidada).
pub fn simple_commands(cmd: &str) -> Result<Vec<String>, String> {
    let mut out = vec![];
    collect(cmd, 0, &mut out)?;
    Ok(out)
}

fn split_segments(s: &str) -> Vec<String> {
    let mut segs = vec![];
    let mut cur = String::new();
    let (mut sq, mut dq) = (false, false);
    let mut paren = 0i32;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' if !dq => sq = !sq,
            '"' if !sq => dq = !dq,
            '(' if !sq && i > 0 && chars[i - 1] == '$' => paren += 1,
            ')' if !sq && paren > 0 => paren -= 1,
            _ => {}
        }
        let sep = !sq && !dq && paren == 0 && matches!(c, ';' | '|' | '&' | '\n');
        if sep {
            segs.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        i += 1;
    }
    segs.push(cur);
    segs.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn substitutions(s: &str) -> Vec<String> {
    let mut v = vec![];
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut sq = false;
    while i < b.len() {
        if b[i] == '\'' {
            sq = !sq;
        }
        if !sq && b[i] == '$' && i + 1 < b.len() && b[i + 1] == '(' {
            let mut depth = 1;
            let mut j = i + 2;
            while j < b.len() && depth > 0 {
                match b[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            v.push(b[i + 2..j.saturating_sub(1)].iter().collect());
            i = j;
            continue;
        }
        if !sq
            && b[i] == '`'
            && let Some(k) = b[i + 1..].iter().position(|c| *c == '`')
        {
            v.push(b[i + 1..i + 1 + k].iter().collect());
            i += k + 2;
            continue;
        }
        i += 1;
    }
    v
}

fn collect(cmd: &str, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("orden demasiado anidada".into());
    }
    for sub in substitutions(cmd) {
        collect(&sub, depth + 1, out)?;
    }
    for seg in split_segments(cmd) {
        let Ok(mut words) = shell_words::split(&seg) else {
            out.push(seg);
            continue;
        };
        // Quita prefijos (sudo, env VAR=x, VAR=x, nohup…).
        loop {
            match words.first().map(String::as_str) {
                Some(w) if PREFIXES.contains(&w) => {
                    words.remove(0);
                }
                Some("env") => {
                    words.remove(0);
                    while words
                        .first()
                        .is_some_and(|w| w.contains('=') || w.starts_with('-'))
                    {
                        words.remove(0);
                    }
                }
                Some(w)
                    if w.contains('=')
                        && !w.starts_with('-')
                        && w.find('=').is_some_and(|p| p > 0) =>
                {
                    words.remove(0);
                }
                _ => break,
            }
        }
        if words.is_empty() {
            continue;
        }
        // Windows: `cmd /C …` y `powershell -Command …` también ocultan órdenes.
        let w0 = words[0].to_ascii_lowercase();
        if matches!(w0.as_str(), "cmd" | "cmd.exe")
            && let Some(p) = words.iter().position(|w| w.eq_ignore_ascii_case("/c"))
        {
            collect(&words[p + 1..].join(" "), depth + 1, out)?;
            continue;
        }
        if matches!(
            w0.as_str(),
            "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
        ) && let Some(p) = words
            .iter()
            .position(|w| w.eq_ignore_ascii_case("-command") || w.eq_ignore_ascii_case("-c"))
        {
            collect(&words[p + 1..].join(" "), depth + 1, out)?;
            continue;
        }
        if matches!(words[0].as_str(), "sh" | "bash" | "zsh")
            && let Some(p) = words.iter().position(|w| w == "-c")
            && let Some(script) = words.get(p + 1)
        {
            collect(script, depth + 1, out)?;
            continue;
        }
        out.push(words.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separa_and_or_punto_y_coma() {
        let v = simple_commands("a && b || c; d | e").unwrap();
        assert_eq!(v, ["a", "b", "c", "d", "e"]);
    }
    #[test]
    fn bash_c_recursivo() {
        assert_eq!(
            simple_commands("bash -c 'git stash'").unwrap(),
            ["git stash"]
        );
    }
    #[test]
    fn subshell_dolar_parentesis() {
        let v = simple_commands("echo $(git push)").unwrap();
        assert!(v.contains(&"git push".to_string()));
    }
    #[test]
    fn quita_sudo_y_env() {
        assert_eq!(
            simple_commands("sudo env A=1 rm -rf x").unwrap(),
            ["rm -rf x"]
        );
        assert_eq!(simple_commands("A=1 make").unwrap(), ["make"]);
    }
    #[test]
    fn cmd_y_powershell_recursivos() {
        assert_eq!(simple_commands("cmd /C git stash").unwrap(), ["git stash"]);
        assert_eq!(
            simple_commands("powershell -Command \"git push origin main\"").unwrap(),
            ["git push origin main"]
        );
    }
    #[test]
    fn respeta_comillas() {
        assert_eq!(simple_commands("echo 'a && b'").unwrap(), ["echo a && b"]);
    }
    #[test]
    fn anidamiento_excesivo_deniega() {
        let s = "bash -c \"bash -c \\\"bash -c \\\\\\\"bash -c \\\\\\\\\\\\\\\"bash -c \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"x\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"\\\\\\\\\\\\\\\"\\\\\\\"\\\"\"";
        let _ = simple_commands(s); // no debe entrar en pánico
        let nested = "$($($($($($(x))))))";
        assert!(simple_commands(nested).is_err());
    }
}
