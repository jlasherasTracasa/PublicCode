//! Cola de aprobaciones: las decisiones `Ask` esperan a un humano desde cualquier cliente.
use pcode_protocol::{ApprovalDto, Event};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};

/// Gestor de aprobaciones pendientes.
pub struct ApprovalBroker {
    pending: Mutex<BTreeMap<u64, (ApprovalDto, oneshot::Sender<bool>)>>,
    next: Mutex<u64>,
    events: broadcast::Sender<Event>,
    timeout: Duration,
}

impl ApprovalBroker {
    /// Crea el gestor.
    pub fn new(events: broadcast::Sender<Event>, timeout: Duration) -> Arc<Self> {
        Arc::new(ApprovalBroker {
            pending: Mutex::new(BTreeMap::new()),
            next: Mutex::new(1),
            events,
            timeout,
        })
    }
    /// Pide una aprobación y espera la decisión (vencido el tiempo = denegar).
    pub async fn request(
        &self,
        session: i64,
        action: &str,
        reason: &str,
        provenance: &str,
    ) -> bool {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut n = self.next.lock().unwrap();
            let id = *n;
            *n += 1;
            id
        };
        let dto = ApprovalDto {
            id,
            session,
            action: action.into(),
            reason: reason.into(),
            provenance: provenance.into(),
        };
        self.pending.lock().unwrap().insert(id, (dto.clone(), tx));
        let _ = self.events.send(Event::ApprovalRequested { approval: dto });
        let allowed = matches!(tokio::time::timeout(self.timeout, rx).await, Ok(Ok(true)));
        self.pending.lock().unwrap().remove(&id);
        let _ = self.events.send(Event::ApprovalResolved { id, allowed });
        allowed
    }
    /// Aprobaciones pendientes.
    pub fn pending(&self) -> Vec<ApprovalDto> {
        self.pending
            .lock()
            .unwrap()
            .values()
            .map(|(d, _)| d.clone())
            .collect()
    }
    /// Decide una aprobación; `false` si no existe.
    pub fn decide(&self, id: u64, allow: bool) -> bool {
        match self.pending.lock().unwrap().remove(&id) {
            Some((_, tx)) => tx.send(allow).is_ok(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn broker(ms: u64) -> (Arc<ApprovalBroker>, broadcast::Receiver<Event>) {
        let (tx, rx) = broadcast::channel(16);
        (ApprovalBroker::new(tx, Duration::from_millis(ms)), rx)
    }
    #[tokio::test]
    async fn aprobacion_permitida() {
        let (b, _rx) = broker(5000);
        let b2 = b.clone();
        let h = tokio::spawn(async move {
            b2.request(1, "git push", "irreversible", "agente:s-1")
                .await
        });
        while b.pending().is_empty() {
            tokio::task::yield_now().await;
        }
        assert!(b.decide(b.pending()[0].id, true));
        assert!(h.await.unwrap());
    }
    #[tokio::test]
    async fn aprobacion_denegada() {
        let (b, _rx) = broker(5000);
        let b2 = b.clone();
        let h = tokio::spawn(async move { b2.request(1, "a", "r", "p").await });
        while b.pending().is_empty() {
            tokio::task::yield_now().await;
        }
        b.decide(b.pending()[0].id, false);
        assert!(!h.await.unwrap());
    }
    #[tokio::test]
    async fn aprobacion_vence_y_deniega() {
        let (b, _rx) = broker(50);
        assert!(!b.request(1, "a", "r", "p").await);
        assert!(b.pending().is_empty());
    }
    #[test]
    fn decide_id_inexistente_false() {
        assert!(!broker(10).0.decide(99, true));
    }
    #[tokio::test]
    async fn emite_eventos() {
        let (b, mut rx) = broker(50);
        b.request(1, "a", "r", "p").await;
        assert!(matches!(
            rx.recv().await.unwrap(),
            Event::ApprovalRequested { .. }
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            Event::ApprovalResolved { allowed: false, .. }
        ));
    }
}
