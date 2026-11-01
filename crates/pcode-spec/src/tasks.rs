//! `tareas.md`: los pasos del plan, cada uno con los requisitos que cubre, los archivos que toca,
//! sus dependencias y la orden que lo comprueba. Lo genera el planificador, se puede editar a mano
//! y `[x]` marca las tareas hechas (con su `acepta` en verde).
//!
//! ```text
//! # Tareas: Exportar facturas a CSV
//!
//! <!-- spec: R1=1a2b3c4d R2=5e6f7a8b -->
//!
//! - [ ] T1. Serializar una factura a CSV
//!   - requisitos: R1
//!   - toca: src/csv.rs, tests/csv.rs
//!   - depende: —
//!   - acepta: `cargo test csv`
//!   - Qué hacer, en una o varias líneas.
//! ```
use crate::spec::{Spec, bullet, split_id};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Una tarea del plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecTask {
    /// Identificador (`T1`).
    pub id: String,
    /// Título.
    pub title: String,
    /// Hecha (`[x]`).
    pub done: bool,
    /// Requisitos que cubre.
    pub requirements: Vec<String>,
    /// Archivos que prevé tocar.
    pub touches: Vec<String>,
    /// Tareas que tienen que estar hechas antes.
    pub depends: Vec<String>,
    /// Orden que la comprueba.
    pub accept: Option<String>,
    /// Qué hacer (el resto de líneas).
    pub detail: String,
}

/// `tareas.md` ya leído.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TaskList {
    /// Tareas en el orden del archivo.
    pub tasks: Vec<SpecTask>,
    /// Huella de cada requisito cuando se planificó (`<!-- spec: R1=… -->`).
    pub spec_hashes: BTreeMap<String, String>,
}

impl TaskList {
    /// La tarea `id`.
    pub fn get(&self, id: &str) -> Option<&SpecTask> {
        self.tasks.iter().find(|t| t.id == id)
    }
    /// Cuántas están hechas.
    pub fn done(&self) -> usize {
        self.tasks.iter().filter(|t| t.done).count()
    }
}

fn list(v: &str) -> Vec<String> {
    v.split([',', ' '])
        .map(|x| x.trim().trim_matches('`'))
        .filter(|x| !x.is_empty() && !["—", "-", "–", "ninguna", "nada", "none"].contains(x))
        .map(String::from)
        .collect()
}

/// Lee `tareas.md` (con tolerancia: lo que no entiende lo deja en el detalle de la tarea).
pub fn parse_tasks(text: &str) -> TaskList {
    let text = text.replace("\r\n", "\n");
    let mut out = TaskList::default();
    if let Some(i) = text.find("<!-- spec:") {
        let rest = &text[i + "<!-- spec:".len()..];
        let end = rest.find("-->").unwrap_or(rest.len());
        for kv in rest[..end].split_whitespace() {
            if let Some((k, v)) = kv.split_once('=') {
                out.spec_hashes.insert(k.to_string(), v.to_string());
            }
        }
    }
    let mut detail: Vec<String> = vec![];
    // ¿La última tarea sigue recibiendo líneas? Una línea sin sangría que no es tarea la cierra.
    let mut open = false;
    let flush = |out: &mut TaskList, detail: &mut Vec<String>| {
        if let Some(t) = out.tasks.last_mut() {
            t.detail = detail.join("\n").trim().to_string();
        }
        detail.clear();
    };
    for line in text.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let t = line.trim();
        let checkbox = t
            .strip_prefix("- [ ] ")
            .map(|r| (false, r))
            .or_else(|| t.strip_prefix("- [x] ").map(|r| (true, r)))
            .or_else(|| t.strip_prefix("- [X] ").map(|r| (true, r)));
        if !indented
            && let Some((done, rest)) = checkbox
            && let Some((id, title)) = split_id(rest)
        {
            if open {
                flush(&mut out, &mut detail);
            }
            out.tasks.push(SpecTask {
                id,
                title,
                done,
                requirements: vec![],
                touches: vec![],
                depends: vec![],
                accept: None,
                detail: String::new(),
            });
            open = true;
            continue;
        }
        if !open {
            continue;
        }
        if !indented {
            if !t.is_empty() {
                flush(&mut out, &mut detail);
                open = false;
            }
            continue;
        }
        let Some(task) = out.tasks.last_mut() else {
            continue;
        };
        let item = bullet(t).unwrap_or(t);
        let (key, value) = item.split_once(':').unwrap_or(("", ""));
        match key.trim().to_lowercase().as_str() {
            "requisitos" | "requisito" | "requirements" => task.requirements = list(value),
            "toca" | "archivos" | "files" => task.touches = list(value),
            "depende" | "depende de" | "depends" => task.depends = list(value),
            "acepta" | "accept" => {
                let c = value.trim().trim_matches('`').trim();
                task.accept = (!c.is_empty()).then(|| c.to_string());
            }
            _ => detail.push(item.to_string()),
        }
    }
    if open {
        flush(&mut out, &mut detail);
    }
    out
}

/// Problemas del plan frente a la especificación, sin modelo: tareas sin `acepta`, requisitos sin
/// tarea o inexistentes, dependencias rotas o en ciclo. Vacío = el plan se puede ejecutar.
pub fn validate_plan(spec: &Spec, list: &TaskList) -> Vec<String> {
    let mut p = vec![];
    if list.tasks.is_empty() {
        p.push("no hay tareas (cada una empieza por «- [ ] T1. Título»)".to_string());
        return p;
    }
    let ids: HashSet<&str> = list.tasks.iter().map(|t| t.id.as_str()).collect();
    let mut seen = HashSet::new();
    for t in &list.tasks {
        if !seen.insert(t.id.as_str()) {
            p.push(format!("la tarea {} está repetida", t.id));
        }
        if t.accept.is_none() {
            p.push(format!(
                "{}: falta «acepta: `orden`» (cómo se comprueba que está hecha)",
                t.id
            ));
        }
        if t.requirements.is_empty() {
            p.push(format!("{}: no dice qué requisitos cubre", t.id));
        }
        for r in &t.requirements {
            if spec.requirement(r).is_none() {
                p.push(format!("{}: el requisito {r} no existe en spec.md", t.id));
            }
        }
        for d in &t.depends {
            if !ids.contains(d.as_str()) {
                p.push(format!("{} depende de {d}, que no existe", t.id));
            }
        }
    }
    for r in &spec.requirements {
        if !list.tasks.iter().any(|t| t.requirements.contains(&r.id)) {
            p.push(format!("{} no tiene ninguna tarea", r.id));
        }
    }
    if let Some(c) = cycle(list) {
        p.push(format!("dependencias en ciclo: {}", c.join(" → ")));
    }
    p
}

fn cycle(list: &TaskList) -> Option<Vec<String>> {
    let deps: HashMap<&str, &Vec<String>> = list
        .tasks
        .iter()
        .map(|t| (t.id.as_str(), &t.depends))
        .collect();
    // 0 = sin visitar, 1 = en la pila, 2 = terminado.
    let mut mark: HashMap<&str, u8> = HashMap::new();
    fn dfs<'a>(
        n: &'a str,
        deps: &HashMap<&'a str, &'a Vec<String>>,
        mark: &mut HashMap<&'a str, u8>,
        stack: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        mark.insert(n, 1);
        stack.push(n);
        for d in deps.get(n).map(|v| v.iter()).into_iter().flatten() {
            let Some((&k, _)) = deps.get_key_value(d.as_str()) else {
                continue;
            };
            match mark.get(k).copied().unwrap_or(0) {
                1 => {
                    let i = stack.iter().position(|x| *x == k).unwrap_or(0);
                    let mut c: Vec<String> = stack[i..].iter().map(|s| s.to_string()).collect();
                    c.push(k.to_string());
                    return Some(c);
                }
                0 => {
                    if let Some(c) = dfs(k, deps, mark, stack) {
                        return Some(c);
                    }
                }
                _ => {}
            }
        }
        stack.pop();
        mark.insert(n, 2);
        None
    }
    for t in &list.tasks {
        if mark.get(t.id.as_str()).copied().unwrap_or(0) == 0
            && let Some(c) = dfs(&t.id, &deps, &mut mark, &mut vec![])
        {
            return Some(c);
        }
    }
    None
}

/// Lo que cambió en la especificación desde que se planificó.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Drift {
    /// Requisitos cuyo texto cambió.
    pub changed: Vec<String>,
    /// Requisitos nuevos.
    pub added: Vec<String>,
    /// Requisitos que ya no están.
    pub removed: Vec<String>,
}

impl Drift {
    /// No hay nada que replanificar.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }
    /// Requisitos que hay que replanificar (cambiados y nuevos).
    pub fn to_replan(&self) -> Vec<String> {
        self.changed.iter().chain(&self.added).cloned().collect()
    }
}

/// Compara la especificación con las huellas guardadas en `tareas.md`. Sin huellas (un plan
/// escrito a mano), no hay deriva que medir.
pub fn drift(spec: &Spec, list: &TaskList) -> Drift {
    let mut d = Drift::default();
    if list.spec_hashes.is_empty() {
        return d;
    }
    for r in &spec.requirements {
        match list.spec_hashes.get(&r.id) {
            None => d.added.push(r.id.clone()),
            Some(h) if *h != r.hash => d.changed.push(r.id.clone()),
            _ => {}
        }
    }
    for id in list.spec_hashes.keys() {
        if spec.requirement(id).is_none() {
            d.removed.push(id.clone());
        }
    }
    d
}

/// La línea con las huellas de los requisitos.
pub fn hashes_line(spec: &Spec) -> String {
    let kv: Vec<String> = spec
        .requirements
        .iter()
        .map(|r| format!("{}={}", r.id, r.hash))
        .collect();
    format!("<!-- spec: {} -->", kv.join(" "))
}

/// `tareas.md` con las huellas de la especificación actual (sustituye las que hubiera; si no hay,
/// van tras el título).
pub fn stamp(tasks_text: &str, spec: &Spec) -> String {
    let line = hashes_line(spec);
    let text = tasks_text.replace("\r\n", "\n");
    let mut out: Vec<String> = vec![];
    let mut placed = false;
    for l in text.lines() {
        if l.trim_start().starts_with("<!-- spec:") {
            if !placed {
                out.push(line.clone());
                placed = true;
            }
            continue;
        }
        out.push(l.to_string());
    }
    if !placed {
        let at = out
            .iter()
            .position(|l| l.starts_with("# "))
            .map(|i| i + 1)
            .unwrap_or(0);
        out.insert(at, String::new());
        out.insert(at + 1, line);
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// `tareas.md` con la tarea `id` marcada como hecha o no (`None` si no está).
pub fn set_done(tasks_text: &str, id: &str, done: bool) -> Option<String> {
    let mut found = false;
    let lines: Vec<String> = tasks_text
        .replace("\r\n", "\n")
        .lines()
        .map(|l| {
            let t = l.trim_start();
            let rest = t
                .strip_prefix("- [ ] ")
                .or_else(|| t.strip_prefix("- [x] "))
                .or_else(|| t.strip_prefix("- [X] "));
            match rest.and_then(split_id) {
                Some((tid, _)) if !found && tid == id && l.len() == t.len() => {
                    found = true;
                    format!(
                        "- [{}] {}",
                        if done { "x" } else { " " },
                        rest.unwrap_or_default()
                    )
                }
                _ => l.to_string(),
            }
        })
        .collect();
    found.then(|| {
        let mut s = lines.join("\n");
        s.push('\n');
        s
    })
}

/// Las tareas pendientes en un orden que respeta las dependencias (las que dependen de una
/// pendiente van después). Las que tienen dependencias imposibles quedan fuera.
pub fn pending_in_order(list: &TaskList) -> Vec<&SpecTask> {
    let mut done: HashSet<&str> = list
        .tasks
        .iter()
        .filter(|t| t.done)
        .map(|t| t.id.as_str())
        .collect();
    let mut left: Vec<&SpecTask> = list.tasks.iter().filter(|t| !t.done).collect();
    let mut out = vec![];
    // Como mucho una vuelta por tarea: sin bucles sin cota.
    for _ in 0..=list.tasks.len() {
        let Some(i) = left
            .iter()
            .position(|t| t.depends.iter().all(|d| done.contains(d.as_str())))
        else {
            break;
        };
        let t = left.remove(i);
        done.insert(t.id.as_str());
        out.push(t);
    }
    out
}

/// Tareas afectadas por la deriva: las de requisitos cambiados o eliminados.
pub fn affected(list: &TaskList, d: &Drift) -> Vec<String> {
    list.tasks
        .iter()
        .filter(|t| {
            t.requirements
                .iter()
                .any(|r| d.changed.contains(r) || d.removed.contains(r))
        })
        .map(|t| t.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::parse_spec;

    const SPEC: &str =
        "# Facturas\n## Requisitos\n### R1. Exportar\n- csv\n### R2. Mes vacío\n- cabecera\n";
    const TAREAS: &str = "# Tareas: Facturas

- [x] T1. Serializar una factura
  - requisitos: R1
  - toca: src/csv.rs, tests/csv.rs
  - depende: —
  - acepta: `cargo test csv`
  - Añade `to_csv` a `Factura`.
  - Sin comas sin escapar.
- [ ] T2. Mes sin facturas
  - requisitos: R2
  - depende: T1
  - acepta: cargo test mes_vacio

Notas al final que no son de ninguna tarea.
";

    #[test]
    fn lee_tareas_con_sus_campos() {
        let l = parse_tasks(TAREAS);
        assert_eq!(l.tasks.len(), 2);
        let t1 = &l.tasks[0];
        assert!(t1.done);
        assert_eq!(t1.title, "Serializar una factura");
        assert_eq!(t1.requirements, ["R1"]);
        assert_eq!(t1.touches, ["src/csv.rs", "tests/csv.rs"]);
        assert!(t1.depends.is_empty());
        assert_eq!(t1.accept.as_deref(), Some("cargo test csv"));
        assert_eq!(
            t1.detail,
            "Añade `to_csv` a `Factura`.\nSin comas sin escapar."
        );
        let t2 = &l.tasks[1];
        assert!(!t2.done);
        assert_eq!(t2.depends, ["T1"]);
        assert_eq!(t2.accept.as_deref(), Some("cargo test mes_vacio"));
        assert_eq!(t2.detail, "", "las notas sin sangría no son de la tarea");
        assert_eq!(l.done(), 1);
    }

    #[test]
    fn valida_cobertura_dependencias_y_ciclos() {
        let s = parse_spec("f", SPEC).unwrap();
        assert!(validate_plan(&s, &parse_tasks(TAREAS)).is_empty());
        let malo = "- [ ] T1. a\n  - requisitos: R1, R9\n  - depende: T3\n- [ ] T2. b\n  - requisitos: R1\n  - depende: T4\n  - acepta: x\n- [ ] T4. c\n  - requisitos: R1\n  - depende: T2\n  - acepta: y\n";
        let p = validate_plan(&s, &parse_tasks(malo));
        let txt = p.join("\n");
        assert!(txt.contains("T1: falta «acepta"), "{txt}");
        assert!(txt.contains("T1: el requisito R9 no existe"));
        assert!(txt.contains("T1 depende de T3, que no existe"));
        assert!(txt.contains("R2 no tiene ninguna tarea"));
        assert!(txt.contains("dependencias en ciclo: T2 → T4 → T2"), "{txt}");
        assert_eq!(
            validate_plan(&s, &parse_tasks("# nada")),
            ["no hay tareas (cada una empieza por «- [ ] T1. Título»)"]
        );
    }

    #[test]
    fn deriva_y_huellas() {
        let s = parse_spec("f", SPEC).unwrap();
        // Un plan sin huellas (escrito a mano): no se mide.
        assert!(drift(&s, &parse_tasks(TAREAS)).is_empty());
        let sellado = stamp(TAREAS, &s);
        assert!(sellado.starts_with("# Tareas: Facturas\n\n<!-- spec: R1="));
        let l = parse_tasks(&sellado);
        assert!(drift(&s, &l).is_empty());
        assert_eq!(
            stamp(&sellado, &s),
            sellado,
            "sellar dos veces no duplica la línea"
        );
        // R1 cambia, R2 desaparece, R3 es nuevo.
        let s2 = parse_spec(
            "f",
            "## Requisitos\n### R1. Exportar\n- csv con BOM\n### R3. Por cliente\n- filtro\n",
        )
        .unwrap();
        let d = drift(&s2, &l);
        assert_eq!(d.changed, ["R1"]);
        assert_eq!(d.added, ["R3"]);
        assert_eq!(d.removed, ["R2"]);
        assert_eq!(d.to_replan(), ["R1", "R3"]);
        assert_eq!(affected(&l, &d), ["T1", "T2"]);
    }

    #[test]
    fn marcar_hecha_y_orden_de_pendientes() {
        let t = set_done(TAREAS, "T2", true).unwrap();
        assert!(t.contains("- [x] T2. Mes sin facturas"));
        assert!(t.contains("- [x] T1."), "lo demás no cambia");
        let t = set_done(&t, "T1", false).unwrap();
        assert!(t.contains("- [ ] T1. Serializar"));
        assert!(set_done(TAREAS, "T9", true).is_none());
        let l = parse_tasks(
            "- [ ] T3. c\n  - depende: T2\n- [ ] T2. b\n  - depende: T1\n- [x] T1. a\n- [ ] T4. d\n  - depende: T9\n",
        );
        let ids: Vec<&str> = pending_in_order(&l).iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["T2", "T3"], "T4 depende de algo que no existe");
    }
}
