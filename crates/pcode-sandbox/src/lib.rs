//! Mini sandbox N1: namespaces de usuario + overlay copy-on-write + raíz de solo lectura.
pub mod apply;
pub mod layer;
pub mod run;

pub use layer::{Change, ChangeKind, filter_ignored, read_changes};
pub use run::{Network, RunStatus, Sandbox, SandboxError, SandboxSpec, tmp_base, userns_available};
