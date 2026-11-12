//! Descarga de extensiones de Open VSX para `pcode extensiones instalar editor.nombre`.
use anyhow::{Context, bail};
use pcode_extensions::openvsx::{OPEN_VSX, api_url, download_of, parse_id};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Tamaño máximo de un `.vsix` descargado.
pub const MAX_VSIX: usize = 64 * 1024 * 1024;

/// Registro que se usa: `PCODE_OPENVSX_URL` o Open VSX.
pub fn base_url() -> String {
    match std::env::var("PCODE_OPENVSX_URL") {
        Ok(v) if !v.is_empty() => v.trim_end_matches('/').to_string(),
        _ => OPEN_VSX.to_string(),
    }
}

/// Devuelve `origen` si existe; si es `editor.nombre`, lo descarga a `tmp` y devuelve la ruta del `.vsix`.
pub async fn resolve_source(origen: &Path, base: &str, tmp: &Path) -> anyhow::Result<PathBuf> {
    if origen.exists() {
        return Ok(origen.to_path_buf());
    }
    let texto = origen.to_string_lossy().into_owned();
    let Some((ns, name)) = parse_id(&texto) else {
        bail!(
            "no existe {} ni es un identificador editor.nombre",
            origen.display()
        );
    };
    eprintln!("Descargando {texto} de {base}…");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("no se pudo crear el cliente HTTP")?;
    let meta = client
        .get(api_url(base, &ns, &name))
        .header("User-Agent", "pcode")
        .send()
        .await
        .with_context(|| format!("no se pudo consultar {base}"))?;
    let cuerpo = meta
        .text()
        .await
        .context("no se pudo leer la respuesta del registro")?;
    let (version, url) = download_of(&cuerpo, base)?;
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        bail!("versión no válida en el registro: {version:?}");
    }

    let mut r = client
        .get(&url)
        .header("User-Agent", "pcode")
        .send()
        .await
        .with_context(|| format!("no se pudo descargar {url}"))?;
    if !r.status().is_success() {
        bail!("descarga fallida: HTTP {}", r.status().as_u16());
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(parte) = r.chunk().await.context("descarga interrumpida")? {
        if bytes.len() + parte.len() > MAX_VSIX {
            bail!("el .vsix es demasiado grande");
        }
        bytes.extend_from_slice(&parte);
    }

    let destino = tmp.join(format!("{ns}.{name}-{version}.vsix"));
    std::fs::write(&destino, &bytes)
        .with_context(|| format!("no se pudo escribir {}", destino.display()))?;
    Ok(destino)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Servidor HTTP falso en `127.0.0.1:0`. Acepta como mucho 2 conexiones. Los metadatos de
    /// `/api/ana/temas` son `meta` con `__BASE__` sustituido por su propia URL; `/f/ana.temas-1.0.0.vsix`
    /// devuelve `PK-falso`. Devuelve la URL base.
    async fn servidor_falso(meta: &'static str) -> String {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind en loopback");
        let base = format!("http://{}", l.local_addr().expect("dirección local"));
        let b = base.clone();
        tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut s, _)) = l.accept().await else {
                    return;
                };
                let mut pedido: Vec<u8> = Vec::new();
                let mut buf = [0u8; 1024];
                while !pedido.windows(4).any(|w| w == b"\r\n\r\n") && pedido.len() < 8192 {
                    match s.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => pedido.extend_from_slice(&buf[..n]),
                    }
                }
                let primera = String::from_utf8_lossy(&pedido)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let ruta = primera.split_whitespace().nth(1).unwrap_or("").to_string();
                let (estado, cuerpo): (&str, Vec<u8>) = match ruta.as_str() {
                    "/api/ana/temas" => ("200 OK", meta.replace("__BASE__", &b).into_bytes()),
                    "/f/ana.temas-1.0.0.vsix" => ("200 OK", b"PK-falso".to_vec()),
                    _ => ("404 Not Found", Vec::new()),
                };
                let cabecera = format!(
                    "HTTP/1.1 {estado}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    cuerpo.len()
                );
                let _ = s.write_all(cabecera.as_bytes()).await;
                let _ = s.write_all(&cuerpo).await;
                let _ = s.shutdown().await;
            }
        });
        base
    }

    #[tokio::test]
    async fn origen_local_no_usa_la_red() {
        let dir = tempfile::tempdir().unwrap();
        let archivo = dir.path().join("mi.vsix");
        std::fs::write(&archivo, b"PK-local").unwrap();
        let r = resolve_source(&archivo, "http://127.0.0.1:1", dir.path())
            .await
            .unwrap();
        assert_eq!(r, archivo);
    }

    #[tokio::test]
    async fn descarga_de_un_servidor_falso() {
        let base = servidor_falso(
            r#"{"version":"1.0.0","files":{"download":"__BASE__/f/ana.temas-1.0.0.vsix"}}"#,
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let r = resolve_source(Path::new("ana.temas"), &base, dir.path())
            .await
            .unwrap();
        assert_eq!(r, dir.path().join("ana.temas-1.0.0.vsix"));
        assert_eq!(std::fs::read(&r).unwrap(), b"PK-falso");
    }

    #[tokio::test]
    async fn origen_que_no_existe_ni_es_id() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_source(Path::new("no/existe/x"), "http://127.0.0.1:1", dir.path())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("ni es un identificador"));
    }

    #[tokio::test]
    async fn registro_con_error() {
        let base = servidor_falso(r#"{"error":"no está"}"#).await;
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_source(Path::new("ana.temas"), &base, dir.path())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no está"));
    }
}
