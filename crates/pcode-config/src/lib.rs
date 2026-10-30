//! Lectura de `pcode.toml` con sustitución `{env:VAR}`.
use std::collections::BTreeMap;
use std::path::Path;

/// Errores de configuración.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// No se pudo leer el archivo.
    #[error("no se pudo leer {0}: {1}")]
    Io(String, std::io::Error),
    /// TOML inválido.
    #[error("TOML inválido: {0}")]
    Toml(#[from] toml::de::Error),
    /// Falta una variable de entorno referenciada.
    #[error("falta la variable de entorno {0}")]
    MissingEnv(String),
}

/// Configuración completa del proyecto.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    /// Datos del proyecto.
    #[serde(rename = "proyecto")]
    pub project: ProjectCfg,
    /// Proveedores de modelos.
    #[serde(rename = "proveedores")]
    pub providers: BTreeMap<String, ProviderCfg>,
    /// Backends de agentes.
    #[serde(rename = "backends")]
    pub backends: BTreeMap<String, BackendCfg>,
    /// Rol → backend.
    #[serde(rename = "roles")]
    pub roles: BTreeMap<String, String>,
    /// Sandbox.
    #[serde(rename = "sandbox")]
    pub sandbox: SandboxCfg,
}

/// Sección `[proyecto]`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ProjectCfg {
    /// Archivos de instrucciones.
    #[serde(rename = "instrucciones")]
    pub instructions: Vec<String>,
}

/// Un proveedor de modelos.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ProviderCfg {
    /// Tipo: openai-compatible, anthropic, gemini.
    #[serde(rename = "tipo")]
    pub kind: String,
    /// URL base.
    pub url: String,
    /// Clave de API.
    #[serde(rename = "clave")]
    pub api_key: Option<String>,
}

/// Un backend de agente.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct BackendCfg {
    /// Tipo: agente-cli, agente-github, modelo.
    #[serde(rename = "tipo")]
    pub kind: String,
    /// Binario de la CLI.
    #[serde(rename = "binario")]
    pub binary: Option<String>,
    /// Modelo.
    #[serde(rename = "modelo")]
    pub model: Option<String>,
}

/// Sección `[sandbox]`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct SandboxCfg {
    /// Nivel por defecto.
    #[serde(rename = "nivel_por_defecto")]
    pub default_level: String,
    /// Política de red.
    #[serde(rename = "red")]
    pub network: String,
}

impl Default for SandboxCfg {
    fn default() -> Self {
        SandboxCfg {
            default_level: "N1".into(),
            network: "denegar".into(),
        }
    }
}

/// Sustituye cada `{env:NOMBRE}` por su valor.
pub fn expand_env(s: &str) -> Result<String, ConfigError> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{env:") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 5..];
        let Some(j) = after.find('}') else {
            out.push_str(&rest[i..]);
            return Ok(out);
        };
        let name = &after[..j];
        let val = std::env::var(name).map_err(|_| ConfigError::MissingEnv(name.to_string()))?;
        out.push_str(&val);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl Config {
    /// Parsea un TOML y expande variables en proveedores.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Config, ConfigError> {
        let mut c: Config = toml::from_str(s)?;
        for p in c.providers.values_mut() {
            p.url = expand_env(&p.url)?;
            if let Some(k) = &p.api_key {
                p.api_key = Some(expand_env(k)?);
            }
        }
        Ok(c)
    }

    /// Lee un archivo; si no existe, devuelve la configuración por defecto.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => Config::from_str(&s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(path.display().to_string(), e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EJEMPLO: &str = r#"
[proyecto]
instrucciones = ["AGENTS.md"]
[proveedores.litellm]
tipo = "openai-compatible"
url = "http://127.0.0.1:30400/v1"
clave = "k"
[backends.claude-code]
tipo = "agente-cli"
binario = "claude"
[backends.copilot]
tipo = "agente-cli"
[backends.copilot-github]
tipo = "agente-github"
[backends.gemini]
tipo = "agente-cli"
[backends.glm]
tipo = "modelo"
modelo = "litellm/GLM-5.3-Flash"
[roles]
planificador = "claude-code"
[sandbox]
nivel_por_defecto = "N1"
red = "denegar"
"#;

    #[test]
    fn parsea_ejemplo_del_readme() {
        let c = Config::from_str(EJEMPLO).unwrap();
        assert_eq!(c.providers.len(), 1);
        assert_eq!(c.backends.len(), 5);
    }
    #[test]
    fn expand_env_sustituye() {
        // SAFETY-free: variable única de este test.
        unsafe { std::env::set_var("PCODE_TEST_X", "abc") };
        assert_eq!(expand_env("k={env:PCODE_TEST_X}").unwrap(), "k=abc");
    }
    #[test]
    fn expand_env_falta_variable() {
        assert!(matches!(
            expand_env("{env:PCODE_NO_EXISTE_9}"),
            Err(ConfigError::MissingEnv(_))
        ));
    }
    #[test]
    fn load_sin_archivo_da_default() {
        let c = Config::load(Path::new("/no/existe/pcode.toml")).unwrap();
        assert!(c.providers.is_empty());
    }
    #[test]
    fn sandbox_por_defecto_es_n1_sin_red() {
        let c = Config::default();
        assert_eq!(c.sandbox.default_level, "N1");
        assert_eq!(c.sandbox.network, "denegar");
    }
}
