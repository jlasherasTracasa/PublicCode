//! Edición de `pcode.toml` para la pantalla de Ajustes y `pcode config proveedor`.
//!
//! Se edita **por secciones de texto** (no se vuelve a serializar el archivo entero) para conservar
//! los comentarios y el orden de lo que el usuario escribió a mano. Cada cambio se valida volviendo
//! a leer el resultado antes de devolverlo. Las claves nunca se escriben: un proveedor guarda
//! `clave = "{env:VARIABLE}"`.
use crate::{Config, ConfigError};
use std::path::Path;

/// Tipos de proveedor que entiende PublicCode.
pub const KINDS: &[&str] = &["anthropic", "gemini", "openai-compatible"];

/// Un proveedor tal como está escrito en un archivo (sin expandir variables).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderEntry {
    /// Nombre (`[proveedores.<nombre>]`).
    pub name: String,
    /// `anthropic`, `gemini` u `openai-compatible`.
    pub kind: String,
    /// URL base (vacía = la oficial del tipo).
    pub url: String,
    /// Variable de entorno de la clave, si la clave es `{env:VARIABLE}`.
    pub key_env: Option<String>,
    /// Si la clave está escrita en claro en el archivo (desaconsejado: se avisa).
    pub key_inline: bool,
    /// Modelo por defecto del proveedor.
    pub model: Option<String>,
}

/// Lo que se quiere guardar de un proveedor.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct ProviderInput {
    /// Nombre.
    pub name: String,
    /// Tipo.
    pub kind: String,
    /// URL base (vacía = la oficial).
    #[serde(default)]
    pub url: String,
    /// Variable de entorno de la clave (vacía = sin clave, p. ej. Ollama).
    #[serde(default)]
    pub key_env: String,
    /// Modelo por defecto (vacío = ninguno).
    #[serde(default)]
    pub model: String,
}

fn invalid(m: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(m.into())
}

/// Nombre válido para un proveedor: letras minúsculas, números, `-` y `_`, hasta 40.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 40
        && n.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Nombre válido de variable de entorno: `A-Z`, `0-9` y `_`, sin empezar por número.
pub fn valid_env(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 80
        && !n.starts_with(|c: char| c.is_ascii_digit())
        && n.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// URL `http://` o `https://` sin espacios ni comillas.
pub fn valid_url(u: &str) -> bool {
    (u.starts_with("http://") || u.starts_with("https://"))
        && u.len() > 8
        && u.len() <= 300
        && !u
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
}

/// Modelo: texto corto sin saltos de línea ni comillas.
fn valid_model(m: &str) -> bool {
    m.len() <= 200 && !m.chars().any(|c| c.is_control() || c == '"' || c == '\\')
}

/// Valor TOML de una cadena (con las comillas y el escapado correctos).
fn q(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// `(inicio, fin)` en líneas de la sección `[header]`: desde la cabecera hasta antes de la
/// siguiente cabecera, sin incluir los comentarios y líneas en blanco que preceden a esa siguiente
/// cabecera (pertenecen a ella).
fn section_range(lines: &[&str], header: &str) -> Option<(usize, usize)> {
    let target = format!("[{header}]");
    let start = lines.iter().position(|l| {
        let t = l.trim();
        t == target || t.starts_with(&format!("{target} ")) || t.starts_with(&format!("{target}#"))
    })?;
    let mut end = lines.len();
    for (i, l) in lines.iter().enumerate().skip(start + 1) {
        if l.trim_start().starts_with('[') {
            end = i;
            break;
        }
    }
    while end > start + 1 {
        let t = lines[end - 1].trim();
        if t.is_empty() || t.starts_with('#') {
            end -= 1;
        } else {
            break;
        }
    }
    Some((start, end))
}

fn join(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    while s.ends_with("\n\n\n") {
        s.pop();
    }
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Sustituye (o añade al final) la sección `[header]` por estas líneas `clave = valor`.
fn set_section(text: &str, header: &str, body: &[String]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut block = vec![format!("[{header}]")];
    block.extend(body.iter().cloned());
    let out: Vec<String> = match section_range(&lines, header) {
        Some((a, b)) => {
            let mut v: Vec<String> = lines[..a].iter().map(|l| l.to_string()).collect();
            v.extend(block);
            v.extend(lines[b..].iter().map(|l| l.to_string()));
            v
        }
        None => {
            let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            if v.last().is_some_and(|l| !l.trim().is_empty()) {
                v.push(String::new());
            }
            v.extend(block);
            v
        }
    };
    join(&out)
}

fn remove_section(text: &str, header: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    match section_range(&lines, header) {
        Some((a, b)) => {
            let mut v: Vec<String> = lines[..a].iter().map(|l| l.to_string()).collect();
            let mut rest = b;
            while rest < lines.len() && lines[rest].trim().is_empty() {
                rest += 1;
            }
            if rest < lines.len() && v.last().is_some_and(|l| !l.trim().is_empty()) {
                v.push(String::new());
            }
            v.extend(lines[rest..].iter().map(|l| l.to_string()));
            join(&v)
        }
        None => text.to_string(),
    }
}

/// Pone (`Some`) o quita (`None`) una clave de una sección, conservando el resto de la sección.
fn set_key(text: &str, header: &str, key: &str, value: Option<String>) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let is_key = |l: &str| {
        let t = l.trim_start();
        t.strip_prefix(key)
            .is_some_and(|r| r.trim_start().starts_with('='))
    };
    let Some((a, b)) = section_range(&lines, header) else {
        return match value {
            Some(v) => set_section(text, header, &[format!("{key} = {v}")]),
            None => text.to_string(),
        };
    };
    let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    match (lines[a + 1..b].iter().position(|l| is_key(l)), value) {
        (Some(i), Some(val)) => v[a + 1 + i] = format!("{key} = {val}"),
        (Some(i), None) => {
            v.remove(a + 1 + i);
        }
        (None, Some(val)) => v.insert(b, format!("{key} = {val}")),
        (None, None) => {}
    }
    join(&v)
}

fn check(text: &str) -> Result<(), ConfigError> {
    toml::from_str::<Config>(text)
        .map(|_| ())
        .map_err(|e| invalid(format!("el resultado no es un pcode.toml válido: {e}")))
}

/// Crea o sustituye `[proveedores.<nombre>]`. La clave se escribe como `{env:VARIABLE}`.
pub fn upsert_provider(text: &str, p: &ProviderInput) -> Result<String, ConfigError> {
    let name = p.name.trim();
    if !valid_name(name) {
        return Err(invalid(
            "nombre de proveedor no válido: usa minúsculas, números, - o _ (p. ej. «ollama»)",
        ));
    }
    if !KINDS.contains(&p.kind.as_str()) {
        return Err(invalid(format!(
            "tipo desconocido «{}»: {}",
            p.kind,
            KINDS.join(", ")
        )));
    }
    let url = p.url.trim();
    if p.kind == "openai-compatible" && url.is_empty() {
        return Err(invalid(
            "un proveedor OpenAI-compatible necesita la URL base (p. ej. http://127.0.0.1:11434/v1)",
        ));
    }
    if !url.is_empty() && !valid_url(url) {
        return Err(invalid(
            "la URL tiene que empezar por http:// o https:// y no llevar espacios",
        ));
    }
    let key_env = p.key_env.trim();
    if !key_env.is_empty() && !valid_env(key_env) {
        return Err(invalid(
            "la variable de la clave tiene que ser como MI_API_KEY (mayúsculas, números y _); \
             la clave en sí no se guarda en archivos",
        ));
    }
    let model = p.model.trim();
    if !valid_model(model) {
        return Err(invalid("nombre de modelo no válido"));
    }
    let mut body = vec![format!("tipo = {}", q(&p.kind))];
    if !url.is_empty() {
        body.push(format!("url = {}", q(url)));
    }
    if !key_env.is_empty() {
        body.push(format!("clave = {}", q(&format!("{{env:{key_env}}}"))));
    }
    if !model.is_empty() {
        body.push(format!("modelo = {}", q(model)));
    }
    let out = set_section(text, &format!("proveedores.{name}"), &body);
    check(&out)?;
    Ok(out)
}

/// Quita `[proveedores.<nombre>]` y, si era el proveedor por defecto, también esa preferencia.
pub fn remove_provider(text: &str, name: &str) -> Result<String, ConfigError> {
    let mut out = remove_section(text, &format!("proveedores.{name}"));
    if raw_preferences(&out).0.as_deref() == Some(name) {
        out = set_key(&out, "preferencias", "proveedor", None);
        out = set_key(&out, "preferencias", "modelo", None);
    }
    check(&out)?;
    Ok(out)
}

/// Fija el proveedor y el modelo por defecto (`None` o vacío quita la preferencia) y, si se da, el
/// límite de contexto.
pub fn set_defaults(
    text: &str,
    provider: Option<&str>,
    model: Option<&str>,
    context_max: Option<u64>,
) -> Result<String, ConfigError> {
    let provider = provider.map(str::trim).filter(|p| !p.is_empty());
    if let Some(p) = provider
        && !valid_name(p)
    {
        return Err(invalid("nombre de proveedor no válido"));
    }
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    if let Some(m) = model
        && !valid_model(m)
    {
        return Err(invalid("nombre de modelo no válido"));
    }
    let mut out = set_key(text, "preferencias", "proveedor", provider.map(q));
    out = set_key(&out, "preferencias", "modelo", model.map(q));
    if let Some(c) = context_max {
        if !(1_000..=10_000_000).contains(&c) {
            return Err(invalid(
                "contexto_max tiene que estar entre 1 000 y 10 000 000",
            ));
        }
        out = set_key(&out, "preferencias", "contexto_max", Some(c.to_string()));
    }
    check(&out)?;
    Ok(out)
}

/// Proveedores escritos en un texto, sin expandir variables (para enseñar de qué variable sale
/// cada clave).
pub fn raw_providers(text: &str) -> Vec<ProviderEntry> {
    let Ok(v) = toml::from_str::<toml::Value>(text) else {
        return vec![];
    };
    let Some(t) = v.get("proveedores").and_then(|p| p.as_table()) else {
        return vec![];
    };
    t.iter()
        .map(|(name, p)| {
            let s = |k: &str| p.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let clave = s("clave");
            let key_env = clave
                .strip_prefix("{env:")
                .and_then(|r| r.strip_suffix('}'))
                .map(String::from);
            ProviderEntry {
                name: name.clone(),
                kind: if s("tipo").is_empty() {
                    "openai-compatible".into()
                } else {
                    s("tipo")
                },
                url: s("url"),
                key_inline: !clave.is_empty() && key_env.is_none(),
                key_env,
                model: Some(s("modelo")).filter(|m| !m.is_empty()),
            }
        })
        .collect()
}

/// `[preferencias]` escritas en un texto: (proveedor, modelo, contexto_max).
pub fn raw_preferences(text: &str) -> (Option<String>, Option<String>, Option<u64>) {
    let Ok(v) = toml::from_str::<toml::Value>(text) else {
        return (None, None, None);
    };
    let p = v.get("preferencias");
    let s = |k: &str| {
        p.and_then(|t| t.get(k))
            .and_then(|x| x.as_str())
            .map(String::from)
    };
    let c = p
        .and_then(|t| t.get("contexto_max"))
        .and_then(|x| x.as_integer())
        .and_then(|i| u64::try_from(i).ok());
    (s("proveedor"), s("modelo"), c)
}

/// Lee un archivo (vacío si no existe), aplica `f` y lo escribe (creando la carpeta).
pub fn update_file(
    path: &Path,
    f: impl FnOnce(&str) -> Result<String, ConfigError>,
) -> Result<(), ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(ConfigError::Io(path.display().to_string(), e)),
    };
    let out = f(&text)?;
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| ConfigError::Io(dir.display().to_string(), e))?;
    }
    std::fs::write(path, out).map_err(|e| ConfigError::Io(path.display().to_string(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLANTILLA: &str = "# Configuración global de PublicCode\n\n[preferencias]\n# proveedor = \"anthropic\"\nmodelo = \"viejo\"\n\n# Un proveedor de ejemplo:\n# [proveedores.anthropic]\n# tipo = \"anthropic\"\n\n[proveedores.anthropic]\ntipo = \"anthropic\" # comentario\nclave = \"{env:ANTHROPIC_API_KEY}\"\n\n# MCP\n[mcp.github]\norden = [\"npx\"]\n";

    fn input(name: &str, kind: &str, url: &str, env: &str, model: &str) -> ProviderInput {
        ProviderInput {
            name: name.into(),
            kind: kind.into(),
            url: url.into(),
            key_env: env.into(),
            model: model.into(),
        }
    }

    #[test]
    fn añadir_proveedor_conserva_comentarios_y_no_guarda_claves() {
        let out = upsert_provider(
            PLANTILLA,
            &input(
                "ollama",
                "openai-compatible",
                "http://127.0.0.1:11434/v1",
                "",
                "llama3.2",
            ),
        )
        .unwrap();
        assert!(out.starts_with("# Configuración global de PublicCode\n"));
        assert!(out.contains("# Un proveedor de ejemplo:\n# [proveedores.anthropic]"));
        assert!(
            out.contains("# MCP\n[mcp.github]"),
            "el comentario sigue con su sección:\n{out}"
        );
        assert!(out.ends_with(
            "[proveedores.ollama]\ntipo = \"openai-compatible\"\nurl = \"http://127.0.0.1:11434/v1\"\nmodelo = \"llama3.2\"\n"
        ), "{out}");
        let c = Config::from_str(&out).unwrap();
        assert_eq!(c.providers["ollama"].model.as_deref(), Some("llama3.2"));
        let out2 = upsert_provider(
            &out,
            &input(
                "litellm",
                "openai-compatible",
                "http://10.0.0.5:4000/v1",
                "LITELLM_API_KEY",
                "",
            ),
        )
        .unwrap();
        assert!(out2.contains("clave = \"{env:LITELLM_API_KEY}\""));
    }

    #[test]
    fn sustituir_un_proveedor_existente() {
        let out = upsert_provider(
            PLANTILLA,
            &input("anthropic", "anthropic", "", "MI_CLAVE", "claude-opus-5-5"),
        )
        .unwrap();
        assert_eq!(
            out.matches("[proveedores.anthropic]").count(),
            2,
            "la comentada y la real"
        );
        assert!(out.contains("[proveedores.anthropic]\ntipo = \"anthropic\"\nclave = \"{env:MI_CLAVE}\"\nmodelo = \"claude-opus-5-5\"\n\n# MCP"), "{out}");
        assert!(!out.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn validaciones() {
        let bad = [
            input("Mal Nombre", "anthropic", "", "", ""),
            input("x", "otro", "", "", ""),
            input("x", "openai-compatible", "", "", ""),
            input("x", "openai-compatible", "ftp://a", "", ""),
            input("x", "openai-compatible", "http://a b", "", ""),
            input("x", "anthropic", "", "sk-ant-abc123", ""),
            input("x", "anthropic", "", "", "a\"b"),
        ];
        for b in bad {
            assert!(upsert_provider("", &b).is_err(), "{b:?}");
        }
        assert!(valid_env("OPENROUTER_API_KEY") && !valid_env("1X") && !valid_env("x"));
    }

    #[test]
    fn preferencias_y_quitar() {
        let out = set_defaults(
            PLANTILLA,
            Some("anthropic"),
            Some("claude-sonnet-5-5"),
            Some(200_000),
        )
        .unwrap();
        assert!(out.contains("[preferencias]\n# proveedor = \"anthropic\"\nmodelo = \"claude-sonnet-5-5\"\nproveedor = \"anthropic\"\ncontexto_max = 200000\n"), "{out}");
        assert_eq!(
            raw_preferences(&out),
            (
                Some("anthropic".into()),
                Some("claude-sonnet-5-5".into()),
                Some(200_000)
            )
        );
        let out = remove_provider(&out, "anthropic").unwrap();
        assert!(!out.contains("tipo = \"anthropic\" # comentario"));
        assert_eq!(
            raw_preferences(&out).0,
            None,
            "si era el predeterminado, se quita"
        );
        assert!(out.contains("# MCP\n[mcp.github]"));
        assert!(set_defaults("", None, None, Some(5)).is_err());
        let vacio = set_defaults("", Some("ollama"), Some("qwen3"), None).unwrap();
        assert_eq!(
            vacio,
            "[preferencias]\nproveedor = \"ollama\"\nmodelo = \"qwen3\"\n"
        );
    }

    #[test]
    fn lista_cruda_con_variables() {
        let t = "[proveedores.a]\ntipo = \"anthropic\"\nclave = \"{env:A_KEY}\"\n[proveedores.b]\nurl = \"http://x/v1\"\nclave = \"sk-123\"\nmodelo = \"m\"\n";
        let v = raw_providers(t);
        assert_eq!(v[0].key_env.as_deref(), Some("A_KEY"));
        assert!(!v[0].key_inline);
        assert_eq!(v[1].kind, "openai-compatible");
        assert!(v[1].key_inline && v[1].key_env.is_none());
        assert_eq!(v[1].model.as_deref(), Some("m"));
        assert!(raw_providers("no es toml [").is_empty());
    }

    #[test]
    fn actualizar_archivo_crea_la_carpeta() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("pcode/pcode.toml");
        update_file(&p, |t| {
            upsert_provider(
                t,
                &input(
                    "lm",
                    "openai-compatible",
                    "http://127.0.0.1:1234/v1",
                    "",
                    "",
                ),
            )
        })
        .unwrap();
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .starts_with("[proveedores.lm]")
        );
        let e = update_file(&p, |t| {
            upsert_provider(t, &input("MAL", "anthropic", "", "", ""))
        });
        assert!(e.is_err());
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("[proveedores.lm]"),
            "no se escribe si falla"
        );
    }
}
