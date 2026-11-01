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
            e.contains("no se pudo conectar") && e.contains("PCODE_TEST_NO_EXISTE_JAMAS"),
            "{e}"
        );
    }
}
