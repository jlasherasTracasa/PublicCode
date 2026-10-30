//! Tool `escribir`.
use crate::registry::*;
use pcode_models::ToolSpec;
use serde_json::{Value, json};

/// Crea o sobrescribe un archivo.
pub struct WriteFile;

#[async_trait::async_trait]
impl Tool for WriteFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "escribir".into(),
            description: "Crea o sobrescribe un archivo con `content`.".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Write]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let (Some(p), Some(c)) = (
            args.get("path").and_then(Value::as_str),
            args.get("content").and_then(Value::as_str),
        ) else {
            return ToolOutput::error("faltan `path` o `content`");
        };
        let path = match resolve_in_root(&ctx.root, p) {
            Ok(x) => x,
            Err(e) => return ToolOutput::error(e),
        };
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return ToolOutput::error(format!("no se pudo crear el directorio: {e}"));
        }
        match std::fs::write(&path, c) {
            Ok(()) => ToolOutput::ok(format!("escrito {p} ({} bytes)", c.len())),
            Err(e) => ToolOutput::error(format!("no se pudo escribir {p}: {e}")),
        }
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
    async fn write_crea_padres() {
        let d = tempfile::tempdir().unwrap();
        let o = WriteFile
            .call(json!({"path":"a/b/c.txt","content":"hi"}), &ctx(&d))
            .await;
        assert!(!o.is_error);
        assert_eq!(
            std::fs::read_to_string(d.path().join("a/b/c.txt")).unwrap(),
            "hi"
        );
    }
    #[tokio::test]
    async fn write_fuera_de_root_es_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            WriteFile
                .call(json!({"path":"../x","content":""}), &ctx(&d))
                .await
                .is_error
        );
    }
}
