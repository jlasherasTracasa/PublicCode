//! Flujos de tareas: DAG, estado por eventos, planificación, aceptación, reintentos, integración, CI.
pub mod accept;
pub mod ci;
pub mod dag;
pub mod events;
pub mod integrate;
pub mod retry;
pub mod runner;
pub mod scheduler;
pub mod task;

pub use dag::Dag;
pub use events::{FlowEvent, FlowState, TaskState};
pub use task::{Mode, TaskDef};
