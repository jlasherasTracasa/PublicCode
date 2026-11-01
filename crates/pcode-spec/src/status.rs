//! Estado de las especificaciones de un proyecto: dónde están, en qué fase va cada una, la
//! trazabilidad requisito → tareas → comprobaciones y el resultado de la última verificación.
use crate::spec::{Spec, bullet, parse_spec};
use crate::tasks::{Drift, SpecTask, TaskList, drift, parse_tasks, validate_plan};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Carpeta de las especificaciones, en la raíz del proyecto.
pub const SPECS_DIR: &str = "specs";
/// Lo que escribe la persona.
pub const SPEC_FILE: &str = "spec.md";
/// El diseño (lo genera el planificador).
pub const PLAN_FILE: &str = "plan.md";
/// Las tareas (las genera el planificador).
pub const TASKS_FILE: &str = "tareas.md";

/// Carpeta de la especificación `name`.
pub fn dir(root: &Path, name: &str) -> PathBuf {
    root.join(SPECS_DIR).join(name)
}

/// Nombre válido: minúsculas, números, `-` y `_` (es el nombre de una carpeta).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Nombres de las especificaciones del proyecto (carpetas de `specs/` con `spec.md`), en orden.
pub fn names(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root.join(SPECS_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join(SPEC_FILE).is_file())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| valid_name(n))
        .collect();
    v.sort();
    v
}

/// Una especificación con sus archivos.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Texto de `spec.md`.
    pub spec_text: String,
    /// `spec.md` leído.
    pub spec: Spec,
    /// Texto de `plan.md`, si existe.
    pub plan_text: Option<String>,
    /// Texto de `tareas.md`, si existe.
    pub tasks_text: Option<String>,
    /// `tareas.md` leído (vacío si no existe).
    pub tasks: TaskList,
}

/// Lee los archivos de la especificación `name`.
pub fn load(root: &Path, name: &str) -> Result<Loaded, String> {
    if !valid_name(name) {
        return Err(format!(
            "nombre no válido: «{name}» (minúsculas, números, - y _)"
        ));
    }
    let d = dir(root, name);
    let spec_text = std::fs::read_to_string(d.join(SPEC_FILE)).map_err(|_| {
        format!(
            "no existe {}/{name}/{SPEC_FILE}: créala con `pcode spec nueva {name}`",
            SPECS_DIR
        )
    })?;
    let spec = parse_spec(name, &spec_text)?;
    let plan_text = std::fs::read_to_string(d.join(PLAN_FILE)).ok();
    let tasks_text = std::fs::read_to_string(d.join(TASKS_FILE)).ok();
    let tasks = tasks_text.as_deref().map(parse_tasks).unwrap_or_default();
    Ok(Loaded {
        spec_text,
        spec,
        plan_text,
        tasks_text,
        tasks,
    })
}

/// Crea `specs/<name>/spec.md` con `content`. Falla si ya existe.
pub fn create(root: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(format!(
            "nombre no válido: «{name}» (minúsculas, números, - y _)"
        ));
    }
    let d = dir(root, name);
    let p = d.join(SPEC_FILE);
    if p.exists() {
        return Err(format!("ya existe {SPECS_DIR}/{name}/{SPEC_FILE}"));
    }
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(&p, content).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Resultado de una orden de comprobación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// La orden.
    pub command: String,
    /// De dónde sale: requisitos (`R1`) y tareas (`T2`).
    pub sources: Vec<String>,
    /// Terminó con 0.
    pub ok: bool,
    /// Código de salida (`None` si no llegó a terminar).
    pub code: Option<i32>,
    /// Las últimas líneas de su salida.
    pub tail: String,
    /// Cuánto tardó.
    pub millis: u64,
}

/// Una verificación completa de la especificación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// Cuándo (segundos desde 1970).
    pub time: u64,
    /// Resultado de cada orden.
    pub results: Vec<CheckResult>,
}

/// Dónde se guarda la última verificación (local, no se versiona).
pub fn verification_path(root: &Path, name: &str) -> PathBuf {
    root.join(".pcode")
        .join("specs")
        .join(name)
        .join("verificacion.json")
}

/// La última verificación guardada.
pub fn load_verification(root: &Path, name: &str) -> Option<Verification> {
    let s = std::fs::read_to_string(verification_path(root, name)).ok()?;
    serde_json::from_str(&s).ok()
}

/// Guarda una verificación.
pub fn save_verification(root: &Path, name: &str, v: &Verification) -> std::io::Result<()> {
    let p = verification_path(root, name);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(v)?)
}

/// Las órdenes que verifican la especificación, sin repetir, con de dónde sale cada una: primero
/// las de los criterios de cada requisito y después las `acepta` de las tareas.
pub fn checks(spec: &Spec, tasks: &TaskList) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    let mut add = |cmd: &str, src: &str| match out.iter_mut().find(|(c, _)| c == cmd) {
        Some((_, s)) => {
            if !s.iter().any(|x| x == src) {
                s.push(src.to_string());
            }
        }
        None => out.push((cmd.to_string(), vec![src.to_string()])),
    };
    for r in &spec.requirements {
        for c in r.commands() {
            add(c, &r.id);
        }
    }
    for t in &tasks.tasks {
        if let Some(a) = &t.accept {
            add(a, &t.id);
        }
    }
    out
}

/// Órdenes que comprueban el requisito `r`: las suyas y las `acepta` de sus tareas.
pub fn requirement_commands(spec: &Spec, tasks: &TaskList, r: &str) -> Vec<String> {
    let mut v: Vec<String> = spec
        .requirement(r)
        .map(|r| r.commands().into_iter().map(String::from).collect())
        .unwrap_or_default();
    for t in tasks
        .tasks
        .iter()
        .filter(|t| t.requirements.iter().any(|x| x == r))
    {
        if let Some(a) = &t.accept
            && !v.contains(a)
        {
            v.push(a.clone());
        }
    }
    v
}

/// Veredicto de un requisito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Todas sus comprobaciones pasaron en la última verificación.
    Ok,
    /// Alguna falló.
    Failed,
    /// Tiene comprobaciones pero no se han ejecutado todas.
    NotRun,
    /// No tiene ninguna orden que lo compruebe (ni propia ni de sus tareas).
    NoChecks,
}

/// Veredicto de cada requisito según la verificación `v`.
pub fn verdicts(
    spec: &Spec,
    tasks: &TaskList,
    v: Option<&Verification>,
) -> BTreeMap<String, Verdict> {
    spec.requirements
        .iter()
        .map(|r| {
            let cmds = requirement_commands(spec, tasks, &r.id);
            let verdict = if cmds.is_empty() {
                Verdict::NoChecks
            } else {
                let res: Vec<Option<&CheckResult>> = cmds
                    .iter()
                    .map(|c| v.and_then(|v| v.results.iter().find(|x| &x.command == c)))
                    .collect();
                if res.iter().flatten().any(|x| !x.ok) {
                    Verdict::Failed
                } else if res.iter().any(|x| x.is_none()) {
                    Verdict::NotRun
                } else {
                    Verdict::Ok
                }
            };
            (r.id.clone(), verdict)
        })
        .collect()
}

/// Un requisito en la vista de estado.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementStatus {
    /// Identificador.
    pub id: String,
    /// Título.
    pub title: String,
    /// Criterios de aceptación.
    pub criteria: Vec<String>,
    /// Órdenes que lo comprueban.
    pub commands: Vec<String>,
    /// Tareas que lo cubren.
    pub tasks: Vec<String>,
    /// Cuántas de ellas están hechas.
    pub tasks_done: usize,
    /// Veredicto de la última verificación.
    pub verdict: Verdict,
    /// Cambió desde que se planificó.
    pub changed: bool,
}

/// En qué punto está una especificación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Sin plan: toca planificar.
    Plan,
    /// La especificación cambió o el plan tiene problemas: toca replanificar.
    Replan,
    /// Hay tareas pendientes.
    Implement,
    /// Tareas hechas; falta verificar (o algo falló).
    Verify,
    /// Todo verificado.
    Done,
}

/// Todo lo que enseña la vista de una especificación.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecStatus {
    /// Nombre.
    pub name: String,
    /// Título.
    pub title: String,
    /// Objetivo.
    pub objective: String,
    /// Carpeta, relativa al proyecto.
    pub dir: String,
    /// Hay `plan.md`.
    pub has_plan: bool,
    /// Hay `tareas.md` con tareas.
    pub planned: bool,
    /// Requisitos con su trazabilidad.
    pub requirements: Vec<RequirementStatus>,
    /// Tareas.
    pub tasks: Vec<SpecTask>,
    /// Tareas hechas.
    pub tasks_done: usize,
    /// Qué cambió en la especificación desde el plan.
    pub drift: Drift,
    /// Problemas del plan (vacío = se puede ejecutar).
    pub problems: Vec<String>,
    /// Avisos de la especificación.
    pub warnings: Vec<String>,
    /// Preguntas abiertas del plan (lo que la especificación no deja claro).
    pub open_questions: Vec<String>,
    /// Restricciones.
    pub constraints: Vec<String>,
    /// Última verificación.
    pub verification: Option<Verification>,
    /// Fase.
    pub phase: Phase,
}

/// Las viñetas de la sección `## Preguntas abiertas` de `plan.md` (sin «ninguna»).
pub fn open_questions(plan: &str) -> Vec<String> {
    let mut inside = false;
    let mut v = vec![];
    for l in plan.lines() {
        if let Some(h) = l.strip_prefix("## ") {
            inside = h.trim().to_lowercase().starts_with("preguntas");
            continue;
        }
        if inside && let Some(b) = bullet(l) {
            let low = b.to_lowercase();
            if !b.is_empty() && !low.starts_with("ninguna") && !low.starts_with("no hay") {
                v.push(b.to_string());
            }
        }
    }
    v
}

/// Estado de la especificación `name`.
pub fn status(root: &Path, name: &str) -> Result<SpecStatus, String> {
    let l = load(root, name)?;
    let d = drift(&l.spec, &l.tasks);
    let planned = !l.tasks.tasks.is_empty();
    let problems = if planned {
        validate_plan(&l.spec, &l.tasks)
    } else {
        vec![]
    };
    let verification = load_verification(root, name);
    let verd = verdicts(&l.spec, &l.tasks, verification.as_ref());
    let requirements: Vec<RequirementStatus> = l
        .spec
        .requirements
        .iter()
        .map(|r| {
            let tasks: Vec<&SpecTask> = l
                .tasks
                .tasks
                .iter()
                .filter(|t| t.requirements.contains(&r.id))
                .collect();
            RequirementStatus {
                id: r.id.clone(),
                title: r.title.clone(),
                criteria: r.criteria.iter().map(|c| c.text.clone()).collect(),
                commands: requirement_commands(&l.spec, &l.tasks, &r.id),
                tasks_done: tasks.iter().filter(|t| t.done).count(),
                tasks: tasks.iter().map(|t| t.id.clone()).collect(),
                verdict: verd.get(&r.id).copied().unwrap_or(Verdict::NoChecks),
                changed: d.changed.contains(&r.id) || d.added.contains(&r.id),
            }
        })
        .collect();
    let tasks_done = l.tasks.done();
    let phase = if !planned {
        Phase::Plan
    } else if !d.is_empty() || !problems.is_empty() {
        Phase::Replan
    } else if tasks_done < l.tasks.tasks.len() {
        Phase::Implement
    } else if requirements.iter().all(|r| r.verdict == Verdict::Ok) {
        Phase::Done
    } else {
        Phase::Verify
    };
    Ok(SpecStatus {
        name: name.to_string(),
        title: l.spec.title.clone(),
        objective: l.spec.objective.clone(),
        dir: format!("{SPECS_DIR}/{name}"),
        has_plan: l.plan_text.is_some(),
        planned,
        requirements,
        tasks_done,
        tasks: l.tasks.tasks.clone(),
        drift: d,
        problems,
        warnings: l.spec.warnings.clone(),
        open_questions: l
            .plan_text
            .as_deref()
            .map(open_questions)
            .unwrap_or_default(),
        constraints: l.spec.constraints.clone(),
        verification,
        phase,
    })
}

/// Una especificación en la lista.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecSummary {
    /// Nombre.
    pub name: String,
    /// Título.
    pub title: String,
    /// Requisitos.
    pub requirements: usize,
    /// Requisitos verificados.
    pub verified: usize,
    /// Tareas hechas.
    pub tasks_done: usize,
    /// Tareas en total.
    pub tasks_total: usize,
    /// Fase (`None` si `spec.md` tiene errores).
    pub phase: Option<Phase>,
    /// El error de `spec.md`, si no se puede leer.
    pub error: Option<String>,
}

/// Todas las especificaciones del proyecto.
pub fn list(root: &Path) -> Vec<SpecSummary> {
    names(root)
        .into_iter()
        .map(|n| match status(root, &n) {
            Ok(s) => SpecSummary {
                name: n,
                title: s.title,
                verified: s
                    .requirements
                    .iter()
                    .filter(|r| r.verdict == Verdict::Ok)
                    .count(),
                requirements: s.requirements.len(),
                tasks_done: s.tasks_done,
                tasks_total: s.tasks.len(),
                phase: Some(s.phase),
                error: None,
            },
            Err(e) => SpecSummary {
                title: n.clone(),
                name: n,
                requirements: 0,
                verified: 0,
                tasks_done: 0,
                tasks_total: 0,
                phase: None,
                error: Some(e),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{set_done, stamp};

    const SPEC: &str = "# Facturas\n## Objetivo\nexportar\n## Requisitos\n### R1. Exportar\n- csv\n- acepta: `true`\n### R2. Mes vacío\n- cabecera\n## Restricciones\n- sin dependencias\n";
    const TAREAS: &str = "# Tareas\n\n- [ ] T1. CSV\n  - requisitos: R1\n  - acepta: `cargo test csv`\n- [ ] T2. Vacío\n  - requisitos: R2\n  - depende: T1\n  - acepta: `cargo test vacio`\n";

    fn proyecto() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        create(d.path(), "facturas", SPEC).unwrap();
        d
    }

    #[test]
    fn fases_de_una_especificacion() {
        let d = proyecto();
        let r = d.path();
        assert_eq!(names(r), ["facturas"]);
        let s = status(r, "facturas").unwrap();
        assert_eq!(s.phase, Phase::Plan);
        assert_eq!(s.dir, "specs/facturas");
        assert_eq!(s.constraints, ["sin dependencias"]);
        assert_eq!(
            s.requirements[0].verdict,
            Verdict::NotRun,
            "R1 tiene su acepta"
        );
        assert_eq!(s.requirements[1].verdict, Verdict::NoChecks);
        // Planificado y sellado.
        let spec = parse_spec("facturas", SPEC).unwrap();
        let tareas = dir(r, "facturas").join(TASKS_FILE);
        std::fs::write(&tareas, stamp(TAREAS, &spec)).unwrap();
        std::fs::write(
            dir(r, "facturas").join(PLAN_FILE),
            "# Plan\n## Preguntas abiertas\n- ¿separador `;` o `,`?\n",
        )
        .unwrap();
        let s = status(r, "facturas").unwrap();
        assert_eq!(s.phase, Phase::Implement);
        assert!(s.problems.is_empty(), "{:?}", s.problems);
        assert_eq!(s.open_questions, ["¿separador `;` o `,`?"]);
        assert_eq!(s.requirements[0].commands, ["true", "cargo test csv"]);
        assert_eq!(s.requirements[1].tasks, ["T2"]);
        // Las dos tareas hechas: falta verificar.
        let t = std::fs::read_to_string(&tareas).unwrap();
        let t = set_done(&set_done(&t, "T1", true).unwrap(), "T2", true).unwrap();
        std::fs::write(&tareas, &t).unwrap();
        assert_eq!(status(r, "facturas").unwrap().phase, Phase::Verify);
        // Verificación en verde: hecho.
        let ok = |c: &str| CheckResult {
            command: c.into(),
            sources: vec![],
            ok: true,
            code: Some(0),
            tail: String::new(),
            millis: 1,
        };
        let v = Verification {
            time: 1,
            results: vec![ok("true"), ok("cargo test csv"), ok("cargo test vacio")],
        };
        save_verification(r, "facturas", &v).unwrap();
        let s = status(r, "facturas").unwrap();
        assert_eq!(s.phase, Phase::Done);
        assert_eq!(list(r)[0].verified, 2);
        // Cambia R2: toca replanificar.
        std::fs::write(
            dir(r, "facturas").join(SPEC_FILE),
            SPEC.replace("- cabecera", "- cabecera y BOM"),
        )
        .unwrap();
        let s = status(r, "facturas").unwrap();
        assert_eq!(s.phase, Phase::Replan);
        assert_eq!(s.drift.changed, ["R2"]);
        assert!(s.requirements[1].changed && !s.requirements[0].changed);
    }

    #[test]
    fn veredicto_con_un_fallo() {
        let spec = parse_spec("f", SPEC).unwrap();
        let tasks = crate::tasks::parse_tasks(TAREAS);
        let v = Verification {
            time: 1,
            results: vec![CheckResult {
                command: "cargo test csv".into(),
                sources: vec!["T1".into()],
                ok: false,
                code: Some(101),
                tail: "falla".into(),
                millis: 3,
            }],
        };
        let m = verdicts(&spec, &tasks, Some(&v));
        assert_eq!(m["R1"], Verdict::Failed);
        assert_eq!(m["R2"], Verdict::NotRun);
        let c = checks(&spec, &tasks);
        assert_eq!(c[0], ("true".to_string(), vec!["R1".to_string()]));
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn nombres_y_errores() {
        let d = proyecto();
        assert!(
            create(d.path(), "facturas", "x")
                .unwrap_err()
                .contains("ya existe")
        );
        assert!(create(d.path(), "Mal Nombre", "x").is_err());
        assert!(
            load(d.path(), "otra")
                .unwrap_err()
                .contains("pcode spec nueva otra")
        );
        std::fs::create_dir_all(d.path().join("specs/rota")).unwrap();
        std::fs::write(d.path().join("specs/rota/spec.md"), "# Rota\n").unwrap();
        let l = list(d.path());
        assert_eq!(l.len(), 2);
        let rota = l.iter().find(|s| s.name == "rota").unwrap();
        assert!(
            rota.phase.is_none() && rota.error.as_deref().unwrap().contains("no hay requisitos")
        );
        for ok in ["facturas", "api-v2", "r_1"] {
            assert!(valid_name(ok));
        }
        for no in ["", "-x", "Mayus", "con espacio", "../fuera"] {
            assert!(!valid_name(no), "{no}");
        }
    }
}
