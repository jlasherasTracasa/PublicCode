//! Backends: agentes externos (Claude Code, Copilot, Gemini, opencode) y modelo propio.
//!
//! Las órdenes y los formatos de salida de las CLIs de terceros se **verifican en F0.03**
//! (`docs/backends-verificado.md`); aquí se fijan según la documentación pública conocida y los
//! parsers son tolerantes (ignoran lo que no reconocen).
pub mod claude_code;
pub mod copilot_cli;
pub mod gemini_cli;
pub mod mock;
pub mod model;
pub mod opencode;
pub mod types;

pub use mock::MockBackend;
pub use types::*;
