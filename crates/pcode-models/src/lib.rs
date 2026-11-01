//! Modelos: tipos neutros, proveedores y conversión de formatos.
pub mod anthropic;
pub mod discover;
pub mod gemini;
pub mod mock;
pub mod openai;
pub mod openai_http;
pub mod provider;
pub mod repair;
pub mod retry;
pub mod sse;
mod stream;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use mock::MockProvider;
pub use openai_http::OpenAiCompatProvider;
pub use provider::{ModelError, Provider};
pub use retry::RetryProvider;
pub use types::*;
