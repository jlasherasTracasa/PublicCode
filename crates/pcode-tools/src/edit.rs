//! Tool `editar` (reemplazo exacto y único).
use crate::registry::*;
use pcode_models::ToolSpec;
use serde_json::{Value, json};

/// Reemplaza texto exacto en un archivo.
pub struct EditFile;

#[async_trait::async_trait]
impl Tool for EditFile {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "editar".into(),
            description:
                "Reemplaza `old` por `new` en un archivo. `old` debe ser único salvo replace_all."
                    .into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["path","old","new"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Write]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let g = |k: &str| args.get(k).and_then(Value::as_str);
        let (Some(p), Some(old), Some(new)) = (g("path"), g("old"), g("new")) else {
            return ToolOutput::error("faltan `path`, `old` o `new`");
        };
        if old.is_empty() {
            return ToolOutput::error("`old` no puede estar vacío");
        }
        let path = match resolve_in_root(&ctx.root, p) {
            Ok(x) => x,
            Err(e) => return ToolOutput::error(e),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return ToolOutput::error(format!("no se pudo leer {p}: {e}")),
        };
        let n = text.matches(old).count();
        let all = args
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if n == 0 {
            return ToolOutput::error(format!(
                "no se encontró el texto a reemplazar en {p}; vuelve a leer el archivo"
            ));
        }
        if n > 1 && !all {
            return ToolOutput::error(format!(
                "{n} coincidencias; da más contexto o usa replace_all"
            ));
        }
        let out = if all {
            text.replace(old, new)
        } else {
            text.replacen(old, new, 1)
        };
        match std::fs::write(&path, out) {
            Ok(()) => ToolOutput::ok(format!("editado {p} ({n} reemplazos)")),
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
    fn f(d: &tempfile::TempDir, s: &str) {
        std::fs::write(d.path().join("a"), s).unwrap();
    }
    #[tokio::test]
    async fn edit_unico() {
        let d = tempfile::tempdir().unwrap();
        f(&d, "uno dos");
        let o = EditFile
            .call(json!({"path":"a","old":"dos","new":"tres"}), &ctx(&d))
            .await;
        assert!(!o.is_error);
        assert_eq!(
            std::fs::read_to_string(d.path().join("a")).unwrap(),
            "uno tres"
        );
    }
    #[tokio::test]
    async fn edit_cero_coincidencias_mensaje() {
        let d = tempfile::tempdir().unwrap();
        f(&d, "x");
        let o = EditFile
            .call(json!({"path":"a","old":"zz","new":"y"}), &ctx(&d))
            .await;
        assert!(o.is_error && o.content.contains("vuelve a leer"));
    }
    #[tokio::test]
    async fn edit_varias_sin_replace_all_es_error() {
        let d = tempfile::tempdir().unwrap();
        f(&d, "a a");
        let o = EditFile
            .call(json!({"path":"a","old":"a","new":"b"}), &ctx(&d))
            .await;
        assert!(o.is_error && o.content.contains("2 coincidencias"));
    }
    #[tokio::test]
    async fn edit_replace_all() {
        let d = tempfile::tempdir().unwrap();
        f(&d, "a a");
        let o = EditFile
            .call(
                json!({"path":"a","old":"a","new":"b","replace_all":true}),
                &ctx(&d),
            )
            .await;
        assert!(!o.is_error);
        assert_eq!(std::fs::read_to_string(d.path().join("a")).unwrap(), "b b");
    }
}
