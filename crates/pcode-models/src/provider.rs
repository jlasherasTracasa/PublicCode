//! Trait `Provider` y errores.
use crate::types::{ChatRequest, ChatResponse};

/// Errores de un proveedor.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Error HTTP.
    #[error("error HTTP: {0}")]
    Http(String),
    /// Respuesta inválida.
    #[error("respuesta inválida: {0}")]
    InvalidResponse(String),
    /// Límite de peticiones.
    #[error("límite de peticiones del proveedor")]
    RateLimited,
    /// El guion del mock se agotó.
    #[error("el guion del mock se ha agotado")]
    MockExhausted,
}

/// Un proveedor de modelos.
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Nombre.
    fn name(&self) -> &str;
    /// Una petición de chat.
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError>;
}
