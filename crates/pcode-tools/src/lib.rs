//! Tools nativas con efectos declarados.
pub mod bash;
pub mod edit;
pub mod read;
pub mod registry;
pub mod search;
pub mod write;

pub use bash::{Bash, head_tail};
pub use edit::EditFile;
pub use read::ReadFile;
pub use registry::*;
pub use search::{Glob, Grep};
pub use write::WriteFile;

/// Registro con las tools nativas de la fase F1.
pub fn native_registry() -> ToolRegistry {
    let mut r = ToolRegistry::new();
    let tools: Vec<std::sync::Arc<dyn Tool>> = vec![
        std::sync::Arc::new(ReadFile),
        std::sync::Arc::new(WriteFile),
        std::sync::Arc::new(EditFile),
        std::sync::Arc::new(Grep),
        std::sync::Arc::new(Glob),
        std::sync::Arc::new(Bash),
    ];
    for t in tools {
        r.register(t).expect("nombres únicos");
    }
    r
}
