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
    /// Preferencias del usuario.
    #[serde(rename = "preferencias")]
    pub preferences: PrefsCfg,
    /// Servidores MCP cuyas tools se añaden al agente.
    #[serde(rename = "mcp")]
    pub mcp: BTreeMap<String, McpCfg>,
}

/// Un servidor MCP (`[mcp.<nombre>]`).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct McpCfg {
    /// Orden y argumentos, p. ej. `["npx", "-y", "@modelcontextprotocol/server-github"]`.
    #[serde(rename = "orden")]
    pub command: Vec<String>,
    /// Variables de entorno (admite `{env:VAR}`).
    #[serde(rename = "entorno")]
    pub env: BTreeMap<String, String>,
    /// Pedir aprobación antes de cada llamada a sus tools.
    #[serde(rename = "preguntar")]
    pub ask: bool,
    /// No conectar.
    #[serde(rename = "desactivado")]
    pub disabled: bool,
}

/// Sección `[preferencias]` (sobre todo en la configuración global del usuario).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct PrefsCfg {
    /// Proveedor por defecto (nombre de `[proveedores.*]`).
    #[serde(rename = "proveedor")]
    pub provider: Option<String>,
    /// Modelo por defecto.
    #[serde(rename = "modelo")]
    pub model: Option<String>,
    /// Tema de la TUI: `oscuro`, `claro` o `alto-contraste`.
    #[serde(rename = "tema")]
    pub theme: Option<String>,
    /// Tokens de entrada a partir de los que se compacta la conversación.
    #[serde(rename = "contexto_max")]
    pub max_context_tokens: Option<u64>,
}

/// Ruta de la configuración global del usuario: `PCODE_CONFIG`, o
/// `%APPDATA%\\pcode\\pcode.toml` en Windows, o `$XDG_CONFIG_HOME/pcode/pcode.toml`
/// (por defecto `~/.config/pcode/pcode.toml`) en el resto.
pub fn user_config_path() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(p) = std::env::var_os("PCODE_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("pcode").join("pcode.toml"))
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
        for m in c.mcp.values_mut() {
            for a in m.command.iter_mut() {
                *a = expand_env(a)?;
            }
            for v in m.env.values_mut() {
                *v = expand_env(v)?;
            }
        }
        for p in c.providers.values_mut() {
            p.url = expand_env(&p.url)?;
            if let Some(k) = &p.api_key {
                p.api_key = Some(expand_env(k)?);
            }
        }
        Ok(c)
    }

    /// Une la configuración global (`user`) y la del proyecto (`project`): el proyecto manda en
    /// los proveedores, backends y roles con el mismo nombre, en las instrucciones si las define y
    /// en cada preferencia que ponga. Si el proyecto define proveedores y no elige uno, se usa el
    /// primero del proyecto.
    pub fn merge(user: Config, project: Config) -> Config {
        let mut c = user;
        let project_first = project.providers.keys().next().cloned();
        c.providers.extend(project.providers);
        c.backends.extend(project.backends);
        c.roles.extend(project.roles);
        c.mcp.extend(project.mcp);
        if !project.project.instructions.is_empty() {
            c.project = project.project;
        }
        let p = project.preferences;
        c.preferences.provider = p
            .provider
            .or(project_first)
            .or(c.preferences.provider)
            .or_else(|| c.providers.keys().next().cloned());
        c.preferences.model = p.model.or(c.preferences.model);
        c.preferences.theme = p.theme.or(c.preferences.theme);
        c.preferences.max_context_tokens =
            p.max_context_tokens.or(c.preferences.max_context_tokens);
        c.sandbox = project.sandbox;
        c
    }

    /// Carga la configuración global del usuario (si existe) y la del proyecto, y las une con
    /// [`Config::merge`]. Devuelve también los archivos que se leyeron.
    pub fn load_layered(project: &Path) -> Result<(Config, Vec<std::path::PathBuf>), ConfigError> {
        let mut read = vec![];
        let user = match user_config_path() {
            Some(p) if p.is_file() => {
                read.push(p.clone());
                Config::load(&p)?
            }
            _ => Config::default(),
        };
        if project.is_file() {
            read.push(project.to_path_buf());
        }
        Ok((Config::merge(user, Config::load(project)?), read))
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

    #[test]
    fn mcp_se_lee_y_expande_el_entorno() {
        // SAFETY de la prueba: solo este test usa PCODE_TEST_MCP_TOKEN.
        unsafe { std::env::set_var("PCODE_TEST_MCP_TOKEN", "secreto") };
        let c = Config::from_str(
            "[mcp.github]\norden = [\"npx\", \"-y\", \"srv\"]\nentorno = { TOKEN = \"{env:PCODE_TEST_MCP_TOKEN}\" }\npreguntar = true\n",
        )
        .unwrap();
        let m = &c.mcp["github"];
        assert_eq!(m.command, vec!["npx", "-y", "srv"]);
        assert_eq!(m.env["TOKEN"], "secreto");
        assert!(m.ask && !m.disabled);
    }

    #[test]
    fn merge_proyecto_manda_y_global_rellena() {
        let user = Config::from_str(
            "[preferencias]\nmodelo = \"m-global\"\ntema = \"claro\"\n[proveedores.anthropic]\ntipo = \"anthropic\"\n",
        )
        .unwrap();
        let only_user = Config::merge(user.clone(), Config::default());
        assert_eq!(only_user.preferences.provider.as_deref(), Some("anthropic"));
        let project = Config::from_str(
            "[preferencias]\nmodelo = \"m-proyecto\"\n[proveedores.litellm]\ntipo = \"openai-compatible\"\nurl = \"http://x\"\n",
        )
        .unwrap();
        let c = Config::merge(user, project);
        assert_eq!(c.providers.len(), 2);
        assert_eq!(c.preferences.provider.as_deref(), Some("litellm"));
        assert_eq!(c.preferences.model.as_deref(), Some("m-proyecto"));
        assert_eq!(c.preferences.theme.as_deref(), Some("claro"));
    }

    #[test]
    fn load_layered_lee_pcode_config() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path().join("global.toml");
        std::fs::write(&g, "[preferencias]\nmodelo = \"g\"\n").unwrap();
        // SAFETY de la prueba: ningún otro test de este crate lee PCODE_CONFIG.
        unsafe { std::env::set_var("PCODE_CONFIG", &g) };
        let (c, read) = Config::load_layered(&d.path().join("no-existe.toml")).unwrap();
        unsafe { std::env::remove_var("PCODE_CONFIG") };
        assert_eq!(c.preferences.model.as_deref(), Some("g"));
        assert_eq!(read, vec![g]);
    }

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
