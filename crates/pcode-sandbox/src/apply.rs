//! Aplicar, descartar y diff de los cambios de una sandbox.
use crate::layer::{Change, ChangeKind};
use crate::run::{Sandbox, SandboxError};
use similar::TextDiff;
use std::path::Path;

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    let md = std::fs::symlink_metadata(src)?;
    if md.file_type().is_symlink() {
        let t = std::fs::read_link(src)?;
        let _ = std::fs::remove_file(dst);
        std::os::unix::fs::symlink(t, dst)
    } else if md.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        let _ = std::fs::remove_file(dst);
        std::fs::copy(src, dst).map(|_| ())
    }
}

fn remove_any(p: &Path) {
    if let Ok(md) = std::fs::symlink_metadata(p) {
        if md.is_dir() {
            let _ = std::fs::remove_dir_all(p);
        } else {
            let _ = std::fs::remove_file(p);
        }
    }
}

impl Sandbox {
    /// Lleva los cambios al repo y borra la capa.
    pub fn apply(self) -> Result<Vec<Change>, SandboxError> {
        let changes = self.changes()?;
        let up = self.state.join("upper");
        for c in &changes {
            let (src, dst) = (up.join(&c.path), self.root.join(&c.path));
            match c.kind {
                ChangeKind::Deleted => remove_any(&dst),
                ChangeKind::Replaced => {
                    remove_any(&dst);
                    copy_tree(&src, &dst)?;
                }
                ChangeKind::Added | ChangeKind::Modified => copy_tree(&src, &dst)?,
            }
        }
        self.discard()?;
        Ok(changes)
    }
    /// Borra la capa sin tocar el repo.
    pub fn discard(self) -> Result<(), SandboxError> {
        fn fix(p: &Path) {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(rd) = std::fs::read_dir(p) {
                let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
                for e in rd.flatten() {
                    if e.path().is_dir() && !e.path().is_symlink() {
                        fix(&e.path());
                    }
                }
            } else {
                let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
            }
        }
        fix(&self.state);
        std::fs::remove_dir_all(&self.state)?;
        Ok(())
    }
    /// Diff unificado de un archivo de texto (< 1 MiB).
    pub fn diff(&self, path: &str) -> Result<Option<String>, SandboxError> {
        let new = self.state.join("upper").join(path);
        let old = self.root.join(path);
        let Ok(md) = std::fs::metadata(&new) else {
            return Ok(None);
        };
        if !md.is_file() || md.len() >= 1 << 20 {
            return Ok(None);
        }
        let Ok(b) = std::fs::read_to_string(&new) else {
            return Ok(None);
        };
        let a = std::fs::read_to_string(&old).unwrap_or_default();
        Ok(Some(
            TextDiff::from_lines(&a, &b)
                .unified_diff()
                .header(&format!("a/{path}"), &format!("b/{path}"))
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{Network, SandboxSpec, userns_available};
    use std::time::Duration;

    async fn sandbox(cmd: &str) -> Option<(tempfile::TempDir, Sandbox)> {
        if !userns_available() {
            eprintln!("sin namespaces de usuario: test omitido");
            return None;
        }
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(repo.join("d")).unwrap();
        std::fs::write(repo.join("a.txt"), "hola\n").unwrap();
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        std::fs::write(repo.join("d/x"), "x").unwrap();
        let sb = Sandbox::create(SandboxSpec {
            root: repo,
            state: Some(d.path().join("st")),
            network: Network::None,
            writable: vec![],
            timeout: Duration::from_secs(20),
        })
        .unwrap();
        sb.run(&["sh".into(), "-c".into(), cmd.into()], &[])
            .await
            .unwrap();
        Some((d, sb))
    }

    #[tokio::test]
    async fn apply_lleva_cambios() {
        let Some((d, sb)) = sandbox("echo mas >> a.txt; echo n > c.txt; rm b.txt; rm -r d").await
        else {
            return;
        };
        let c = sb.apply().unwrap();
        assert!(c.len() >= 3);
        let repo = d.path().join("repo");
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "hola\nmas\n"
        );
        assert!(
            repo.join("c.txt").exists()
                && !repo.join("b.txt").exists()
                && !repo.join("d/x").exists()
        );
        assert!(!d.path().join("st").exists());
    }
    #[tokio::test]
    async fn apply_enlace_simbolico() {
        let Some((d, sb)) = sandbox("ln -s a.txt enlace").await else {
            return;
        };
        sb.apply().unwrap();
        assert!(d.path().join("repo/enlace").is_symlink());
    }
    #[tokio::test]
    async fn discard_no_toca_repo() {
        let Some((d, sb)) = sandbox("echo x > nuevo; rm a.txt").await else {
            return;
        };
        sb.discard().unwrap();
        let repo = d.path().join("repo");
        assert!(repo.join("a.txt").exists() && !repo.join("nuevo").exists());
    }
    #[tokio::test]
    async fn diff_unificado() {
        let Some((_d, sb)) = sandbox("echo mas >> a.txt").await else {
            return;
        };
        let df = sb.diff("a.txt").unwrap().unwrap();
        assert!(df.contains("+mas") && df.contains("--- a/a.txt"));
    }
}
