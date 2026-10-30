//! Modelos: tipos neutros, proveedores y conversión de formatos.
pub mod mock;
pub mod openai;
pub mod openai_http;
pub mod provider;
pub mod repair;
pub mod sse;
pub mod types;

pub use mock::MockProvider;
pub use openai_http::OpenAiCompatProvider;
pub use provider::{ModelError, Provider};
pub use types::*;
