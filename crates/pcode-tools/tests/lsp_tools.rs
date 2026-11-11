//! Tools de LSP contra un servidor falso que corre en el propio proceso (sin red ni servidores reales).
use pcode_lsp::{LspClient, LspManager, path_to_uri, read_msg};
use pcode_tools::lsp_tools::{DiagStore, lsp_tools};
use pcode_tools::{Tool, ToolContext};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWrite;

/// Escribe un mensaje JSON-RPC con su cabecera `Content-Length`.
async fn write_msg(w: &mut (impl AsyncWrite + Unpin), v: &Value) {
    use tokio::io::AsyncWriteExt;
    let b = v.to_string();
    w.write_all(format!("Content-Length: {}\r\n\r\n{b}", b.len()).as_bytes())
        .await
        .unwrap();
    w.flush().await.unwrap();
}

/// Gestor con un servidor falso: publica un error en cada `didOpen` y responde a definición y referencias.
fn manager(root: &Path, store: &DiagStore) -> Arc<LspManager> {
    Arc::new(
        LspManager::new(root, HashMap::new(), store.sink()).with_spawner(|root, _cmd, sink| {
            Box::pin(async move {
                let (a, b) = tokio::io::duplex(1 << 16);
                let (cr, cw) = tokio::io::split(a);
                let (sr, mut sw) = tokio::io::split(b);
                let uri = path_to_uri(&root);
                tokio::spawn(async move {
                    let mut r = tokio::io::BufReader::new(sr);
                    while let Some(v) = read_msg(&mut r).await {
                        let loc = json!({
                            "uri": format!("{uri}/src/lib.rs"),
                            "range": {"start": {"line": 0, "character": 7}, "end": {"line": 0, "character": 12}}
                        });
                        let out = match v["method"].as_str().unwrap_or("") {
                            "textDocument/didOpen" | "textDocument/didChange" => json!({
                                "jsonrpc": "2.0",
                                "method": "textDocument/publishDiagnostics",
                                "params": {
                                    "uri": v["params"]["textDocument"]["uri"],
                                    "diagnostics": [{
                                        "range": {"start": {"line": 1, "character": 4}, "end": {"line": 1, "character": 9}},
                                        "severity": 1,
                                        "message": "tipo incorrecto"
                                    }]
                                }
                            }),
                            "textDocument/definition" => {
                                json!({"jsonrpc": "2.0", "id": v["id"], "result": [loc]})
                            }
                            "textDocument/references" => {
                                json!({"jsonrpc": "2.0", "id": v["id"], "result": [loc.clone(), loc]})
                            }
                            _ if v.get("id").is_some() => {
                                json!({"jsonrpc": "2.0", "id": v["id"], "result": null})
                            }
                            _ => continue,
                        };
                        write_msg(&mut sw, &out).await;
                    }
                });
                Ok(LspClient::from_streams(&root, cr, cw, sink))
            })
        }),
    )
}

/// Proyecto con `src/lib.rs` (define `doble`) y `src/main.rs` (lo usa).
fn proyecto() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn doble() {}\n").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {\n    doble();\n}\n").unwrap();
    (d, root)
}

fn ctx(root: &Path) -> ToolContext {
    ToolContext {
        root: root.to_path_buf(),
        timeout: Duration::from_secs(10),
    }
}

/// La tool con ese nombre.
fn por_nombre(tools: &[Arc<dyn Tool>], nombre: &str) -> Arc<dyn Tool> {
    tools
        .iter()
        .find(|t| t.spec().name == nombre)
        .cloned()
        .unwrap()
}

#[tokio::test]
async fn diagnosticos_de_un_archivo() {
    let (_d, root) = proyecto();
    let store = DiagStore::new();
    let tools = lsp_tools(manager(&root, &store), store.clone());
    let out = por_nombre(&tools, "diagnosticos")
        .call(json!({"path": "src/main.rs"}), &ctx(&root))
        .await;
    assert!(!out.is_error, "{}", out.content);
    assert_eq!(
        out.content,
        "src/main.rs: 1 errores, 0 avisos\nsrc/main.rs:2:5 error tipo incorrecto"
    );
}

#[tokio::test]
async fn definicion_en_otro_archivo() {
    let (_d, root) = proyecto();
    let store = DiagStore::new();
    let tools = lsp_tools(manager(&root, &store), store.clone());
    let out = por_nombre(&tools, "definicion")
        .call(
            json!({"path": "src/main.rs", "line": 2, "symbol": "doble"}),
            &ctx(&root),
        )
        .await;
    assert!(!out.is_error, "{}", out.content);
    assert_eq!(out.content, "src/lib.rs:1:8  pub fn doble() {}");
}

#[tokio::test]
async fn referencias_lista_los_usos() {
    let (_d, root) = proyecto();
    let store = DiagStore::new();
    let tools = lsp_tools(manager(&root, &store), store.clone());
    let out = por_nombre(&tools, "referencias")
        .call(
            json!({"path": "src/main.rs", "line": 2, "symbol": "doble"}),
            &ctx(&root),
        )
        .await;
    assert!(!out.is_error, "{}", out.content);
    let lineas: Vec<&str> = out.content.lines().collect();
    assert_eq!(lineas.len(), 2, "{}", out.content);
    assert!(lineas.iter().all(|l| l.starts_with("src/lib.rs:1:8")));
}

#[tokio::test]
async fn simbolo_que_no_esta_en_la_linea_es_error() {
    let (_d, root) = proyecto();
    let store = DiagStore::new();
    let tools = lsp_tools(manager(&root, &store), store.clone());
    let out = por_nombre(&tools, "definicion")
        .call(
            json!({"path": "src/main.rs", "line": 2, "symbol": "triple"}),
            &ctx(&root),
        )
        .await;
    assert!(out.is_error);
    assert!(
        out.content.contains("«triple» no aparece en la línea 2"),
        "{}",
        out.content
    );
}

#[tokio::test]
async fn sin_servidor_explica_que_instalar() {
    let (_d, root) = proyecto();
    let store = DiagStore::new();
    let m =
        LspManager::new(&root, HashMap::new(), store.sink()).with_spawner(|_root, _cmd, _sink| {
            Box::pin(async {
                Err::<Arc<LspClient>, String>("no se pudo lanzar rust-analyzer".into())
            })
        });
    let tools = lsp_tools(Arc::new(m), store);
    let out = por_nombre(&tools, "diagnosticos")
        .call(json!({"path": "src/main.rs"}), &ctx(&root))
        .await;
    assert!(out.is_error);
    assert!(
        out.content.contains("instala rust-analyzer"),
        "{}",
        out.content
    );
}

#[tokio::test]
async fn archivo_sin_servidor_de_lenguaje() {
    let (_d, root) = proyecto();
    std::fs::write(root.join("notas.txt"), "hola\n").unwrap();
    let store = DiagStore::new();
    let tools = lsp_tools(manager(&root, &store), store.clone());
    let out = por_nombre(&tools, "diagnosticos")
        .call(json!({"path": "notas.txt"}), &ctx(&root))
        .await;
    assert!(out.is_error);
    assert!(
        out.content.contains("sin servidor de lenguaje"),
        "{}",
        out.content
    );
}
