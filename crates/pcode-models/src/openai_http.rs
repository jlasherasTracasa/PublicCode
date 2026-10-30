//! Cliente HTTP OpenAI-compatible (LiteLLM, vLLM, Ollama).
use crate::openai;
use crate::provider::{ModelError, Provider};
use crate::types::{ChatRequest, ChatResponse};
use std::time::Duration;

/// Proveedor OpenAI-compatible.
pub struct OpenAiCompatProvider {
    name: String,
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    /// Crea el proveedor.
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        OpenAiCompatProvider {
            name: name.into(),
            base_url: base_url.into(),
            api_key,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()
                .unwrap(),
        }
    }
    /// Cambia el tiempo máximo.
    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.client = reqwest::Client::builder().timeout(t).build().unwrap();
        self
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.name
    }
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut rb = self.client.post(url).json(&openai::to_request_body(req));
        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        }
        let resp = rb
            .send()
            .await
            .map_err(|e| ModelError::Http(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(ModelError::RateLimited);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| ModelError::Http(e.to_string()))?;
        if !status.is_success() {
            let cut: String = text.chars().take(500).collect();
            return Err(ModelError::Http(format!("{}: {}", status.as_u16(), cut)));
        }
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| ModelError::InvalidResponse(e.to_string()))?;
        openai::from_response_body(&v)
    }
}

#[cfg(test)]
mod openai_http_tests {
    use super::*;
    use crate::types::Message;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Servidor falso: devuelve (cabeceras recibidas, respuesta fija).
    async fn fake(status: &str, body: &str) -> (String, tokio::task::JoinHandle<String>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let mut got = 0;
            loop {
                let n = s.read(&mut buf[got..]).await.unwrap();
                got += n;
                let t = String::from_utf8_lossy(&buf[..got]).to_string();
                if let Some(i) = t.find("\r\n\r\n") {
                    let cl = t[..i]
                        .to_lowercase()
                        .split("content-length:")
                        .nth(1)
                        .and_then(|x| x.lines().next())
                        .and_then(|x| x.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if got >= i + 4 + cl {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            s.write_all(resp.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..got]).to_string()
        });
        (url, h)
    }
    fn req() -> ChatRequest {
        ChatRequest {
            model: "m".into(),
            messages: vec![Message::user("hola")],
            ..Default::default()
        }
    }
    const OK: &str = r#"{"choices":[{"message":{"content":"hey"},"finish_reason":"stop"}]}"#;

    #[tokio::test]
    async fn openai_http_envia_y_parsea() {
        let (url, h) = fake("200 OK", OK).await;
        let p = OpenAiCompatProvider::new("t", url, None);
        assert_eq!(p.chat(&req()).await.unwrap().message.content, "hey");
        assert!(h.await.unwrap().contains("POST /chat/completions"));
    }
    #[tokio::test]
    async fn openai_http_cabecera_authorization() {
        let (url, h) = fake("200 OK", OK).await;
        OpenAiCompatProvider::new("t", url, Some("sekret".into()))
            .chat(&req())
            .await
            .unwrap();
        assert!(
            h.await
                .unwrap()
                .to_lowercase()
                .contains("authorization: bearer sekret")
        );
    }
    #[tokio::test]
    async fn openai_http_sin_clave_no_envia_authorization() {
        let (url, h) = fake("200 OK", OK).await;
        OpenAiCompatProvider::new("t", url, None)
            .chat(&req())
            .await
            .unwrap();
        assert!(!h.await.unwrap().to_lowercase().contains("authorization"));
    }
    #[tokio::test]
    async fn openai_http_429_es_rate_limited() {
        let (url, _h) = fake("429 Too Many Requests", "{}").await;
        assert!(matches!(
            OpenAiCompatProvider::new("t", url, None).chat(&req()).await,
            Err(ModelError::RateLimited)
        ));
    }
    #[tokio::test]
    async fn openai_http_500_es_http_con_cuerpo() {
        let (url, _h) = fake("500 Internal Server Error", "boom").await;
        match OpenAiCompatProvider::new("t", url, None).chat(&req()).await {
            Err(ModelError::Http(m)) => assert!(m.contains("500") && m.contains("boom")),
            o => panic!("{o:?}"),
        }
    }
}
