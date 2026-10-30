//! Utilidades HTTP comunes: comprobar el estado de una respuesta y leerla como SSE.
use crate::provider::ModelError;
use crate::sse::{SseEvent, SseParser};

/// Envía la petición y devuelve la respuesta si el estado es 2xx (429 → `RateLimited`).
pub(crate) async fn send_checked(
    rb: reqwest::RequestBuilder,
) -> Result<reqwest::Response, ModelError> {
    let resp = rb
        .send()
        .await
        .map_err(|e| ModelError::Http(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 429 {
        return Err(ModelError::RateLimited);
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let cut: String = text.chars().take(500).collect();
        return Err(ModelError::Http(format!("{}: {}", status.as_u16(), cut)));
    }
    Ok(resp)
}

/// Lee el cuerpo como SSE y llama a `f` con cada evento completo. Termina cuando el servidor
/// cierra el cuerpo (el tiempo máximo lo pone el cliente HTTP).
pub(crate) async fn read_sse(
    mut resp: reqwest::Response,
    mut f: impl FnMut(SseEvent) -> Result<(), ModelError>,
) -> Result<(), ModelError> {
    let mut parser = SseParser::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ModelError::Http(e.to_string()))?
    {
        for e in parser.push(&chunk) {
            f(e)?;
        }
    }
    // Un último evento sin la línea en blanco final.
    for e in parser.push(b"\n\n") {
        f(e)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod fake {
    //! Servidor HTTP falso en 127.0.0.1 que responde con un cuerpo SSE enviado en trozos.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Arranca el servidor; devuelve la URL y una tarea con la petición recibida (en minúsculas).
    pub async fn sse_server(chunks: Vec<String>) -> (String, tokio::task::JoinHandle<String>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let n = s.read(&mut buf).await.unwrap();
            let got = String::from_utf8_lossy(&buf[..n]).to_lowercase();
            s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
            for c in chunks {
                s.write_all(format!("{:x}\r\n{c}\r\n", c.len()).as_bytes())
                    .await
                    .unwrap();
                s.flush().await.unwrap();
            }
            s.write_all(b"0\r\n\r\n").await.unwrap();
            got
        });
        (url, h)
    }
}
