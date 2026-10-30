//! Lectura de `SKILL.md`, registro e índice.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Una skill.
#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    /// Nombre.
    pub name: String,
    /// Descripción (lo único que entra siempre en contexto).
    pub description: String,
    /// Skills de las que depende.
    pub depends: Vec<String>,
    /// Cuándo no usarla.
    pub do_not_use_when: Option<String>,
    /// Procedencia.
    pub provenance: Option<String>,
    /// Carpeta.
    pub dir: PathBuf,
    /// Cuerpo (tras el frontmatter).
    pub body: String,
}

/// Errores.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// E/S.
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    /// Skill inválida.
    #[error("{0}: {1}")]
    Invalid(String, String),
}

fn list_value(v: &str) -> Vec<String> {
    let t = v.trim();
    let inner = t
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(t);
    inner
        .split(',')
        .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parsea un `SKILL.md`.
pub fn parse_skill_md(text: &str, dir: &Path) -> Result<Skill, SkillError> {
    let d = dir.display().to_string();
    let rest = text
        .strip_prefix("---\n")
        .ok_or_else(|| SkillError::Invalid(d.clone(), "falta el frontmatter".into()))?;
    let end = rest
        .find("\n---")
        .ok_or_else(|| SkillError::Invalid(d.clone(), "frontmatter sin cerrar".into()))?;
    let (fm, body) = (&rest[..end], rest[end + 4..].trim_start_matches('\n'));
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut last: Option<String> = None;
    for line in fm.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ")
            && let Some(k) = &last
        {
            map.entry(k.clone())
                .or_default()
                .push(item.trim().trim_matches(['"', '\'']).to_string());
        } else if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_string();
            last = Some(k.clone());
            if !v.trim().is_empty() {
                map.insert(
                    k,
                    if v.trim().starts_with('[') {
                        list_value(v)
                    } else {
                        vec![v.trim().trim_matches(['"', '\'']).to_string()]
                    },
                );
            }
        }
    }
    let one = |k: &str| map.get(k).and_then(|v| v.first()).cloned();
    let name = one("name").ok_or_else(|| SkillError::Invalid(d.clone(), "falta `name`".into()))?;
    let description = one("description")
        .ok_or_else(|| SkillError::Invalid(d.clone(), "falta `description`".into()))?;
    Ok(Skill {
        name,
        description,
        depends: map.get("depende").cloned().unwrap_or_default(),
        do_not_use_when: one("no_usar_cuando"),
        provenance: one("procedencia"),
        dir: dir.to_path_buf(),
        body: body.to_string(),
    })
}

/// Registro de skills.
#[derive(Debug, Default)]
pub struct Registry {
    skills: BTreeMap<String, Skill>,
}

impl Registry {
    /// Carga todas las skills de los directorios (cada subcarpeta con `SKILL.md`).
    pub fn load(dirs: &[PathBuf]) -> Result<Registry, SkillError> {
        let mut r = Registry::default();
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(d) else {
                continue;
            };
            let mut subs: Vec<_> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.join("SKILL.md").is_file())
                .collect();
            subs.sort();
            for p in subs {
                let s = parse_skill_md(&std::fs::read_to_string(p.join("SKILL.md"))?, &p)?;
                if r.skills.contains_key(&s.name) {
                    return Err(SkillError::Invalid(s.name, "skill duplicada".into()));
                }
                r.skills.insert(s.name.clone(), s);
            }
        }
        Ok(r)
    }
    /// Busca por nombre.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }
    /// Todas, ordenadas por nombre.
    pub fn all(&self) -> impl Iterator<Item = &Skill> {
        self.skills.values()
    }
    /// Índice ligero: una línea por skill.
    pub fn index(&self) -> String {
        self.skills
            .values()
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// La skill y sus dependencias (transitivas, sin ciclos).
    pub fn with_dependencies(&self, name: &str) -> Result<Vec<&Skill>, SkillError> {
        fn go<'a>(
            r: &'a Registry,
            n: &str,
            seen: &mut Vec<String>,
            stack: &mut Vec<String>,
            out: &mut Vec<&'a Skill>,
        ) -> Result<(), SkillError> {
            if stack.iter().any(|x| x == n) {
                return Err(SkillError::Invalid(
                    n.into(),
                    "ciclo de dependencias".into(),
                ));
            }
            if seen.iter().any(|x| x == n) {
                return Ok(());
            }
            let s = r
                .skills
                .get(n)
                .ok_or_else(|| SkillError::Invalid(n.into(), "skill inexistente".into()))?;
            stack.push(n.into());
            for d in &s.depends {
                go(r, d, seen, stack, out)?;
            }
            stack.pop();
            seen.push(n.into());
            out.push(s);
            Ok(())
        }
        let mut out = vec![];
        go(self, name, &mut vec![], &mut vec![], &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mk(d: &Path, name: &str, extra: &str) {
        let p = d.join(name);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: hace {name}\n{extra}---\n\n# cuerpo\n"),
        )
        .unwrap();
    }
    #[test]
    fn parsea_skill_del_repo() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/skills/tarea-haiku");
        let s = parse_skill_md(&std::fs::read_to_string(p.join("SKILL.md")).unwrap(), &p).unwrap();
        assert_eq!(s.name, "tarea-haiku");
        assert!(s.description.len() > 20 && s.body.contains("Ejecutar una tarea"));
    }
    #[test]
    fn falta_name_es_error() {
        assert!(parse_skill_md("---\ndescription: x\n---\n", Path::new("x")).is_err());
        assert!(parse_skill_md("sin frontmatter", Path::new("x")).is_err());
    }
    #[test]
    fn lista_en_dos_formatos() {
        let a = parse_skill_md(
            "---\nname: a\ndescription: d\ndepende: [b, c]\n---\n",
            Path::new("x"),
        )
        .unwrap();
        let b = parse_skill_md(
            "---\nname: a\ndescription: d\ndepende:\n  - b\n  - c\n---\n",
            Path::new("x"),
        )
        .unwrap();
        assert_eq!(a.depends, ["b", "c"]);
        assert_eq!(b.depends, ["b", "c"]);
    }
    #[test]
    fn registro_duplicado_es_error() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        mk(&a, "x", "");
        mk(&b, "x", "");
        assert!(Registry::load(&[a, b]).is_err());
    }
    #[test]
    fn dependencias_transitivas() {
        let d = tempfile::tempdir().unwrap();
        mk(d.path(), "a", "depende: [b]\n");
        mk(d.path(), "b", "depende: [c]\n");
        mk(d.path(), "c", "");
        let r = Registry::load(&[d.path().to_path_buf()]).unwrap();
        let n: Vec<_> = r
            .with_dependencies("a")
            .unwrap()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(n, ["c", "b", "a"]);
        assert!(r.index().contains("- a: hace a"));
    }
    #[test]
    fn ciclo_es_error() {
        let d = tempfile::tempdir().unwrap();
        mk(d.path(), "a", "depende: [b]\n");
        mk(d.path(), "b", "depende: [a]\n");
        let r = Registry::load(&[d.path().to_path_buf()]).unwrap();
        assert!(r.with_dependencies("a").is_err());
    }
}
