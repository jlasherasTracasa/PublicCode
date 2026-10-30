//! Proveedor falso con guion.
use crate::provider::{ModelError, Provider};
use crate::types::{ChatRequest, ChatResponse};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Devuelve respuestas en orden y guarda las peticiones.
pub struct MockProvider {
    script: Mutex<VecDeque<ChatResponse>>,
    seen: Mutex<Vec<ChatRequest>>,
}

impl MockProvider {
    /// Crea un mock con un guion.
    pub fn new(script: Vec<ChatResponse>) -> Self {
        MockProvider {
            script: Mutex::new(script.into()),
            seen: Mutex::new(vec![]),
        }
    }
    /// Carga el guion desde un JSON (array de `ChatResponse`).
    pub fn from_script_file(path: &std::path::Path) -> Result<Self, ModelError> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        let v: Vec<ChatResponse> =
            serde_json::from_str(&s).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        Ok(Self::new(v))
    }
    /// Peticiones recibidas.
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError> {
        self.seen.lock().unwrap().push(req.clone());
        self.script
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(ModelError::MockExhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    fn r(t: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(t),
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
        }
    }
    #[tokio::test]
    async fn mock_devuelve_en_orden() {
        let m = MockProvider::new(vec![r("a"), r("b")]);
        let q = ChatRequest::default();
        assert_eq!(m.chat(&q).await.unwrap().message.content, "a");
        assert_eq!(m.chat(&q).await.unwrap().message.content, "b");
    }
    #[tokio::test]
    async fn mock_agotado_da_error() {
        let m = MockProvider::new(vec![]);
        assert!(matches!(
            m.chat(&ChatRequest::default()).await,
            Err(ModelError::MockExhausted)
        ));
    }
    #[tokio::test]
    async fn mock_guarda_peticiones() {
        let m = MockProvider::new(vec![r("a")]);
        m.chat(&ChatRequest {
            model: "x".into(),
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(m.requests()[0].model, "x");
    }
    #[tokio::test]
    async fn mock_desde_archivo() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        std::fs::write(&p, serde_json::to_string(&vec![r("z")]).unwrap()).unwrap();
        let m = MockProvider::from_script_file(&p).unwrap();
        assert_eq!(
            m.chat(&ChatRequest::default())
                .await
                .unwrap()
                .message
                .content,
            "z"
        );
    }
}
