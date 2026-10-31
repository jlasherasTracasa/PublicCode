//! Espacio de trabajo para el editor: archivos, búsqueda, git y terminal, siempre dentro de la
//! raíz del proyecto (también frente a enlaces simbólicos que apunten fuera).
use pcode_protocol::{FileEntry, GitFile, SearchHit};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// Tamaño máximo de un archivo que se abre en el editor.
pub const MAX_READ: u64 = 4 * 1024 * 1024;
/// Resultados máximos de una búsqueda.
pub const MAX_HITS: usize = 500;
/// Archivos máximos que se recorren sin git.
const MAX_FILES: usize = 50_000;
/// Directorios que nunca se recorren al buscar sin git.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".pcode",
    "build",
    "dist",
    ".gradle",
];

/// Errores del espacio de trabajo (se devuelven como texto al cliente).
pub type WsResult<T> = Result<T, String>;

/// Resuelve `rel` dentro de `root`. Rechaza `..` que salga de la raíz, rutas absolutas ajenas y
/// enlaces simbólicos (o carpetas enlazadas) que lleven fuera.
pub fn safe_path(root: &Path, rel: &str) -> WsResult<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("raíz inválida: {e}"))?;
    let rel = rel.trim_start_matches(['/', '\\']);
    let mut out = root.clone();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(x) => out.push(x),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() || !out.starts_with(&root) {
                    return Err(format!("ruta fuera del proyecto: {rel}"));
                }
            }
            _ => return Err(format!("ruta no permitida: {rel}")),
        }
    }
    // Lo que ya existe se comprueba con su ruta real; si no existe, se comprueba el antecesor
    // más cercano que sí exista.
    let mut probe = out.clone();
    loop {
        if let Ok(real) = probe.canonicalize() {
            if !real.starts_with(&root) {
                return Err(format!("ruta fuera del proyecto: {rel}"));
            }
            break;
        }
        if !probe.pop() {
            break;
        }
    }
    Ok(out)
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Lista un directorio: carpetas primero y luego archivos, por nombre. Oculta `.git` y `.pcode`.
pub fn list(root: &Path, rel: &str) -> WsResult<Vec<FileEntry>> {
    let dir = safe_path(root, rel)?;
    let croot = root.canonicalize().map_err(|e| e.to_string())?;
    let mut v: Vec<FileEntry> = std::fs::read_dir(&dir)
        .map_err(|e| format!("no se puede listar {rel}: {e}"))?
        .flatten()
        .filter(|e| e.file_name() != ".git" && e.file_name() != ".pcode")
        .map(|e| {
            let md = e.metadata().ok();
            let dir = md.as_ref().is_some_and(|m| m.is_dir());
            FileEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: rel_of(&croot, &e.path()),
                dir,
                size: if dir {
                    0
                } else {
                    md.map(|m| m.len()).unwrap_or(0)
                },
            }
        })
        .collect();
    v.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(v)
}

/// Contenido de un archivo para el editor.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FileContent {
    /// Texto (vacío si es binario).
    pub content: String,
    /// Es binario (tiene bytes nulos o no es UTF-8).
    pub binary: bool,
    /// Tamaño en bytes.
    pub size: u64,
}

/// Lee un archivo de texto (como mucho [`MAX_READ`] bytes).
pub fn read(root: &Path, rel: &str) -> WsResult<FileContent> {
    let p = safe_path(root, rel)?;
    let size = std::fs::metadata(&p)
        .map_err(|e| format!("no se puede abrir {rel}: {e}"))?
        .len();
    if size > MAX_READ {
        return Err(format!(
            "{rel} ocupa {size} bytes: demasiado para el editor"
        ));
    }
    let bytes = std::fs::read(&p).map_err(|e| format!("no se puede leer {rel}: {e}"))?;
    let binary = bytes.contains(&0);
    match (binary, String::from_utf8(bytes)) {
        (false, Ok(content)) => Ok(FileContent {
            content,
            binary: false,
            size,
        }),
        _ => Ok(FileContent {
            content: String::new(),
            binary: true,
            size,
        }),
    }
}

/// Escribe un archivo (crea las carpetas que falten).
pub fn write(root: &Path, rel: &str, content: &str) -> WsResult<()> {
    if rel.trim().is_empty() {
        return Err("falta la ruta".into());
    }
    let p = safe_path(root, rel)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| format!("no se puede escribir {rel}: {e}"))
}

fn git(root: &Path, args: &[&str]) -> WsResult<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("no se pudo ejecutar git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Si la raíz es un repositorio git.
pub fn is_git(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|s| s.trim() == "true")
}

/// Todos los archivos del proyecto: con git, los versionados y los nuevos no ignorados; sin git,
/// un recorrido que salta `target`, `node_modules` y similares.
pub fn files(root: &Path) -> WsResult<Vec<String>> {
    if is_git(root) {
        let s = git(
            root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        let mut v: Vec<String> = s
            .split('\0')
            .filter(|x| !x.is_empty())
            .filter(|x| root.join(x).is_file())
            .map(String::from)
            .collect();
        v.sort();
        v.dedup();
        return Ok(v);
    }
    let croot = root.canonicalize().map_err(|e| e.to_string())?;
    let mut out = vec![];
    let mut stack = vec![croot.clone()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(e.path());
                }
            } else if ft.is_file() {
                out.push(rel_of(&croot, &e.path()));
                if out.len() >= MAX_FILES {
                    out.sort();
                    return Ok(out);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Busca `query` (texto literal o regex) en los archivos de texto del proyecto.
pub fn search(
    root: &Path,
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
) -> WsResult<Vec<SearchHit>> {
    if query.is_empty() {
        return Ok(vec![]);
    }
    let pat = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    let re = regex::RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| format!("expresión inválida: {e}"))?;
    let mut hits = vec![];
    for f in files(root)? {
        let p = root.join(&f);
        if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX) > 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                hits.push(SearchHit {
                    path: f.clone(),
                    line: i as u32 + 1,
                    text: line.trim().chars().take(240).collect(),
                });
                if hits.len() >= MAX_HITS {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

/// Rama actual y archivos con cambios.
pub fn git_status(root: &Path) -> WsResult<(String, Vec<GitFile>)> {
    if !is_git(root) {
        return Err("el proyecto no es un repositorio git".into());
    }
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "(sin commits)".into());
    let raw = git(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut files = vec![];
    let mut parts = raw.split('\0').filter(|x| !x.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y, path) = (&entry[0..1], &entry[1..2], &entry[3..]);
        // En un renombrado, la ruta antigua viene detrás: se descarta.
        if x == "R" || x == "C" {
            parts.next();
        }
        files.push(GitFile {
            path: path.to_string(),
            index: x.to_string(),
            worktree: y.to_string(),
        });
    }
    Ok((branch, files))
}

/// Diff de un archivo (o de todo); para un archivo nuevo sin versionar, su contenido como añadido.
pub fn git_diff(root: &Path, rel: Option<&str>, staged: bool) -> WsResult<String> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    if let Some(r) = rel {
        safe_path(root, r)?;
        args.push("--");
        args.push(r);
    }
    let d = git(root, &args)?;
    if d.is_empty()
        && let Some(r) = rel
        && !staged
        && git(root, &["ls-files", "--error-unmatch", "--", r]).is_err()
    {
        let c = read(root, r)?;
        let body: String = c.content.lines().map(|l| format!("+{l}\n")).collect();
        return Ok(format!("--- /dev/null\n+++ b/{r}\n{body}"));
    }
    Ok(d)
}

/// Prepara o quita del índice.
pub fn git_stage(root: &Path, paths: &[String], stage: bool) -> WsResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    for p in paths {
        safe_path(root, p)?;
    }
    let mut args: Vec<&str> = if stage {
        vec!["add", "--"]
    } else {
        vec!["restore", "--staged", "--"]
    };
    args.extend(paths.iter().map(String::as_str));
    git(root, &args).map(|_| ())
}

/// Commit de lo preparado.
pub fn git_commit(root: &Path, message: &str) -> WsResult<String> {
    if message.trim().is_empty() {
        return Err("el mensaje del commit está vacío".into());
    }
    git(root, &["commit", "-m", message])
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    .success(),
                "{a:?}"
            );
        };
        g(&["init", "-q"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::create_dir_all(r.join("src")).unwrap();
        std::fs::write(
            r.join("src/main.rs"),
            "fn main() {\n    println!(\"hola\");\n}\n",
        )
        .unwrap();
        std::fs::write(r.join(".gitignore"), "target/\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-qm", "inicio"]);
        d
    }

    #[test]
    fn rutas_fuera_de_la_raiz_se_rechazan() {
        let d = repo();
        assert!(safe_path(d.path(), "src/main.rs").is_ok());
        assert!(safe_path(d.path(), "src/../../fuera").is_err());
        assert!(safe_path(d.path(), "nuevo/dir/x.txt").is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc", d.path().join("enlace")).unwrap();
            assert!(
                safe_path(d.path(), "enlace/passwd").is_err(),
                "un enlace no saca de la raíz"
            );
        }
    }

    #[test]
    fn lista_lee_y_escribe() {
        let d = repo();
        let l = list(d.path(), "").unwrap();
        assert_eq!(l[0].name, "src");
        assert!(l[0].dir);
        assert!(!l.iter().any(|e| e.name == ".git"));
        assert!(
            read(d.path(), "src/main.rs")
                .unwrap()
                .content
                .contains("hola")
        );
        write(d.path(), "docs/nota.md", "# Nota\n").unwrap();
        assert_eq!(read(d.path(), "docs/nota.md").unwrap().content, "# Nota\n");
        std::fs::write(d.path().join("bin.dat"), [0u8, 1, 2]).unwrap();
        assert!(read(d.path(), "bin.dat").unwrap().binary);
    }

    #[test]
    fn files_respeta_gitignore_y_search_encuentra() {
        let d = repo();
        std::fs::create_dir_all(d.path().join("target")).unwrap();
        std::fs::write(d.path().join("target/x.rs"), "hola").unwrap();
        std::fs::write(d.path().join("nuevo.rs"), "// HOLA nuevo").unwrap();
        let f = files(d.path()).unwrap();
        assert!(f.contains(&"src/main.rs".to_string()) && f.contains(&"nuevo.rs".to_string()));
        assert!(!f.iter().any(|x| x.starts_with("target/")));
        let h = search(d.path(), "hola", false, false).unwrap();
        assert_eq!(h.len(), 2, "{h:?}");
        assert!(search(d.path(), "hola", false, true).unwrap().len() == 1);
        assert_eq!(
            search(d.path(), r"println!\(", true, true).unwrap()[0].line,
            2
        );
        assert!(search(d.path(), "(", true, true).is_err());
    }

    #[test]
    fn git_estado_diff_stage_y_commit() {
        let d = repo();
        std::fs::write(d.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(d.path().join("nuevo.txt"), "uno\n").unwrap();
        let (branch, st) = git_status(d.path()).unwrap();
        assert!(!branch.is_empty());
        assert!(
            st.iter()
                .any(|f| f.path == "src/main.rs" && f.worktree == "M")
        );
        assert!(st.iter().any(|f| f.path == "nuevo.txt" && f.index == "?"));
        assert!(
            git_diff(d.path(), Some("src/main.rs"), false)
                .unwrap()
                .contains("-    println!")
        );
        assert!(
            git_diff(d.path(), Some("nuevo.txt"), false)
                .unwrap()
                .contains("+uno")
        );
        git_stage(d.path(), &["nuevo.txt".into()], true).unwrap();
        assert!(git_diff(d.path(), None, true).unwrap().contains("+uno"));
        git_stage(d.path(), &["nuevo.txt".into()], false).unwrap();
        git_stage(d.path(), &["src/main.rs".into()], true).unwrap();
        assert!(git_commit(d.path(), "").is_err());
        git_commit(d.path(), "cambia main").unwrap();
        let (_, st) = git_status(d.path()).unwrap();
        assert!(!st.iter().any(|f| f.path == "src/main.rs"));
    }
}
