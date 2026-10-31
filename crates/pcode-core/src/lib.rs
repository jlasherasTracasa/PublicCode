//! Bucle del agente con política, auditoría, presupuestos, hook de parada y detector de atasco.
pub mod agent;
pub mod checkpoint;
pub mod compress;
pub mod stop_hook;
pub mod stuck;

pub use agent::*;
pub use checkpoint::{Checkpoints, FileChange, TurnInfo};
pub use stop_hook::{StopHook, run_acceptance};
