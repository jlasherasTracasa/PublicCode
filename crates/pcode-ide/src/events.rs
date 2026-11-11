//! Eventos del demonio por SSE (`GET /eventos`), leídos en un hilo: nada de sondeo en reposo.
use crate::rpc::Client;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

/// Reintentos de conexión como mucho (con 2 s entre uno y otro: más de un día).
const MAX_RECONNECTS: u32 = 50_000;

/// Acumula líneas SSE y devuelve el `data:` de cada evento completo (línea en blanco).
#[derive(Debug, Default)]
pub struct SseParser {
    data: Vec<String>,
}

impl SseParser {
    /// Procesa una línea (sin el salto); `Some(datos)` cuando termina un evento con datos.
    pub fn line(&mut self, l: &str) -> Option<String> {
        let l = l.trim_end_matches('\r');
        if l.is_empty() {
            if self.data.is_empty() {
                return None;
            }
            return Some(std::mem::take(&mut self.data).join("\n"));
        }
        if let Some(d) = l.strip_prefix("data:") {
            self.data.push(d.strip_prefix(' ').unwrap_or(d).to_string());
        }
        None
    }
}

/// Lee `/eventos` y llama a `on_event` con cada evento (JSON). Se reconecta si se corta y termina
/// cuando `on_event` devuelve `false` (la ventana se cerró).
pub fn listen(client: &Client, mut on_event: impl FnMut(serde_json::Value) -> bool) {
    for _ in 0..MAX_RECONNECTS {
        if let Ok(mut s) = client.connect() {
            // Sin tiempo máximo de lectura: el servidor manda latidos (keep-alive) cada 15 s.
            s.set_read_timeout(Some(Duration::from_secs(120))).ok();
            let req = format!(
                "GET /eventos HTTP/1.0\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nAccept: text/event-stream\r\n\r\n",
                client.ep.host, client.ep.port, client.ep.token
            );
            if s.write_all(req.as_bytes()).is_ok() {
                let mut r = BufReader::new(s);
                let mut head = true;
                let mut p = SseParser::default();
                let mut line = String::new();
                while matches!(r.read_line(&mut line), Ok(n) if n > 0) {
                    if head {
                        head = !line.trim_end().is_empty();
                    } else if let Some(d) = p.line(&line)
                        && let Ok(v) = serde_json::from_str(&d)
                        && !on_event(v)
                    {
                        return;
                    }
                    line.clear();
                }
            }
        }
        if !on_event(serde_json::Value::Null) {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eventos_sse() {
        let mut p = SseParser::default();
        assert_eq!(p.line(": latido"), None);
        assert_eq!(p.line(""), None, "sin datos no hay evento");
        assert_eq!(p.line("data: {\"tipo\":\"filesChanged\"}\r"), None);
        assert_eq!(p.line("\r").as_deref(), Some("{\"tipo\":\"filesChanged\"}"));
        p.line("data:a");
        p.line("data: b");
        assert_eq!(p.line("").as_deref(), Some("a\nb"));
    }
}
