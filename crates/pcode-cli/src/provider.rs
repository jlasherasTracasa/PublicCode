//! Elección del proveedor de modelos: `pcode.toml` primero y, si no hay, variables de entorno.
use anyhow::{Context, bail};
use pcode_config::{Config, ProviderCfg};
use pcode_models::{
    AnthropicProvider, GeminiProvider, MockProvider, OpenAiCompatProvider, Provider, RetryProvider,
};
use std::sync::Arc;
use std::time::Duration;

/// Proveedor elegido y el modelo que se usa si el usuario no indica otro.
pub struct Selected {
    /// El proveedor.
    pub provider: Arc<dyn Provider>,
    /// Modelo por defecto del proveedor.
    pub default_model: Option<String>,
    /// Tokens de entrada a partir de los que conviene compactar (ventana del modelo con margen).
    pub context_limit: u64,
}

impl Selected {
    fn new(p: Arc<dyn Provider>, model: Option<&str>, context_limit: u64) -> Selected {
        // Reintentos ante 429, 5xx y cortes de red: 4 intentos, esperando 2, 4 y 8 s.
        Selected {
            provider: Arc::new(RetryProvider::new(p, 4, Duration::from_secs(2))),
            default_model: model.map(String::from),
            context_limit,
        }
    }
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
        "openai-compatible" | "" => Ok(Selected::new(
            Arc::new(OpenAiCompatProvider::new(
                name,
                p.url.clone(),
                p.api_key.clone(),
            )),
            None,
            100_000,
        )),
        "anthropic" => {
            let key = p
                .api_key
                .clone()
                .or_else(|| env("ANTHROPIC_API_KEY"))
                .with_context(|| {
                    format!("al proveedor {name} le falta `clave` o ANTHROPIC_API_KEY")
                })?;
            Ok(Selected::new(
                Arc::new(AnthropicProvider::new(name, url(ANTHROPIC_URL), key)),
                Some(ANTHROPIC_MODEL),
                160_000,
            ))
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
            Ok(Selected::new(
                Arc::new(GeminiProvider::new(name, url(GEMINI_URL), key)),
                Some(GEMINI_MODEL),
                400_000,
            ))
        }
        k => bail!(
            "el proveedor {name} es de tipo desconocido: {k} (openai-compatible, anthropic o gemini)"
        ),
    }
}

/// Construye el proveedor. Orden: `PCODE_MOCK_SCRIPT` (pruebas), el indicado con `--proveedor`,
/// `[preferencias] proveedor` (o el primero configurado) y, por último, la clave que haya en el entorno
/// (`ANTHROPIC_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`, `OPENAI_API_KEY`).
pub fn build_provider(cfg: &Config, name: Option<&str>) -> anyhow::Result<Selected> {
    if let Ok(script) = std::env::var("PCODE_MOCK_SCRIPT") {
        return Ok(Selected {
            provider: Arc::new(MockProvider::from_script_file(std::path::Path::new(
                &script,
            ))?),
            default_model: None,
            context_limit: 100_000,
        });
    }
    if let Some(n) = name.or(cfg.preferences.provider.as_deref()) {
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

/// Modelo a usar: `--modelo`, `PCODE_MODEL`, `[preferencias] modelo`, el del proveedor o
/// `default` (LiteLLM lo resuelve).
pub fn pick_model(flag: Option<&str>, cfg: &Config, sel: &Selected) -> String {
    flag.map(String::from)
        .or_else(|| env("PCODE_MODEL"))
        .or_else(|| cfg.preferences.model.clone())
        .or_else(|| sel.default_model.clone())
        .unwrap_or_else(|| "default".into())
}

/// Prompt de sistema del agente interactivo, con el directorio, la plataforma y las instrucciones
/// del proyecto (`AGENTS.md`, `CLAUDE.md`… lo que diga `[proyecto] instrucciones`).
pub fn system_prompt(cfg: &Config, root: &std::path::Path) -> String {
    let mut s = format!(
        "Eres PublicCode, un agente de programación que trabaja en la terminal del usuario.\n\
         Directorio de trabajo: {}\nSistema: {} (la tool bash usa {})\n\
         Usa las tools (esquema, leer, buscar, listar, editar, escribir, bash) para investigar y cambiar el código. \
         Localiza antes de leer: `esquema` da los símbolos de un archivo o de una carpeta y dónde se define un \
         símbolo, sin gastar contexto en archivos enteros; después lee solo lo necesario, edita y comprueba con \
         los tests cuando existan.\n\
         Ediciones mínimas: cambia solo lo que hace falta para lo que se pide; conserva el estilo, los nombres, \
         los comentarios y la estructura del código que no tienes que tocar; no reformatees ni reescribas \
         funciones enteras para un arreglo pequeño; edita con `editar` (no reescribas archivos completos) \
         y no añadas cosas que nadie ha pedido.\n\
         Responde en el idioma del usuario y sé breve: di qué hiciste y qué queda.",
        root.display(),
        std::env::consts::OS,
        if cfg!(windows) { "cmd.exe" } else { "sh" }
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

/// Conecta con los servidores MCP configurados (en paralelo, como mucho 30 s cada uno) y
/// devuelve sus tools y los avisos de los que no se pudieron usar.
pub async fn mcp_tools(cfg: &Config) -> (Vec<Arc<dyn pcode_tools::Tool>>, Vec<String>) {
    let servers: Vec<pcode_tools::mcp::McpServer> = cfg
        .mcp
        .iter()
        .filter(|(_, m)| !m.disabled)
        .map(|(n, m)| pcode_tools::mcp::McpServer {
            name: n.clone(),
            command: m.command.clone(),
            env: m.env.clone(),
            ask: m.ask,
        })
        .collect();
    let results = futures::future::join_all(servers.iter().map(|s| async move {
        match tokio::time::timeout(Duration::from_secs(30), pcode_tools::mcp::tools_of(s)).await {
            Ok(r) => (s.name.clone(), r),
            Err(_) => (s.name.clone(), Err("no respondió en 30 s".into())),
        }
    }))
    .await;
    let (mut tools, mut warnings) = (vec![], vec![]);
    for (name, r) in results {
        match r {
            Ok(t) => tools.extend(t),
            Err(e) => warnings.push(format!("MCP {name}: {e}")),
        }
    }
    (tools, warnings)
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
        assert_eq!(pick_model(Some("otro"), &Config::default(), &s), "otro");
        let mut cfg = Config::default();
        cfg.preferences.model = Some("pref".into());
        assert_eq!(pick_model(None, &cfg, &s), "pref");
    }

    #[test]
    fn prompt_de_sistema_pide_ediciones_minimas() {
        let d = tempfile::tempdir().unwrap();
        let s = system_prompt(&Config::default(), d.path());
        assert!(s.contains("Ediciones mínimas") && s.contains("conserva el estilo"));
        assert!(s.contains("Localiza antes de leer: `esquema`"));
    }

    #[test]
    fn system_prompt_incluye_instrucciones_del_proyecto() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("AGENTS.md"), "REGLA-UNICA").unwrap();
        let s = system_prompt(&Config::default(), d.path());
        assert!(s.contains("REGLA-UNICA") && s.contains("Directorio de trabajo"));
    }
}
