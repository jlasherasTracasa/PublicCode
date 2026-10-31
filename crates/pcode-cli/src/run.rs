//! `pcode run`.
use crate::provider::{build_provider, pick_model};
use pcode_config::Config;
use pcode_core::{Agent, AgentConfig, Budget, Outcome, StopHook};
use pcode_policy::{AuditLog, Capabilities, Policy};
use pcode_store::Store;
use pcode_tools::ToolContext;
use std::path::PathBuf;
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
    /// Backend de `[backends.*]` (si falta, se usa el bucle propio con el proveedor).
    #[arg(long = "backend")]
    pub backend: Option<String>,
    /// Aplica los cambios de la sandbox si `--acepta` pasa dentro de ella.
    #[arg(long = "aplicar-si-acepta")]
    pub apply_if_accepted: bool,
    /// Archivo de configuración.
    #[arg(long = "config", default_value = "pcode.toml")]
    pub config: PathBuf,
}

#[derive(serde::Deserialize, Default)]
struct PolicyToml {
    #[serde(default, rename = "politica")]
    policy: Capabilities,
}

/// Crea `.pcode/` (auditoría, sesiones) y, si el proyecto es un repo git, lo añade a
/// `.git/info/exclude` para que no aparezca como cambio (es local: no se versiona). Solo para el
/// uso interactivo, nunca dentro de una sandbox.
pub fn ensure_state_dir(root: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root.join(".pcode"))?;
    let git = root.join(".git");
    if git.is_dir() {
        let info = git.join("info");
        let exclude = info.join("exclude");
        let current = std::fs::read_to_string(&exclude).unwrap_or_default();
        if !current
            .lines()
            .any(|l| l.trim() == ".pcode/" || l.trim() == ".pcode")
        {
            std::fs::create_dir_all(&info)?;
            let sep = if current.is_empty() || current.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            std::fs::write(&exclude, format!("{current}{sep}# PublicCode\n.pcode/\n"))?;
        }
    }
    Ok(())
}

/// Capacidades de la sección `[politica]` de `pcode.toml` (vacías si falta o no se entiende).
pub fn load_capabilities(path: &std::path::Path) -> Capabilities {
    match std::fs::read_to_string(path) {
        Ok(s) => toml::from_str::<PolicyToml>(&s).unwrap_or_default().policy,
        Err(_) => Capabilities::default(),
    }
}

/// Ejecuta la tarea y devuelve el código de salida.
pub async fn run(a: RunArgs) -> anyhow::Result<i32> {
    let (cfg, _) = Config::load_layered(&a.config)?;
    if let Some(b) = &a.backend {
        return crate::backend_run::run_with_backend(
            &cfg,
            b,
            &a.task,
            a.accept.as_deref(),
            a.model.as_deref(),
            a.apply_if_accepted,
        )
        .await;
    }
    let caps = load_capabilities(&a.config);
    let sel = build_provider(&cfg, a.provider.as_deref())?;
    let provider = sel.provider.clone();
    let root = std::env::current_dir()?;
    let model = pick_model(a.model.as_deref(), &cfg, &sel);
    // Sin tocar `.git`: `pcode run` también corre dentro de las mini sandboxes de los flujos.
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
        Policy::new(caps),
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

#[cfg(test)]
mod tests {
    #[test]
    fn pcode_queda_excluido_de_git_una_sola_vez() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".git/info")).unwrap();
        std::fs::write(d.path().join(".git/info/exclude"), "*.log").unwrap();
        super::ensure_state_dir(d.path()).unwrap();
        super::ensure_state_dir(d.path()).unwrap();
        let ex = std::fs::read_to_string(d.path().join(".git/info/exclude")).unwrap();
        assert_eq!(ex, "*.log\n# PublicCode\n.pcode/\n");
        assert!(d.path().join(".pcode").is_dir());
    }
}
