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
    /// Igual que [`Provider::chat`], pero entrega el texto a `on_text` según llega. Por defecto
    /// (proveedores sin streaming) lo entrega entero al final.
    async fn chat_stream(
        &self,
        req: &ChatRequest,
        on_text: &(dyn for<'s> Fn(&'s str) + Send + Sync),
    ) -> Result<ChatResponse, ModelError> {
        let r = self.chat(req).await?;
        if !r.message.content.is_empty() {
            on_text(&r.message.content);
        }
        Ok(r)
    }
}
