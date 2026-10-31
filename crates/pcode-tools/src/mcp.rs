//! Cliente MCP (Model Context Protocol) por stdio: cada tool de un servidor MCP se registra como
//! una tool más del agente (`<servidor>_<tool>`), sujeta a la misma política y auditoría.
use crate::registry::{Effect, Tool, ToolContext, ToolOutput};
use pcode_models::ToolSpec;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, oneshot};

/// Versión del protocolo MCP que se anuncia.
pub const MCP_PROTOCOL: &str = "2025-06-18";
/// Tiempo máximo de `initialize` y `tools/list`.
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuración de un servidor MCP.
#[derive(Debug, Clone, Default)]
pub struct McpServer {
    /// Nombre (prefijo de sus tools).
    pub name: String,
    /// Orden y argumentos.
    pub command: Vec<String>,
    /// Variables de entorno extra.
    pub env: BTreeMap<String, String>,
    /// Pedir aprobación antes de cada llamada.
    pub ask: bool,
}

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// Conexión con un servidor MCP.
pub struct McpClient {
    writer: Mutex<Box<dyn AsyncWrite + Unpin + Send>>,
    pending: Pending,
    next: AtomicU64,
    _child: Option<std::sync::Mutex<tokio::process::Child>>,
}

impl McpClient {
    /// Cliente sobre un par de flujos (para pruebas o transportes propios).
    pub fn from_streams(
        reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Arc<McpClient> {
        Self::build(reader, writer, None)
    }

    fn build(
        reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
        child: Option<tokio::process::Child>,
    ) -> Arc<McpClient> {
        let pending: Pending = Arc::default();
        let client = Arc::new(McpClient {
            writer: Mutex::new(Box::new(writer)),
            pending: pending.clone(),
            next: AtomicU64::new(1),
            _child: child.map(std::sync::Mutex::new),
        });
        let weak = Arc::downgrade(&client);
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                match (v.get("id").and_then(Value::as_u64), v.get("method")) {
                    // Respuesta a una petición nuestra.
                    (Some(id), None) => {
                        if let Some(tx) = pending.lock().unwrap().remove(&id) {
                            let _ = tx.send(v);
                        }
                    }
                    // Petición del servidor (roots/list, sampling…): no la soportamos, pero hay
                    // que contestar para que no se quede esperando.
                    (Some(_), Some(_)) => {
                        if let Some(c) = weak.upgrade() {
                            let resp = json!({"jsonrpc": "2.0", "id": v["id"],
                                "error": {"code": -32601, "message": "no soportado por pcode"}});
                            let _ = c.send_line(&resp).await;
                        }
                    }
                    _ => {}
                }
            }
            // El servidor cerró: las peticiones pendientes fallan.
            pending.lock().unwrap().clear();
        });
        client
    }

    /// Lanza el servidor y hace el `initialize`.
    pub async fn connect(cfg: &McpServer) -> Result<Arc<McpClient>, String> {
        let (prog, args) = cfg
            .command
            .split_first()
            .ok_or_else(|| format!("el servidor MCP {} no tiene orden", cfg.name))?;
        let mut cmd = tokio::process::Command::new(prog);
        cmd.args(args)
            .envs(&cfg.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("no se pudo lanzar el servidor MCP {}: {e}", cfg.name))?;
        let (Some(out), Some(inp)) = (child.stdout.take(), child.stdin.take()) else {
            return Err("sin stdio".into());
        };
        // El proceso vive lo mismo que el cliente (y muere con él: `kill_on_drop`).
        let c = Self::build(out, inp, Some(child));
        c.initialize().await?;
        Ok(c)
    }

    async fn send_line(&self, v: &Value) -> std::io::Result<()> {
        let mut w = self.writer.lock().await;
        w.write_all(format!("{v}\n").as_bytes()).await?;
        w.flush().await
    }

    /// Petición JSON-RPC con tiempo máximo.
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
        if let Err(e) = self.send_line(&msg).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("servidor MCP caído: {e}"));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => match v.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("error MCP").to_string()),
                None => Ok(v["result"].clone()),
            },
            Ok(Err(_)) => Err("el servidor MCP cerró la conexión".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!(
                    "el servidor MCP no respondió a {method} en {timeout:?}"
                ))
            }
        }
    }

    async fn initialize(&self) -> Result<(), String> {
        self.request(
            "initialize",
            json!({"protocolVersion": MCP_PROTOCOL, "capabilities": {},
                   "clientInfo": {"name": "pcode", "version": env!("CARGO_PKG_VERSION")}}),
            SETUP_TIMEOUT,
        )
        .await?;
        self.send_line(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await
            .map_err(|e| e.to_string())
    }

    /// Tools del servidor: `(nombre, descripción, esquema)`. Sigue la paginación (hasta 20 páginas).
    pub async fn list_tools(&self) -> Result<Vec<(String, String, Value)>, String> {
        let mut out = vec![];
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let r = self.request("tools/list", params, SETUP_TIMEOUT).await?;
            for t in r["tools"].as_array().into_iter().flatten() {
                out.push((
                    t["name"].as_str().unwrap_or_default().to_string(),
                    t["description"].as_str().unwrap_or_default().to_string(),
                    t.get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object"})),
                ));
            }
            cursor = r["nextCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Llama a una tool y junta su contenido como texto.
    pub async fn call(&self, name: &str, args: Value, timeout: Duration) -> ToolOutput {
        match self
            .request(
                "tools/call",
                json!({"name": name, "arguments": args}),
                timeout,
            )
            .await
        {
            Err(e) => ToolOutput::error(e),
            Ok(r) => {
                let text: Vec<String> = r["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| match c["type"].as_str() {
                        Some("text") => c["text"].as_str().unwrap_or_default().to_string(),
                        Some("resource") => c["resource"]["text"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| format!("[recurso {}]", c["resource"]["uri"])),
                        Some(t) => format!("[{t}]"),
                        None => c.to_string(),
                    })
                    .collect();
                let body = if text.is_empty() {
                    r.get("structuredContent")
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                } else {
                    text.join("\n")
                };
                if r["isError"].as_bool().unwrap_or(false) {
                    ToolOutput::error(body)
                } else {
                    ToolOutput::ok(body)
                }
            }
        }
    }
}

/// Nombre de tool válido para los proveedores (`^[a-zA-Z0-9_-]{1,64}$`).
pub fn tool_name(server: &str, tool: &str) -> String {
    let raw = format!("{server}_{tool}");
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    s.chars().take(64).collect()
}

/// Una tool remota.
pub struct McpTool {
    client: Arc<McpClient>,
    server: String,
    name: String,
    remote: String,
    description: String,
    schema: Value,
    ask: bool,
}

#[async_trait::async_trait]
impl Tool for McpTool {
    fn spec(&self) -> ToolSpec {
        let mut schema = self.schema.clone();
        if schema.get("type").is_none() {
            schema["type"] = json!("object");
        }
        ToolSpec {
            name: self.name.clone(),
            description: format!("[MCP {}] {}", self.server, self.description),
            parameters: schema,
        }
    }
    fn effects(&self) -> &'static [Effect] {
        if self.ask {
            &[Effect::Network, Effect::Exec, Effect::Irreversible]
        } else {
            &[Effect::Network, Effect::Exec]
        }
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        self.client.call(&self.remote, args, ctx.timeout).await
    }
}

/// Conecta con un servidor y devuelve sus tools listas para registrar.
pub async fn tools_of(cfg: &McpServer) -> Result<Vec<Arc<dyn Tool>>, String> {
    let c = McpClient::connect(cfg).await?;
    tools_from_client(c, cfg).await
}

/// Tools de un cliente ya conectado.
pub async fn tools_from_client(
    c: Arc<McpClient>,
    cfg: &McpServer,
) -> Result<Vec<Arc<dyn Tool>>, String> {
    Ok(c.list_tools()
        .await?
        .into_iter()
        .map(|(remote, description, schema)| {
            Arc::new(McpTool {
                client: c.clone(),
                server: cfg.name.clone(),
                name: tool_name(&cfg.name, &remote),
                remote,
                description,
                schema,
                ask: cfg.ask,
            }) as Arc<dyn Tool>
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Servidor MCP falso en el propio proceso: una tool `eco` y otra que falla.
    fn fake() -> Arc<McpClient> {
        let (client_side, server_side) = tokio::io::duplex(1 << 16);
        let (cr, cw) = tokio::io::split(client_side);
        let (sr, mut sw) = tokio::io::split(server_side);
        tokio::spawn(async move {
            let mut lines = BufReader::new(sr).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let v: Value = serde_json::from_str(&l).unwrap();
                let Some(id) = v.get("id").cloned() else {
                    continue;
                };
                if v.get("method").is_none() {
                    continue; // respuesta del cliente a nuestra petición
                }
                let result = match v["method"].as_str().unwrap() {
                    "initialize" => {
                        // El servidor también pregunta algo al cliente.
                        let ask = json!({"jsonrpc":"2.0","id":99,"method":"roots/list"});
                        sw.write_all(format!("{ask}\n").as_bytes()).await.unwrap();
                        json!({"protocolVersion": MCP_PROTOCOL, "capabilities": {"tools": {}}})
                    }
                    "tools/list" if v["params"].get("cursor").is_none() => json!({
                        "tools": [{"name": "eco", "description": "Repite", "inputSchema": {"type":"object","properties":{"texto":{"type":"string"}}}}],
                        "nextCursor": "p2"}),
                    "tools/list" => {
                        json!({"tools": [{"name": "falla.siempre", "description": "Falla"}]})
                    }
                    "tools/call" if v["params"]["name"] == "eco" => json!({
                        "content": [{"type":"text","text": format!("eco: {}", v["params"]["arguments"]["texto"].as_str().unwrap())}]}),
                    "tools/call" => {
                        json!({"content": [{"type":"text","text":"no"}], "isError": true})
                    }
                    _ => json!({}),
                };
                let r = json!({"jsonrpc":"2.0","id": id, "result": result});
                sw.write_all(format!("{r}\n").as_bytes()).await.unwrap();
            }
        });
        McpClient::from_streams(cr, cw)
    }

    #[tokio::test]
    async fn mcp_lista_con_paginacion_y_llama() {
        let c = fake();
        c.initialize().await.unwrap();
        let cfg = McpServer {
            name: "demo srv".into(),
            ..Default::default()
        };
        let tools = tools_from_client(c, &cfg).await.unwrap();
        let names: Vec<String> = tools.iter().map(|t| t.spec().name).collect();
        assert_eq!(names, vec!["demo_srv_eco", "demo_srv_falla_siempre"]);
        assert!(tools[0].spec().description.starts_with("[MCP demo srv]"));
        assert_eq!(tools[1].spec().parameters["type"], "object");
        let ctx = ToolContext {
            root: std::env::temp_dir(),
            timeout: Duration::from_secs(5),
        };
        let o = tools[0].call(json!({"texto": "hola"}), &ctx).await;
        assert_eq!(o, ToolOutput::ok("eco: hola"));
        assert!(tools[1].call(json!({}), &ctx).await.is_error);
        assert!(!tools[0].effects().contains(&Effect::Irreversible));
    }

    #[tokio::test]
    async fn mcp_tiempo_maximo_y_servidor_caido() {
        let (client_side, _server_side) = tokio::io::duplex(1024);
        let (cr, cw) = tokio::io::split(client_side);
        let c = McpClient::from_streams(cr, cw);
        let e = c
            .request("tools/list", json!({}), Duration::from_millis(50))
            .await;
        assert!(e.unwrap_err().contains("no respondió"));
        assert!(
            McpClient::connect(&McpServer {
                name: "x".into(),
                command: vec!["/no/existe/servidor-mcp".into()],
                ..Default::default()
            })
            .await
            .is_err()
        );
    }

    #[test]
    fn nombres_validos_para_los_proveedores() {
        assert_eq!(tool_name("git hub", "crear.issue"), "git_hub_crear_issue");
        assert_eq!(tool_name("a", &"x".repeat(100)).len(), 64);
    }
}
