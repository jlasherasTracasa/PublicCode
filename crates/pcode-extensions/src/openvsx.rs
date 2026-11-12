//! Registro Open VSX: solo las URL y la lectura de la respuesta (la descarga la hace la CLI).
use crate::ExtError;

/// Registro por defecto.
pub const OPEN_VSX: &str = "https://open-vsx.org";

/// Parte `editor.nombre` en (espacio de nombres, nombre); `None` si no tiene esa forma.
pub fn parse_id(id: &str) -> Option<(String, String)> {
    let (ns, name) = id.split_once('.')?;
    if valido(ns, false) && valido(name, true) {
        Some((ns.to_string(), name.to_string()))
    } else {
        None
    }
}

/// Un segmento válido: entre 1 y 100 caracteres ASCII (letras, cifras, `-`, `_` y, si `punto`, `.`).
fn valido(p: &str, punto: bool) -> bool {
    (1..=100).contains(&p.chars().count())
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || (punto && c == '.'))
}

/// URL de los metadatos de la última versión.
pub fn api_url(base: &str, namespace: &str, name: &str) -> String {
    format!("{}/api/{namespace}/{name}", base.trim_end_matches('/'))
}

/// (versión, URL del `.vsix`) a partir del JSON de `api_url`.
pub fn download_of(body: &str, base: &str) -> Result<(String, String), ExtError> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| ExtError::Remote("respuesta no válida".to_string()))?;
    if let Some(error) = v["error"].as_str() {
        return Err(ExtError::Remote(error.to_string()));
    }
    let url = v["files"]["download"]
        .as_str()
        .ok_or_else(|| ExtError::Remote("la extensión no tiene archivo .vsix".to_string()))?;
    let prefijo = format!("{}/", base.trim_end_matches('/'));
    if !url.starts_with(&prefijo) {
        return Err(ExtError::Remote(format!(
            "descarga fuera del registro: {url}"
        )));
    }
    let version = v["version"].as_str().unwrap_or("").to_string();
    Ok((version, url.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCARGA: &str = r#"{"version":"1.2.0","files":{"download":"https://open-vsx.org/api/redhat/java/1.2.0/file/redhat.java-1.2.0.vsix"}}"#;

    #[test]
    fn id_con_editor_y_nombre() {
        assert_eq!(
            parse_id("redhat.java"),
            Some(("redhat".to_string(), "java".to_string()))
        );
        assert_eq!(parse_id("sin-punto"), None);
        assert_eq!(parse_id(""), None);
        assert_eq!(parse_id("con/barra.nombre"), None);
        assert_eq!(parse_id("con espacio.nombre"), None);
    }

    #[test]
    fn url_de_metadatos() {
        assert_eq!(
            api_url("https://open-vsx.org/", "redhat", "java"),
            "https://open-vsx.org/api/redhat/java"
        );
    }

    #[test]
    fn lee_version_y_descarga() {
        let (version, url) = download_of(DESCARGA, "https://open-vsx.org").unwrap();
        assert_eq!(version, "1.2.0");
        assert_eq!(
            url,
            "https://open-vsx.org/api/redhat/java/1.2.0/file/redhat.java-1.2.0.vsix"
        );
    }

    #[test]
    fn error_del_registro() {
        let r = download_of(
            r#"{"error":"Extension not found: a.b"}"#,
            "https://open-vsx.org",
        );
        assert!(matches!(
            r,
            Err(ExtError::Remote(ref m)) if m == "Extension not found: a.b"
        ));
    }

    #[test]
    fn descarga_de_otro_sitio_se_rechaza() {
        let body = r#"{"version":"1.0.0","files":{"download":"https://otro.example/x.vsix"}}"#;
        let r = download_of(body, "https://open-vsx.org");
        assert!(matches!(
            r,
            Err(ExtError::Remote(ref m)) if m.contains("fuera del registro")
        ));
    }

    #[test]
    fn json_roto() {
        let r = download_of("no es json", "https://open-vsx.org");
        assert!(matches!(
            r,
            Err(ExtError::Remote(ref m)) if m == "respuesta no válida"
        ));
    }
}
