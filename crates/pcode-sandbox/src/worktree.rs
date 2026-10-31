//! Nivel N0: copia de trabajo con `git worktree`. No aísla red ni disco (el proceso corre con los
//! permisos del usuario), pero sí separa los cambios del agente del repo hasta aplicarlos. Es lo
//! que se usa en Windows sin WSL y donde no hay namespaces de usuario.
use crate::layer::{Change, ChangeKind};
use crate::run::{RunStatus, Sandbox, SandboxError};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

static DEFAULT_N0: AtomicBool = AtomicBool::new(false);

/// Hace que `Sandbox::create` use N0 en este proceso (lo decide la CLI según la configuración).
pub fn set_default_n0(on: bool) {
    DEFAULT_N0.store(on, Ordering::Relaxed);
}

/// Si `Sandbox::create` usa N0.
pub fn default_n0() -> bool {
    DEFAULT_N0.load(Ordering::Relaxed)
}

/// Identidad para los commits internos del worktree (no se publican).
const IDENT: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "pcode"),
    ("GIT_AUTHOR_EMAIL", "pcode@localhost"),
    ("GIT_COMMITTER_NAME", "pcode"),
    ("GIT_COMMITTER_EMAIL", "pcode@localhost"),
];

/// En Windows, `canonicalize` devuelve rutas `\\?\C:\…` que git no entiende (las convierte en
/// `//?/C:/…` y falla al crear directorios): a git se le pasan sin ese prefijo.
pub(crate) fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, SandboxError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(plain(dir))
        .args(args)
        .envs(IDENT)
        .output()
        .map_err(|e| SandboxError::Invalid(format!("no se pudo ejecutar git: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(SandboxError::Invalid(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Directorio de la copia de trabajo dentro del estado.
pub(crate) fn wt_dir(state: &Path) -> PathBuf {
    state.join("wt")
}

/// Crea el worktree en `state/wt`, le copia los cambios sin commitear de `root` y fija la base.
pub(crate) fn create(root: &Path, state: &Path) -> Result<(), SandboxError> {
    let need = || {
        SandboxError::Invalid(
            "la sandbox N0 necesita un repositorio git con al menos un commit".into(),
        )
    };
    git(root, &["rev-parse", "--verify", "HEAD"]).map_err(|_| need())?;
    let wt = wt_dir(state);
    git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            &plain(&wt).to_string_lossy(),
            "HEAD",
        ],
    )?;
    // Lo que el usuario tiene sin commitear también lo ve el agente.
    let status = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )?;
    for entry in status.split('\0').filter(|e| e.len() > 3) {
        let (xy, path) = (&entry[..2], &entry[3..]);
        let (src, dst) = (root.join(path), wt.join(path));
        if xy.contains('D') && !src.exists() {
            let _ = std::fs::remove_file(&dst);
        } else if src.is_file() {
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&src, &dst)?;
        }
    }
    git(&wt, &["add", "-A"])?;
    git(
        &wt,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "--no-verify",
            "-m",
            "pcode: base",
        ],
    )?;
    Ok(())
}

/// Traduce `sh -c X` a la shell de la plataforma (en Windows, `cmd /C X`).
fn platform_argv(argv: &[String]) -> Vec<String> {
    if cfg!(windows) && argv.len() >= 3 && argv[0] == "sh" && argv[1] == "-c" {
        let mut v = vec!["cmd".to_string(), "/C".to_string(), argv[2].clone()];
        v.extend(argv[3..].iter().cloned());
        return v;
    }
    argv.to_vec()
}

impl Sandbox {
    pub(crate) async fn run_worktree(
        &self,
        argv: &[String],
        env: &[(String, String)],
        capture: bool,
    ) -> Result<(RunStatus, String), SandboxError> {
        let argv = platform_argv(argv);
        let (prog, args) = argv
            .split_first()
            .ok_or_else(|| SandboxError::Invalid("orden vacía".into()))?;
        let mut cmd = tokio::process::Command::new(prog);
        cmd.args(args)
            .current_dir(wt_dir(&self.state))
            .envs(env.iter().map(|(k, v)| (k, v)))
            .kill_on_drop(true);
        if capture {
            cmd.stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
        }
        let t0 = Instant::now();
        let child = cmd.spawn()?;
        match tokio::time::timeout(self.spec.timeout, child.wait_with_output()).await {
            Ok(out) => {
                let out = out?;
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                Ok((
                    RunStatus {
                        code: out.status.code().unwrap_or(-1),
                        timed_out: false,
                        millis: t0.elapsed().as_millis(),
                    },
                    text,
                ))
            }
            Err(_) => Ok((
                RunStatus {
                    code: 124,
                    timed_out: true,
                    millis: t0.elapsed().as_millis(),
                },
                String::new(),
            )),
        }
    }

    /// Cambios del agente respecto a la base (respeta `.gitignore`).
    pub(crate) fn worktree_changes(&self) -> Result<Vec<Change>, SandboxError> {
        let wt = wt_dir(&self.state);
        git(&wt, &["add", "-A"])?;
        let out = git(
            &wt,
            &[
                "diff",
                "--cached",
                "--name-status",
                "--no-renames",
                "-z",
                "HEAD",
            ],
        )?;
        let mut parts = out.split('\0').filter(|x| !x.is_empty());
        let mut v = vec![];
        while let (Some(st), Some(path)) = (parts.next(), parts.next()) {
            let kind = match st.chars().next() {
                Some('A') => ChangeKind::Added,
                Some('D') => ChangeKind::Deleted,
                Some('T') => ChangeKind::Replaced,
                _ => ChangeKind::Modified,
            };
            v.push(Change {
                path: path.to_string(),
                kind,
            });
        }
        Ok(v)
    }

    /// Quita el worktree del repo.
    pub(crate) fn remove_worktree(&self) {
        let wt = wt_dir(&self.state);
        let _ = git(
            &self.root,
            &[
                "worktree",
                "remove",
                "--force",
                &plain(&wt).to_string_lossy(),
            ],
        );
        let _ = git(&self.root, &["worktree", "prune"]);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rutas_para_git_sin_prefijo_de_windows() {
        assert_eq!(
            super::plain(std::path::Path::new(r"\\?\C:\Users\a\wt")),
            std::path::PathBuf::from(r"C:\Users\a\wt")
        );
        assert_eq!(
            super::plain(std::path::Path::new(r"\\?\UNC\srv\comp\wt")),
            std::path::PathBuf::from(r"\\srv\comp\wt")
        );
        assert_eq!(
            super::plain(std::path::Path::new("/tmp/x/wt")),
            std::path::PathBuf::from("/tmp/x/wt")
        );
    }

    use crate::layer::ChangeKind;
    use crate::run::{Network, Sandbox, SandboxSpec};
    use std::path::Path;
    use std::time::Duration;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let g = |a: &[&str]| super::git(d.path(), a).unwrap();
        g(&["init", "-q"]);
        std::fs::write(d.path().join("a.txt"), "a\n").unwrap();
        std::fs::write(d.path().join("b.txt"), "b\n").unwrap();
        std::fs::write(d.path().join(".gitignore"), "build/\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-qm", "inicio"]);
        d
    }

    fn spec(root: &Path) -> SandboxSpec {
        SandboxSpec {
            root: root.to_path_buf(),
            state: None,
            network: Network::None,
            writable: vec![],
            timeout: Duration::from_secs(30),
        }
    }

    fn sh(c: &str) -> Vec<String> {
        if cfg!(windows) {
            vec!["sh".into(), "-c".into(), c.replace("; ", " & ")]
        } else {
            vec!["sh".into(), "-c".into(), c.into()]
        }
    }

    #[tokio::test]
    async fn worktree_crea_aplica_y_descarta() {
        let d = repo();
        // Cambio sin commitear del usuario: el agente lo ve, pero no cuenta como suyo.
        std::fs::write(d.path().join("a.txt"), "a editado por el usuario\n").unwrap();
        let sb = Sandbox::create_n0(spec(d.path())).unwrap();
        let (st, out) = sb
            .run_capture(
                &sh(
                    "cat a.txt; echo nuevo > nuevo.txt; rm b.txt; mkdir -p build; echo x > build/o",
                ),
                &[],
            )
            .await
            .unwrap();
        assert_eq!(st.code, 0, "{out}");
        assert!(out.contains("editado por el usuario"), "{out}");
        assert!(
            !d.path().join("nuevo.txt").exists(),
            "el repo no cambia hasta aplicar"
        );
        let mut ch = sb.changes().unwrap();
        ch.sort_by(|a, b| a.path.cmp(&b.path));
        let got: Vec<(&str, ChangeKind)> = ch.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(
            got,
            vec![
                ("b.txt", ChangeKind::Deleted),
                ("nuevo.txt", ChangeKind::Added)
            ]
        );
        assert!(sb.diff("nuevo.txt").unwrap().unwrap().contains("+nuevo"));
        sb.apply().unwrap();
        assert!(d.path().join("nuevo.txt").exists() && !d.path().join("b.txt").exists());
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
            "a editado por el usuario\n"
        );
        let list = super::git(d.path(), &["worktree", "list"]).unwrap();
        assert_eq!(list.lines().count(), 1, "el worktree se elimina: {list}");
        let sb = Sandbox::create_n0(spec(d.path())).unwrap();
        sb.run(&sh("echo y > otro.txt"), &[]).await.unwrap();
        let state = sb.state_dir().to_path_buf();
        sb.discard().unwrap();
        assert!(!d.path().join("otro.txt").exists() && !state.exists());
    }

    #[test]
    fn worktree_sin_git_falla_con_mensaje() {
        let d = tempfile::tempdir().unwrap();
        let e = Sandbox::create_n0(spec(d.path()))
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("repositorio git"), "{e}");
    }
}
