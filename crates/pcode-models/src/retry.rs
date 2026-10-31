//! Reintentos con espera exponencial ante límites de peticiones, errores 5xx y fallos de red.
use crate::provider::{ModelError, Provider};
use crate::types::{ChatRequest, ChatResponse};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Envuelve un proveedor y reintenta los errores transitorios.
pub struct RetryProvider {
    inner: Arc<dyn Provider>,
    attempts: u32,
    base: Duration,
}

impl RetryProvider {
    /// `attempts` intentos en total; la espera empieza en `base` y se duplica.
    pub fn new(inner: Arc<dyn Provider>, attempts: u32, base: Duration) -> Self {
        RetryProvider {
            inner,
            attempts: attempts.max(1),
            base,
        }
    }
}

/// Si merece la pena reintentar: 429, 5xx (incluido 529, sobrecarga) y errores de red. Los 4xx
/// (clave mala, petición inválida) no se arreglan reintentando.
pub fn is_transient(e: &ModelError) -> bool {
    match e {
        ModelError::RateLimited => true,
        ModelError::Http(m) => {
            let code: String = m.chars().take_while(|c| c.is_ascii_digit()).collect();
            match code.parse::<u16>() {
                Ok(c) => c >= 500 || c == 408,
                Err(_) => true,
            }
        }
        _ => false,
    }
}

#[async_trait::async_trait]
impl Provider for RetryProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse, ModelError> {
        let mut wait = self.base;
        for n in 1..=self.attempts {
            match self.inner.chat(req).await {
                Err(e) if n < self.attempts && is_transient(&e) => {
                    tracing::warn!("{e}; reintento {n} en {wait:?}");
                    tokio::time::sleep(wait).await;
                    wait *= 2;
                }
                r => return r,
            }
        }
        unreachable!("el bucle siempre devuelve en el último intento")
    }
    async fn chat_stream(
        &self,
        req: &ChatRequest,
        on_text: &(dyn for<'s> Fn(&'s str) + Send + Sync),
    ) -> Result<ChatResponse, ModelError> {
        let mut wait = self.base;
        for n in 1..=self.attempts {
            // Solo se reintenta si aún no se ha entregado texto: si no, se duplicaría.
            let sent = AtomicBool::new(false);
            let cb = |t: &str| {
                sent.store(true, Ordering::Relaxed);
                on_text(t)
            };
            match self.inner.chat_stream(req, &cb).await {
                Err(e)
                    if n < self.attempts && is_transient(&e) && !sent.load(Ordering::Relaxed) =>
                {
                    tracing::warn!("{e}; reintento {n} en {wait:?}");
                    tokio::time::sleep(wait).await;
                    wait *= 2;
                }
                r => return r,
            }
        }
        unreachable!("el bucle siempre devuelve en el último intento")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, StopReason, Usage};
    use std::sync::Mutex;

    struct Flaky {
        errors: Mutex<Vec<ModelError>>,
        calls: Mutex<u32>,
    }

    #[async_trait::async_trait]
    impl Provider for Flaky {
        fn name(&self) -> &str {
            "flaky"
        }
        async fn chat(&self, _: &ChatRequest) -> Result<ChatResponse, ModelError> {
            *self.calls.lock().unwrap() += 1;
            if let Some(e) = self.errors.lock().unwrap().pop() {
                return Err(e);
            }
            Ok(ChatResponse {
                message: Message::assistant("ok"),
                usage: Usage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn flaky(errors: Vec<ModelError>) -> Arc<Flaky> {
        Arc::new(Flaky {
            errors: Mutex::new(errors),
            calls: Mutex::new(0),
        })
    }

    #[tokio::test]
    async fn reintenta_429_y_5xx_hasta_acertar() {
        let f = flaky(vec![
            ModelError::Http("529: overloaded".into()),
            ModelError::RateLimited,
        ]);
        let p = RetryProvider::new(f.clone(), 4, Duration::from_millis(1));
        let r = p.chat(&ChatRequest::default()).await.unwrap();
        assert_eq!(r.message.content, "ok");
        assert_eq!(*f.calls.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn no_reintenta_un_400_ni_pasa_del_maximo() {
        let f = flaky(vec![ModelError::Http("401: clave inválida".into())]);
        let p = RetryProvider::new(f.clone(), 4, Duration::from_millis(1));
        assert!(p.chat(&ChatRequest::default()).await.is_err());
        assert_eq!(*f.calls.lock().unwrap(), 1);
        let f = flaky((0..3).map(|_| ModelError::RateLimited).collect());
        let p = RetryProvider::new(f.clone(), 2, Duration::from_millis(1));
        assert!(p.chat(&ChatRequest::default()).await.is_err());
        assert_eq!(*f.calls.lock().unwrap(), 2);
    }

    #[test]
    fn clasifica_errores_transitorios() {
        assert!(is_transient(&ModelError::Http(
            "error sending request".into()
        )));
        assert!(is_transient(&ModelError::Http("503: x".into())));
        assert!(!is_transient(&ModelError::Http("404: x".into())));
        assert!(!is_transient(&ModelError::InvalidResponse("x".into())));
    }
}
