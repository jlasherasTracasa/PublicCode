//! `pcode doctor` y `pcode config`: diagnóstico del entorno y configuración global.
use crate::provider::{build_provider, pick_model};
use pcode_config::{Config, user_config_path};
use std::path::{Path, PathBuf};

/// Busca un ejecutable en el `PATH` (con las extensiones de `PATHEXT` en Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|e| dir.join(format!("{name}{e}")))
            .find(|p| p.is_file())
    })
}

fn line(ok: bool, what: &str, detail: &str) {
    let sym = if ok { "✓" } else { "✗" };
    println!("{sym} {what:<26} {detail}");
}

/// Diagnóstico. Devuelve 0 si se puede usar el agente (hay modelo); 1 si no.
pub fn doctor(project_cfg: &Path) -> i32 {
    println!(
        "pcode {} · {} {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let user = user_config_path();
    match &user {
        Some(p) if p.is_file() => line(true, "configuración global", &p.display().to_string()),
        Some(p) => line(
            true,
            "configuración global",
            &format!(
                "no existe ({}); créala con `pcode config init`",
                p.display()
            ),
        ),
        None => line(
            false,
            "configuración global",
            "no se encuentra el directorio de usuario",
        ),
    }
    line(
        true,
        "configuración del proyecto",
        &if project_cfg.is_file() {
            project_cfg.display().to_string()
        } else {
            format!("sin {} (opcional)", project_cfg.display())
        },
    );
    for k in [
        "ANTHROPIC_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
    ] {
        if std::env::var_os(k).is_some_and(|v| !v.is_empty()) {
            line(true, k, "definida");
        }
    }
    let mut code = 0;
    match Config::load_layered(project_cfg) {
        Err(e) => {
            line(false, "configuración", &e.to_string());
            code = 1;
        }
        Ok((cfg, _)) => match build_provider(&cfg, None) {
            Ok(sel) => line(
                true,
                "modelo",
                &format!(
                    "{} con {}",
                    pick_model(None, &cfg, &sel),
                    sel.provider.name()
                ),
            ),
            Err(e) => {
                line(false, "modelo", &e.to_string());
                code = 1;
            }
        },
    }
    line(
        which("git").is_some(),
        "git",
        if which("git").is_some() {
            "en el PATH"
        } else {
            "no está en el PATH (lo usan los flujos y la integración)"
        },
    );
    line(
        true,
        "shell de la tool bash",
        if cfg!(windows) { "cmd.exe" } else { "sh" },
    );
    let n1 = pcode_sandbox::userns_available();
    line(
        n1,
        "mini sandbox N1",
        if n1 {
            "disponible (namespaces de usuario)"
        } else if cfg!(windows) {
            "no en Windows: se usa N0 (git worktree, sin aislar red ni disco); N1 dentro de WSL2"
        } else {
            "no disponible: se usa N0 (git worktree); para N1 activa los namespaces de usuario"
        },
    );
    let agents: Vec<&str> = ["claude", "gemini", "copilot", "opencode"]
        .into_iter()
        .filter(|a| which(a).is_some())
        .collect();
    line(
        true,
        "agentes externos",
        &if agents.is_empty() {
            "ninguno en el PATH (opcional, para --backend)".to_string()
        } else {
            agents.join(", ")
        },
    );
    println!();
    if code == 0 {
        println!("Todo listo: ejecuta `pcode` en tu proyecto.");
    } else {
        println!(
            "Falta un modelo: define ANTHROPIC_API_KEY (o GEMINI_API_KEY u OPENAI_API_KEY) o ejecuta `pcode config init`."
        );
    }
    code
}

/// `pcode mcp`: conecta con cada servidor MCP configurado y lista sus tools.
pub async fn mcp_cmd(project_cfg: &Path) -> anyhow::Result<i32> {
    let (cfg, _) = Config::load_layered(project_cfg)?;
    if cfg.mcp.is_empty() {
        println!("No hay servidores MCP. Añade uno en la configuración, por ejemplo:\n");
        println!(
            "[mcp.github]\norden = [\"npx\", \"-y\", \"@modelcontextprotocol/server-github\"]\nentorno = {{ GITHUB_PERSONAL_ACCESS_TOKEN = \"{{env:GITHUB_TOKEN}}\" }}"
        );
        return Ok(0);
    }
    let mut code = 0;
    for (name, m) in &cfg.mcp {
        if m.disabled {
            println!("· {name}: desactivado");
            continue;
        }
        let srv = pcode_tools::mcp::McpServer {
            name: name.clone(),
            command: m.command.clone(),
            env: m.env.clone(),
            ask: m.ask,
        };
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            pcode_tools::mcp::tools_of(&srv),
        )
        .await
        {
            Ok(Ok(tools)) => {
                println!(
                    "✓ {name}: {} tools{}",
                    tools.len(),
                    if m.ask { " (piden aprobación)" } else { "" }
                );
                for t in tools {
                    let s = t.spec();
                    let d: String = s.description.chars().take(90).collect();
                    println!("    {}  {d}", s.name);
                }
            }
            Ok(Err(e)) => {
                println!("✗ {name}: {e}");
                code = 1;
            }
            Err(_) => {
                println!("✗ {name}: no respondió en 30 s");
                code = 1;
            }
        }
    }
    Ok(code)
}

/// Plantilla de la configuración global.
pub const TEMPLATE: &str = r#"# Configuración global de PublicCode (la del proyecto, pcode.toml, manda sobre esta).
# Las claves no se escriben aquí: se leen del entorno con {env:VARIABLE}.

[preferencias]
# proveedor = "anthropic"          # nombre de una sección [proveedores.*]
# modelo = "claude-sonnet-5-5"
# tema = "oscuro"                  # oscuro, claro o alto-contraste
# contexto_max = 160000            # tokens a partir de los que se compacta la conversación

# [proveedores.anthropic]
# tipo = "anthropic"
# clave = "{env:ANTHROPIC_API_KEY}"

# [proveedores.gemini]
# tipo = "gemini"
# clave = "{env:GEMINI_API_KEY}"

# [proveedores.litellm]
# tipo = "openai-compatible"
# url = "http://127.0.0.1:4000/v1"
# clave = "{env:LITELLM_API_KEY}"

# [mcp.github]                     # tools externas por MCP (stdio)
# orden = ["npx", "-y", "@modelcontextprotocol/server-github"]
# entorno = { GITHUB_PERSONAL_ACCESS_TOKEN = "{env:GITHUB_TOKEN}" }
# preguntar = false                # true: pedir aprobación antes de cada llamada

# [politica]                       # se suma a la capa segura por defecto; solo puede endurecerla
# preguntar_ordenes = ["docker *"]
# denegar_ordenes = ["git push --force*"]
"#;

/// Subórdenes de `pcode config`.
#[derive(Debug, clap::Subcommand)]
pub enum ConfigCmd {
    /// Muestra la ruta de la configuración global.
    Ruta,
    /// Crea la configuración global con una plantilla comentada (no sobrescribe).
    Init,
    /// Muestra la configuración que se usaría aquí (sin claves).
    Mostrar {
        /// Configuración del proyecto.
        #[arg(long = "config", default_value = "pcode.toml")]
        config: PathBuf,
    },
}

/// Ejecuta `pcode config …`.
pub fn config_cmd(c: ConfigCmd) -> anyhow::Result<i32> {
    let path = user_config_path().ok_or_else(|| {
        anyhow::anyhow!("no se encuentra el directorio de configuración del usuario")
    })?;
    match c {
        ConfigCmd::Ruta => println!("{}", path.display()),
        ConfigCmd::Init => {
            if path.exists() {
                println!("Ya existe: {}", path.display());
                return Ok(0);
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, TEMPLATE)?;
            println!("Creada: {}", path.display());
        }
        ConfigCmd::Mostrar { config } => {
            let (cfg, read) = Config::load_layered(&config)?;
            println!("Archivos leídos:");
            for r in &read {
                println!("  {}", r.display());
            }
            if read.is_empty() {
                println!("  (ninguno: se usan las claves del entorno)");
            }
            println!("Proveedores:");
            for (n, p) in &cfg.providers {
                let key = if p.api_key.is_some() {
                    "con clave"
                } else {
                    "sin clave"
                };
                println!("  {n}: {} {} ({key})", p.kind, p.url);
            }
            let pr = &cfg.preferences;
            println!(
                "Preferencias: proveedor={} modelo={} tema={} contexto_max={}",
                pr.provider.as_deref().unwrap_or("-"),
                pr.model.as_deref().unwrap_or("-"),
                pr.theme.as_deref().unwrap_or("-"),
                pr.max_context_tokens
                    .map(|n| n.to_string())
                    .unwrap_or("-".into())
            );
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_plantilla_es_toml_valido_y_carga_vacia() {
        let c = Config::from_str(TEMPLATE).unwrap();
        assert!(c.providers.is_empty());
        assert!(c.preferences.model.is_none());
    }
}
