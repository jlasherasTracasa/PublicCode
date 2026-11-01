//! Descubrir modelos: probar un proveedor (¿responde?, ¿qué modelos ofrece?) y detectar servidores
//! locales (Ollama, LM Studio). Lo usan la pantalla de Ajustes de Studio y `pcode config proveedores`.
//!
//! Solo lectura y con tiempo máximo: una petición `GET` a la lista de modelos de cada API.
use std::time::Duration;

/// URL oficial de la API de Anthropic.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";
/// URL oficial de la API de Gemini.
pub const GEMINI_URL: &str = "https://generativelanguage.googleapis.com";

/// Un servidor de modelos encontrado en este equipo.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocalServer {
    /// Nombre sugerido para el proveedor (`ollama`, `lmstudio`).
    pub name: String,
    /// Nombre para mostrar.
    pub label: String,
    /// URL base OpenAI-compatible.
    pub url: String,
    /// Modelos que ofrece.
    pub models: Vec<String>,
}

/// Servidores locales que se buscan: (nombre, etiqueta, URL base).
pub const LOCAL_CANDIDATES: &[(&str, &str, &str)] = &[
    ("ollama", "Ollama", "http://127.0.0.1:11434/v1"),
    ("lmstudio", "LM Studio", "http://127.0.0.1:1234/v1"),
    ("vllm", "vLLM", "http://127.0.0.1:8000/v1"),
];

fn client(timeout: Duration, url: &str) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder().timeout(timeout);
    // Lo local no pasa por el proxy de la empresa.
    if url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]")
    {
        b = b.no_proxy();
    }
    b.build().map_err(|e| e.to_string())
}

/// Lista los modelos de un proveedor. `kind`: `openai-compatible`, `anthropic` o `gemini`; `url`
/// vacía = la oficial; `key` = la clave ya resuelta (si hace falta). Devuelve los identificadores
/// ordenados, o un error legible («401: clave incorrecta», «no responde»…).
pub async fn list_models(
    kind: &str,
    url: &str,
    key: Option<&str>,
    timeout: Duration,
) -> Result<Vec<String>, String> {
    let base = |d: &str| {
        if url.trim().is_empty() {
            d.to_string()
        } else {
            url.trim().trim_end_matches('/').to_string()
        }
    };
    let (endpoint, field) = match kind {
        "anthropic" => (
            format!("{}/v1/models?limit=100", base(ANTHROPIC_URL)),
            "data",
        ),
        "gemini" => (
            format!("{}/v1beta/models?pageSize=200", base(GEMINI_URL)),
            "models",
        ),
        "openai-compatible" | "" => {
            if url.trim().is_empty() {
                return Err("falta la URL base".into());
            }
            (format!("{}/models", base("")), "data")
        }
        k => return Err(format!("tipo desconocido: {k}")),
    };
    let c = client(timeout, &endpoint)?;
    let mut req = c.get(&endpoint);
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        req = match kind {
            "anthropic" => req
                .header("x-api-key", k)
                .header("anthropic-version", "2023-06-01"),
            "gemini" => req.header("x-goog-api-key", k),
            _ => req.bearer_auth(k),
        };
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_timeout() {
            format!("no responde en {} s", timeout.as_secs())
        } else if e.is_connect() {
            "no se pudo conectar: ¿está arrancado el servidor y es correcta la URL?".to_string()
        } else {
            e.to_string()
        }
    })?;
    let status = resp.status();
    if !status.is_success() {
        let why = match status.as_u16() {
            401 | 403 => "clave incorrecta o sin permiso",
            404 => "no existe esa ruta: revisa la URL base (suele acabar en /v1)",
            429 => "límite de peticiones",
            _ => "error del servidor",
        };
        return Err(format!("{}: {why}", status.as_u16()));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("respuesta que no es JSON: {e}"))?;
    let mut ids: Vec<String> = v[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            m.get("id")
                .or_else(|| m.get("name"))
                .and_then(|x| x.as_str())
                .map(|s| s.trim_start_matches("models/").to_string())
        })
        .collect();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Busca servidores OpenAI-compatibles en `candidates` (en paralelo, como mucho `timeout` cada
/// uno) y devuelve los que responden.
pub async fn detect_local(
    candidates: &[(&str, &str, &str)],
    timeout: Duration,
) -> Vec<LocalServer> {
    let probes = candidates.iter().map(|(name, label, url)| async move {
        list_models("openai-compatible", url, None, timeout)
            .await
            .ok()
            .map(|models| LocalServer {
                name: name.to_string(),
                label: label.to_string(),
                url: url.to_string(),
                models,
            })
    });
    futures::future::join_all(probes)
        .await
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Servidor HTTP mínimo en 127.0.0.1 que responde siempre `status` con `body` y guarda las
    /// cabeceras de la petición.
    async fn server(
        status: u16,
        body: &'static str,
    ) -> (String, std::sync::Arc<std::sync::Mutex<String>>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let s2 = seen.clone();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = l.accept().await {
                let mut buf = vec![0u8; 4096];
                let n = c.read(&mut buf).await.unwrap_or(0);
                *s2.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).into_owned();
                let r = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = c.write_all(r.as_bytes()).await;
            }
        });
        (format!("http://127.0.0.1:{}", addr.port()), seen)
    }

    #[tokio::test]
    async fn lista_modelos_openai_compatible_con_clave() {
        let (url, seen) = server(200, r#"{"data":[{"id":"qwen3"},{"id":"llama3.2"}]}"#).await;
        let m = list_models(
            "openai-compatible",
            &format!("{url}/v1/"),
            Some("sk-x"),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(m, vec!["llama3.2", "qwen3"]);
        let req = seen.lock().unwrap().to_lowercase();
        assert!(req.starts_with("get /v1/models "), "{req}");
        assert!(req.contains("authorization: bearer sk-x"));
    }

    #[tokio::test]
    async fn anthropic_y_gemini_usan_su_cabecera_y_su_formato() {
        let (url, seen) = server(200, r#"{"data":[{"id":"claude-sonnet-5-5"}]}"#).await;
        let m = list_models("anthropic", &url, Some("k"), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(m, vec!["claude-sonnet-5-5"]);
        assert!(seen.lock().unwrap().to_lowercase().contains("x-api-key: k"));
        let (url, seen) = server(200, r#"{"models":[{"name":"models/gemini-2.5-pro"}]}"#).await;
        let m = list_models("gemini", &url, Some("g"), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(m, vec!["gemini-2.5-pro"]);
        let req = seen.lock().unwrap().to_lowercase();
        assert!(req.starts_with("get /v1beta/models") && req.contains("x-goog-api-key: g"));
    }

    #[tokio::test]
    async fn errores_legibles() {
        let (url, _) = server(401, r#"{"error":"no"}"#).await;
        let e = list_models("openai-compatible", &url, None, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert_eq!(e, "401: clave incorrecta o sin permiso");
        let (url, _) = server(404, "{}").await;
        assert!(
            list_models("openai-compatible", &url, None, Duration::from_secs(5))
                .await
                .unwrap_err()
                .contains("/v1")
        );
        // Un puerto en el que no escucha nadie.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let e = list_models(
            "openai-compatible",
            &format!("http://127.0.0.1:{port}/v1"),
            None,
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert!(e.contains("no se pudo conectar"), "{e}");
        assert!(
            list_models("openai-compatible", "", None, Duration::from_secs(1))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn detecta_solo_los_servidores_que_responden() {
        let (url, _) = server(200, r#"{"data":[{"id":"phi4"}]}"#).await;
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let muerto = format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port());
        drop(l);
        let vivo = format!("{url}/v1");
        let found = detect_local(
            &[
                ("ollama", "Ollama", &vivo),
                ("lmstudio", "LM Studio", &muerto),
            ],
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "ollama");
        assert_eq!(found[0].models, vec!["phi4"]);
    }
}
