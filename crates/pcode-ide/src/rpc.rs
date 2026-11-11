//! Cliente JSON-RPC mínimo del demonio: HTTP/1.0 sobre TCP en loopback, sin dependencias.
//! Solo habla con `127.0.0.1`/`localhost`: para otro equipo se usa `pcode remoto` (túnel SSH).
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Tiempo máximo de una petición.
pub const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// Tamaño máximo de una respuesta (los archivos grandes ya los rechaza el demonio).
const MAX_RESPONSE: usize = 64 * 1024 * 1024;

/// Dónde está el demonio y con qué token se entra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// `127.0.0.1` o `localhost`.
    pub host: String,
    /// Puerto.
    pub port: u16,
    /// Token de acceso (el `t=` de la dirección).
    pub token: String,
}

impl Endpoint {
    /// Lee una dirección de Studio: `http://127.0.0.1:PUERTO/?t=TOKEN`.
    pub fn parse(url: &str) -> Result<Endpoint, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or("la dirección debe empezar por http://")?;
        let (hostport, query) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = hostport
            .rsplit_once(':')
            .ok_or("falta el puerto en la dirección")?;
        if host != "127.0.0.1" && host != "localhost" {
            return Err(format!(
                "{host} no es este equipo: para otro servidor usa `pcode remoto <servidor> --ide`"
            ));
        }
        let port: u16 = port
            .parse()
            .map_err(|_| format!("puerto no válido: {port}"))?;
        let token = query
            .trim_start_matches('?')
            .split('&')
            .find_map(|kv| kv.strip_prefix("t="))
            .unwrap_or_default()
            .to_string();
        if token.is_empty() {
            return Err("falta el token (?t=…) en la dirección".into());
        }
        Ok(Endpoint {
            host: host.to_string(),
            port,
            token,
        })
    }
}

/// Bytes de una petición `POST /rpc`.
pub fn request_bytes(ep: &Endpoint, method: &str, params: &Value, id: u64) -> Vec<u8> {
    let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
    format!(
        "POST /rpc HTTP/1.0\r\nHost: {}:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        ep.host,
        ep.port,
        ep.token,
        body.len()
    )
    .into_bytes()
}

/// Decodifica un cuerpo `Transfer-Encoding: chunked`.
pub fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    for _ in 0..1_000_000 {
        let i = b
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("trozo sin fin de línea")?;
        let size_txt = std::str::from_utf8(&b[..i]).map_err(|_| "tamaño de trozo no válido")?;
        let size = usize::from_str_radix(size_txt.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| format!("tamaño de trozo no válido: {size_txt}"))?;
        b = &b[i + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size {
            return Err("respuesta cortada".into());
        }
        out.extend_from_slice(&b[..size]);
        b = b.get(size + 2..).unwrap_or_default();
    }
    Err("demasiados trozos".into())
}

/// Separa una respuesta HTTP en (código, cuerpo), quitando el troceado si lo hay.
pub fn parse_http_response(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("respuesta HTTP sin cabeceras")?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("respuesta HTTP sin código")?;
    let body = &raw[end + 4..];
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    Ok((
        status,
        if chunked {
            dechunk(body)?
        } else {
            body.to_vec()
        },
    ))
}

/// El `result` de una respuesta JSON-RPC, o su error como texto.
pub fn rpc_result(body: &[u8]) -> Result<Value, String> {
    let v: Value = serde_json::from_slice(body).map_err(|e| format!("respuesta no válida: {e}"))?;
    if let Some(e) = v.get("error") {
        return Err(e["message"]
            .as_str()
            .unwrap_or("error del demonio")
            .to_string());
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

/// Cliente del demonio. Cada llamada abre una conexión (en loopback cuesta microsegundos y así
/// no queda nada abierto en reposo).
#[derive(Debug)]
pub struct Client {
    /// Dónde está el demonio.
    pub ep: Endpoint,
    next: AtomicU64,
}

impl Client {
    /// Cliente para `ep`.
    pub fn new(ep: Endpoint) -> Client {
        Client {
            ep,
            next: AtomicU64::new(1),
        }
    }

    /// Conexión con tiempo máximo de lectura y escritura.
    pub fn connect(&self) -> Result<TcpStream, String> {
        let s = TcpStream::connect((self.ep.host.as_str(), self.ep.port))
            .map_err(|e| format!("no se pudo conectar con el demonio: {e}"))?;
        s.set_read_timeout(Some(RPC_TIMEOUT)).ok();
        s.set_write_timeout(Some(RPC_TIMEOUT)).ok();
        Ok(s)
    }

    /// Llama a `method` y devuelve su `result`.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let mut s = self.connect()?;
        s.write_all(&request_bytes(&self.ep, method, &params, id))
            .map_err(|e| e.to_string())?;
        let mut raw = vec![];
        s.take(MAX_RESPONSE as u64)
            .read_to_end(&mut raw)
            .map_err(|e| format!("{method}: {e}"))?;
        let (status, body) = parse_http_response(&raw)?;
        match status {
            200 => rpc_result(&body),
            401 => Err("token no válido: abre el IDE con la dirección que imprime pcode".into()),
            s => Err(format!("{method}: HTTP {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direccion_de_studio() {
        let ep = Endpoint::parse("http://127.0.0.1:4100/?t=abc123").unwrap();
        assert_eq!(
            ep,
            Endpoint {
                host: "127.0.0.1".into(),
                port: 4100,
                token: "abc123".into()
            }
        );
        assert!(
            Endpoint::parse("http://10.0.0.2:4100/?t=x")
                .unwrap_err()
                .contains("pcode remoto")
        );
        assert!(
            Endpoint::parse("http://localhost:4100/")
                .unwrap_err()
                .contains("token")
        );
        assert!(Endpoint::parse("https://localhost:1/?t=x").is_err());
        assert!(Endpoint::parse("http://localhost:99999/?t=x").is_err());
    }

    #[test]
    fn peticion_y_respuesta() {
        let ep = Endpoint::parse("http://localhost:1/?t=tok").unwrap();
        let req =
            String::from_utf8(request_bytes(&ep, "fs.read", &json!({"path": "a.rs"}), 7)).unwrap();
        assert!(req.starts_with("POST /rpc HTTP/1.0\r\n"));
        assert!(req.contains("Authorization: Bearer tok\r\n"));
        let body = req.split("\r\n\r\n").nth(1).unwrap();
        assert!(req.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap()["method"],
            "fs.read"
        );

        let ok = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"content\":\"hola\"}}";
        let (st, b) = parse_http_response(ok).unwrap();
        assert_eq!(st, 200);
        assert_eq!(rpc_result(&b).unwrap()["content"], "hola");
        let err = br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"no existe"}}"#;
        assert_eq!(rpc_result(err).unwrap_err(), "no existe");
    }

    #[test]
    fn cuerpo_troceado() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        let (_, b) = parse_http_response(raw).unwrap();
        assert_eq!(b, b"{\"a\":1}");
        assert!(dechunk(b"zz\r\n").is_err());
        assert!(dechunk(b"10\r\ncorto").is_err());
    }

    #[test]
    fn llamada_contra_un_servidor_falso() {
        let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"jsonrpc":"2.0","id":1,"result":{"version":"1.0.0"}}"#;
            write!(
                s,
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            req
        });
        let c =
            Client::new(Endpoint::parse(&format!("http://127.0.0.1:{port}/?t=secreto")).unwrap());
        assert_eq!(
            c.call("server.info", json!({})).unwrap()["version"],
            "1.0.0"
        );
        assert!(h.join().unwrap().contains("Bearer secreto"));
    }
}
