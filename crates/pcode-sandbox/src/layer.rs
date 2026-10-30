//! Lectura de la capa superior del overlay como lista de cambios.
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

/// Tipo de cambio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChangeKind {
    /// Añadido.
    Added,
    /// Modificado.
    Modified,
    /// Borrado.
    Deleted,
    /// Directorio reemplazado.
    Replaced,
}

/// Un cambio.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Change {
    /// Ruta relativa con `/`.
    pub path: String,
    /// Tipo.
    pub kind: ChangeKind,
}

fn is_opaque(p: &Path) -> bool {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else {
        return false;
    };
    for name in [c"trusted.overlay.opaque", c"user.overlay.opaque"] {
        let mut buf = [0u8; 8];
        // SAFETY: `c` y `name` son cadenas C válidas y `buf` tiene la longitud indicada.
        let n = unsafe {
            libc::lgetxattr(
                c.as_ptr(),
                name.as_ptr(),
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if n >= 1 && buf[0] == b'y' {
            return true;
        }
    }
    false
}

fn walk(up: &Path, lower: &Path, dir: &Path, out: &mut Vec<Change>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        let rel = p
            .strip_prefix(up)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let md = std::fs::symlink_metadata(&p)?;
        let ft = md.file_type();
        let exists = std::fs::symlink_metadata(lower.join(&rel)).is_ok();
        if ft.is_char_device() && md.rdev() == 0 {
            out.push(Change {
                path: rel,
                kind: ChangeKind::Deleted,
            });
        } else if ft.is_dir() {
            if is_opaque(&p) && exists {
                out.push(Change {
                    path: rel,
                    kind: ChangeKind::Replaced,
                });
            } else {
                if !exists && std::fs::read_dir(&p)?.next().is_none() {
                    out.push(Change {
                        path: rel.clone(),
                        kind: ChangeKind::Added,
                    });
                }
                walk(up, lower, &p, out)?;
            }
        } else {
            out.push(Change {
                path: rel,
                kind: if exists {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Added
                },
            });
        }
    }
    Ok(())
}

/// Cambios de la capa `upper` respecto a `lower`.
pub fn read_changes(upper: &Path, lower: &Path) -> std::io::Result<Vec<Change>> {
    let mut v = vec![];
    if upper.exists() {
        walk(upper, lower, upper, &mut v)?;
    }
    v.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(v)
}

/// Quita los cambios que git ignora.
pub fn filter_ignored(lower: &Path, changes: Vec<Change>) -> std::io::Result<Vec<Change>> {
    if changes.is_empty() || !lower.join(".git").exists() {
        return Ok(changes);
    }
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(lower)
        .args(["check-ignore", "--stdin"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let input: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.join("\n").as_bytes())?;
    let out = child.wait_with_output()?;
    let ign: std::collections::HashSet<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(String::from)
        .collect();
    Ok(changes
        .into_iter()
        .filter(|c| !ign.contains(&c.path))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn w(base: &Path, p: &str, s: &str) {
        let f = base.join(p);
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(f, s).unwrap();
    }
    #[test]
    fn capa_anadido_y_modificado() {
        let d = tempfile::tempdir().unwrap();
        let (up, lo) = (d.path().join("up"), d.path().join("lo"));
        w(&lo, "a.txt", "1");
        w(&up, "a.txt", "2");
        w(&up, "d/b.txt", "3");
        let c = read_changes(&up, &lo).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(
            (c[0].path.as_str(), c[0].kind),
            ("a.txt", ChangeKind::Modified)
        );
        assert_eq!(
            (c[1].path.as_str(), c[1].kind),
            ("d/b.txt", ChangeKind::Added)
        );
    }
    #[test]
    fn capa_directorio_nuevo_vacio() {
        let d = tempfile::tempdir().unwrap();
        let (up, lo) = (d.path().join("up"), d.path().join("lo"));
        fs::create_dir_all(up.join("vacio")).unwrap();
        fs::create_dir_all(&lo).unwrap();
        assert_eq!(read_changes(&up, &lo).unwrap()[0].kind, ChangeKind::Added);
    }
    #[test]
    fn capa_ordenada() {
        let d = tempfile::tempdir().unwrap();
        let (up, lo) = (d.path().join("up"), d.path().join("lo"));
        fs::create_dir_all(&lo).unwrap();
        w(&up, "z", "");
        w(&up, "a", "");
        let c = read_changes(&up, &lo).unwrap();
        assert_eq!((c[0].path.as_str(), c[1].path.as_str()), ("a", "z"));
    }
    #[test]
    fn filtra_ignorados_por_git() {
        let d = tempfile::tempdir().unwrap();
        let lo = d.path().join("lo");
        fs::create_dir_all(&lo).unwrap();
        let g = |a: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&lo)
                .args(a)
                .output()
                .unwrap()
        };
        g(&["init", "-q"]);
        w(&lo, ".gitignore", "target/\n");
        let c = vec![
            Change {
                path: "target/x".into(),
                kind: ChangeKind::Added,
            },
            Change {
                path: "src/a.rs".into(),
                kind: ChangeKind::Added,
            },
        ];
        let f = filter_ignored(&lo, c).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].path, "src/a.rs");
    }
}
