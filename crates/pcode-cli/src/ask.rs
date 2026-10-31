//! `pcode preguntar`: una respuesta del modelo sin tools ni cambios en el proyecto (F8.44).
//!
//! Sirve para preguntas sobre el repositorio (`pcode preguntar "¿dónde se valida el token?" @src/auth.rs`)
//! y para encadenar en scripts: lo que llegue por stdin se adjunta como contexto
//! (`git diff --cached | pcode preguntar "escribe el mensaje de commit"`). Studio lo usa para generar
//! mensajes de commit, revisar cambios y concretar peticiones.
use crate::provider::{build_provider, pick_model, system_prompt};
use pcode_config::Config;
use pcode_models::{ChatRequest, Message};
use std::io::{IsTerminal, Read, Write};

/// Máximo de stdin que se adjunta (en caracteres).
const MAX_STDIN: usize = 400_000;

/// Argumentos de `pcode preguntar`.
#[derive(Debug, clap::Args)]
pub struct AskArgs {
    /// La pregunta (con `@archivo` para adjuntar archivos). Si falta, se lee de stdin.
    pub prompt: Vec<String>,
    /// Modelo.
    #[arg(long = "modelo")]
    pub model: Option<String>,
    /// Proveedor de `[proveedores]`.
    #[arg(long = "proveedor")]
    pub provider: Option<String>,
    /// Configuración del proyecto.
    #[arg(long = "config", default_value = "pcode.toml")]
    pub config: std::path::PathBuf,
    /// `texto` (en streaming) o `json` (`{"texto", "entrada", "salida"}`).
    #[arg(long = "formato", default_value = "texto")]
    pub format: String,
    /// Tokens de salida máximos.
    #[arg(long = "max-tokens", default_value_t = 4000)]
    pub max_tokens: u32,
}

/// Instrucción de sistema para respuestas directas.
const ASK_NOTE: &str = "Responde directamente, sin usar tools: no puedes leer más archivos ni ejecutar nada. \
Básate solo en el contexto que te dan; si falta algo para responder con seguridad, dilo. \
Sé concreto y cita rutas y líneas cuando las tengas.";

/// Mensaje de usuario: la pregunta (con los `@archivo` ya expandidos) y, si hay, lo que llegó por
/// stdin como bloque de contexto.
pub fn build_question(root: &std::path::Path, text: &str, stdin: Option<&str>) -> String {
    let mut q = pcode_tui::run::expand_message(root, text, &[]);
    if let Some(s) = stdin.map(str::trim_end).filter(|s| !s.is_empty()) {
        let cut: String = s.chars().take(MAX_STDIN).collect();
        let more = if s.chars().count() > MAX_STDIN {
            "\n[… recortado]"
        } else {
            ""
        };
        if q.trim().is_empty() {
            q = format!("{cut}{more}");
        } else {
            q.push_str(&format!("\n\n# Entrada\n```\n{cut}{more}\n```"));
        }
    }
    q
}

/// Ejecuta `pcode preguntar` y devuelve el código de salida.
pub async fn ask(a: AskArgs) -> anyhow::Result<i32> {
    let (cfg, _) = Config::load_layered(&a.config)?;
    let sel = build_provider(&cfg, a.provider.as_deref())?;
    let model = pick_model(a.model.as_deref(), &cfg, &sel);
    let root = std::env::current_dir()?;
    let stdin = if std::io::stdin().is_terminal() {
        None
    } else {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Some(s)
    };
    let question = build_question(&root, &a.prompt.join(" "), stdin.as_deref());
    if question.trim().is_empty() {
        anyhow::bail!("falta la pregunta: pcode preguntar \"…\" (o por stdin)");
    }
    let req = ChatRequest {
        model,
        messages: vec![
            Message::system(format!("{}\n\n{ASK_NOTE}", system_prompt(&cfg, &root))),
            Message::user(question),
        ],
        tools: vec![],
        max_tokens: Some(a.max_tokens),
        ..Default::default()
    };
    let json = a.format == "json";
    let printed = std::sync::atomic::AtomicBool::new(false);
    let on_text = |t: &str| {
        if !json {
            printed.store(true, std::sync::atomic::Ordering::Relaxed);
            print!("{t}");
            let _ = std::io::stdout().flush();
        }
    };
    let r = sel.provider.chat_stream(&req, &on_text).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "texto": r.message.content,
                "entrada": r.usage.input_tokens,
                "salida": r.usage.output_tokens,
            })
        );
    } else {
        if !printed.load(std::sync::atomic::Ordering::Relaxed) {
            print!("{}", r.message.content);
        }
        if !r.message.content.ends_with('\n') {
            println!();
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pregunta_con_archivo_y_stdin() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.rs"), "fn a() {}").unwrap();
        let q = build_question(d.path(), "explica @a.rs", Some("diff --git a/a.rs\n"));
        assert!(q.starts_with("explica @a.rs"));
        assert!(q.contains("fn a() {}"), "adjunta el archivo");
        assert!(q.contains("# Entrada\n```\ndiff --git a/a.rs\n```"));
    }

    #[test]
    fn solo_stdin_es_la_pregunta() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            build_question(d.path(), "", Some("¿qué es esto?\n")),
            "¿qué es esto?"
        );
        assert_eq!(build_question(d.path(), "", None), "");
    }
}
