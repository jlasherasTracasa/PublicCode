//! DAG de tareas.
use crate::task::TaskDef;
use std::collections::{BTreeMap, HashSet};

/// Grafo de tareas validado.
pub struct Dag {
    tasks: BTreeMap<String, TaskDef>,
}

impl Dag {
    /// Valida ciclos y dependencias inexistentes.
    pub fn new(tasks: Vec<TaskDef>) -> Result<Dag, String> {
        let map: BTreeMap<String, TaskDef> = tasks.into_iter().map(|t| (t.id.clone(), t)).collect();
        for t in map.values() {
            for d in &t.depends {
                if !map.contains_key(d) {
                    return Err(format!("{}: depende de {d}, que no existe", t.id));
                }
            }
        }
        fn visit<'a>(
            m: &'a BTreeMap<String, TaskDef>,
            id: &'a str,
            state: &mut BTreeMap<&'a str, u8>,
            path: &mut Vec<&'a str>,
        ) -> Result<(), String> {
            match state.get(id) {
                Some(2) => return Ok(()),
                Some(1) => {
                    path.push(id);
                    return Err(format!("ciclo: {}", path.join(" → ")));
                }
                _ => {}
            }
            state.insert(id, 1);
            path.push(id);
            for d in &m[id].depends {
                visit(m, d, state, path)?;
            }
            path.pop();
            state.insert(id, 2);
            Ok(())
        }
        let mut st = BTreeMap::new();
        for id in map.keys() {
            visit(&map, id, &mut st, &mut vec![])?;
        }
        Ok(Dag { tasks: map })
    }
    /// Tarea por id.
    pub fn get(&self, id: &str) -> Option<&TaskDef> {
        self.tasks.get(id)
    }
    /// Todas las tareas.
    pub fn all(&self) -> impl Iterator<Item = &TaskDef> {
        self.tasks.values()
    }
    /// Tareas listas: dependencias hechas y ni hechas, ni en marcha, ni bloqueadas (orden estable).
    pub fn ready(
        &self,
        done: &HashSet<String>,
        running: &HashSet<String>,
        blocked: &HashSet<String>,
    ) -> Vec<&TaskDef> {
        self.tasks
            .values()
            .filter(|t| {
                !done.contains(&t.id) && !running.contains(&t.id) && !blocked.contains(&t.id)
            })
            .filter(|t| t.depends.iter().all(|d| done.contains(d)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Mode;
    fn t(id: &str, deps: &[&str]) -> TaskDef {
        TaskDef {
            id: id.into(),
            accept: "true".into(),
            mode: Mode::Isolated,
            depends: deps.iter().map(|s| s.to_string()).collect(),
            touches: vec![],
            backend: None,
            skills: vec![],
            budget_minutes: None,
            body: String::new(),
        }
    }
    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn dag_ciclo_con_camino() {
        let e = Dag::new(vec![t("a", &["b"]), t("b", &["a"])])
            .err()
            .unwrap();
        assert!(e.contains("ciclo") && e.contains("a") && e.contains("b"));
    }
    #[test]
    fn dag_dependencia_inexistente() {
        assert!(
            Dag::new(vec![t("a", &["zz"])])
                .err()
                .unwrap()
                .contains("no existe")
        );
    }
    #[test]
    fn dag_ready_respeta_dependencias() {
        let d = Dag::new(vec![t("a", &[]), t("b", &["a"])]).unwrap();
        let r: Vec<_> = d
            .ready(&set(&[]), &set(&[]), &set(&[]))
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(r, ["a"]);
        let r: Vec<_> = d
            .ready(&set(&["a"]), &set(&[]), &set(&[]))
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(r, ["b"]);
    }
    #[test]
    fn dag_ready_orden_estable() {
        let d = Dag::new(vec![t("c", &[]), t("a", &[]), t("b", &[])]).unwrap();
        let r: Vec<_> = d
            .ready(&set(&[]), &set(&["b"]), &set(&[]))
            .iter()
            .map(|t| t.id.clone())
            .collect();
        assert_eq!(r, ["a", "c"]);
    }
    #[test]
    fn dag_acepta_el_plan_real() {
        let ts = crate::task::load_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tareas"),
        )
        .unwrap();
        assert!(Dag::new(ts).is_ok());
    }
}
