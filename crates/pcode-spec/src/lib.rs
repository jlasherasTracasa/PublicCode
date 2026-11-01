//! Desarrollo guiado por especificaciones: la persona escribe solo `specs/<nombre>/spec.md` (qué
//! debe hacer el sistema y cómo se comprueba) y PublicCode genera el resto.
//!
//! | Paso | Archivo | Quién |
//! |---|---|---|
//! | Especificar | `spec.md`: objetivo, requisitos `### R1.` con criterios (y `acepta:` ejecutables), restricciones | la persona (o ✦ un borrador desde una frase) |
//! | Planificar | `plan.md` (diseño) y `tareas.md` (pasos con requisitos, archivos, dependencias y `acepta`) | el planificador, validado sin modelo |
//! | Implementar | el código y los tests, tarea a tarea, con punto de control | el agente (o Copilot, Claude Code…) |
//! | Verificar | todas las comprobaciones, con el veredicto de cada requisito | PublicCode |
//!
//! Este crate es la parte determinista: formato, validación del plan, deriva entre la
//! especificación y el plan, trazabilidad y prompts. La ejecución está en `pcode spec`.
pub mod prompts;
pub mod spec;
pub mod status;
pub mod tasks;

pub use spec::{Criterion, Requirement, Spec, parse_spec, template};
pub use status::{
    CheckResult, PLAN_FILE, Phase, SPEC_FILE, SPECS_DIR, SpecStatus, SpecSummary, TASKS_FILE,
    Verdict, Verification, list, load, status,
};
pub use tasks::{Drift, SpecTask, TaskList, drift, parse_tasks, validate_plan};
