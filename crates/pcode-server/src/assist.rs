//! Asistente de Studio: arma prompts con el contexto justo del proyecto (F8.37, F8.38, F8.40, F8.42)
//! y los resuelve con `pcode preguntar` (sin tools) o los entrega para mandarlos al agente.
//!
//! | Tipo | Contexto | Inspirado en |
//! |---|---|---|
//! | `commit` | resumen por archivo, símbolos tocados y diff preparado | HierDiff (2026.findings-emnlp.451) |
//! | `revisar` | diff de trabajo y lista de mantenibilidad | OmniCode (2026.findings-acl.2020), CodeThread (2026.emnlp-main.2528) |
//! | `arreglar` | archivo, línea, símbolo, mensaje y código alrededor | SHERLOC (2026.emnlp-main.711), LintEval (2026.findings-emnlp.157) |
//! | `concretar` | archivos y símbolos relacionados con la petición | CodeScout (2026.findings-acl.2032), Ask or Assume? (2026.findings-emnlp.15) |
//! | `explicar`, `tests` | archivo (o selección) con su esquema | SWE-QA (2026.findings-acl.402) |
use crate::outline::{language_of, outline, workspace_symbols};
use crate::workspace::{WsResult, files, git, is_git, read, safe_path};
use serde::Deserialize;
use std::path::Path;

/// Tamaño máximo del diff que se manda (en caracteres).
pub const MAX_DIFF: usize = 60_000;

/// Un diagnóstico que hay que arreglar.
#[derive(Debug, Clone, Deserialize)]
pub struct Diag {
    /// Línea (desde 0, como el LSP).
    pub line: u32,
    /// Mensaje.
    pub message: String,
    /// 1 error, 2 aviso…
    #[serde(default)]
    pub severity: u8,
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut c: String = s.chars().take(n).collect();
        c.push_str("\n[… diff recortado]");
        c
    }
}

/// Símbolos tocados por un diff, según el contexto de función que git pone en cada hunk
/// (`@@ -1,2 +1,3 @@ fn carga(`), sin repetir.
pub fn touched_symbols(diff: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for l in diff.lines().filter(|l| l.starts_with("@@ ")) {
        if let Some(ctx) = l.splitn(3, "@@").nth(2).map(str::trim)
            && !ctx.is_empty()
        {
            let s: String = ctx.chars().take(80).collect();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// Prompt para el mensaje de commit: resumen por archivo (+/−), símbolos tocados, estilo de los
/// últimos commits y el diff. Usa lo preparado; si no hay nada preparado, todos los cambios.
pub fn commit_prompt(root: &Path) -> WsResult<String> {
    if !is_git(root) {
        return Err("el proyecto no es un repositorio git".into());
    }
    let mut staged = true;
    let mut diff = git(root, &["diff", "--cached", "--no-color", "--no-ext-diff"])?;
    if diff.trim().is_empty() {
        staged = false;
        diff = git(root, &["diff", "--no-color", "--no-ext-diff", "HEAD"]).unwrap_or_default();
    }
    if diff.trim().is_empty() {
        return Err("no hay cambios para el commit".into());
    }
    let numstat = git(
        root,
        if staged {
            &["diff", "--cached", "--numstat"]
        } else {
            &["diff", "--numstat", "HEAD"]
        },
    )
    .unwrap_or_default();
    let resumen: Vec<String> = numstat
        .lines()
        .filter_map(|l| {
            let mut p = l.split('\t');
            let (a, d, f) = (p.next()?, p.next()?, p.next()?);
            Some(format!("- {f} (+{a} −{d})"))
        })
        .collect();
    let simbolos = touched_symbols(&diff);
    let recientes = git(root, &["log", "-8", "--format=%s"]).unwrap_or_default();
    let mut s = String::from(
        "Escribe el mensaje de commit para estos cambios.\n\
         Formato: una primera línea de 72 caracteres como mucho que diga qué cambia y para qué; si hace \
         falta, una línea en blanco y viñetas cortas con lo importante. Sigue el idioma y el estilo de \
         los commits recientes. Responde SOLO con el mensaje, sin comillas ni bloques de código.\n\n",
    );
    s.push_str(&format!(
        "# Archivos ({})\n{}\n\n",
        if staged {
            "preparados"
        } else {
            "todos los cambios"
        },
        resumen.join("\n")
    ));
    if !simbolos.is_empty() {
        s.push_str(&format!(
            "# Funciones y tipos tocados\n{}\n\n",
            simbolos
                .iter()
                .map(|x| format!("- {x}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !recientes.trim().is_empty() {
        s.push_str(&format!("# Commits recientes\n{}\n", recientes.trim()));
    }
    s.push_str(&format!(
        "# Diff\n```diff\n{}\n```\n",
        clip(&diff, MAX_DIFF)
    ));
    Ok(s)
}

/// Prompt para revisar los cambios sin commit con una lista de mantenibilidad.
pub fn review_prompt(root: &Path) -> WsResult<String> {
    if !is_git(root) {
        return Err("el proyecto no es un repositorio git".into());
    }
    let diff = git(root, &["diff", "--no-color", "--no-ext-diff", "HEAD"])
        .or_else(|_| git(root, &["diff", "--no-color", "--no-ext-diff"]))?;
    if diff.trim().is_empty() {
        return Err("no hay cambios que revisar".into());
    }
    Ok(format!(
        "Revisa estos cambios como un revisor exigente antes del commit. Para cada problema di archivo, \
         línea, gravedad (🔴 bloquea, 🟡 conviene, 🔵 detalle) y el arreglo concreto. Revisa:\n\
         1. Errores y casos límite (None/null, vacíos, índices, errores sin tratar, concurrencia).\n\
         2. Alcance: ¿hay cambios que no hacen falta o reescrituras más grandes de lo necesario?\n\
         3. Mantenibilidad: duplicación, nombres, funciones demasiado largas, comentarios engañosos.\n\
         4. Tests: ¿qué falta probar?\n\
         5. Seguridad: secretos, entradas sin validar, órdenes o rutas construidas con datos externos.\n\
         Si está bien, dilo en una línea. Termina con un veredicto: listo para commit o no.\n\n\
         # Símbolos tocados\n{}\n\n# Diff\n```diff\n{}\n```\n",
        touched_symbols(&diff).join("\n"),
        clip(&diff, MAX_DIFF)
    ))
}

/// El símbolo más interno que contiene la línea `line` (desde 1).
fn enclosing(symbols: &[crate::outline::Symbol], line: u32) -> Option<String> {
    let mut path: Vec<&crate::outline::Symbol> = vec![];
    for s in symbols.iter().filter(|s| s.line <= line) {
        while path.last().is_some_and(|p| p.depth >= s.depth) {
            path.pop();
        }
        path.push(s);
    }
    (!path.is_empty()).then(|| {
        path.iter()
            .map(|s| format!("{} {}", s.kind, s.name))
            .collect::<Vec<_>>()
            .join(" › ")
    })
}

/// Prompt para que el agente arregle uno o varios diagnósticos de `rel`, con el diagnóstico
/// estructurado: dónde, en qué símbolo, qué dice y el código alrededor.
pub fn fix_prompt(root: &Path, rel: &str, diags: &[Diag]) -> WsResult<String> {
    safe_path(root, rel)?;
    if diags.is_empty() {
        return Err("no hay diagnósticos que arreglar".into());
    }
    let content = read(root, rel)?.content;
    let lines: Vec<&str> = content.lines().collect();
    let symbols = outline(&content, language_of(rel));
    let mut s = if diags.len() == 1 {
        format!("Arregla este problema de `{rel}`.\n")
    } else {
        format!("Arregla estos {} problemas de `{rel}`.\n", diags.len())
    };
    s.push_str(
        "Haz la edición mínima: no cambies nada que no haga falta para el arreglo ni introduzcas \
         avisos nuevos. Si la causa está en otro archivo, localízala antes de editar. Cuando termines, \
         comprueba que compila o pasa los tests si hay una orden para ello.\n",
    );
    for (i, d) in diags.iter().take(30).enumerate() {
        let n = d.line + 1;
        let sev = match d.severity {
            1 => "error",
            2 => "aviso",
            3 => "información",
            _ => "problema",
        };
        s.push_str(&format!(
            "\n## {}. {sev} en {rel}:{n}\n- Mensaje: {}\n",
            i + 1,
            d.message.trim()
        ));
        if let Some(sym) = enclosing(&symbols, n) {
            s.push_str(&format!("- Dentro de: {sym}\n"));
        }
        let from = d.line.saturating_sub(8) as usize;
        let to = ((d.line + 9) as usize).min(lines.len());
        if from < to {
            s.push_str("```\n");
            for (k, l) in lines[from..to].iter().enumerate() {
                let num = from + k + 1;
                let mark = if num as u32 == n { "▶" } else { " " };
                s.push_str(&format!("{mark}{num:>5} │ {l}\n"));
            }
            s.push_str("```\n");
        }
    }
    Ok(s)
}

/// Prompt para que el agente arregle lo que hizo fallar una orden del terminal: la orden, su código
/// de salida y el final de la salida (donde suelen estar los errores).
pub fn command_prompt(command: &str, code: Option<i32>, output: &str) -> WsResult<String> {
    if command.trim().is_empty() {
        return Err("falta la orden".into());
    }
    let lines: Vec<&str> = output.lines().collect();
    let tail = lines[lines.len().saturating_sub(80)..].join("\n");
    let tail: String = tail
        .chars()
        .rev()
        .take(12_000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Ok(format!(
        "La orden `{command}` falló ({}). Final de la salida:\n```text\n{tail}\n```\n\
         Encuentra la causa (lee el código que señala el error antes de tocar nada), arréglala con la \
         edición mínima y vuelve a ejecutar `{command}` para comprobar que ya pasa.",
        code.map(|c| format!("código {c}"))
            .unwrap_or_else(|| "sin código de salida".into())
    ))
}

/// Palabras útiles para buscar en el proyecto: identificadores y palabras de 4 o más letras que
/// no sean de uso común.
fn keywords(text: &str) -> Vec<String> {
    const COMMON: &[&str] = &[
        "para", "como", "que", "este", "esta", "esto", "quiero", "hacer", "haz", "cuando", "donde",
        "porque", "pero", "todo", "todos", "with", "that", "this", "from", "into", "when", "make",
        "should", "would", "have", "algo", "añade", "añadir", "cambia", "cambiar", "arregla",
        "arreglar", "mejora", "mejorar", "sobre", "también", "tiene", "tienen", "puede", "please",
        "file", "archivo", "código", "code", "función", "function", "más", "menos", "solo",
    ];
    let mut out: Vec<String> = vec![];
    for w in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        let lw = w.to_lowercase();
        if w.chars().count() >= 4 && !COMMON.contains(&lw.as_str()) && !out.contains(&lw) {
            out.push(lw);
        }
    }
    out.truncate(12);
    out
}

/// Prompt para convertir una petición ambigua en un enunciado accionable, con los archivos y
/// símbolos del proyecto que parecen relacionados (exploración ligera previa).
pub fn refine_prompt(root: &Path, text: &str) -> WsResult<String> {
    if text.trim().is_empty() {
        return Err("escribe primero la petición".into());
    }
    let all = files(root).unwrap_or_default();
    let words = keywords(text);
    let mut rel_files: Vec<String> = vec![];
    for w in &words {
        for f in all.iter().filter(|f| f.to_lowercase().contains(w.as_str())) {
            if !rel_files.contains(f) && rel_files.len() < 15 {
                rel_files.push(f.clone());
            }
        }
    }
    let mut syms: Vec<String> = vec![];
    for w in &words {
        for h in workspace_symbols(root, &all, w, 6) {
            let s = format!("{} {} — {}:{}", h.kind, h.name, h.path, h.line);
            if !syms.contains(&s) && syms.len() < 25 {
                syms.push(s);
            }
        }
    }
    let tree: Vec<&String> = all.iter().take(150).collect();
    Ok(format!(
        "Convierte esta petición de un desarrollador en un enunciado claro para un agente de \
         programación que trabajará en el proyecto. No resuelvas la tarea. Responde en el idioma de la \
         petición y con este formato exacto:\n\
         **Objetivo:** una o dos frases.\n\
         **Dónde:** archivos y símbolos probables (solo de las listas de abajo; si no hay, dilo).\n\
         **Pasos:** 2 a 5 pasos concretos.\n\
         **Hecho cuando:** criterio comprobable (test, orden o comportamiento).\n\
         **Preguntas:** lo que falte especificar y cambie la solución; «ninguna» si está claro.\n\n\
         # Petición\n{}\n\n# Archivos cuyo nombre coincide\n{}\n\n# Símbolos relacionados\n{}\n\n\
         # Archivos del proyecto (primeros {})\n{}\n",
        text.trim(),
        if rel_files.is_empty() {
            "(ninguno)".to_string()
        } else {
            rel_files.join("\n")
        },
        if syms.is_empty() {
            "(ninguno)".to_string()
        } else {
            syms.join("\n")
        },
        tree.len(),
        tree.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

/// Prompt para explicar un archivo (o unas líneas) o para escribirle tests.
pub fn file_prompt(
    root: &Path,
    rel: &str,
    kind: &str,
    lines: Option<(u32, u32)>,
) -> WsResult<String> {
    safe_path(root, rel)?;
    let content = read(root, rel)?.content;
    let syms = outline(&content, language_of(rel));
    let esquema = syms
        .iter()
        .map(|s| {
            format!(
                "{}- {} {} (línea {})",
                "  ".repeat(s.depth as usize),
                s.kind,
                s.name,
                s.line
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (head, body) = match lines {
        Some((a, b)) => {
            let sel: Vec<&str> = content
                .lines()
                .skip(a.saturating_sub(1) as usize)
                .take((b.saturating_sub(a) + 1) as usize)
                .collect();
            (format!("las líneas {a}-{b} de `{rel}`"), sel.join("\n"))
        }
        None => (format!("`{rel}`"), clip(&content, MAX_DIFF)),
    };
    let task = if kind == "tests" {
        format!(
            "Escribe tests para {head}: sigue el framework y el estilo de los tests que ya tenga el \
             proyecto (búscalos antes), cubre el caso normal, los límites y los errores, y ejecútalos."
        )
    } else {
        format!(
            "Explica {head}: para qué sirve, cómo encaja en el proyecto (quién lo usa y a quién llama), \
             el flujo principal y lo que no es obvio (invariantes, trampas). Sé concreto y cita líneas."
        )
    };
    Ok(format!(
        "{task}\n\n# Esquema de {rel}\n{esquema}\n\n# Código\n```\n{body}\n```\n"
    ))
}

/// Ejecuta `pcode preguntar --formato json` con `prompt` por stdin y devuelve el texto. Como mucho
/// 180 s.
pub async fn ask(pcode_bin: &Path, root: &Path, prompt: &str) -> WsResult<String> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(pcode_bin)
        .args(["preguntar", "--formato", "json"])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("no se pudo lanzar pcode preguntar: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| "el modelo no respondió en 180 s".to_string())?
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("pcode preguntar falló")
            .trim()
            .trim_start_matches("Error: ");
        return Err(line.to_string());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("respuesta inválida de pcode preguntar: {e}"))?;
    let t = v["texto"].as_str().unwrap_or_default().trim();
    // Quita un bloque de código envolvente si el modelo lo puso.
    let t = t
        .strip_prefix("```")
        .and_then(|x| x.split_once('\n').map(|(_, r)| r))
        .and_then(|x| x.trim_end().strip_suffix("```"))
        .unwrap_or(t)
        .trim();
    Ok(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let g = |a: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(r)
                    .args(a)
                    .output()
                    .unwrap()
                    .status
                    .success()
            )
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(
            r.join("lib.rs"),
            "pub fn suma(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn resta(a: i32, b: i32) -> i32 {\n    a - b\n}\n",
        )
        .unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "feat: suma y resta"]);
        d
    }

    #[test]
    fn prompt_commit_lleva_resumen_y_simbolos() {
        let d = repo();
        let r = d.path();
        assert_eq!(
            commit_prompt(r).unwrap_err(),
            "no hay cambios para el commit"
        );
        std::fs::write(
            r.join("lib.rs"),
            "pub fn suma(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn resta(a: i32, b: i32) -> i32 {\n    a.saturating_sub(b)\n}\n",
        )
        .unwrap();
        let p = commit_prompt(r).unwrap();
        assert!(p.contains("- lib.rs (+1 −1)"), "{p}");
        assert!(p.contains("todos los cambios"));
        assert!(p.contains("pub fn resta"), "símbolo del hunk: {p}");
        assert!(p.contains("feat: suma y resta"), "estilo de los commits");
        assert!(p.contains("a.saturating_sub(b)"));
        Command::new("git")
            .arg("-C")
            .arg(r)
            .args(["add", "lib.rs"])
            .output()
            .unwrap();
        assert!(commit_prompt(r).unwrap().contains("preparados"));
    }

    #[test]
    fn prompt_commit_recorta_el_diff() {
        let d = repo();
        let r = d.path();
        let big: String = (0..20_000).map(|i| format!("// línea {i}\n")).collect();
        std::fs::write(r.join("grande.rs"), big).unwrap();
        Command::new("git")
            .arg("-C")
            .arg(r)
            .args(["add", "grande.rs"])
            .output()
            .unwrap();
        let p = commit_prompt(r).unwrap();
        assert!(p.contains("[… diff recortado]"));
        assert!(p.chars().count() < MAX_DIFF + 5_000);
    }

    #[test]
    fn prompt_revisar_lleva_diff_y_lista() {
        let d = repo();
        let r = d.path();
        assert!(review_prompt(r).is_err());
        std::fs::write(r.join("lib.rs"), "pub fn suma() {}\n").unwrap();
        let p = review_prompt(r).unwrap();
        assert!(p.contains("Mantenibilidad") && p.contains("-    a + b"));
        assert!(p.contains("veredicto"));
    }

    #[test]
    fn prompt_arreglar_lleva_simbolo_y_codigo() {
        let d = repo();
        let p = fix_prompt(
            d.path(),
            "lib.rs",
            &[Diag {
                line: 5,
                message: "overflow posible".into(),
                severity: 2,
            }],
        )
        .unwrap();
        assert!(p.contains("aviso en lib.rs:6"), "{p}");
        assert!(p.contains("Dentro de: fn resta"), "{p}");
        assert!(p.contains("▶    6 │     a - b"), "{p}");
        assert!(p.contains("edición mínima"));
        assert!(fix_prompt(d.path(), "../x", &[]).is_err());
    }

    #[test]
    fn prompt_arreglar_varios_diagnosticos() {
        let d = repo();
        let diags = vec![
            Diag {
                line: 0,
                message: "a".into(),
                severity: 1,
            },
            Diag {
                line: 4,
                message: "b".into(),
                severity: 1,
            },
        ];
        let p = fix_prompt(d.path(), "lib.rs", &diags).unwrap();
        assert!(p.contains("estos 2 problemas"));
        assert!(p.contains("## 1. error en lib.rs:1") && p.contains("## 2. error en lib.rs:5"));
        assert!(fix_prompt(d.path(), "lib.rs", &[]).is_err());
    }

    #[test]
    fn prompt_concretar_lleva_archivos_y_simbolos_relevantes() {
        let d = repo();
        let p = refine_prompt(d.path(), "haz que resta no desborde").unwrap();
        assert!(p.contains("fn resta — lib.rs:5"), "{p}");
        assert!(p.contains("**Preguntas:**"));
        assert!(p.contains("# Petición\nhaz que resta no desborde"));
        assert!(refine_prompt(d.path(), "  ").is_err());
    }

    #[test]
    fn prompt_explicar_y_tests_llevan_esquema() {
        let d = repo();
        let p = file_prompt(d.path(), "lib.rs", "explicar", Some((5, 7))).unwrap();
        assert!(p.contains("las líneas 5-7 de `lib.rs`"));
        assert!(p.contains("- fn suma (línea 1)") && p.contains("a - b"));
        assert!(!p.contains("a + b"), "solo la selección");
        let t = file_prompt(d.path(), "lib.rs", "tests", None).unwrap();
        assert!(t.starts_with("Escribe tests para `lib.rs`"));
    }

    #[test]
    fn prompt_orden_fallida_lleva_el_final_de_la_salida() {
        let out: String = (0..200).map(|i| format!("línea {i}\n")).collect();
        let p = command_prompt("cargo test", Some(101), &out).unwrap();
        assert!(p.contains("`cargo test` falló (código 101)"));
        assert!(
            p.contains("línea 199") && !p.contains("línea 100\n"),
            "solo el final"
        );
        assert!(command_prompt(" ", None, "").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn assist_sin_modelo_da_error_claro() {
        // Se ejecuta `sh preguntar …`: el «binario» es /bin/sh y lee el guion `preguntar` del
        // directorio. Así no se ejecuta un archivo recién escrito (ETXTBSY si otro hilo del test
        // hace fork justo entonces).
        let sh = std::path::Path::new("/bin/sh");
        let guion = |cuerpo: &str| {
            let d = tempfile::tempdir().unwrap();
            std::fs::write(d.path().join("preguntar"), cuerpo).unwrap();
            d
        };
        let ok =
            guion("cat > /dev/null\nprintf '%s\\n' '{\"texto\":\"```\\nfeat: algo\\n```\"}'\n");
        let fail =
            guion("cat > /dev/null\necho 'Error: no hay ningún modelo configurado' >&2\nexit 1\n");
        assert_eq!(ask(sh, ok.path(), "x").await.unwrap(), "feat: algo");
        assert_eq!(
            ask(sh, fail.path(), "x").await.unwrap_err(),
            "no hay ningún modelo configurado"
        );
        assert!(
            ask(&ok.path().join("no-existe"), ok.path(), "x")
                .await
                .is_err()
        );
    }
}
