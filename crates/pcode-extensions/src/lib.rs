//! Extensiones de VS Code, solo lo declarativo (ADR-0012): temas, fragmentos, lenguajes y gramáticas.
//! Lo ejecutable (`main`, `browser`) no se carga nunca: no hay anfitrión de extensiones, que es lo que
//! hace pesado a VS Code. Lo equivalente llega por LSP, DAP y MCP. Un `.vsix` es un zip; se abre con
//! `unzip` del sistema (sin dependencias nuevas).
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Errores.
#[derive(Debug, thiserror::Error)]
pub enum ExtError {
    /// E/S.
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    /// `package.json` no válido.
    #[error("package.json no válido: {0}")]
    Manifest(String),
    /// No se pudo descomprimir.
    #[error("no se pudo abrir el .vsix (¿está instalado unzip?): {0}")]
    Unzip(String),
}

/// Un tema de color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Nombre visible.
    pub label: String,
    /// `vs-dark`, `vs`, `hc-black`…
    pub ui_theme: String,
    /// Ruta relativa al JSON del tema.
    pub path: String,
}

/// Un archivo de fragmentos para un lenguaje.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetFile {
    /// Lenguaje (`python`, `rust`…).
    pub language: String,
    /// Ruta relativa.
    pub path: String,
}

/// Un lenguaje declarado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// Identificador.
    pub id: String,
    /// Extensiones (`.py`).
    pub extensions: Vec<String>,
}

/// Lo que se usa de un `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// `editor.nombre`.
    pub id: String,
    /// Versión.
    pub version: String,
    /// Nombre visible.
    pub display_name: String,
    /// Temas.
    pub themes: Vec<Theme>,
    /// Fragmentos.
    pub snippets: Vec<SnippetFile>,
    /// Lenguajes.
    pub languages: Vec<Language>,
    /// Gramáticas TextMate (se guardan para el futuro).
    pub grammars: Vec<String>,
    /// Aportaciones que se ignoran porque necesitan ejecutar código (`commands`, `debuggers`…).
    pub ignored: Vec<String>,
    /// Si trae código ejecutable (`main` o `browser`).
    pub executable: bool,
}

/// Aportaciones que se pueden usar sin ejecutar nada.
pub const DECLARATIVE: &[&str] = &["themes", "snippets", "languages", "grammars"];

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn arr(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

/// Lee un `package.json` de extensión.
pub fn parse_package_json(text: &str) -> Result<Manifest, ExtError> {
    let v: Value = serde_json::from_str(text).map_err(|e| ExtError::Manifest(e.to_string()))?;
    let (publisher, name) = (s(&v["publisher"]), s(&v["name"]));
    if name.is_empty() {
        return Err(ExtError::Manifest("falta `name`".into()));
    }
    let c = &v["contributes"];
    let mut ignored: Vec<String> = c
        .as_object()
        .map(|o| {
            o.keys()
                .filter(|k| !DECLARATIVE.contains(&k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    ignored.sort();
    Ok(Manifest {
        id: if publisher.is_empty() {
            name.clone()
        } else {
            format!("{publisher}.{name}")
        },
        version: s(&v["version"]),
        display_name: v["displayName"].as_str().map(String::from).unwrap_or(name),
        themes: arr(&c["themes"])
            .map(|t| Theme {
                label: s(&t["label"]),
                ui_theme: s(&t["uiTheme"]),
                path: s(&t["path"]),
            })
            .collect(),
        snippets: arr(&c["snippets"])
            .map(|t| SnippetFile {
                language: s(&t["language"]),
                path: s(&t["path"]),
            })
            .collect(),
        languages: arr(&c["languages"])
            .map(|l| Language {
                id: s(&l["id"]),
                extensions: arr(&l["extensions"]).map(s).collect(),
            })
            .collect(),
        grammars: arr(&c["grammars"]).map(|g| s(&g["path"])).collect(),
        ignored,
        executable: v.get("main").is_some() || v.get("browser").is_some(),
    })
}

/// Avisos para el usuario sobre lo que no se usa.
pub fn warnings(m: &Manifest) -> Vec<String> {
    let mut w = vec![];
    if m.executable {
        w.push(
            "trae código ejecutable: no se carga (PublicCode no tiene anfitrión de extensiones)"
                .into(),
        );
    }
    if !m.ignored.is_empty() {
        w.push(format!("se ignora: {}", m.ignored.join(", ")));
    }
    if m.themes.is_empty()
        && m.snippets.is_empty()
        && m.languages.is_empty()
        && m.grammars.is_empty()
    {
        w.push("no aporta nada declarativo que PublicCode pueda usar".into());
    }
    w
}

/// Quita comentarios `//` y `/* */` de un JSONC (respetando las cadenas) y comas finales.
pub fn strip_jsonc(text: &str) -> String {
    let b: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_str) = (0, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 1;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let next = b[i + 1..].iter().find(|x| !x.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Un fragmento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// Prefijo que lo activa.
    pub prefix: String,
    /// Cuerpo con marcadores (`$1`, `${1:x}`, `$0`).
    pub body: String,
    /// Descripción.
    pub description: String,
}

/// Lee un archivo de fragmentos de VS Code (`prefix` y `body` pueden ser cadena o lista).
pub fn parse_snippets(text: &str) -> Vec<Snippet> {
    let Ok(v) = serde_json::from_str::<Value>(&strip_jsonc(text)) else {
        return vec![];
    };
    let mut out = vec![];
    for (name, sn) in v.as_object().into_iter().flatten() {
        let body = match &sn["body"] {
            Value::Array(a) => a.iter().map(s).collect::<Vec<_>>().join("\n"),
            b => s(b),
        };
        let prefixes: Vec<String> = match &sn["prefix"] {
            Value::Array(a) => a.iter().map(s).collect(),
            p => vec![s(p)],
        };
        for prefix in prefixes.into_iter().filter(|p| !p.is_empty()) {
            out.push(Snippet {
                prefix,
                body: body.clone(),
                description: sn["description"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| name.clone()),
            });
        }
    }
    out.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    out
}

/// Carpeta de extensiones: `$XDG_DATA_HOME/pcode/extensiones` (o `~/.local/share/…`).
pub fn default_root() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    Some(data.join("pcode").join("extensiones"))
}

fn io<T>(what: &Path, r: std::io::Result<T>) -> Result<T, ExtError> {
    r.map_err(|e| ExtError::Io(what.display().to_string(), e))
}

/// Copia solo lo declarativo de una extensión ya descomprimida (`src` contiene `package.json`).
fn copy_declarative(src: &Path, m: &Manifest, dest: &Path) -> Result<(), ExtError> {
    io(dest, std::fs::create_dir_all(dest))?;
    io(
        dest,
        std::fs::copy(src.join("package.json"), dest.join("package.json")).map(|_| ()),
    )?;
    let files = m
        .themes
        .iter()
        .map(|t| &t.path)
        .chain(m.snippets.iter().map(|s| &s.path))
        .chain(m.grammars.iter());
    for rel in files {
        let rel = rel.trim_start_matches("./");
        if rel.is_empty() || rel.contains("..") {
            continue;
        }
        let (from, to) = (src.join(rel), dest.join(rel));
        if from.is_file() {
            if let Some(d) = to.parent() {
                io(d, std::fs::create_dir_all(d))?;
            }
            io(&from, std::fs::copy(&from, &to).map(|_| ()))?;
        }
    }
    Ok(())
}

/// Instala desde un `.vsix` o una carpeta de extensión (p. ej. de `~/.vscode/extensions`).
/// Devuelve el manifiesto y los avisos.
pub fn install(source: &Path, root: &Path) -> Result<(Manifest, Vec<String>), ExtError> {
    let tmp;
    let dir = if source.is_dir() {
        source.to_path_buf()
    } else {
        tmp = root.join(".descomprimiendo");
        let _ = std::fs::remove_dir_all(&tmp);
        io(&tmp, std::fs::create_dir_all(&tmp))?;
        let out = std::process::Command::new("unzip")
            .args(["-q", "-o"])
            .arg(source)
            .arg("extension/*")
            .arg("-d")
            .arg(&tmp)
            .output()
            .map_err(|e| ExtError::Unzip(e.to_string()))?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(ExtError::Unzip(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        tmp.join("extension")
    };
    let pj = dir.join("package.json");
    let m = parse_package_json(&io(&pj, std::fs::read_to_string(&pj))?)?;
    let dest = root.join(&m.id);
    let _ = std::fs::remove_dir_all(&dest);
    let r = copy_declarative(&dir, &m, &dest);
    let _ = std::fs::remove_dir_all(root.join(".descomprimiendo"));
    r?;
    let w = warnings(&m);
    Ok((m, w))
}

/// Extensiones instaladas, por id.
pub fn list(root: &Path) -> Vec<Manifest> {
    let mut v: Vec<Manifest> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| std::fs::read_to_string(e.path().join("package.json")).ok())
        .filter_map(|t| parse_package_json(&t).ok())
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// Quita una extensión; `false` si no estaba.
pub fn uninstall(root: &Path, id: &str) -> Result<bool, ExtError> {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return Ok(false);
    }
    let d = root.join(id);
    if !d.is_dir() {
        return Ok(false);
    }
    io(&d, std::fs::remove_dir_all(&d))?;
    Ok(true)
}

/// Todos los fragmentos instalados para un lenguaje.
pub fn snippets_for(root: &Path, language: &str) -> Vec<Snippet> {
    let mut out = vec![];
    for m in list(root) {
        for f in m.snippets.iter().filter(|f| f.language == language) {
            let p = root.join(&m.id).join(f.path.trim_start_matches("./"));
            if let Ok(t) = std::fs::read_to_string(p) {
                out.extend(parse_snippets(&t));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PJ: &str = r#"{
      "name": "temas", "publisher": "ana", "version": "1.0.0", "displayName": "Temas de Ana",
      "main": "./out/extension.js",
      "contributes": {
        "themes": [{"label": "Noche", "uiTheme": "vs-dark", "path": "./themes/noche.json"}],
        "snippets": [{"language": "python", "path": "./snippets/py.json"}],
        "languages": [{"id": "python", "extensions": [".py", ".pyi"]}],
        "commands": [{"command": "x"}], "debuggers": []
      }
    }"#;

    #[test]
    fn lee_contributes_de_un_package_json() {
        let m = parse_package_json(PJ).unwrap();
        assert_eq!(m.id, "ana.temas");
        assert_eq!(m.display_name, "Temas de Ana");
        assert_eq!(m.themes[0].ui_theme, "vs-dark");
        assert_eq!(m.snippets[0].language, "python");
        assert_eq!(m.languages[0].extensions, [".py", ".pyi"]);
        assert!(parse_package_json("{}").is_err());
    }

    #[test]
    fn ignora_lo_ejecutable_y_lo_dice() {
        let m = parse_package_json(PJ).unwrap();
        assert!(m.executable);
        assert_eq!(m.ignored, ["commands", "debuggers"]);
        let w = warnings(&m);
        assert!(w[0].contains("no se carga"));
        assert!(w[1].contains("commands, debuggers"));
    }

    #[test]
    fn fragmentos_con_comentarios_y_listas() {
        let t = r#"{
          // comentario
          "Imprimir": {"prefix": ["pr", "print"], "body": ["print(\"$1\")", "$0"], "description": "imprime"},
          "Def": {"prefix": "def", "body": "def ${1:f}():", },
        }"#;
        let v = parse_snippets(t);
        assert_eq!(
            v.iter().map(|s| s.prefix.as_str()).collect::<Vec<_>>(),
            ["def", "pr", "print"]
        );
        assert_eq!(v[1].body, "print(\"$1\")\n$0");
        assert_eq!(v[0].description, "Def");
        assert_eq!(
            strip_jsonc(r#"{"a": "http://x"} // c"#).trim(),
            r#"{"a": "http://x"}"#
        );
    }

    #[test]
    fn instalar_y_quitar() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src/extension");
        std::fs::create_dir_all(src.join("themes")).unwrap();
        std::fs::create_dir_all(src.join("snippets")).unwrap();
        std::fs::create_dir_all(src.join("out")).unwrap();
        std::fs::write(src.join("package.json"), PJ).unwrap();
        std::fs::write(src.join("themes/noche.json"), "{}").unwrap();
        std::fs::write(
            src.join("snippets/py.json"),
            r#"{"P": {"prefix": "pr", "body": "print($1)"}}"#,
        )
        .unwrap();
        std::fs::write(src.join("out/extension.js"), "x").unwrap();
        let root = d.path().join("ext");
        // Desde carpeta.
        let (m, w) = install(&src, &root).unwrap();
        assert_eq!(m.id, "ana.temas");
        assert!(!w.is_empty());
        assert!(root.join("ana.temas/themes/noche.json").is_file());
        assert!(
            !root.join("ana.temas/out/extension.js").exists(),
            "lo ejecutable no se copia"
        );
        assert_eq!(snippets_for(&root, "python")[0].prefix, "pr");
        assert_eq!(list(&root).len(), 1);
        assert!(uninstall(&root, "ana.temas").unwrap());
        assert!(!uninstall(&root, "ana.temas").unwrap());
        assert!(!uninstall(&root, "../x").unwrap());
        // Desde .vsix (si hay zip y unzip).
        let zip_ok = std::process::Command::new("zip").arg("-v").output().is_ok();
        if zip_ok {
            let vsix = d.path().join("t.vsix");
            let st = std::process::Command::new("zip")
                .args(["-qr", vsix.to_str().unwrap(), "extension"])
                .current_dir(d.path().join("src"))
                .status()
                .unwrap();
            assert!(st.success());
            let (m, _) = install(&vsix, &root).unwrap();
            assert_eq!(m.id, "ana.temas");
            assert!(root.join("ana.temas/snippets/py.json").is_file());
            assert!(!root.join(".descomprimiendo").exists());
        }
    }
}
