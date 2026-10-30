//! Lectura de tareas (cabeceras HTML de SO_IA y frontmatter propio).
use std::path::Path;

/// Modo de ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Mode {
    /// Toca el árbol principal: una a la vez.
    Kernel,
    /// Aislada: puede ir en paralelo.
    Isolated,
    /// La hace el coordinador.
    Coordinator,
}

/// Una tarea.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskDef {
    /// Id (nombre del archivo sin `.md`).
    pub id: String,
    /// Orden de aceptación.
    pub accept: String,
    /// Modo.
    pub mode: Mode,
    /// Dependencias.
    pub depends: Vec<String>,
    /// Archivos que puede tocar.
    pub touches: Vec<String>,
    /// Backend preferido.
    pub backend: Option<String>,
    /// Skills.
    pub skills: Vec<String>,
    /// Presupuesto en minutos.
    pub budget_minutes: Option<u32>,
    /// Texto de la tarea.
    pub body: String,
}

fn header(text: &str, key: &str) -> Option<String> {
    let pat = "<!--";
    for l in text.lines().take(12) {
        let l = l.trim();
        if l.starts_with(pat)
            && let Some(rest) = l
                .trim_start_matches("<!--")
                .trim()
                .strip_prefix(&format!("{key}:"))
        {
            return Some(rest.trim_end_matches("-->").trim().to_string());
        }
    }
    None
}

fn csv(s: &str) -> Vec<String> {
    s.trim_matches(['[', ']'])
        .split(',')
        .map(|x| x.trim().trim_matches(['"', '\'']).to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn mode(s: &str) -> Mode {
    match s.trim() {
        "aislado" | "isolated" => Mode::Isolated,
        "coordinador" | "coordinator" => Mode::Coordinator,
        _ => Mode::Kernel,
    }
}

/// Parsea una tarea. Si hay frontmatter, manda sobre las cabeceras HTML.
pub fn parse_task(id: &str, text: &str) -> Result<TaskDef, String> {
    let (mut accept, mut m, mut deps) = (
        header(text, "acepta"),
        header(text, "modo"),
        header(text, "depende"),
    );
    let (mut touches, mut backend, mut skills, mut budget) = (vec![], None, vec![], None);
    let mut body = text;
    // El frontmatter puede ir tras las cabeceras HTML.
    let mut after = text;
    while after.trim_start().starts_with("<!--") {
        after = after
            .trim_start()
            .split_once('\n')
            .map(|x| x.1)
            .unwrap_or("");
    }
    if let Some(rest) = after.trim_start().strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        for l in rest[..end].lines() {
            let Some((k, v)) = l.split_once(':') else {
                continue;
            };
            let v = v.trim().to_string();
            match k.trim() {
                "acepta" => accept = Some(v),
                "modo" => m = Some(v),
                "depende" => deps = Some(v),
                "toca" => touches = csv(&v),
                "backend" => backend = Some(v),
                "skills" => skills = csv(&v),
                "presupuesto_minutos" => budget = v.parse().ok(),
                _ => {}
            }
        }
        body = rest[end + 4..].trim_start_matches('\n');
    }
    let accept = accept
        .filter(|a| !a.is_empty())
        .ok_or_else(|| format!("{id}: falta `acepta`"))?;
    Ok(TaskDef {
        id: id.into(),
        accept,
        mode: m.as_deref().map(mode).unwrap_or(Mode::Kernel),
        depends: deps.as_deref().map(csv).unwrap_or_default(),
        touches,
        backend,
        skills,
        budget_minutes: budget,
        body: body.to_string(),
    })
}

/// Carga todos los `.md` de un directorio (ignora los que no tienen `acepta`).
pub fn load_dir(dir: &Path) -> Result<Vec<TaskDef>, String> {
    let mut v = vec![];
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    for p in files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
    {
        let id = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        match parse_task(&id, &text) {
            Ok(t) => v.push(t),
            Err(e) => tracing::warn!("tarea ignorada: {e}"),
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsea_cabecera_so_ia() {
        let t = parse_task("T1.2", "<!-- acepta: make lint && cargo xtask test -->\n<!-- modo: aislado -->\n<!-- depende: T1.1, T1.0 -->\n# T\nx").unwrap();
        assert_eq!(t.accept, "make lint && cargo xtask test");
        assert_eq!(t.mode, Mode::Isolated);
        assert_eq!(t.depends, ["T1.1", "T1.0"]);
    }
    #[test]
    fn parsea_frontmatter() {
        let t = parse_task("x", "---\nacepta: cargo test\nmodo: aislado\ndepende: a\ntoca: [a.rs, b.rs]\nbackend: glm\nskills: [s1]\npresupuesto_minutos: 30\n---\ncuerpo").unwrap();
        assert_eq!(
            (t.touches.len(), t.backend.as_deref(), t.budget_minutes),
            (2, Some("glm"), Some(30))
        );
        assert_eq!(t.body, "cuerpo");
    }
    #[test]
    fn frontmatter_gana() {
        let t = parse_task("x", "<!-- acepta: viejo -->\n---\nacepta: nuevo\n---\n").unwrap();
        assert_eq!(t.accept, "nuevo");
    }
    #[test]
    fn modo_por_defecto_kernel() {
        assert_eq!(
            parse_task("x", "<!-- acepta: a -->").unwrap().mode,
            Mode::Kernel
        );
    }
    #[test]
    fn sin_acepta_se_ignora_en_load_dir() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.md"), "<!-- acepta: true -->\n").unwrap();
        std::fs::write(d.path().join("b.md"), "sin cabecera").unwrap();
        let v = load_dir(d.path()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "a");
    }
    #[test]
    fn carga_las_tareas_del_plan_del_repo() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tareas");
        let v = load_dir(&p).unwrap();
        assert!(v.len() >= 100, "{}", v.len());
        assert!(v.iter().any(
            |t| t.id == "F1.16-bucle" && t.depends.contains(&"F1.03-provider-mock".to_string())
        ));
    }
}
