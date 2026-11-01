//! `pcode doctor` y `pcode config`: diagnóstico del entorno y configuración global.
use crate::provider::{build_provider, pick_model};
use pcode_config::{Config, user_config_path};
use std::path::{Path, PathBuf};

pub use pcode_tools::which;

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
    let mut total = 0usize;
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
                let specs: Vec<_> = tools.iter().map(|t| t.spec()).collect();
                let tok: usize = specs.iter().map(pcode_tools::schema_tokens).sum();
                total += tok;
                println!(
                    "✓ {name}: {} tools · ~{tok} tokens de esquema por petición{}",
                    specs.len(),
                    if m.ask { " (piden aprobación)" } else { "" }
                );
                for s in specs {
                    let d: String = s.description.chars().take(80).collect();
                    println!(
                        "    {}  ~{} tok  {d}",
                        s.name,
                        pcode_tools::schema_tokens(&s)
                    );
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
    let nativas: usize = pcode_tools::native_registry()
        .specs_filtered(|_, _| true)
        .iter()
        .map(pcode_tools::schema_tokens)
        .sum();
    println!(
        "\nEsquemas MCP: ~{total} tokens en cada petición al modelo (las tools nativas, ~{nativas})."
    );
    if total > MCP_SCHEMA_WARN {
        println!(
            "⚠ Es mucho contexto fijo: desactiva los servidores que no uses (`desactivado = true`) o elige \
             servidores con menos tools. El coste de MCP está en los esquemas (ProMCP)."
        );
    }
    Ok(code)
}

/// A partir de cuántos tokens de esquemas MCP se avisa.
pub const MCP_SCHEMA_WARN: usize = 4000;

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
    /// Proveedores de modelos (global y del proyecto), con el estado de su clave; `--probar` lista
    /// sus modelos y `--detectar` busca Ollama, LM Studio y vLLM en este equipo.
    Proveedores {
        /// Prueba cada proveedor (lista sus modelos).
        #[arg(long = "probar")]
        test: bool,
        /// Busca servidores de modelos locales.
        #[arg(long = "detectar")]
        detect: bool,
    },
    /// Añade o cambia un proveedor (la clave no se guarda: se indica su variable de entorno).
    Proveedor {
        /// Nombre (`ollama`, `openrouter`, `empresa`…). Si coincide con una plantilla conocida, rellena
        /// el resto.
        nombre: String,
        /// `anthropic`, `gemini` u `openai-compatible`.
        #[arg(long = "tipo")]
        kind: Option<String>,
        /// URL base (p. ej. `http://127.0.0.1:11434/v1`).
        #[arg(long = "url")]
        url: Option<String>,
        /// Variable de entorno con la clave (p. ej. `OPENROUTER_API_KEY`).
        #[arg(long = "clave-env")]
        key_env: Option<String>,
        /// Modelo por defecto de este proveedor.
        #[arg(long = "modelo")]
        model: Option<String>,
        /// Dejarlo como proveedor por defecto.
        #[arg(long = "predeterminado")]
        default: bool,
        /// Guardarlo en el `pcode.toml` del proyecto en lugar de en la configuración global.
        #[arg(long = "proyecto")]
        project: bool,
    },
    /// Quita un proveedor.
    Quitar {
        /// Nombre.
        nombre: String,
        /// Del `pcode.toml` del proyecto.
        #[arg(long = "proyecto")]
        project: bool,
    },
}

fn settings_paths(global: &Path) -> pcode_server::settings::ConfigPaths {
    pcode_server::settings::ConfigPaths {
        global: Some(global.to_path_buf()),
        project: PathBuf::from("pcode.toml"),
    }
}

fn print_providers(v: &pcode_server::settings::SettingsView) {
    if v.providers.is_empty() {
        println!(
            "No hay proveedores configurados{}.",
            v.fallback
                .as_ref()
                .map(|e| format!(" (se usará la clave de {e})"))
                .unwrap_or_default()
        );
        println!(
            "Añade uno: pcode config proveedor ollama  ·  pcode config proveedor anthropic --predeterminado"
        );
        return;
    }
    for p in &v.providers {
        let clave = match (&p.key_env, p.key_set, p.key_inline) {
            (_, _, true) => "⚠ clave escrita en el archivo".to_string(),
            (Some(e), true, _) => format!("✓ {e}"),
            (Some(e), false, _) => format!("✗ {e} sin definir"),
            (None, _, _) if p.kind == "openai-compatible" => "sin clave".to_string(),
            (None, true, _) => "✓ clave del entorno".to_string(),
            (None, false, _) => "✗ sin clave".to_string(),
        };
        println!(
            "{} {:<12} {:<18} {:<34} {:<22} {}{}",
            if p.default { "★" } else { " " },
            p.name,
            p.kind,
            if p.url.is_empty() {
                "(URL oficial)"
            } else {
                &p.url
            },
            p.model.as_deref().unwrap_or("-"),
            clave,
            if p.scope == "proyecto" {
                "  [proyecto]"
            } else {
                ""
            }
        );
    }
}

fn print_agents(v: &pcode_server::settings::SettingsView) {
    println!();
    println!("Agentes con tu cuenta (pcode run --backend <nombre>, o elígelos en Studio):");
    for a in &v.agents {
        let detalle = match (&a.path, a.binary.is_empty()) {
            (_, true) => "listo".to_string(),
            (Some(p), _) => format!(
                "{p}{}{}",
                a.token_env
                    .as_ref()
                    .map(|e| format!(" · {e} definida"))
                    .unwrap_or_default(),
                a.model
                    .as_ref()
                    .map(|m| format!(" · modelo {m}"))
                    .unwrap_or_default()
            ),
            (None, _) if a.install.is_empty() => format!("no se encuentra «{}»", a.binary),
            (None, _) => format!("instálalo: {}", a.install),
        };
        println!(
            "{} {:<14} {:<16} {detalle}",
            if a.installed { "✓" } else { "✗" },
            a.name,
            a.label
        );
    }
}

/// Ejecuta `pcode config …`.
pub async fn config_cmd(c: ConfigCmd) -> anyhow::Result<i32> {
    let path = user_config_path().ok_or_else(|| {
        anyhow::anyhow!("no se encuentra el directorio de configuración del usuario")
    })?;
    match c {
        ConfigCmd::Ruta => println!("{}", path.display()),
        ConfigCmd::Proveedores { test, detect } => {
            let v = pcode_server::settings::get(&settings_paths(&path));
            print_providers(&v);
            print_agents(&v);
            if test {
                for p in &v.providers {
                    let r = pcode_server::settings::test_provider(
                        &p.kind,
                        &p.url,
                        p.key_env.as_deref().unwrap_or(""),
                    )
                    .await;
                    match r.error {
                        None => println!(
                            "  {}: ✓ {} modelos{}",
                            p.name,
                            r.models.len(),
                            if r.models.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    " ({}{})",
                                    r.models
                                        .iter()
                                        .take(6)
                                        .cloned()
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    if r.models.len() > 6 { ", …" } else { "" }
                                )
                            }
                        ),
                        Some(e) => println!("  {}: ✗ {e}", p.name),
                    }
                }
            }
            if detect {
                let found = pcode_models::discover::detect_local(
                    pcode_models::discover::LOCAL_CANDIDATES,
                    std::time::Duration::from_millis(900),
                )
                .await;
                if found.is_empty() {
                    println!("No hay Ollama, LM Studio ni vLLM escuchando en este equipo.");
                }
                for s in found {
                    println!(
                        "● {} en {} con {} modelos → pcode config proveedor {} --url {}",
                        s.label,
                        s.url,
                        s.models.len(),
                        s.name,
                        s.url
                    );
                }
            }
        }
        ConfigCmd::Proveedor {
            nombre,
            kind,
            url,
            key_env,
            model,
            default,
            project,
        } => {
            let preset = pcode_server::settings::PRESETS
                .iter()
                .find(|p| p.name == nombre);
            // Copilot, Claude Code y opencode no son APIs de modelos sino agentes con su propia
            // cuenta: no se configuran como proveedor.
            if preset.is_none()
                && url.is_none()
                && let Some(a) = pcode_server::settings::AGENTS
                    .iter()
                    .find(|a| a.name == nombre || a.binary == nombre)
            {
                println!(
                    "{} no es una API de modelos: es un agente que usa tu cuenta con su CLI oficial.\n\
                     · Instálalo: {}\n· Inicia sesión: {}\n· Úsalo: pcode run \"tarea\" --backend {} --aplicar, o elígelo en Studio (Opciones → Modelo).",
                    a.label,
                    if cfg!(windows) {
                        a.install_windows
                    } else {
                        a.install_linux
                    },
                    a.login,
                    a.name
                );
                return Ok(2);
            }
            let input = pcode_config::edit::ProviderInput {
                kind: kind
                    .or_else(|| preset.map(|p| p.kind.to_string()))
                    .unwrap_or_else(|| "openai-compatible".into()),
                url: url
                    .or_else(|| preset.map(|p| p.url.to_string()))
                    .unwrap_or_default(),
                key_env: key_env
                    .or_else(|| preset.map(|p| p.key_env.to_string()))
                    .unwrap_or_default(),
                model: model
                    .or_else(|| preset.map(|p| p.model.to_string()))
                    .unwrap_or_default(),
                name: nombre,
            };
            let v = pcode_server::settings::save_provider(
                &settings_paths(&path),
                if project { "proyecto" } else { "global" },
                &input,
                default,
            )
            .map_err(anyhow::Error::msg)?;
            println!(
                "Guardado en {}",
                if project {
                    "pcode.toml (proyecto)".to_string()
                } else {
                    path.display().to_string()
                }
            );
            print_providers(&v);
        }
        ConfigCmd::Quitar { nombre, project } => {
            let v = pcode_server::settings::delete_provider(
                &settings_paths(&path),
                if project { "proyecto" } else { "global" },
                &nombre,
            )
            .map_err(anyhow::Error::msg)?;
            println!("Quitado {nombre}.");
            print_providers(&v);
        }
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
