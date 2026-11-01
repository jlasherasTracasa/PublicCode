//! Esquema de un archivo y símbolos del proyecto (F8.39), sin servidor de lenguaje: expresiones
//! regulares por lenguaje y profundidad por sangría. Es lo que usan el panel Esquema, las migas de
//! pan y «ir a símbolo» de Studio, el contexto de las acciones del asistente y la tool `esquema`
//! del agente (F8.45).
//!
//! Inspirado en RepoNav (2026.emnlp-main.1474): una vista centrada en archivos con los símbolos
//! hermanos a la vista ayuda a elegir el sitio correcto; y en SHERLOC (2026.emnlp-main.711): los
//! agentes gastan la mitad del presupuesto en localizar antes de editar.
use crate::registry::*;
use pcode_models::ToolSpec;
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::OnceLock;

/// Un símbolo de un archivo.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Symbol {
    /// Nombre.
    pub name: String,
    /// Tipo: `fn`, `method`, `struct`, `enum`, `trait`, `impl`, `mod`, `class`, `interface`, `type`,
    /// `const`, `macro`, `h1`…`h6`.
    pub kind: String,
    /// Línea (desde 1).
    pub line: u32,
    /// Profundidad de anidamiento (0 = nivel superior).
    pub depth: u32,
}

/// Un símbolo encontrado en el proyecto.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SymbolHit {
    /// Archivo, relativo a la raíz.
    pub path: String,
    /// Nombre.
    pub name: String,
    /// Tipo.
    pub kind: String,
    /// Línea (desde 1).
    pub line: u32,
}

/// Lenguaje para el esquema según la extensión (`""` si no se conoce).
pub fn language_of(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts" => "js",
        "go" => "go",
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" => "c",
        "java" | "kt" | "kts" | "cs" | "scala" => "java",
        "md" | "markdown" => "markdown",
        _ => "",
    }
}

struct Rule {
    re: Regex,
    kind: &'static str,
}

fn rules(lang: &str) -> &'static [Rule] {
    static R: OnceLock<Vec<(&'static str, Vec<Rule>)>> = OnceLock::new();
    let all = R.get_or_init(|| {
        let r = |p: &str, kind: &'static str| Rule {
            re: Regex::new(p).expect("regex del esquema"),
            kind,
        };
        let vis = r"(?:pub(?:\([^)]*\))?\s+)?";
        vec![
            (
                "rust",
                vec![
                    r(
                        &format!(
                            r#"^\s*{vis}(?:default\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z_]\w*)"#
                        ),
                        "fn",
                    ),
                    r(&format!(r"^\s*{vis}struct\s+([A-Za-z_]\w*)"), "struct"),
                    r(&format!(r"^\s*{vis}enum\s+([A-Za-z_]\w*)"), "enum"),
                    r(&format!(r"^\s*{vis}union\s+([A-Za-z_]\w*)"), "struct"),
                    r(
                        &format!(r"^\s*{vis}(?:unsafe\s+)?trait\s+([A-Za-z_]\w*)"),
                        "trait",
                    ),
                    r(&format!(r"^\s*{vis}type\s+([A-Za-z_]\w*)"), "type"),
                    r(&format!(r"^\s*{vis}mod\s+([A-Za-z_]\w*)"), "mod"),
                    r(
                        r"^\s*(?:unsafe\s+)?impl(?:\s*<[^{]*?>)?\s+([^{]+?)\s*(?:where\b[^{]*)?\{?\s*$",
                        "impl",
                    ),
                    r(r"^\s*macro_rules!\s*([A-Za-z_]\w*)", "macro"),
                    r(
                        &format!(r"^\s*{vis}(?:const|static)\s+(?:mut\s+)?([A-Z_][A-Z0-9_]*)\s*:"),
                        "const",
                    ),
                ],
            ),
            (
                "python",
                vec![
                    r(r"^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)", "fn"),
                    r(r"^\s*class\s+([A-Za-z_]\w*)", "class"),
                ],
            ),
            (
                "js",
                vec![
                    r(
                        r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)",
                        "fn",
                    ),
                    r(
                        r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)",
                        "class",
                    ),
                    r(
                        r"^\s*(?:export\s+)?(?:declare\s+)?interface\s+([A-Za-z_$][\w$]*)",
                        "interface",
                    ),
                    r(
                        r"^\s*(?:export\s+)?(?:declare\s+)?type\s+([A-Za-z_$][\w$]*)\s*(?:<[^=]*>)?\s*=",
                        "type",
                    ),
                    r(
                        r"^\s*(?:export\s+)?(?:const\s+|declare\s+)?enum\s+([A-Za-z_$][\w$]*)",
                        "enum",
                    ),
                    r(
                        r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:function\b|(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>)",
                        "fn",
                    ),
                    r(
                        r"^\s+(?:(?:public|private|protected|static|readonly|override|async|get|set)\s+)*([A-Za-z_$][\w$]*)\s*(?:<[^>]*>)?\([^)]*\)\s*(?::[^{]+)?\{\s*$",
                        "method",
                    ),
                ],
            ),
            (
                "go",
                vec![
                    r(r"^func\s+\([^)]*\)\s*([A-Za-z_]\w*)", "method"),
                    r(r"^func\s+([A-Za-z_]\w*)", "fn"),
                    r(r"^type\s+([A-Za-z_]\w*)\s+struct\b", "struct"),
                    r(r"^type\s+([A-Za-z_]\w*)\s+interface\b", "interface"),
                    r(r"^type\s+([A-Za-z_]\w*)\s", "type"),
                ],
            ),
            (
                "c",
                vec![
                    r(r"^\s*(?:typedef\s+)?struct\s+([A-Za-z_]\w*)\s*\{?\s*$", "struct"),
                    r(r"^\s*(?:typedef\s+)?enum\s+(?:class\s+)?([A-Za-z_]\w*)", "enum"),
                    r(r"^\s*(?:template\s*<[^>]*>\s*)?class\s+([A-Za-z_]\w*)\s*(?:[:{].*)?$", "class"),
                    r(r"^\s*namespace\s+([A-Za-z_]\w*)", "mod"),
                    r(
                        r"^(?:static\s+|inline\s+|extern\s+|virtual\s+|const\s+)*[A-Za-z_][\w:<>,\*& ]*?[\s\*&]([A-Za-z_]\w*(?:::~?[A-Za-z_]\w*)?)\s*\([^;]*\)\s*(?:const\s*)?\{?\s*$",
                        "fn",
                    ),
                ],
            ),
            (
                "java",
                vec![
                    r(
                        r"^\s*(?:(?:public|private|protected|internal|static|final|abstract|sealed|data|open|partial)\s+)*(?:class|object)\s+([A-Za-z_]\w*)",
                        "class",
                    ),
                    r(
                        r"^\s*(?:(?:public|private|protected|internal|static|sealed)\s+)*(?:interface|record)\s+([A-Za-z_]\w*)",
                        "interface",
                    ),
                    r(
                        r"^\s*(?:(?:public|private|protected|internal|static)\s+)*enum\s+(?:class\s+)?([A-Za-z_]\w*)",
                        "enum",
                    ),
                    r(
                        r"^\s*(?:(?:public|private|protected|internal|override|suspend|inline|open|private)\s+)*fun\s+(?:<[^>]*>\s*)?(?:[\w.]+\.)?([A-Za-z_]\w*)\s*\(",
                        "fn",
                    ),
                    r(
                        r"^\s+(?:(?:public|private|protected|static|final|abstract|synchronized|override|async|virtual)\s+)+[\w<>\[\],.? ]*?\s([A-Za-z_]\w*)\s*\([^;]*$",
                        "method",
                    ),
                ],
            ),
        ]
    });
    all.iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, r)| r.as_slice())
        .unwrap_or(&[])
}

const NOT_NAMES: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "return",
    "else",
    "do",
    "try",
    "with",
    "new",
    "sizeof",
    "function",
    "constructor_",
    "match",
    "loop",
];

fn indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn markdown(text: &str) -> Vec<Symbol> {
    let mut out = vec![];
    let mut fence = false;
    for (i, l) in text.lines().enumerate() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let level = l.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && l[level..].starts_with(' ') {
            let name = l[level..].trim().trim_end_matches('#').trim();
            if !name.is_empty() {
                out.push(Symbol {
                    name: name.to_string(),
                    kind: format!("h{level}"),
                    line: i as u32 + 1,
                    depth: level as u32 - 1,
                });
            }
        }
    }
    out
}

/// Símbolos de `text` en el lenguaje `lang` (ver [`language_of`]), en orden de aparición.
pub fn outline(text: &str, lang: &str) -> Vec<Symbol> {
    if lang == "markdown" {
        return markdown(text);
    }
    let rules = rules(lang);
    if rules.is_empty() {
        return vec![];
    }
    let mut out: Vec<Symbol> = vec![];
    // Sangrías de los símbolos abiertos, para la profundidad.
    let mut stack: Vec<usize> = vec![];
    let mut in_block_comment = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if in_block_comment {
            if t.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if t.starts_with("/*") && !t.contains("*/") {
            in_block_comment = true;
            continue;
        }
        if t.is_empty()
            || t.starts_with("//")
            || t.starts_with('*')
            || (t.starts_with('#') && lang != "c")
            || line.len() > 400
        {
            continue;
        }
        let Some((name, kind)) = rules.iter().find_map(|r| {
            let c = r.re.captures(line)?;
            let n = c.get(1)?.as_str().trim();
            (!n.is_empty() && !NOT_NAMES.contains(&n)).then(|| (n.to_string(), r.kind))
        }) else {
            continue;
        };
        let ind = indent(line);
        while stack.last().is_some_and(|&s| s >= ind) {
            stack.pop();
        }
        let depth = stack.len() as u32;
        stack.push(ind);
        let kind = if kind == "fn" && depth > 0 && matches!(lang, "python" | "rust") {
            "method"
        } else {
            kind
        };
        let name = if kind == "impl" {
            name.split_whitespace().collect::<Vec<_>>().join(" ")
        } else {
            name
        };
        out.push(Symbol {
            name,
            kind: kind.to_string(),
            line: i as u32 + 1,
            depth,
        });
    }
    out
}

/// Busca símbolos cuyo nombre contenga `query` (sin distinguir mayúsculas) en los archivos del
/// proyecto. Primero los que empiezan por la consulta y los más cortos; como mucho `limit`.
pub fn workspace_symbols(
    root: &Path,
    files: &[String],
    query: &str,
    limit: usize,
) -> Vec<SymbolHit> {
    let q = query.trim().to_lowercase();
    let mut hits = vec![];
    for f in files.iter().take(20_000) {
        let lang = language_of(f);
        if lang.is_empty() || lang == "markdown" {
            continue;
        }
        let p = root.join(f);
        if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX) > 512 * 1024 {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        for s in outline(&text, lang) {
            if s.kind == "impl" {
                continue;
            }
            if q.is_empty() || s.name.to_lowercase().contains(&q) {
                hits.push(SymbolHit {
                    path: f.clone(),
                    name: s.name,
                    kind: s.kind,
                    line: s.line,
                });
            }
        }
        if hits.len() > limit * 20 {
            break;
        }
    }
    hits.sort_by_key(|h| {
        let n = h.name.to_lowercase();
        (
            n != q,
            !n.starts_with(&q),
            h.name.len(),
            h.path.len(),
            h.line,
        )
    });
    hits.truncate(limit);
    hits
}

/// Archivos del proyecto: con git, los versionados y los nuevos no ignorados; si no, un recorrido
/// que salta `.git`, `target`, `node_modules` y `.pcode`. Como mucho 20 000.
pub fn project_files(root: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output();
    if let Ok(o) = out
        && o.status.success()
    {
        return String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .take(20_000)
            .map(String::from)
            .collect();
    }
    let mut v = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let p = e.path();
            if p.is_dir() {
                if ![".git", "target", "node_modules", ".pcode"].contains(&name.as_str()) {
                    stack.push(p);
                }
            } else if let Ok(r) = p.strip_prefix(root) {
                v.push(r.to_string_lossy().replace('\\', "/"));
                if v.len() >= 20_000 {
                    return v;
                }
            }
        }
    }
    v.sort();
    v
}

fn render_outline(symbols: &[Symbol]) -> String {
    symbols
        .iter()
        .map(|s| {
            format!(
                "{:>5}  {}{} {}",
                s.line,
                "  ".repeat(s.depth as usize),
                s.kind,
                s.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tool `esquema`: la estructura del código sin leer los archivos enteros.
///
/// - `{"path": "src/lib.rs"}`: símbolos del archivo con su línea y anidamiento.
/// - `{"path": "src"}`: vista centrada en archivos de una carpeta, con los símbolos de primer nivel.
/// - `{"query": "parse"}`: dónde se definen los símbolos que contienen `parse` en todo el proyecto.
pub struct Outline;

#[async_trait::async_trait]
impl Tool for Outline {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "esquema".into(),
            description: "Estructura del código sin leer archivos enteros: `path` de un archivo (sus funciones, tipos y clases con línea), `path` de una carpeta (cada archivo con sus símbolos principales) o `query` para encontrar dónde se define un símbolo en el proyecto. Úsala para localizar antes de leer o editar.".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"query":{"type":"string"}}}),
        }
    }
    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }
    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let root = ctx.root.clone();
        let path = args.get("path").and_then(Value::as_str).map(String::from);
        let query = args.get("query").and_then(Value::as_str).map(String::from);
        let r = tokio::task::spawn_blocking(move || outline_tool(&root, path, query)).await;
        match r {
            Ok(Ok(s)) => ToolOutput::ok(s),
            Ok(Err(e)) => ToolOutput::error(e),
            Err(e) => ToolOutput::error(e.to_string()),
        }
    }
}

fn outline_tool(
    root: &Path,
    path: Option<String>,
    query: Option<String>,
) -> Result<String, String> {
    if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
        let files = project_files(root);
        let hits = workspace_symbols(root, &files, &q, 40);
        if hits.is_empty() {
            return Ok(format!("ningún símbolo contiene «{q}»"));
        }
        return Ok(hits
            .iter()
            .map(|h| format!("{}:{}  {} {}", h.path, h.line, h.kind, h.name))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    let Some(p) = path.filter(|p| !p.trim().is_empty()) else {
        return Err("falta `path` o `query`".into());
    };
    let full = resolve_in_root(root, &p)?;
    if full.is_dir() {
        let rel = full
            .strip_prefix(root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let prefix = if rel.is_empty() {
            String::new()
        } else {
            format!("{rel}/")
        };
        let mut out = vec![];
        let files: Vec<String> = project_files(root)
            .into_iter()
            .filter(|f| f.starts_with(&prefix))
            .collect();
        for f in files.iter().take(60) {
            let lang = language_of(f);
            let tops: Vec<String> = if lang.is_empty() {
                vec![]
            } else {
                std::fs::read_to_string(root.join(f))
                    .map(|t| {
                        outline(&t, lang)
                            .into_iter()
                            .filter(|s| s.depth == 0)
                            .take(12)
                            .map(|s| format!("{} {}", s.kind, s.name))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            out.push(if tops.is_empty() {
                f.clone()
            } else {
                format!("{f}: {}", tops.join(", "))
            });
        }
        if files.len() > 60 {
            out.push(format!("… y {} archivos más", files.len() - 60));
        }
        if out.is_empty() {
            return Ok(format!("{p}: carpeta vacía o ignorada"));
        }
        return Ok(out.join("\n"));
    }
    let text = std::fs::read_to_string(&full).map_err(|e| format!("no se pudo leer {p}: {e}"))?;
    let lang = language_of(&p);
    if lang.is_empty() {
        return Ok(format!(
            "{p}: lenguaje sin esquema ({} líneas); usa `leer`",
            text.lines().count()
        ));
    }
    let syms = outline(&text, lang);
    if syms.is_empty() {
        return Ok(format!(
            "{p}: sin símbolos ({} líneas)",
            text.lines().count()
        ));
    }
    Ok(format!(
        "{p} ({} líneas)\n{}",
        text.lines().count(),
        render_outline(&syms)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[Symbol]) -> Vec<String> {
        v.iter()
            .map(|s| format!("{}:{}:{}:{}", s.depth, s.kind, s.name, s.line))
            .collect()
    }

    #[test]
    fn esquema_rust() {
        let src = "//! doc\nuse std::fmt;\n\n/// Un punto.\npub struct Punto { x: i32 }\n\nimpl fmt::Display for Punto {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        if true { }\n        Ok(())\n    }\n}\n\npub(crate) async fn principal() {}\nconst MAXIMO: u32 = 3;\npub enum Color { Rojo }\nmacro_rules! hola { () => {} }\nmod tests {\n    #[test]\n    fn prueba() {}\n}\n";
        assert_eq!(
            names(&outline(src, "rust")),
            vec![
                "0:struct:Punto:5",
                "0:impl:fmt::Display for Punto:7",
                "1:method:fmt:8",
                "0:fn:principal:14",
                "0:const:MAXIMO:15",
                "0:enum:Color:16",
                "0:macro:hola:17",
                "0:mod:tests:18",
                "1:method:prueba:20",
            ]
        );
    }

    #[test]
    fn esquema_python_anidado() {
        let src = "import os\n\nclass Cola:\n    \"\"\"Una cola.\"\"\"\n    def __init__(self):\n        self.x = []\n\n    async def saca(self):\n        def interna():\n            pass\n\ndef main():\n    # def comentada():\n    pass\n";
        assert_eq!(
            names(&outline(src, "python")),
            vec![
                "0:class:Cola:3",
                "1:method:__init__:5",
                "1:method:saca:8",
                "2:method:interna:9",
                "0:fn:main:12",
            ]
        );
    }

    #[test]
    fn esquema_js_y_ts() {
        let src = "export interface Opciones { a: number }\nexport type Id = string;\nexport async function carga(url) {\n  if (x) {\n  }\n}\nconst suma = (a, b) => a + b;\nexport default class Panel extends Base {\n  constructor(p) {\n    super(p);\n  }\n  async pinta(): Promise<void> {\n    for (const x of y) {\n    }\n  }\n}\n";
        assert_eq!(
            names(&outline(src, "js")),
            vec![
                "0:interface:Opciones:1",
                "0:type:Id:2",
                "0:fn:carga:3",
                "0:fn:suma:7",
                "0:class:Panel:8",
                "1:method:constructor:9",
                "1:method:pinta:12",
            ]
        );
    }

    #[test]
    fn esquema_go_c_y_java() {
        let go = "package main\n\ntype Servidor struct {\n}\n\nfunc (s *Servidor) Arranca() error {\n}\n\nfunc main() {\n}\n";
        assert_eq!(
            names(&outline(go, "go")),
            vec!["0:struct:Servidor:3", "0:method:Arranca:6", "0:fn:main:9"]
        );
        let c = "#include <stdio.h>\nstruct nodo {\n  int v;\n};\nstatic int suma(int a, int b) {\n  return a + b;\n}\nint main(void)\n{\n  if (x) {\n  }\n}\n";
        assert_eq!(
            names(&outline(c, "c")),
            vec!["0:struct:nodo:2", "0:fn:suma:5", "0:fn:main:8"]
        );
        let java = "public class Banco {\n    private int saldo;\n    public void ingresa(int x) {\n        if (x > 0) {\n        }\n    }\n}\n";
        assert_eq!(
            names(&outline(java, "java")),
            vec!["0:class:Banco:1", "1:method:ingresa:3"]
        );
    }

    #[test]
    fn esquema_markdown() {
        let md =
            "# Título\n\ntexto\n\n## Instalar\n\n```sh\n# no es un título\n```\n\n### Linux ###\n";
        assert_eq!(
            names(&outline(md, "markdown")),
            vec!["0:h1:Título:1", "1:h2:Instalar:5", "2:h3:Linux:11"]
        );
        assert!(outline("x", "desconocido").is_empty());
        assert_eq!(language_of("src/main.RS"), "rust");
        assert_eq!(language_of("a.tsx"), "js");
    }

    #[test]
    fn simbolos_del_proyecto_filtra_y_limita() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(
            d.path().join("src/a.rs"),
            "fn carga_config() {}\nfn carga() {}\nstruct Cargador;\nfn otra() {}\n",
        )
        .unwrap();
        std::fs::write(d.path().join("b.py"), "def carga_datos():\n    pass\n").unwrap();
        std::fs::write(d.path().join("notas.md"), "# carga\n").unwrap();
        let files = vec![
            "src/a.rs".to_string(),
            "b.py".to_string(),
            "notas.md".to_string(),
        ];
        let h = workspace_symbols(d.path(), &files, "CARGA", 10);
        let n: Vec<_> = h.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(n, vec!["carga", "Cargador", "carga_datos", "carga_config"]);
        assert_eq!(h[0].path, "src/a.rs");
        assert_eq!(h[0].line, 2);
        assert_eq!(workspace_symbols(d.path(), &files, "carga", 2).len(), 2);
    }

    #[tokio::test]
    async fn tool_esquema_archivo_carpeta_y_consulta() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(
            d.path().join("src/lib.rs"),
            "pub struct Caja;\nimpl Caja {\n    pub fn abre(&self) {}\n}\npub fn parse() {}\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("src/util.py"),
            "def parse_args():\n    pass\n",
        )
        .unwrap();
        std::fs::write(d.path().join("notas.txt"), "x\n").unwrap();
        let ctx = ToolContext {
            root: d.path().to_path_buf(),
            timeout: std::time::Duration::from_secs(5),
        };
        let o = Outline.call(json!({"path": "src/lib.rs"}), &ctx).await;
        assert!(!o.is_error, "{}", o.content);
        assert!(o.content.contains("    1  struct Caja"), "{}", o.content);
        assert!(o.content.contains("    3    method abre"), "{}", o.content);
        let o = Outline.call(json!({"path": "src"}), &ctx).await;
        assert!(
            o.content
                .contains("src/lib.rs: struct Caja, impl Caja, fn parse"),
            "{}",
            o.content
        );
        assert!(
            o.content.contains("src/util.py: fn parse_args"),
            "{}",
            o.content
        );
        let o = Outline.call(json!({"query": "parse"}), &ctx).await;
        let lines: Vec<&str> = o.content.lines().collect();
        assert_eq!(lines[0], "src/lib.rs:5  fn parse", "{}", o.content);
        assert!(o.content.contains("src/util.py:1  fn parse_args"));
        assert!(
            Outline
                .call(json!({"path": "../fuera"}), &ctx)
                .await
                .is_error
        );
        assert!(Outline.call(json!({}), &ctx).await.is_error);
        assert!(
            Outline
                .call(json!({"path": "notas.txt"}), &ctx)
                .await
                .content
                .contains("sin esquema")
        );
    }
}
