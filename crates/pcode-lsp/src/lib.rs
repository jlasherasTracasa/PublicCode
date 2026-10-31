//! Cliente LSP (Language Server Protocol) para el editor: ir a la definición, referencias, hover y
//! diagnósticos, con los servidores de cada lenguaje (rust-analyzer, typescript-language-server,
//! pyright, gopls, clangd…). Las posiciones van en unidades UTF-16, como en el navegador.
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, oneshot};

/// Tiempo máximo de las peticiones normales (`initialize` puede tardar más).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const INIT_TIMEOUT: Duration = Duration::from_secs(60);

/// Una posición en un archivo del proyecto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// Ruta relativa a la raíz (con `/`), o absoluta si está fuera del proyecto.
    pub path: String,
    /// Línea (desde 0).
    pub line: u32,
    /// Columna en unidades UTF-16 (desde 0).
    pub character: u32,
}

/// Un diagnóstico (error, aviso…).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// Línea inicial (desde 0).
    pub line: u32,
    /// Columna inicial.
    pub character: u32,
    /// Línea final.
    pub end_line: u32,
    /// Columna final.
    pub end_character: u32,
    /// 1 error, 2 aviso, 3 información, 4 pista.
    pub severity: u8,
    /// Mensaje.
    pub message: String,
    /// Origen (rustc, pyright…).
    pub source: String,
}

/// Recibe los diagnósticos publicados: `(ruta relativa, diagnósticos)`.
pub type DiagnosticsSink = Arc<dyn Fn(String, Vec<Diagnostic>) + Send + Sync>;

/// `file://` URI de una ruta absoluta (también para rutas de Windows).
pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Ruta de una `file://` URI.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut dec = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            dec.push(v);
            i += 3;
            continue;
        }
        dec.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&dec).into_owned();
    // `/C:/x` → `C:/x` en Windows.
    let s = if s.len() > 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[2] == b':' {
        s[1..].to_string()
    } else {
        s
    };
    Some(PathBuf::from(s))
}

/// Identificador de lenguaje LSP según la extensión.
pub fn language_id(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        _ => return None,
    })
}

/// Servidor por defecto para un lenguaje (grupo, orden). Los de JS/TS comparten servidor.
pub fn default_server(lang: &str) -> Option<(&'static str, Vec<&'static str>)> {
    Some(match lang {
        "rust" => ("rust", vec!["rust-analyzer"]),
        "typescript" | "typescriptreact" | "javascript" | "javascriptreact" => {
            ("typescript", vec!["typescript-language-server", "--stdio"])
        }
        "python" => ("python", vec!["pyright-langserver", "--stdio"]),
        "go" => ("go", vec!["gopls"]),
        "c" | "cpp" => ("c", vec!["clangd"]),
        _ => return None,
    })
}

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// Conexión con un servidor LSP.
pub struct LspClient {
    root: PathBuf,
    writer: Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
    pending: Pending,
    next: AtomicU64,
    versions: std::sync::Mutex<HashMap<String, i64>>,
    _child: Option<std::sync::Mutex<tokio::process::Child>>,
}

async fn write_msg(w: &mut (dyn AsyncWrite + Unpin + Send), v: &Value) -> std::io::Result<()> {
    let body = v.to_string();
    w.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    w.write_all(body.as_bytes()).await?;
    w.flush().await
}

/// Lee un mensaje con cabeceras `Content-Length`; `None` al cerrarse el flujo.
pub async fn read_msg<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let n = len?;
    if n > 64 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await.ok()?;
    serde_json::from_slice(&buf).ok()
}

impl LspClient {
    /// Cliente sobre un par de flujos (pruebas o transportes propios).
    pub fn from_streams(
        root: &Path,
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
        sink: DiagnosticsSink,
    ) -> Arc<LspClient> {
        Self::build(root, reader, writer, None, sink)
    }

    fn build(
        root: &Path,
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
        child: Option<tokio::process::Child>,
        sink: DiagnosticsSink,
    ) -> Arc<LspClient> {
        let pending: Pending = Arc::default();
        let c = Arc::new(LspClient {
            root: root.to_path_buf(),
            writer: Mutex::new(Box::new(writer)),
            pending: pending.clone(),
            next: AtomicU64::new(1),
            versions: std::sync::Mutex::new(HashMap::new()),
            _child: child.map(std::sync::Mutex::new),
        });
        let weak = Arc::downgrade(&c);
        let root = root.to_path_buf();
        tokio::spawn(async move {
            let mut r = BufReader::new(reader);
            while let Some(v) = read_msg(&mut r).await {
                let method = v.get("method").and_then(Value::as_str);
                match (v.get("id"), method) {
                    (Some(id), None) => {
                        if let Some(id) = id.as_u64()
                            && let Some(tx) = pending.lock().unwrap().remove(&id)
                        {
                            let _ = tx.send(v);
                        }
                    }
                    (Some(_), Some(m)) => {
                        // Peticiones del servidor: se contesta lo mínimo para que no se bloquee.
                        let result = match m {
                            "workspace/configuration" => {
                                let n = v["params"]["items"]
                                    .as_array()
                                    .map(|a| a.len())
                                    .unwrap_or(1);
                                Value::Array(vec![Value::Null; n])
                            }
                            "workspace/workspaceFolders" => json!([]),
                            _ => Value::Null,
                        };
                        if let Some(c) = weak.upgrade() {
                            let resp = json!({"jsonrpc": "2.0", "id": v["id"], "result": result});
                            let _ = write_msg(&mut **c.writer.lock().await, &resp).await;
                        }
                    }
                    (None, Some("textDocument/publishDiagnostics")) => {
                        let p = &v["params"];
                        let Some(path) = p["uri"].as_str().and_then(uri_to_path) else {
                            continue;
                        };
                        let rel = rel_path(&root, &path);
                        let diags = p["diagnostics"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|d| Diagnostic {
                                line: d["range"]["start"]["line"].as_u64().unwrap_or(0) as u32,
                                character: d["range"]["start"]["character"].as_u64().unwrap_or(0)
                                    as u32,
                                end_line: d["range"]["end"]["line"].as_u64().unwrap_or(0) as u32,
                                end_character: d["range"]["end"]["character"].as_u64().unwrap_or(0)
                                    as u32,
                                severity: d["severity"].as_u64().unwrap_or(1) as u8,
                                message: d["message"].as_str().unwrap_or_default().to_string(),
                                source: d["source"].as_str().unwrap_or_default().to_string(),
                            })
                            .collect();
                        sink(rel, diags);
                    }
                    _ => {}
                }
            }
            pending.lock().unwrap().clear();
        });
        c
    }

    /// Lanza el servidor en `root` y lo inicializa.
    pub async fn spawn(
        root: &Path,
        command: &[String],
        sink: DiagnosticsSink,
    ) -> Result<Arc<LspClient>, String> {
        let (prog, args) = command.split_first().ok_or("orden LSP vacía")?;
        let mut cmd = tokio::process::Command::new(prog);
        cmd.args(args)
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("no se pudo lanzar {prog}: {e}"))?;
        let (Some(out), Some(inp)) = (child.stdout.take(), child.stdin.take()) else {
            return Err("sin stdio".into());
        };
        let c = Self::build(root, out, inp, Some(child), sink);
        c.initialize().await?;
        Ok(c)
    }

    /// Petición con tiempo máximo.
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(e) = write_msg(&mut **self.writer.lock().await, &msg).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("servidor LSP caído: {e}"));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => match v.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("error LSP").to_string()),
                None => Ok(v["result"].clone()),
            },
            Ok(Err(_)) => Err("el servidor LSP se cerró".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("el servidor LSP no respondió a {method}"))
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        write_msg(&mut **self.writer.lock().await, &msg)
            .await
            .map_err(|e| e.to_string())
    }

    async fn initialize(&self) -> Result<(), String> {
        let root_uri = path_to_uri(&self.root);
        let name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [{"uri": root_uri, "name": name}],
                "capabilities": {
                    "textDocument": {
                        "synchronization": {"didSave": true},
                        "definition": {"linkSupport": false},
                        "references": {},
                        "hover": {"contentFormat": ["plaintext", "markdown"]},
                        "publishDiagnostics": {"relatedInformation": false}
                    },
                    "workspace": {"configuration": true, "workspaceFolders": true},
                    "general": {"positionEncodings": ["utf-16"]}
                },
                "clientInfo": {"name": "pcode"}
            }),
            INIT_TIMEOUT,
        )
        .await?;
        self.notify("initialized", json!({})).await
    }

    fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Abre o actualiza un documento (sincronización completa).
    pub async fn sync(&self, rel: &str, text: &str) -> Result<(), String> {
        let uri = path_to_uri(&self.abs(rel));
        let version = {
            let mut v = self.versions.lock().unwrap();
            let e = v.entry(rel.to_string()).or_insert(0);
            *e += 1;
            *e
        };
        if version == 1 {
            let lang = language_id(rel).unwrap_or("plaintext");
            self.notify(
                "textDocument/didOpen",
                json!({"textDocument": {"uri": uri, "languageId": lang, "version": version, "text": text}}),
            )
            .await
        } else {
            self.notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]}),
            )
            .await
        }
    }

    /// Avisa de que el documento se guardó (algunos servidores solo comprueban al guardar).
    pub async fn saved(&self, rel: &str) -> Result<(), String> {
        let uri = path_to_uri(&self.abs(rel));
        self.notify(
            "textDocument/didSave",
            json!({"textDocument": {"uri": uri}}),
        )
        .await
    }

    fn position(&self, rel: &str, line: u32, character: u32) -> Value {
        json!({"textDocument": {"uri": path_to_uri(&self.abs(rel))}, "position": {"line": line, "character": character}})
    }

    fn locations(&self, v: &Value) -> Vec<Location> {
        let items: Vec<&Value> = match v {
            Value::Array(a) => a.iter().collect(),
            Value::Null => vec![],
            o => vec![o],
        };
        items
            .into_iter()
            .filter_map(|l| {
                let uri = l.get("uri").or_else(|| l.get("targetUri"))?.as_str()?;
                let range = l.get("range").or_else(|| l.get("targetSelectionRange"))?;
                let path = uri_to_path(uri)?;
                Some(Location {
                    path: rel_path(&self.root, &path),
                    line: range["start"]["line"].as_u64()? as u32,
                    character: range["start"]["character"].as_u64()? as u32,
                })
            })
            .collect()
    }

    /// Ir a la definición.
    pub async fn definition(
        &self,
        rel: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, String> {
        let v = self
            .request(
                "textDocument/definition",
                self.position(rel, line, character),
                REQUEST_TIMEOUT,
            )
            .await?;
        Ok(self.locations(&v))
    }

    /// Referencias (incluida la declaración).
    pub async fn references(
        &self,
        rel: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, String> {
        let mut p = self.position(rel, line, character);
        p["context"] = json!({"includeDeclaration": true});
        let v = self
            .request("textDocument/references", p, REQUEST_TIMEOUT)
            .await?;
        Ok(self.locations(&v))
    }

    /// Texto del hover (tipos, documentación).
    pub async fn hover(&self, rel: &str, line: u32, character: u32) -> Result<String, String> {
        let v = self
            .request(
                "textDocument/hover",
                self.position(rel, line, character),
                REQUEST_TIMEOUT,
            )
            .await?;
        let c = &v["contents"];
        let text = match c {
            Value::String(s) => s.clone(),
            Value::Array(a) => a
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(String::from)
                        .unwrap_or_else(|| x["value"].as_str().unwrap_or_default().to_string())
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Value::Object(_) => c["value"].as_str().unwrap_or_default().to_string(),
            _ => String::new(),
        };
        Ok(text)
    }
}

/// Ruta relativa a `root` con `/` (o la absoluta si está fuera).
pub fn rel_path(root: &Path, p: &Path) -> String {
    let root_c = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let p_c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    p_c.strip_prefix(&root_c)
        .or_else(|_| p.strip_prefix(root))
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| p.to_string_lossy().replace('\\', "/"))
}

/// Servidores LSP de un proyecto, arrancados al abrir el primer archivo de cada lenguaje.
pub struct LspManager {
    root: PathBuf,
    sink: DiagnosticsSink,
    overrides: std::sync::Mutex<HashMap<String, Vec<String>>>,
    clients: Mutex<HashMap<&'static str, Result<Arc<LspClient>, String>>>,
}

impl LspManager {
    /// Crea el gestor; `overrides` cambia la orden por grupo (`rust`, `typescript`, `python`, `go`, `c`).
    pub fn new(
        root: &Path,
        overrides: HashMap<String, Vec<String>>,
        sink: DiagnosticsSink,
    ) -> LspManager {
        LspManager {
            root: root.to_path_buf(),
            sink,
            overrides: std::sync::Mutex::new(overrides),
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Cliente para el lenguaje de `rel` (lo arranca si hace falta). Error si no hay servidor.
    pub async fn client_for(&self, rel: &str) -> Result<Arc<LspClient>, String> {
        let lang = language_id(rel).ok_or("sin servidor de lenguaje para este archivo")?;
        let (group, cmd) =
            default_server(lang).ok_or("sin servidor de lenguaje para este archivo")?;
        let mut g = self.clients.lock().await;
        if let Some(r) = g.get(group) {
            return r.clone();
        }
        let command: Vec<String> = self
            .overrides
            .lock()
            .unwrap()
            .get(group)
            .cloned()
            .unwrap_or_else(|| cmd.iter().map(|s| s.to_string()).collect());
        let r = LspClient::spawn(&self.root, &command, self.sink.clone())
            .await
            .map_err(|e| {
                format!(
                    "{e} (instala {} o configúralo en [lsp.{group}])",
                    command[0]
                )
            });
        g.insert(group, r.clone());
        r
    }

    /// Cambia las órdenes por grupo (de `[lsp.<grupo>] orden`); afecta a los que aún no arrancaron.
    pub fn set_overrides(&self, o: HashMap<String, Vec<String>>) {
        *self.overrides.lock().unwrap() = o;
    }

    /// Estado: grupo → «activo» o el error.
    pub async fn status(&self) -> Vec<(String, String)> {
        self.clients
            .lock()
            .await
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    v.as_ref()
                        .map(|_| "activo".to_string())
                        .unwrap_or_else(|e| e.clone()),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Servidor LSP falso en el propio proceso.
    async fn fake(
        root: &Path,
    ) -> (
        Arc<LspClient>,
        Arc<std::sync::Mutex<Vec<(String, Vec<Diagnostic>)>>>,
    ) {
        let (client_side, server_side) = tokio::io::duplex(1 << 16);
        let (cr, cw) = tokio::io::split(client_side);
        let (sr, mut sw) = tokio::io::split(server_side);
        let root_uri = path_to_uri(root);
        tokio::spawn(async move {
            let mut r = BufReader::new(sr);
            while let Some(v) = read_msg(&mut r).await {
                let m = v["method"].as_str().unwrap_or_default().to_string();
                let reply =
                    |result: Value| json!({"jsonrpc":"2.0","id": v["id"], "result": result});
                let out = match m.as_str() {
                    "initialize" => {
                        // Antes de contestar, el servidor pide configuración.
                        let ask = json!({"jsonrpc":"2.0","id": 900,"method":"workspace/configuration","params":{"items":[{},{}]}});
                        write_msg(&mut sw, &ask).await.unwrap();
                        Some(reply(json!({"capabilities": {}})))
                    }
                    "textDocument/didOpen" | "textDocument/didChange" => {
                        let uri = v["params"]["textDocument"]["uri"].clone();
                        let n = if m.ends_with("didOpen") { 1 } else { 0 };
                        let diags: Vec<Value> = (0..n).map(|_| json!({"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":9}},"severity":1,"message":"tipo incorrecto","source":"falso"})).collect();
                        Some(
                            json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri": uri, "diagnostics": diags}}),
                        )
                    }
                    "textDocument/definition" => Some(reply(
                        json!([{"uri": format!("{root_uri}/src/lib.rs"), "range":{"start":{"line":7,"character":3},"end":{"line":7,"character":8}}}]),
                    )),
                    "textDocument/references" => Some(reply(json!([
                        {"uri": format!("{root_uri}/src/lib.rs"), "range":{"start":{"line":7,"character":3},"end":{"line":7,"character":8}}},
                        {"targetUri": format!("{root_uri}/src/main.rs"), "targetSelectionRange":{"start":{"line":2,"character":1},"end":{"line":2,"character":2}}}]))),
                    "textDocument/hover" => Some(reply(
                        json!({"contents": {"kind":"markdown","value":"fn doble(x: u32) -> u32"}}),
                    )),
                    _ if v.get("id").is_some() && v.get("method").is_some() => {
                        Some(reply(Value::Null))
                    }
                    _ => None,
                };
                if let Some(o) = out {
                    write_msg(&mut sw, &o).await.unwrap();
                }
            }
        });
        let got = Arc::new(std::sync::Mutex::new(vec![]));
        let g2 = got.clone();
        let c = LspClient::from_streams(
            root,
            cr,
            cw,
            Arc::new(move |p, d| g2.lock().unwrap().push((p, d))),
        );
        c.initialize().await.unwrap();
        (c, got)
    }

    #[tokio::test]
    async fn lsp_definicion_referencias_hover_y_diagnosticos() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let (c, got) = fake(&root).await;
        c.sync("src/main.rs", "fn main() {\n    doble(1);\n}\n")
            .await
            .unwrap();
        let defs = c.definition("src/main.rs", 1, 6).await.unwrap();
        assert_eq!(
            defs,
            vec![Location {
                path: "src/lib.rs".into(),
                line: 7,
                character: 3
            }]
        );
        let refs = c.references("src/main.rs", 1, 6).await.unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].path, "src/main.rs");
        assert_eq!(
            c.hover("src/main.rs", 1, 6).await.unwrap(),
            "fn doble(x: u32) -> u32"
        );
        c.sync("src/main.rs", "fn main() {}\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let g = got.lock().unwrap();
        assert_eq!(g[0].0, "src/main.rs");
        assert_eq!(g[0].1[0].message, "tipo incorrecto");
        assert_eq!((g[0].1[0].line, g[0].1[0].end_character), (1, 9));
        assert!(g[1].1.is_empty(), "al corregir se limpian los diagnósticos");
    }

    #[test]
    fn uris_de_linux_y_windows() {
        assert_eq!(
            path_to_uri(Path::new("/home/a b/x.rs")),
            "file:///home/a%20b/x.rs"
        );
        assert_eq!(
            path_to_uri(Path::new("C:\\proj\\src\\ñ.rs")),
            "file:///C:/proj/src/%C3%B1.rs"
        );
        assert_eq!(
            uri_to_path("file:///home/a%20b/x.rs").unwrap(),
            PathBuf::from("/home/a b/x.rs")
        );
        assert_eq!(
            uri_to_path("file:///C:/proj/x.rs").unwrap(),
            PathBuf::from("C:/proj/x.rs")
        );
        assert_eq!(language_id("web/app.tsx"), Some("typescriptreact"));
        assert_eq!(default_server("javascript").unwrap().0, "typescript");
        assert!(language_id("README.md").is_none());
    }

    #[tokio::test]
    async fn lsp_servidor_que_no_existe_explica_que_instalar() {
        let d = tempfile::tempdir().unwrap();
        let mut o = HashMap::new();
        o.insert("python".to_string(), vec!["/no/existe/pyls".to_string()]);
        let m = LspManager::new(d.path(), o, Arc::new(|_, _| {}));
        let e = m.client_for("a.py").await.err().unwrap();
        assert!(e.contains("[lsp.python]"), "{e}");
        assert!(m.client_for("README.md").await.is_err());
    }
}

#[cfg(test)]
mod real {
    use super::*;

    /// Con un servidor real si está instalado (pyright); si no, la prueba no comprueba nada.
    #[tokio::test]
    async fn lsp_pyright_real_si_esta_instalado() {
        let ok = std::process::Command::new("pyright-langserver")
            .arg("--version")
            .output()
            .is_ok();
        if !ok {
            eprintln!("pyright-langserver no está instalado: se salta");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        std::fs::write(
            root.join("util.py"),
            "def doble(x: int) -> int:\n    return x * 2\n",
        )
        .unwrap();
        let src = "from util import doble\n\nresultado: str = doble(21)\n";
        std::fs::write(root.join("main.py"), src).unwrap();
        let got = Arc::new(std::sync::Mutex::new(vec![]));
        let g2 = got.clone();
        let m = LspManager::new(
            &root,
            HashMap::new(),
            Arc::new(move |p, d| g2.lock().unwrap().push((p, d))),
        );
        let c = m.client_for("main.py").await.unwrap();
        c.sync("main.py", src).await.unwrap();
        let defs = c.definition("main.py", 2, 18).await.unwrap();
        assert_eq!(
            defs.first().map(|l| (l.path.as_str(), l.line)),
            Some(("util.py", 0)),
            "{defs:?}"
        );
        assert!(c.hover("main.py", 2, 18).await.unwrap().contains("doble"));
        let mut errores = vec![];
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            errores = got
                .lock()
                .unwrap()
                .iter()
                .filter(|(p, d)| p == "main.py" && !d.is_empty())
                .flat_map(|(_, d)| d.clone())
                .collect();
            if !errores.is_empty() {
                break;
            }
        }
        assert!(
            errores.iter().any(|d| d.severity == 1 && d.line == 2),
            "{errores:?}"
        );
    }
}
