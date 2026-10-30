//! Tools `buscar` (regex) y `listar` (glob).
use crate::registry::*;
use globset::Glob as GlobPat;
use pcode_models::ToolSpec;
use regex::Regex;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const SKIP: &[&str] = &[".git", "target", "node_modules", ".pcode"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Busca con regex en el contenido.
pub struct Grep;

#[async_trait::async_trait]
impl Tool for Grep {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "buscar".into(),
            description: "Busca una regex en los archivos. Opcional: path, glob, max_results."
                .into(),
            parameters: json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string"},"max_results":{"type":"integer"}},"required":["pattern"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let Some(pat) = args.get("pattern").and_then(Value::as_str) else {
            return ToolOutput::error("falta `pattern`");
        };
        let re = match Regex::new(pat) {
            Ok(r) => r,
            Err(e) => return ToolOutput::error(format!("regex inválida: {e}")),
        };
        let base = match args.get("path").and_then(Value::as_str) {
            Some(p) => match resolve_in_root(&ctx.root, p) {
                Ok(x) => x,
                Err(e) => return ToolOutput::error(e),
            },
            None => ctx.root.clone(),
        };
        let glob = match args.get("glob").and_then(Value::as_str) {
            Some(g) => match GlobPat::new(g) {
                Ok(g) => Some(g.compile_matcher()),
                Err(e) => return ToolOutput::error(format!("glob inválido: {e}")),
            },
            None => None,
        };
        let max = args
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(200) as usize;
        let mut files = vec![];
        walk(&base, &mut files);
        let mut res = vec![];
        let mut total = 0usize;
        'f: for f in files {
            let r = rel(&ctx.root, &f);
            if let Some(g) = &glob
                && !g.is_match(&r)
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(&f) else {
                continue;
            };
            if bytes.iter().take(8192).any(|b| *b == 0) {
                continue;
            }
            for (i, l) in String::from_utf8_lossy(&bytes).lines().enumerate() {
                if re.is_match(l) {
                    total += 1;
                    if res.len() < max {
                        res.push(format!("{r}:{}:{l}", i + 1));
                    } else {
                        break 'f;
                    }
                }
            }
        }
        let mut out = res.join("\n");
        if total > max {
            out.push_str(&format!("\n[… hay más resultados; se muestran {max}]"));
        }
        ToolOutput::ok(out)
    }
}

/// Lista archivos por glob.
pub struct Glob;

#[async_trait::async_trait]
impl Tool for Glob {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "listar".into(),
            description: "Lista archivos que casan con un glob (p. ej. crates/**/*.rs).".into(),
            parameters: json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let Some(p) = args.get("pattern").and_then(Value::as_str) else {
            return ToolOutput::error("falta `pattern`");
        };
        let m = match GlobPat::new(p) {
            Ok(g) => g.compile_matcher(),
            Err(e) => return ToolOutput::error(format!("glob inválido: {e}")),
        };
        let mut files = vec![];
        walk(&ctx.root, &mut files);
        let mut v: Vec<String> = files
            .iter()
            .map(|f| rel(&ctx.root, f))
            .filter(|r| m.is_match(r))
            .collect();
        v.sort();
        v.truncate(1000);
        ToolOutput::ok(v.join("\n"))
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
    fn w(d: &tempfile::TempDir, p: &str, s: &str) {
        let f = d.path().join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    #[tokio::test]
    async fn grep_encuentra_con_linea() {
        let d = tempfile::tempdir().unwrap();
        w(&d, "a.rs", "x\nfoo bar\n");
        let o = Grep.call(json!({"pattern":"foo"}), &ctx(&d)).await;
        assert_eq!(o.content, "a.rs:2:foo bar");
    }
    #[tokio::test]
    async fn grep_salta_target_y_git() {
        let d = tempfile::tempdir().unwrap();
        w(&d, "target/x", "foo");
        w(&d, ".git/x", "foo");
        assert_eq!(
            Grep.call(json!({"pattern":"foo"}), &ctx(&d)).await.content,
            ""
        );
    }
    #[tokio::test]
    async fn grep_filtro_glob() {
        let d = tempfile::tempdir().unwrap();
        w(&d, "a.rs", "foo");
        w(&d, "b.md", "foo");
        let o = Grep
            .call(json!({"pattern":"foo","glob":"*.rs"}), &ctx(&d))
            .await;
        assert_eq!(o.content, "a.rs:1:foo");
    }
    #[tokio::test]
    async fn grep_limite_resultados() {
        let d = tempfile::tempdir().unwrap();
        w(&d, "a", "x\nx\nx\n");
        let o = Grep
            .call(json!({"pattern":"x","max_results":2}), &ctx(&d))
            .await;
        assert!(o.content.contains("hay más"));
    }
    #[tokio::test]
    async fn glob_lista_ordenado() {
        let d = tempfile::tempdir().unwrap();
        w(&d, "b/x.rs", "");
        w(&d, "a/y.rs", "");
        let o = Glob.call(json!({"pattern":"**/*.rs"}), &ctx(&d)).await;
        assert_eq!(o.content, "a/y.rs\nb/x.rs");
    }
    #[tokio::test]
    async fn grep_regex_invalida_es_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(Grep.call(json!({"pattern":"("}), &ctx(&d)).await.is_error);
    }
}
