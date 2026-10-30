//! Exportar skills a cada agente (una fuente, todos los formatos).
use crate::registry::{Registry, SkillError};
use std::path::{Path, PathBuf};

/// Destino de la exportación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `.claude/skills/<name>/`.
    Claude,
    /// Sección en `AGENTS.md`.
    AgentsMd,
    /// Sección en `.github/copilot-instructions.md`.
    Copilot,
    /// Sección en `GEMINI.md`.
    Gemini,
    /// `.opencode/skills/<name>/` (ruta a confirmar en F0.03).
    Opencode,
}

const INI: &str = "<!-- skills:inicio -->";
const FIN: &str = "<!-- skills:fin -->";

fn md_target(t: Target, root: &Path) -> Option<PathBuf> {
    match t {
        Target::AgentsMd => Some(root.join("AGENTS.md")),
        Target::Copilot => Some(root.join(".github/copilot-instructions.md")),
        Target::Gemini => Some(root.join("GEMINI.md")),
        _ => None,
    }
}

fn section(reg: &Registry, root: &Path) -> String {
    let mut s = format!("{INI}\n## Skills disponibles\n\n");
    for k in reg.all() {
        let rel = k.dir.strip_prefix(root).unwrap_or(&k.dir).display();
        s.push_str(&format!(
            "- **{}**: {} (`{}/SKILL.md`)\n",
            k.name, k.description, rel
        ));
    }
    s.push_str(FIN);
    s
}

fn replace_section(text: &str, sec: &str) -> String {
    match (text.find(INI), text.find(FIN)) {
        (Some(a), Some(b)) if b > a => format!("{}{}{}", &text[..a], sec, &text[b + FIN.len()..]),
        _ => format!("{}\n\n{}\n", text.trim_end(), sec),
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let (s, d) = (e.path(), dst.join(e.file_name()));
        if s.is_dir() {
            copy_dir(&s, &d)?
        } else if std::fs::read(&d).ok() != std::fs::read(&s).ok() {
            std::fs::copy(&s, &d).map(|_| ())?
        }
    }
    Ok(())
}

fn dir_target(t: Target, root: &Path) -> Option<PathBuf> {
    match t {
        Target::Claude => Some(root.join(".claude/skills")),
        Target::Opencode => Some(root.join(".opencode/skills")),
        _ => None,
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let (Ok(x), Ok(y)) = (std::fs::read(a), std::fs::read(b)) else {
        return false;
    };
    x == y
}

fn stale(reg: &Registry, t: Target, root: &Path) -> Result<Vec<PathBuf>, SkillError> {
    let mut out = vec![];
    if let Some(f) = md_target(t, root) {
        let cur = std::fs::read_to_string(&f).unwrap_or_default();
        if replace_section(&cur, &section(reg, root)) != cur {
            out.push(f);
        }
    } else if let Some(base) = dir_target(t, root) {
        for k in reg.all() {
            let dst = base.join(&k.name);
            if k.dir.canonicalize().ok() != dst.canonicalize().ok()
                && !same_dir(&k.dir.join("SKILL.md"), &dst.join("SKILL.md"))
            {
                out.push(dst);
            }
        }
    }
    Ok(out)
}

/// Exporta; devuelve las rutas escritas.
pub fn export(reg: &Registry, target: Target, root: &Path) -> Result<Vec<PathBuf>, SkillError> {
    let todo = stale(reg, target, root)?;
    if let Some(f) = md_target(target, root) {
        if !todo.is_empty() {
            if let Some(p) = f.parent() {
                std::fs::create_dir_all(p)?;
            }
            let cur = std::fs::read_to_string(&f).unwrap_or_default();
            std::fs::write(&f, replace_section(&cur, &section(reg, root)))?;
        }
    } else {
        for dst in &todo {
            let k = reg
                .get(dst.file_name().and_then(|n| n.to_str()).unwrap_or(""))
                .ok_or_else(|| {
                    SkillError::Invalid(dst.display().to_string(), "skill desconocida".into())
                })?;
            copy_dir(&k.dir, dst)?;
        }
    }
    Ok(todo)
}

/// Rutas desfasadas, sin escribir.
pub fn check(reg: &Registry, target: Target, root: &Path) -> Result<Vec<PathBuf>, SkillError> {
    stale(reg, target, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Registry) {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("fuente/uno");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: uno\ndescription: hace uno\n---\ncuerpo\n",
        )
        .unwrap();
        std::fs::write(src.join("script.sh"), "echo").unwrap();
        let r = Registry::load(&[d.path().join("fuente")]).unwrap();
        (d, r)
    }
    #[test]
    fn exporta_claude() {
        let (d, r) = setup();
        export(&r, Target::Claude, d.path()).unwrap();
        assert!(d.path().join(".claude/skills/uno/SKILL.md").exists());
        assert!(d.path().join(".claude/skills/uno/script.sh").exists());
    }
    #[test]
    fn exporta_agents_md_entre_marcadores() {
        let (d, r) = setup();
        std::fs::write(
            d.path().join("AGENTS.md"),
            format!("# A\n\n{INI}\nviejo\n{FIN}\n\nfin\n"),
        )
        .unwrap();
        export(&r, Target::AgentsMd, d.path()).unwrap();
        let t = std::fs::read_to_string(d.path().join("AGENTS.md")).unwrap();
        assert!(t.contains("**uno**: hace uno") && !t.contains("viejo") && t.contains("fin"));
    }
    #[test]
    fn crea_marcadores_si_faltan() {
        let (d, r) = setup();
        std::fs::write(d.path().join("GEMINI.md"), "# G\n").unwrap();
        export(&r, Target::Gemini, d.path()).unwrap();
        let t = std::fs::read_to_string(d.path().join("GEMINI.md")).unwrap();
        assert!(t.starts_with("# G") && t.contains(INI) && t.contains(FIN));
    }
    #[test]
    fn export_idempotente() {
        let (d, r) = setup();
        for t in [Target::Claude, Target::Copilot] {
            assert!(!export(&r, t, d.path()).unwrap().is_empty());
            assert!(export(&r, t, d.path()).unwrap().is_empty());
        }
    }
    #[test]
    fn check_detecta_desfase() {
        let (d, r) = setup();
        assert!(!check(&r, Target::AgentsMd, d.path()).unwrap().is_empty());
        export(&r, Target::AgentsMd, d.path()).unwrap();
        assert!(check(&r, Target::AgentsMd, d.path()).unwrap().is_empty());
    }
}
