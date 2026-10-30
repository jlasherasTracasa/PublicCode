//! Tool `leer`.
use crate::registry::*;
use pcode_models::ToolSpec;
use serde_json::{Value, json};

/// Lee un archivo con números de línea.
pub struct ReadFile;

#[async_trait::async_trait]
impl Tool for ReadFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "leer".into(),
            description:
                "Lee un archivo con números de línea. offset (1-based) y limit opcionales.".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["path"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let Some(p) = args.get("path").and_then(Value::as_str) else {
            return ToolOutput::error("falta `path`");
        };
        let path = match resolve_in_root(&ctx.root, p) {
            Ok(x) => x,
            Err(e) => return ToolOutput::error(e),
        };
        if path.is_dir() {
            return ToolOutput::error(format!("{p} es un directorio"));
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return ToolOutput::error(format!("no se pudo leer {p}: {e}")),
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            return ToolOutput::error(format!("{p} es binario"));
        }
        let text = String::from_utf8_lossy(&bytes);
        let offset = args
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(2000) as usize;
        let lines: Vec<&str> = text.lines().collect();
        let mut out = String::new();
        let mut shown = 0;
        for (i, l) in lines.iter().enumerate().skip(offset - 1).take(limit) {
            let l: String = if l.chars().count() > 2000 {
                l.chars().take(2000).chain("…".chars()).collect()
            } else {
                (*l).to_string()
            };
            out.push_str(&format!("{:>6}\t{}\n", i + 1, l));
            shown += 1;
        }
        let rest = lines.len().saturating_sub(offset - 1 + shown);
        if rest > 0 {
            out.push_str(&format!("[… {rest} líneas más; usa offset]\n"));
        }
        ToolOutput::ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx(d: &tempfile::TempDir) -> ToolContext {
        ToolContext {
            root: d.path().to_path_buf(),
            timeout: std::time::Duration::from_secs(5),
        }
    }
    #[tokio::test]
    async fn read_numera_lineas() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), "x\ny\n").unwrap();
        let o = ReadFile.call(json!({"path":"a"}), &ctx(&d)).await;
        assert!(o.content.contains("     1\tx") && o.content.contains("     2\ty"));
    }
    #[tokio::test]
    async fn read_offset_y_limit() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), "1\n2\n3\n4\n").unwrap();
        let o = ReadFile
            .call(json!({"path":"a","offset":2,"limit":2}), &ctx(&d))
            .await;
        assert!(
            o.content.contains("\t2") && o.content.contains("\t3") && !o.content.contains("\t4\n")
        );
    }
    #[tokio::test]
    async fn read_avisa_de_lineas_restantes() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), "1\n2\n3\n").unwrap();
        let o = ReadFile.call(json!({"path":"a","limit":1}), &ctx(&d)).await;
        assert!(o.content.contains("2 líneas más"));
    }
    #[tokio::test]
    async fn read_binario_es_error() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), [0u8, 1, 2]).unwrap();
        assert!(ReadFile.call(json!({"path":"a"}), &ctx(&d)).await.is_error);
    }
    #[tokio::test]
    async fn read_inexistente_es_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            ReadFile
                .call(json!({"path":"nada"}), &ctx(&d))
                .await
                .is_error
        );
    }
    #[tokio::test]
    async fn read_fuera_de_root_es_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            ReadFile
                .call(json!({"path":"../x"}), &ctx(&d))
                .await
                .is_error
        );
    }
}
