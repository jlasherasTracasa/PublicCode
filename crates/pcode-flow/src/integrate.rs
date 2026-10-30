//! Integración: aplicar los cambios de una sandbox al repo y commitear solo esas rutas.
//!
//! Evita la clase de fallos por conflictos en `Cargo.toml` y `Cargo.lock`: no hay rama que rebasar; se aplica el resultado ya validado y, si cambió algún
//! manifiesto de Cargo, el `Cargo.lock` se regenera en el repo.
use pcode_sandbox::{ChangeKind, Sandbox};
use std::path::Path;
use std::process::Command;

/// Resultado de integrar.
#[derive(Debug, PartialEq)]
pub struct Integrated {
    /// Commit creado (None si no había cambios que commitear).
    pub commit: Option<String>,
    /// Rutas integradas.
    pub files: Vec<String>,
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=pcode", "-c", "user.email=pcode@local"])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// Aplica la sandbox al repo y hace un commit `<id>: <titulo>` con SOLO las rutas cambiadas.
pub fn integrate(sb: Sandbox, repo: &Path, id: &str, title: &str) -> Result<Integrated, String> {
    let changes = sb.apply().map_err(|e| e.to_string())?;
    let mut files: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();
    let touched_manifest = files.iter().any(|f| f.ends_with("Cargo.toml"));
    if touched_manifest && repo.join("Cargo.toml").exists() {
        // Regenera el lockfile en lugar de resolver conflictos de texto.
        let ok = Command::new("cargo")
            .args(["metadata", "--offline", "--format-version", "1"])
            .current_dir(repo)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok && repo.join("Cargo.lock").exists() && !files.iter().any(|f| f == "Cargo.lock") {
            files.push("Cargo.lock".into());
        }
    }
    for c in &changes {
        match c.kind {
            ChangeKind::Deleted | ChangeKind::Replaced => {
                let _ = git(
                    repo,
                    &[
                        "rm",
                        "-r",
                        "--cached",
                        "-q",
                        "--ignore-unmatch",
                        "--",
                        &c.path,
                    ],
                );
                if repo.join(&c.path).exists() {
                    let _ = git(repo, &["add", "--", &c.path]);
                }
            }
            _ => {
                if repo.join(&c.path).is_file() {
                    git(repo, &["add", "--", &c.path])?;
                }
            }
        }
    }
    if files.iter().any(|f| f == "Cargo.lock") {
        let _ = git(repo, &["add", "--", "Cargo.lock"]);
    }
    if git(repo, &["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(Integrated {
            commit: None,
            files,
        });
    }
    git(repo, &["commit", "-q", "-m", &format!("{id}: {title}")])?;
    Ok(Integrated {
        commit: Some(git(repo, &["rev-parse", "--short", "HEAD"])?),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcode_sandbox::{Network, SandboxSpec, userns_available};
    use std::time::Duration;

    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let f = d.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        }
        git(d.path(), &["init", "-q", "-b", "main"]).unwrap();
        git(d.path(), &["add", "-A"]).unwrap();
        git(d.path(), &["commit", "-q", "-m", "init"]).unwrap();
        d
    }
    async fn sb(repo: &Path, st: &Path, cmd: &str) -> Sandbox {
        let s = Sandbox::create(SandboxSpec {
            root: repo.into(),
            state: Some(st.into()),
            network: Network::None,
            writable: vec![],
            timeout: Duration::from_secs(30),
        })
        .unwrap();
        s.run(&["sh".into(), "-c".into(), cmd.into()], &[])
            .await
            .unwrap();
        s
    }

    #[tokio::test]
    async fn integra_solo_las_rutas_de_la_sandbox() {
        if !userns_available() {
            return;
        }
        let r = repo_with(&[("a.txt", "1")]);
        std::fs::write(r.path().join("ajeno.txt"), "no es mío").unwrap(); // trabajo de otro agente, sin commitear
        let st = tempfile::tempdir().unwrap();
        let s = sb(
            r.path(),
            &st.path().join("s"),
            "echo nuevo > b.txt; echo 2 > a.txt",
        )
        .await;
        let i = integrate(s, r.path(), "T1", "añade b").unwrap();
        assert!(i.commit.is_some());
        let files = git(r.path(), &["show", "--name-only", "--format=", "HEAD"]).unwrap();
        assert!(files.contains("a.txt") && files.contains("b.txt") && !files.contains("ajeno"));
        assert!(
            git(r.path(), &["log", "-1", "--format=%s"])
                .unwrap()
                .starts_with("T1:")
        );
        assert!(
            git(r.path(), &["status", "--short"])
                .unwrap()
                .contains("ajeno.txt")
        );
    }
    #[tokio::test]
    async fn sin_cambios_no_commitea() {
        if !userns_available() {
            return;
        }
        let r = repo_with(&[("a.txt", "1")]);
        let st = tempfile::tempdir().unwrap();
        let s = sb(r.path(), &st.path().join("s"), "true").await;
        assert_eq!(integrate(s, r.path(), "T", "x").unwrap().commit, None);
    }
    #[tokio::test]
    async fn borrado_se_commitea() {
        if !userns_available() {
            return;
        }
        let r = repo_with(&[("a.txt", "1"), ("b.txt", "2")]);
        let st = tempfile::tempdir().unwrap();
        let s = sb(r.path(), &st.path().join("s"), "rm b.txt").await;
        integrate(s, r.path(), "T", "borra").unwrap();
        assert!(!r.path().join("b.txt").exists());
        assert!(
            git(r.path(), &["ls-files"])
                .unwrap()
                .lines()
                .all(|l| l != "b.txt")
        );
    }
    #[tokio::test]
    async fn lockfile_se_regenera_al_cambiar_un_manifiesto() {
        if !userns_available() {
            return;
        }
        let r = repo_with(&[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
            ),
            (
                "crates/a/Cargo.toml",
                "[package]\nname=\"a\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            ),
            ("crates/a/src/lib.rs", ""),
        ]);
        let _ = Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(r.path())
            .output();
        git(r.path(), &["add", "-A"]).unwrap();
        let _ = git(r.path(), &["commit", "-q", "-m", "lock"]);
        let st = tempfile::tempdir().unwrap();
        let s = sb(r.path(), &st.path().join("s"), "mkdir -p crates/b/src && printf '[package]\\nname=\"b\"\\nversion=\"0.1.0\"\\nedition=\"2021\"\\n' > crates/b/Cargo.toml && : > crates/b/src/lib.rs").await;
        let i = integrate(s, r.path(), "T", "añade crate b").unwrap();
        assert!(i.files.iter().any(|f| f == "Cargo.lock"), "{:?}", i.files);
        assert!(
            std::fs::read_to_string(r.path().join("Cargo.lock"))
                .unwrap()
                .contains("name = \"b\"")
        );
    }
}
