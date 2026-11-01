//! Ajustes de modelos para Studio: proveedores (por URL), modelo por defecto, prueba de conexión y
//! servidores locales. Toda la lógica está aquí; Studio solo la presenta.
//!
//! Se edita la configuración global del usuario (`~/.config/pcode/pcode.toml` o
//! `%APPDATA%\pcode\pcode.toml`) o la del proyecto (`pcode.toml`), conservando sus comentarios
//! ([`pcode_config::edit`]). Las claves no se escriben nunca: cada proveedor guarda el nombre de la
//! variable de entorno que la contiene, y aquí solo se dice si está definida.
use pcode_config::Config;
use pcode_config::edit::{self, ProviderInput};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dónde están los dos archivos de configuración.
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    /// Global del usuario (None si no hay carpeta de configuración, p. ej. sin HOME).
    pub global: Option<PathBuf>,
    /// Del proyecto.
    pub project: PathBuf,
}

/// Un proveedor tal como lo enseña la pantalla de Ajustes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    /// Nombre.
    pub name: String,
    /// Tipo.
    pub kind: String,
    /// URL base (vacía = la oficial).
    pub url: String,
    /// Variable de la clave.
    pub key_env: Option<String>,
    /// Si esa variable está definida para el demonio.
    pub key_set: bool,
    /// La clave está escrita en claro en el archivo (se avisa).
    pub key_inline: bool,
    /// Modelo del proveedor.
    pub model: Option<String>,
    /// `global` o `proyecto`.
    pub scope: String,
    /// Es el proveedor que se usará por defecto.
    pub default: bool,
    /// El del proyecto tapa a uno global con el mismo nombre.
    pub shadows_global: bool,
}

/// Una plantilla para añadir un proveedor en un clic.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    /// Nombre sugerido.
    pub name: &'static str,
    /// Etiqueta.
    pub label: &'static str,
    /// Descripción corta.
    pub hint: &'static str,
    /// Tipo.
    pub kind: &'static str,
    /// URL base.
    pub url: &'static str,
    /// Variable de la clave (vacía = sin clave).
    pub key_env: &'static str,
    /// Modelo sugerido.
    pub model: &'static str,
    /// Es local (en este equipo).
    pub local: bool,
}

/// Plantillas de proveedores conocidos.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "anthropic",
        label: "Anthropic",
        hint: "Claude, con caché de prompt",
        kind: "anthropic",
        url: "",
        key_env: "ANTHROPIC_API_KEY",
        model: "claude-sonnet-5-5",
        local: false,
    },
    Preset {
        name: "gemini",
        label: "Google Gemini",
        hint: "contexto muy largo",
        kind: "gemini",
        url: "",
        key_env: "GEMINI_API_KEY",
        model: "gemini-2.5-pro",
        local: false,
    },
    Preset {
        name: "openai",
        label: "OpenAI",
        hint: "API oficial de OpenAI",
        kind: "openai-compatible",
        url: "https://api.openai.com/v1",
        key_env: "OPENAI_API_KEY",
        model: "",
        local: false,
    },
    Preset {
        name: "openrouter",
        label: "OpenRouter",
        hint: "cientos de modelos con una clave",
        kind: "openai-compatible",
        url: "https://openrouter.ai/api/v1",
        key_env: "OPENROUTER_API_KEY",
        model: "",
        local: false,
    },
    Preset {
        name: "ollama",
        label: "Ollama",
        hint: "modelos locales, sin clave",
        kind: "openai-compatible",
        url: "http://127.0.0.1:11434/v1",
        key_env: "",
        model: "",
        local: true,
    },
    Preset {
        name: "lmstudio",
        label: "LM Studio",
        hint: "modelos locales, sin clave",
        kind: "openai-compatible",
        url: "http://127.0.0.1:1234/v1",
        key_env: "",
        model: "",
        local: true,
    },
    Preset {
        name: "vllm",
        label: "vLLM",
        hint: "servidor propio de alto rendimiento",
        kind: "openai-compatible",
        url: "http://127.0.0.1:8000/v1",
        key_env: "",
        model: "",
        local: true,
    },
    Preset {
        name: "litellm",
        label: "LiteLLM",
        hint: "pasarela a varios modelos (GLM, Qwen…)",
        kind: "openai-compatible",
        url: "http://127.0.0.1:4000/v1",
        key_env: "LITELLM_API_KEY",
        model: "",
        local: true,
    },
];

/// Variables de clave habituales que se buscan en el entorno (para sugerirlas).
pub const KNOWN_ENVS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "LITELLM_API_KEY",
];

/// Un agente externo que se usa con su CLI oficial y la cuenta (o suscripción) del propio usuario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentInfo {
    /// Nombre del backend (`pcode run --backend <name>`).
    pub name: &'static str,
    /// Nombre para mostrar.
    pub label: &'static str,
    /// Descripción corta.
    pub hint: &'static str,
    /// Ejecutable de su CLI.
    pub binary: &'static str,
    /// Cómo instalarlo en Linux.
    pub install_linux: &'static str,
    /// Cómo instalarlo en Windows.
    pub install_windows: &'static str,
    /// Cómo iniciar sesión.
    pub login: &'static str,
    /// Variables de entorno con las que también se autentica (por orden de preferencia).
    pub token_envs: &'static [&'static str],
    /// Documentación oficial.
    pub docs: &'static str,
}

/// Agentes externos conocidos, en el orden en que se enseñan.
pub const AGENTS: &[AgentInfo] = &[
    AgentInfo {
        name: "copilot",
        label: "GitHub Copilot",
        hint: "tu suscripción de Copilot, con su CLI oficial",
        binary: "copilot",
        install_linux: "npm install -g @github/copilot",
        install_windows: "winget install GitHub.Copilot",
        login: "Abre `copilot` y escribe /login, o define COPILOT_GITHUB_TOKEN con un token fine-grained con el permiso «Copilot Requests». Necesita una suscripción de Copilot.",
        token_envs: &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
        docs: "https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli",
    },
    AgentInfo {
        name: "claude-code",
        label: "Claude Code",
        hint: "tu cuenta de Claude, con su CLI oficial",
        binary: "claude",
        install_linux: "npm install -g @anthropic-ai/claude-code",
        install_windows: "npm install -g @anthropic-ai/claude-code",
        login: "Abre `claude` e inicia sesión con tu cuenta, o define ANTHROPIC_API_KEY.",
        token_envs: &["ANTHROPIC_API_KEY"],
        docs: "https://code.claude.com/docs/en/setup",
    },
    AgentInfo {
        name: "gemini",
        label: "Gemini CLI",
        hint: "tu cuenta de Google, con su CLI oficial",
        binary: "gemini",
        install_linux: "npm install -g @google/gemini-cli",
        install_windows: "npm install -g @google/gemini-cli",
        login: "Abre `gemini` e inicia sesión con Google, o define GEMINI_API_KEY.",
        token_envs: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        docs: "https://github.com/google-gemini/gemini-cli",
    },
    AgentInfo {
        name: "opencode",
        label: "opencode",
        hint: "agente libre con sus propios proveedores",
        binary: "opencode",
        install_linux: "npm install -g opencode-ai",
        install_windows: "npm install -g opencode-ai",
        login: "Ejecuta `opencode auth login` y elige el proveedor.",
        token_envs: &[],
        docs: "https://opencode.ai/docs",
    },
];

/// Un agente tal como lo enseña la pantalla de Ajustes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentView {
    /// Nombre del backend.
    pub name: String,
    /// Nombre para mostrar.
    pub label: String,
    /// Descripción corta.
    pub hint: String,
    /// Ejecutable que se busca.
    pub binary: String,
    /// Está instalado (su ejecutable está en el `PATH`, o no necesita ninguno).
    pub installed: bool,
    /// Dónde está el ejecutable.
    pub path: Option<String>,
    /// Orden de instalación para este sistema (vacía si no aplica).
    pub install: String,
    /// Cómo iniciar sesión.
    pub login: String,
    /// La primera variable de autenticación definida, si hay alguna.
    pub token_env: Option<String>,
    /// Modelo configurado en `[backends.<name>]` (vacío = el del agente).
    pub model: Option<String>,
    /// Documentación oficial.
    pub docs: String,
    /// Declarado en la configuración (`[backends.<name>]`) en vez de ser uno integrado.
    pub custom: bool,
}

/// Los agentes externos: los conocidos ([`AGENTS`]) y los backends de la configuración que se
/// pueden lanzar. `path` es la lista de directorios donde buscar (el `PATH`) y `env_set` dice si
/// una variable está definida.
pub fn agents(
    cfg: &Config,
    path: &std::ffi::OsStr,
    env_set: &dyn Fn(&str) -> bool,
) -> Vec<AgentView> {
    let find = |bin: &str| pcode_tools::which_in(bin, path).map(|p| p.display().to_string());
    let mut out: Vec<AgentView> = AGENTS
        .iter()
        .map(|a| {
            let b = cfg.backends.get(a.name);
            let binary = b
                .and_then(|b| b.binary.clone())
                .unwrap_or_else(|| a.binary.to_string());
            let found = find(&binary);
            AgentView {
                name: a.name.into(),
                label: a.label.into(),
                hint: a.hint.into(),
                installed: found.is_some(),
                path: found,
                binary,
                install: if cfg!(windows) {
                    a.install_windows
                } else {
                    a.install_linux
                }
                .into(),
                login: a.login.into(),
                token_env: a
                    .token_envs
                    .iter()
                    .find(|e| env_set(e))
                    .map(|e| e.to_string()),
                model: b.and_then(|b| b.model.clone()),
                docs: a.docs.into(),
                custom: false,
            }
        })
        .collect();
    for (name, b) in &cfg.backends {
        if AGENTS.iter().any(|a| a.name == name) {
            continue;
        }
        let known = AGENTS
            .iter()
            .find(|a| name.starts_with(a.binary) || name.starts_with(a.name));
        let (binary, found) = match (b.kind.as_str(), known) {
            ("modelo", _) => (String::new(), None),
            ("agente-cli", Some(a)) => {
                let bin = b.binary.clone().unwrap_or_else(|| a.binary.to_string());
                let f = find(&bin);
                (bin, f)
            }
            // Los demás tipos no se pueden lanzar desde una conversación.
            _ => continue,
        };
        out.push(AgentView {
            name: name.clone(),
            label: name.clone(),
            hint: if b.kind == "modelo" {
                "backend de modelo de la configuración".into()
            } else {
                format!("{} con otra configuración", known.map_or("", |a| a.label))
            },
            installed: b.kind == "modelo" || found.is_some(),
            path: found,
            binary,
            install: String::new(),
            login: String::new(),
            token_env: None,
            model: b.model.clone(),
            docs: String::new(),
            custom: true,
        });
    }
    out
}

/// Todo lo que enseña la pantalla de Ajustes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// Archivo global.
    pub global_path: Option<String>,
    /// Archivo del proyecto.
    pub project_path: String,
    /// Proveedores (global y proyecto).
    pub providers: Vec<ProviderView>,
    /// Proveedor que se usará por defecto (tras unir global y proyecto).
    pub default_provider: Option<String>,
    /// Modelo por defecto.
    pub default_model: Option<String>,
    /// Límite de contexto configurado.
    pub context_max: Option<u64>,
    /// Variables de clave conocidas que están definidas.
    pub env_keys: Vec<String>,
    /// Plantillas.
    pub presets: &'static [Preset],
    /// Si no hay proveedores, de qué variable saldría el modelo.
    pub fallback: Option<String>,
    /// Agentes externos (Copilot, Claude Code, Gemini CLI, opencode…).
    pub agents: Vec<AgentView>,
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Estado actual de los ajustes.
pub fn get(paths: &ConfigPaths) -> SettingsView {
    let global_text = paths.global.as_deref().map(read).unwrap_or_default();
    let project_text = read(&paths.project);
    let user = Config::from_str(&global_text).unwrap_or_default();
    let project = Config::from_str(&project_text).unwrap_or_default();
    let merged = Config::merge(user, project);
    let project_raw = edit::raw_providers(&project_text);
    let mut providers = vec![];
    for (scope, list) in [
        ("global", edit::raw_providers(&global_text)),
        ("proyecto", project_raw.clone()),
    ] {
        for p in list {
            let key_set = match &p.key_env {
                Some(v) => env_set(v),
                None if p.key_inline => true,
                None => match p.kind.as_str() {
                    "anthropic" => env_set("ANTHROPIC_API_KEY"),
                    "gemini" => env_set("GEMINI_API_KEY") || env_set("GOOGLE_API_KEY"),
                    _ => false,
                },
            };
            let shadows_global = scope == "proyecto"
                && edit::raw_providers(&global_text)
                    .iter()
                    .any(|g| g.name == p.name);
            let hidden = scope == "global" && project_raw.iter().any(|x| x.name == p.name);
            providers.push(ProviderView {
                default: !hidden && merged.preferences.provider.as_deref() == Some(&p.name),
                name: p.name,
                kind: p.kind,
                url: p.url,
                key_env: p.key_env,
                key_set,
                key_inline: p.key_inline,
                model: p.model,
                scope: scope.into(),
                shadows_global,
            });
        }
    }
    let env_keys: Vec<String> = KNOWN_ENVS
        .iter()
        .filter(|e| env_set(e))
        .map(|e| e.to_string())
        .collect();
    SettingsView {
        global_path: paths.global.as_ref().map(|p| p.display().to_string()),
        project_path: paths.project.display().to_string(),
        default_provider: merged.preferences.provider.clone(),
        default_model: merged.preferences.model.clone().or_else(|| {
            merged
                .preferences
                .provider
                .as_ref()
                .and_then(|p| merged.providers.get(p))
                .and_then(|p| p.model.clone())
        }),
        context_max: merged.preferences.max_context_tokens,
        fallback: if providers.is_empty() {
            env_keys.first().cloned()
        } else {
            None
        },
        agents: agents(
            &merged,
            &std::env::var_os("PATH").unwrap_or_default(),
            &env_set,
        ),
        providers,
        env_keys,
        presets: PRESETS,
    }
}

fn file_for<'a>(paths: &'a ConfigPaths, scope: &str) -> Result<&'a Path, String> {
    match scope {
        "proyecto" | "project" => Ok(&paths.project),
        _ => paths
            .global
            .as_deref()
            .ok_or_else(|| "no hay carpeta de configuración del usuario".to_string()),
    }
}

/// Crea o cambia un proveedor; con `make_default`, además lo deja como predeterminado (con su
/// modelo) en el mismo archivo.
pub fn save_provider(
    paths: &ConfigPaths,
    scope: &str,
    input: &ProviderInput,
    make_default: bool,
) -> Result<SettingsView, String> {
    let file = file_for(paths, scope)?;
    edit::update_file(file, |t| {
        let out = edit::upsert_provider(t, input)?;
        if make_default {
            edit::set_defaults(&out, Some(&input.name), Some(&input.model), None)
        } else {
            Ok(out)
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(get(paths))
}

/// Borra un proveedor.
pub fn delete_provider(
    paths: &ConfigPaths,
    scope: &str,
    name: &str,
) -> Result<SettingsView, String> {
    let file = file_for(paths, scope)?;
    edit::update_file(file, |t| edit::remove_provider(t, name)).map_err(|e| e.to_string())?;
    Ok(get(paths))
}

/// Fija el proveedor y el modelo por defecto (y el límite de contexto, si se da).
pub fn set_default(
    paths: &ConfigPaths,
    scope: &str,
    provider: Option<&str>,
    model: Option<&str>,
    context_max: Option<u64>,
) -> Result<SettingsView, String> {
    let file = file_for(paths, scope)?;
    edit::update_file(file, |t| {
        edit::set_defaults(t, provider, model, context_max)
    })
    .map_err(|e| e.to_string())?;
    Ok(get(paths))
}

/// Resultado de probar un proveedor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    /// Respondió.
    pub ok: bool,
    /// Modelos que ofrece.
    pub models: Vec<String>,
    /// Qué falló.
    pub error: Option<String>,
    /// La variable de la clave no está definida.
    pub key_missing: bool,
}

/// Prueba un proveedor: resuelve la clave de su variable (o de la habitual del tipo) y lista sus
/// modelos. Como mucho 10 s.
pub async fn test_provider(kind: &str, url: &str, key_env: &str) -> TestResult {
    let key_env = key_env.trim();
    let key = if key_env.is_empty() {
        match kind {
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
            "gemini" => std::env::var("GEMINI_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .ok(),
            _ => None,
        }
    } else {
        std::env::var(key_env).ok()
    };
    let key_missing = (!key_env.is_empty() || matches!(kind, "anthropic" | "gemini"))
        && key.as_deref().is_none_or(str::is_empty);
    match pcode_models::discover::list_models(kind, url, key.as_deref(), Duration::from_secs(10))
        .await
    {
        Ok(models) => TestResult {
            ok: true,
            models,
            error: None,
            key_missing,
        },
        Err(e) => TestResult {
            ok: false,
            models: vec![],
            error: Some(if key_missing {
                format!(
                    "{e} · la variable {} no está definida para este Studio",
                    if key_env.is_empty() {
                        "de la clave"
                    } else {
                        key_env
                    }
                )
            } else {
                e
            }),
            key_missing,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(d: &tempfile::TempDir) -> ConfigPaths {
        ConfigPaths {
            global: Some(d.path().join("usuario/pcode.toml")),
            project: d.path().join("proyecto/pcode.toml"),
        }
    }

    fn input(name: &str, url: &str, env: &str, model: &str) -> ProviderInput {
        ProviderInput {
            name: name.into(),
            kind: "openai-compatible".into(),
            url: url.into(),
            key_env: env.into(),
            model: model.into(),
        }
    }

    #[test]
    fn ajustes_añadir_predeterminado_proyecto_y_borrar() {
        let d = tempfile::tempdir().unwrap();
        let p = paths(&d);
        let v = get(&p);
        assert!(v.providers.is_empty() && v.default_provider.is_none());
        assert_eq!(v.presets.len(), PRESETS.len());
        let v = save_provider(
            &p,
            "global",
            &input("ollama", "http://127.0.0.1:11434/v1", "", "qwen3"),
            true,
        )
        .unwrap();
        assert_eq!(v.default_provider.as_deref(), Some("ollama"));
        assert_eq!(v.default_model.as_deref(), Some("qwen3"));
        assert!(v.providers[0].default && !v.providers[0].key_set);
        let v = save_provider(
            &p,
            "global",
            &input(
                "remoto",
                "https://x.example/v1",
                "PCODE_TEST_NO_EXISTE_JAMAS",
                "",
            ),
            false,
        )
        .unwrap();
        let r = v.providers.iter().find(|x| x.name == "remoto").unwrap();
        assert_eq!(r.key_env.as_deref(), Some("PCODE_TEST_NO_EXISTE_JAMAS"));
        assert!(!r.key_set && !r.default);
        // El proyecto puede tapar a un proveedor global con el mismo nombre.
        let v = save_provider(
            &p,
            "proyecto",
            &input("ollama", "http://10.0.0.9:11434/v1", "", "llama3.3"),
            false,
        )
        .unwrap();
        let proy = v.providers.iter().find(|x| x.scope == "proyecto").unwrap();
        assert!(proy.shadows_global && proy.default);
        assert!(!v.providers.iter().any(|x| x.scope == "global" && x.default));
        let texto = std::fs::read_to_string(p.global.as_ref().unwrap()).unwrap();
        assert!(!texto.contains("sk-"), "no hay claves en el archivo");
        let v = delete_provider(&p, "global", "ollama").unwrap();
        assert!(
            !v.providers
                .iter()
                .any(|x| x.scope == "global" && x.name == "ollama")
        );
        let v = set_default(&p, "global", Some("remoto"), Some("m2"), Some(120_000)).unwrap();
        assert_eq!(v.context_max, Some(120_000));
        assert!(save_provider(&p, "global", &input("MAL", "", "", ""), false).is_err());
    }

    #[tokio::test]
    async fn probar_sin_servidor_explica_el_fallo() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port());
        drop(l);
        let r = test_provider("openai-compatible", &url, "PCODE_TEST_NO_EXISTE_JAMAS").await;
        assert!(!r.ok && r.key_missing);
        let e = r.error.unwrap();
        assert!(
            e.contains("¿está arrancado el servidor") && e.contains("PCODE_TEST_NO_EXISTE_JAMAS"),
            "{e}"
        );
    }

    #[test]
    fn agentes_instalados_token_y_backends_propios() {
        let d = tempfile::tempdir().unwrap();
        let exe = |n: &str| {
            if cfg!(windows) {
                format!("{n}.exe")
            } else {
                n.to_string()
            }
        };
        std::fs::write(d.path().join(exe("copilot")), "").unwrap();
        std::fs::write(d.path().join(exe("claude-mio")), "").unwrap();
        let path = std::env::join_paths([d.path()]).unwrap();
        let cfg = Config::from_str(
            "[backends.copilot]\ntipo = \"agente-cli\"\nmodelo = \"claude-haiku-4.5\"\n\
             [backends.claude-rapido]\ntipo = \"agente-cli\"\nbinario = \"claude-mio\"\n\
             [backends.glm]\ntipo = \"modelo\"\nmodelo = \"litellm/glm\"\n\
             [backends.copilot-github]\ntipo = \"agente-github\"\n",
        )
        .unwrap();
        let v = agents(&cfg, &path, &|e| e == "GH_TOKEN");
        let names: Vec<&str> = v.iter().map(|a| a.name.as_str()).collect();
        // Copilot primero; los propios lanzables después; agente-github no se ofrece.
        assert_eq!(
            names,
            [
                "copilot",
                "claude-code",
                "gemini",
                "opencode",
                "claude-rapido",
                "glm"
            ]
        );
        let c = &v[0];
        assert!(c.installed && c.path.is_some() && !c.custom);
        assert_eq!(c.label, "GitHub Copilot");
        assert_eq!(c.token_env.as_deref(), Some("GH_TOKEN"));
        assert_eq!(c.model.as_deref(), Some("claude-haiku-4.5"));
        assert!(c.install.contains("copilot") || c.install.contains("Copilot"));
        assert!(c.docs.starts_with("https://docs.github.com/"));
        assert!(!v[1].installed && v[1].token_env.is_none());
        assert!(v[4].installed && v[4].custom && v[4].binary == "claude-mio");
        assert!(v[5].installed && v[5].binary.is_empty());
    }
}
