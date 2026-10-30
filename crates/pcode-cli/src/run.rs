//! `pcode run`.
use anyhow::{Context, bail};
use pcode_config::Config;
use pcode_core::{Agent, AgentConfig, Budget, Outcome, StopHook};
use pcode_models::{MockProvider, OpenAiCompatProvider, Provider};
use pcode_policy::{AuditLog, Capabilities, Policy};
use pcode_store::Store;
use pcode_tools::ToolContext;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Argumentos de `pcode run`.
#[derive(Debug, clap::Args)]
pub struct RunArgs {
    /// Tarea a realizar.
    pub task: String,
    /// Modelo.
    #[arg(long = "modelo")]
    pub model: Option<String>,
    /// Proveedor de `pcode.toml`.
    #[arg(long = "proveedor")]
    pub provider: Option<String>,
    /// Orden de aceptación.
    #[arg(long = "acepta")]
    pub accept: Option<String>,
    /// Pasos máximos.
    #[arg(long = "max-pasos", default_value_t = 50)]
    pub max_steps: u32,
    /// Archivo de configuración.
    #[arg(long = "config", default_value = "pcode.toml")]
    pub config: PathBuf,
}

#[derive(serde::Deserialize, Default)]
struct PolicyToml {
    #[serde(default, rename = "politica")]
    policy: Capabilities,
}

fn build_provider(cfg: &Config, name: Option<&str>) -> anyhow::Result<Arc<dyn Provider>> {
    if let Ok(script) = std::env::var("PCODE_MOCK_SCRIPT") {
        return Ok(Arc::new(MockProvider::from_script_file(
            std::path::Path::new(&script),
        )?));
    }
    let (n, p) = match name {
        Some(n) => (
            n.to_string(),
            cfg.providers
                .get(n)
                .with_context(|| format!("proveedor desconocido: {n}"))?,
        ),
        None => cfg
            .providers
            .iter()
            .find(|(_, p)| p.kind == "openai-compatible")
            .map(|(n, p)| (n.clone(), p))
            .context("no hay ningún proveedor openai-compatible en pcode.toml")?,
    };
    if p.kind != "openai-compatible" {
        bail!(
            "el proveedor {n} es de tipo {}; por ahora solo openai-compatible",
            p.kind
        );
    }
    Ok(Arc::new(OpenAiCompatProvider::new(
        n,
        p.url.clone(),
        p.api_key.clone(),
    )))
}

/// Ejecuta la tarea y devuelve el código de salida.
pub async fn run(a: RunArgs) -> anyhow::Result<i32> {
    let cfg = Config::load(&a.config)?;
    let caps: PolicyToml = match std::fs::read_to_string(&a.config) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => PolicyToml::default(),
    };
    let provider = build_provider(&cfg, a.provider.as_deref())?;
    let root = std::env::current_dir()?;
    let model = a.model.clone().unwrap_or_else(|| "default".into());
    let audit = AuditLog::open(&root.join(".pcode/audit.jsonl"))?;
    let store = Store::open(&root.join(".pcode/pcode.db"))?;
    let sid = store.create_session(&model, &a.task)?;
    let hook = a.accept.as_ref().map(|c| StopHook {
        command: c.clone(),
        max_rounds: 5,
        timeout: Duration::from_secs(3000),
        checklist: vec![],
    });
    let mut agent = Agent::new(
        provider,
        pcode_tools::native_registry(),
        Policy::new(caps.policy),
        Some(audit),
        AgentConfig {
            agent_id: format!("s-{sid}"),
            model,
            system_prompt: "Eres un agente de programación. Usa las tools para completar la tarea y confirma al terminar."
                .into(),
            budget: Budget { max_steps: a.max_steps, ..Default::default() },
            stop_hook: hook,
        },
        ToolContext { root, timeout: Duration::from_secs(600) },
    );
    let report = agent.run(&a.task).await;
    for m in &report.transcript {
        store.append_event(sid, "mensaje", &serde_json::to_value(m)?)?;
    }
    let (label, code, text) = match &report.outcome {
        Outcome::Done { final_message } => ("hecho", 0, final_message.clone()),
        Outcome::Failed(m) => ("fallo", 2, m.clone()),
        Outcome::BudgetExhausted => ("presupuesto agotado", 3, String::new()),
        Outcome::Stuck(m) => ("atascado", 4, m.clone()),
    };
    store.finish_session(sid, label)?;
    if !text.is_empty() {
        println!("{text}");
    }
    eprintln!(
        "resultado: {label} · pasos: {} · tokens: {}↑ {}↓",
        report.steps, report.usage.input_tokens, report.usage.output_tokens
    );
    Ok(code)
}
