//! Elección del proveedor de modelos: `pcode.toml` primero y, si no hay, variables de entorno.
use anyhow::{Context, bail};
use pcode_config::{Config, ProviderCfg};
use pcode_models::{
    AnthropicProvider, GeminiProvider, MockProvider, OpenAiCompatProvider, Provider,
};
use std::sync::Arc;

/// Proveedor elegido y el modelo que se usa si el usuario no indica otro.
pub struct Selected {
    /// El proveedor.
    pub provider: Arc<dyn Provider>,
    /// Modelo por defecto del proveedor.
    pub default_model: Option<String>,
}

const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com";
const ANTHROPIC_MODEL: &str = "claude-sonnet-5-5";
const GEMINI_MODEL: &str = "gemini-2.5-pro";

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn from_cfg(name: &str, p: &ProviderCfg) -> anyhow::Result<Selected> {
    let url = |d: &str| {
        if p.url.is_empty() {
            d.to_string()
        } else {
            p.url.clone()
        }
    };
    match p.kind.as_str() {
        "openai-compatible" | "" => Ok(Selected {
            provider: Arc::new(OpenAiCompatProvider::new(
                name,
                p.url.clone(),
                p.api_key.clone(),
            )),
            default_model: None,
        }),
        "anthropic" => {
            let key = p
                .api_key
                .clone()
                .or_else(|| env("ANTHROPIC_API_KEY"))
                .with_context(|| {
                    format!("al proveedor {name} le falta `clave` o ANTHROPIC_API_KEY")
                })?;
            Ok(Selected {
                provider: Arc::new(AnthropicProvider::new(name, url(ANTHROPIC_URL), key)),
                default_model: Some(ANTHROPIC_MODEL.into()),
            })
        }
        "gemini" => {
            let key = p
                .api_key
                .clone()
                .or_else(|| env("GEMINI_API_KEY"))
                .or_else(|| env("GOOGLE_API_KEY"))
                .with_context(|| {
                    format!("al proveedor {name} le falta `clave` o GEMINI_API_KEY")
                })?;
            Ok(Selected {
                provider: Arc::new(GeminiProvider::new(name, url(GEMINI_URL), key)),
                default_model: Some(GEMINI_MODEL.into()),
            })
        }
        k => bail!(
            "el proveedor {name} es de tipo desconocido: {k} (openai-compatible, anthropic o gemini)"
        ),
    }
}

/// Construye el proveedor. Orden: `PCODE_MOCK_SCRIPT` (pruebas), el indicado con `--proveedor`,
/// el primero de `pcode.toml` y, por último, la clave que haya en el entorno
/// (`ANTHROPIC_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`, `OPENAI_API_KEY`).
pub fn build_provider(cfg: &Config, name: Option<&str>) -> anyhow::Result<Selected> {
    if let Ok(script) = std::env::var("PCODE_MOCK_SCRIPT") {
        return Ok(Selected {
            provider: Arc::new(MockProvider::from_script_file(std::path::Path::new(
                &script,
            ))?),
            default_model: None,
        });
    }
    if let Some(n) = name {
        let p = cfg
            .providers
            .get(n)
            .with_context(|| format!("proveedor desconocido: {n}"))?;
        return from_cfg(n, p);
    }
    if let Some((n, p)) = cfg.providers.iter().next() {
        return from_cfg(n, p);
    }
    if let Some(k) = env("ANTHROPIC_API_KEY") {
        return from_cfg(
            "anthropic",
            &ProviderCfg {
                kind: "anthropic".into(),
                api_key: Some(k),
                ..Default::default()
            },
        );
    }
    if env("GEMINI_API_KEY").is_some() || env("GOOGLE_API_KEY").is_some() {
        return from_cfg(
            "gemini",
            &ProviderCfg {
                kind: "gemini".into(),
                ..Default::default()
            },
        );
    }
    if let Some(k) = env("OPENAI_API_KEY") {
        return from_cfg(
            "openai",
            &ProviderCfg {
                kind: "openai-compatible".into(),
                url: env("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com/v1".into()),
                api_key: Some(k),
            },
        );
    }
    bail!(
        "no hay ningún modelo configurado: define ANTHROPIC_API_KEY, GEMINI_API_KEY u OPENAI_API_KEY, \
         o añade un proveedor en pcode.toml (ver README, «Configuración»)"
    )
}

/// Modelo a usar: `--modelo`, `PCODE_MODEL`, el del proveedor o `default` (LiteLLM lo resuelve).
pub fn pick_model(flag: Option<&str>, sel: &Selected) -> String {
    flag.map(String::from)
        .or_else(|| env("PCODE_MODEL"))
        .or_else(|| sel.default_model.clone())
        .unwrap_or_else(|| "default".into())
}

/// Prompt de sistema del agente interactivo, con el directorio, la plataforma y las instrucciones
/// del proyecto (`AGENTS.md`, `CLAUDE.md`… lo que diga `[proyecto] instrucciones`).
pub fn system_prompt(cfg: &Config, root: &std::path::Path) -> String {
    let mut s = format!(
        "Eres PublicCode, un agente de programación que trabaja en la terminal del usuario.\n\
         Directorio de trabajo: {}\nSistema: {}\n\
         Usa las tools (leer, buscar, listar, editar, escribir, bash) para investigar y cambiar el código; \
         lee antes de editar, haz cambios mínimos y comprueba con los tests cuando existan. \
         Responde en el idioma del usuario y sé breve: di qué hiciste y qué queda.",
        root.display(),
        std::env::consts::OS
    );
    let mut files = cfg.project.instructions.clone();
    if files.is_empty() {
        files = vec!["AGENTS.md".into(), "CLAUDE.md".into()];
    }
    for f in files {
        if let Ok(text) = std::fs::read_to_string(root.join(&f)) {
            let cut: String = text.chars().take(20_000).collect();
            s.push_str(&format!("\n\n# Instrucciones del proyecto ({f})\n{cut}"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tipo_desconocido_da_error_claro() {
        let p = ProviderCfg {
            kind: "otro".into(),
            ..Default::default()
        };
        let e = from_cfg("x", &p).err().unwrap().to_string();
        assert!(e.contains("tipo desconocido"));
    }

    #[test]
    fn anthropic_con_clave_en_config_usa_modelo_por_defecto() {
        let p = ProviderCfg {
            kind: "anthropic".into(),
            api_key: Some("k".into()),
            ..Default::default()
        };
        let s = from_cfg("a", &p).unwrap();
        assert_eq!(s.default_model.as_deref(), Some(ANTHROPIC_MODEL));
        assert_eq!(pick_model(Some("otro"), &s), "otro");
    }

    #[test]
    fn system_prompt_incluye_instrucciones_del_proyecto() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("AGENTS.md"), "REGLA-UNICA").unwrap();
        let s = system_prompt(&Config::default(), d.path());
        assert!(s.contains("REGLA-UNICA") && s.contains("Directorio de trabajo"));
    }
}
