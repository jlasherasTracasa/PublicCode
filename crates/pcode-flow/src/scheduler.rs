//! Planificador de ejecución (puro): paralelismo por modo y exclusión de kernel.
use crate::dag::Dag;
use crate::events::{FlowState, TaskState};
use crate::task::Mode;
use std::collections::HashSet;

/// Límites.
pub struct Limits {
    /// Tareas en marcha como máximo.
    pub max_parallel: usize,
    /// Intentos máximos antes de bloquear.
    pub max_attempts: u32,
}

/// Banderas del entorno.
pub struct Flags {
    /// El coordinador está editando el árbol principal.
    pub kernel_busy: bool,
    /// Parada pedida.
    pub stop: bool,
    /// El proveedor de modelos no responde.
    pub provider_down: bool,
}

/// Qué hacer ahora.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Tareas a lanzar.
    pub launch: Vec<String>,
    /// Tareas que esperan al coordinador.
    pub needs_coordinator: Vec<String>,
    /// Tareas que pasan a bloqueadas.
    pub newly_blocked: Vec<String>,
}

/// Decide qué lanzar.
pub fn plan(dag: &Dag, state: &FlowState, limits: &Limits, flags: &Flags) -> Plan {
    let mut p = Plan::default();
    let done = state.done();
    let running: HashSet<String> = state
        .tasks
        .iter()
        .filter(|(_, s)| matches!(s, TaskState::Running(_) | TaskState::Accepted))
        .map(|(k, _)| k.clone())
        .collect();
    let blocked: HashSet<String> = state
        .tasks
        .iter()
        .filter(|(_, s)| matches!(s, TaskState::Blocked(_)))
        .map(|(k, _)| k.clone())
        .collect();
    let mut ready = dag.ready(&done, &running, &blocked);
    // Las de arreglo de CI van antes que cualquier otra.
    ready.sort_by_key(|t| (!t.id.starts_with("CI-arreglo"), t.id.clone()));
    let kernel_running = running
        .iter()
        .any(|id| dag.get(id).is_some_and(|t| t.mode == Mode::Kernel));
    let mut n_running = running.len();
    let mut kernel_taken = kernel_running || flags.kernel_busy;
    for t in ready {
        if state.attempts.get(&t.id).copied().unwrap_or(0) >= limits.max_attempts {
            p.newly_blocked.push(t.id.clone());
            continue;
        }
        if t.mode == Mode::Coordinator {
            p.needs_coordinator.push(t.id.clone());
            continue;
        }
        if flags.stop || flags.provider_down || n_running >= limits.max_parallel {
            continue;
        }
        if t.mode == Mode::Kernel {
            if kernel_taken {
                continue;
            }
            kernel_taken = true;
        }
        n_running += 1;
        p.launch.push(t.id.clone());
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::FlowEvent;
    use crate::task::TaskDef;
    fn t(id: &str, mode: Mode) -> TaskDef {
        TaskDef {
            id: id.into(),
            accept: "true".into(),
            mode,
            depends: vec![],
            touches: vec![],
            backend: None,
            skills: vec![],
            budget_minutes: None,
            body: String::new(),
        }
    }
    fn lim(n: usize) -> Limits {
        Limits {
            max_parallel: n,
            max_attempts: 3,
        }
    }
    fn fl() -> Flags {
        Flags {
            kernel_busy: false,
            stop: false,
            provider_down: false,
        }
    }
    fn run(id: &str, i: u32) -> FlowEvent {
        FlowEvent::Lanzada {
            tarea: id.into(),
            intento: i,
            backend: "b".into(),
        }
    }
    #[test]
    fn plan_respeta_max_parallel() {
        let d = Dag::new(vec![
            t("a", Mode::Isolated),
            t("b", Mode::Isolated),
            t("c", Mode::Isolated),
        ])
        .unwrap();
        assert_eq!(
            plan(&d, &FlowState::default(), &lim(2), &fl()).launch,
            ["a", "b"]
        );
    }
    #[test]
    fn plan_un_kernel_a_la_vez() {
        let d = Dag::new(vec![
            t("a", Mode::Kernel),
            t("b", Mode::Kernel),
            t("c", Mode::Isolated),
        ])
        .unwrap();
        assert_eq!(
            plan(&d, &FlowState::default(), &lim(9), &fl()).launch,
            ["a", "c"]
        );
    }
    #[test]
    fn plan_kernel_ocupado() {
        let d = Dag::new(vec![t("a", Mode::Kernel), t("c", Mode::Isolated)]).unwrap();
        let f = Flags {
            kernel_busy: true,
            ..fl()
        };
        assert_eq!(plan(&d, &FlowState::default(), &lim(9), &f).launch, ["c"]);
    }
    #[test]
    fn plan_ci_arreglo_primero() {
        let d = Dag::new(vec![
            t("A", Mode::Kernel),
            t("CI-arreglo-abc", Mode::Kernel),
        ])
        .unwrap();
        assert_eq!(
            plan(&d, &FlowState::default(), &lim(9), &fl()).launch,
            ["CI-arreglo-abc"]
        );
    }
    #[test]
    fn plan_coordinador_no_se_lanza() {
        let d = Dag::new(vec![t("a", Mode::Coordinator)]).unwrap();
        let p = plan(&d, &FlowState::default(), &lim(9), &fl());
        assert!(p.launch.is_empty() && p.needs_coordinator == ["a"]);
    }
    #[test]
    fn plan_bloquea_tras_max_intentos() {
        let d = Dag::new(vec![t("a", Mode::Isolated)]).unwrap();
        let s = FlowState::replay(&[
            run("a", 3),
            FlowEvent::AceptacionFallo {
                tarea: "a".into(),
                intento: 3,
                resumen: String::new(),
            },
        ]);
        let p = plan(&d, &s, &lim(9), &fl());
        assert!(p.launch.is_empty() && p.newly_blocked == ["a"]);
    }
    #[test]
    fn plan_stop_no_lanza() {
        let d = Dag::new(vec![t("a", Mode::Isolated)]).unwrap();
        assert!(
            plan(
                &d,
                &FlowState::default(),
                &lim(9),
                &Flags { stop: true, ..fl() }
            )
            .launch
            .is_empty()
        );
    }
}
