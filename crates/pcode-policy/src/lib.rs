//! Política de capacidades, análisis de shell y auditoría encadenada.
pub mod audit;
pub mod caps;
pub mod shell;

pub use audit::{AuditEntry, AuditError, AuditLog};
pub use caps::{Capabilities, Decision, Policy};
