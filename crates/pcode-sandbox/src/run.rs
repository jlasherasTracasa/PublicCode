//! Ejecutar una orden en N1.
use crate::layer::{Change, filter_ignored, read_changes};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const PREPARE: &str = include_str!("prepare.sh");

/// Política de red.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// Sin red.
    None,
    /// Red abierta.
    Open,
    /// Red filtrada (proxy de salida; fase F2.19–F2.20).
    Filtered,
}

/// Especificación de una sandbox.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    /// Directorio cubierto por el overlay.
    pub root: PathBuf,
    /// Dónde guardar la capa (tmp nuevo si es `None`).
    pub state: Option<PathBuf>,
    /// Red.
    pub network: Network,
    /// Rutas extra escribibles.
    pub writable: Vec<PathBuf>,
    /// Tiempo máximo.
    pub timeout: Duration,
}

/// Resultado de una ejecución.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStatus {
    /// Código de salida (124 si venció el tiempo).
    pub code: i32,
    /// Si venció el tiempo.
    pub timed_out: bool,
    /// Milisegundos.
    pub millis: u128,
}

/// Errores.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// E/S.
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    /// Sin namespaces de usuario.
    #[error("namespaces de usuario no disponibles")]
    NoUserns,
    /// Petición inválida.
    #[error("{0}")]
    Invalid(String),
}

/// Directorio temporal absoluto (`TMPDIR` si es válido; si no, `/tmp`).
pub fn tmp_base() -> PathBuf {
    if cfg!(windows) {
        return std::env::temp_dir();
    }
    match std::env::var("TMPDIR") {
        Ok(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => PathBuf::from("/tmp"),
    }
}

/// ¿Se pueden crear namespaces de usuario?
pub fn userns_available() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        std::process::Command::new("unshare")
            .args(["--user", "--map-root-user", "--mount", "true"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Una sandbox con su capa.
pub struct Sandbox {
    pub(crate) spec: SandboxSpec,
    pub(crate) root: PathBuf,
    pub(crate) state: PathBuf,
    /// Copia de trabajo con `git worktree` (N0) en lugar de overlay (N1).
    pub(crate) n0: bool,
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Sandbox {
    /// Crea la sandbox con el nivel por defecto del proceso: N1 (overlay) o, si la CLI lo pidió
    /// con [`crate::set_default_n0`], N0 (copia de trabajo con `git worktree`).
    pub fn create(spec: SandboxSpec) -> Result<Sandbox, SandboxError> {
        if crate::worktree::default_n0() {
            Self::create_n0(spec)
        } else {
            Self::create_n1(spec)
        }
    }

    /// Sandbox N0: copia de trabajo con `git worktree` (sin aislamiento de red ni disco).
    pub fn create_n0(spec: SandboxSpec) -> Result<Sandbox, SandboxError> {
        let mut sb = Self::create_n1(spec)?;
        sb.n0 = true;
        std::fs::write(sb.state.join("nivel"), "N0")?;
        if let Err(e) = crate::worktree::create(&sb.root, &sb.state) {
            let _ = std::fs::remove_dir_all(&sb.state);
            return Err(e);
        }
        Ok(sb)
    }

    /// Sandbox N1: overlay sobre el repo en namespaces de usuario.
    pub fn create_n1(spec: SandboxSpec) -> Result<Sandbox, SandboxError> {
        let root = std::fs::canonicalize(&spec.root)?;
        let state = match &spec.state {
            Some(s) => {
                std::fs::create_dir_all(s)?;
                std::fs::canonicalize(s)?
            }
            None => {
                let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let p = tmp_base().join(format!("pcode-sbx-{}-{n}", std::process::id()));
                std::fs::create_dir_all(&p)?;
                p
            }
        };
        if state == root || state.starts_with(&root) {
            return Err(SandboxError::Invalid(format!(
                "el estado ({}) no puede estar dentro del directorio de trabajo ({})",
                state.display(),
                root.display()
            )));
        }
        for s in ["upper", "work"] {
            std::fs::create_dir_all(state.join(s))?;
        }
        std::fs::write(state.join("root"), root.to_string_lossy().as_bytes())?;
        Ok(Sandbox {
            spec,
            root,
            state,
            n0: false,
        })
    }
    /// Reabre una sandbox ya creada a partir de su directorio de estado.
    pub fn open_existing(state: &Path) -> Result<Sandbox, SandboxError> {
        let state = std::fs::canonicalize(state)?;
        let root = std::fs::read_to_string(state.join("root"))
            .map_err(|_| SandboxError::Invalid(format!("{} no es una sandbox", state.display())))?;
        let root = PathBuf::from(root.trim());
        let spec = SandboxSpec {
            root: root.clone(),
            state: Some(state.clone()),
            network: Network::None,
            writable: vec![],
            timeout: Duration::from_secs(3600),
        };
        let n0 = std::fs::read_to_string(state.join("nivel")).is_ok_and(|s| s.trim() == "N0");
        Ok(Sandbox {
            spec,
            root,
            state,
            n0,
        })
    }
    /// Nivel: `N0` o `N1`.
    pub fn level(&self) -> &'static str {
        if self.n0 { "N0" } else { "N1" }
    }
    /// Directorio de estado.
    pub fn state_dir(&self) -> &Path {
        &self.state
    }
    /// Directorio cubierto.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Ejecuta `argv` dentro de la sandbox.
    pub async fn run(
        &self,
        argv: &[String],
        env: &[(String, String)],
    ) -> Result<RunStatus, SandboxError> {
        self.run_with(argv, env, &[]).await
    }

    /// Ejecuta `argv` y devuelve también su salida (stdout y stderr juntos), sin ensuciar la capa del repo.
    pub async fn run_capture(
        &self,
        argv: &[String],
        env: &[(String, String)],
    ) -> Result<(RunStatus, String), SandboxError> {
        if self.n0 {
            return self.run_worktree(argv, env, true).await;
        }
        let out = self.state.join("out");
        std::fs::create_dir_all(&out)?;
        let file = out.join("salida.txt");
        let _ = std::fs::remove_file(&file);
        let mut wrapped = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("exec \"$@\" > {} 2>&1", file.display()),
            "capture".to_string(),
        ];
        wrapped.extend(argv.iter().cloned());
        let st = self
            .run_with(&wrapped, env, std::slice::from_ref(&out))
            .await?;
        let text = std::fs::read(&file)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        Ok((st, text))
    }

    async fn run_with(
        &self,
        argv: &[String],
        env: &[(String, String)],
        extra: &[PathBuf],
    ) -> Result<RunStatus, SandboxError> {
        if self.n0 {
            return Ok(self.run_worktree(argv, env, false).await?.0);
        }
        if !userns_available() {
            return Err(SandboxError::NoUserns);
        }
        let under_tmp = |p: &Path| p.starts_with("/tmp");
        let mut writable: Vec<PathBuf> = self
            .spec
            .writable
            .iter()
            .chain(extra.iter())
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .collect();
        let tmp_new = !(under_tmp(&self.root)
            || under_tmp(&self.state)
            || writable.iter().any(|p| under_tmp(p)));
        let tmp_dir = self.state.join("tmp");
        if !tmp_new {
            std::fs::create_dir_all(&tmp_dir)?;
            writable.push(tmp_dir.clone());
        }
        let mut cmd = tokio::process::Command::new("unshare");
        cmd.args([
            "--user",
            "--map-root-user",
            "--mount",
            "--pid",
            "--fork",
            "--kill-child",
            "--mount-proc",
        ]);
        if self.spec.network == Network::None {
            cmd.arg("--net");
        }
        cmd.args(["sh", "-c", PREPARE, "pcode-sandbox"]).args(argv);
        cmd.env("MS_DIR", &self.root)
            .env("MS_UP", self.state.join("upper"))
            .env("MS_WORK", self.state.join("work"))
            .env(
                "ESCRIBIBLES",
                writable
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .env("MS_TMP_NUEVO", if tmp_new { "1" } else { "0" })
            .env("MS_TMP", &tmp_dir)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "safe.directory")
            .env("GIT_CONFIG_VALUE_0", "*");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.kill_on_drop(true);
        let t0 = Instant::now();
        let mut child = cmd.spawn()?;
        let (code, timed_out) = match tokio::time::timeout(self.spec.timeout, child.wait()).await {
            Ok(st) => (st?.code().unwrap_or(-1), false),
            Err(_) => {
                let _ = child.kill().await;
                (124, true)
            }
        };
        Ok(RunStatus {
            code,
            timed_out,
            millis: t0.elapsed().as_millis(),
        })
    }
    /// Cambios de la capa (sin lo que git ignora).
    pub fn changes(&self) -> Result<Vec<Change>, SandboxError> {
        if self.n0 {
            return self.worktree_changes();
        }
        let c = read_changes(&self.state.join("upper"), &self.root)?;
        Ok(filter_ignored(&self.root, c)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn spec(root: &Path, state: &Path) -> SandboxSpec {
        SandboxSpec {
            root: root.to_path_buf(),
            state: Some(state.to_path_buf()),
            network: Network::None,
            writable: vec![],
            timeout: Duration::from_secs(20),
        }
    }
    fn sh(c: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), c.into()]
    }
    macro_rules! need_userns {
        () => {
            if !userns_available() {
                eprintln!("sin namespaces de usuario: test omitido");
                return;
            }
        };
    }

    #[tokio::test]
    async fn run_escribe_en_capa_no_en_repo() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("a.txt"), "hola").unwrap();
        let sb = Sandbox::create(spec(&repo, &d.path().join("st"))).unwrap();
        let st = sb
            .run(&sh("echo cambio >> a.txt; echo n > b.txt"), &[])
            .await
            .unwrap();
        assert_eq!(st.code, 0);
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "hola");
        assert!(!repo.join("b.txt").exists());
        let c = sb.changes().unwrap();
        assert_eq!(c.len(), 2);
    }
    #[tokio::test]
    async fn run_sin_red() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sb = Sandbox::create(spec(&repo, &d.path().join("st"))).unwrap();
        let cmd = "python3 -c \"import socket,sys; s=socket.socket(); s.settimeout(2); sys.exit(0 if s.connect_ex(('1.1.1.1',53)) else 7)\"";
        assert_eq!(sb.run(&sh(cmd), &[]).await.unwrap().code, 0);
    }
    #[tokio::test]
    async fn run_fuera_solo_lectura() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        let fuera = d.path().join("fuera");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&fuera).unwrap();
        let sb = Sandbox::create(spec(&repo, &d.path().join("st"))).unwrap();
        let st = sb
            .run(&sh(&format!("touch {}/z", fuera.display())), &[])
            .await
            .unwrap();
        assert_ne!(st.code, 0);
        assert!(!fuera.join("z").exists());
    }
    #[tokio::test]
    async fn run_timeout_124() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let mut s = spec(&repo, &d.path().join("st"));
        s.timeout = Duration::from_millis(800);
        let st = Sandbox::create(s)
            .unwrap()
            .run(&sh("sleep 20"), &[])
            .await
            .unwrap();
        assert!(st.timed_out && st.code == 124);
    }
    #[tokio::test]
    async fn run_capture_devuelve_salida_sin_ensuciar_la_capa() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sb = Sandbox::create(spec(&repo, &d.path().join("st"))).unwrap();
        let (st, out) = sb
            .run_capture(&sh("echo hola; echo err >&2"), &[])
            .await
            .unwrap();
        assert_eq!(st.code, 0);
        assert!(out.contains("hola") && out.contains("err"));
        assert!(sb.changes().unwrap().is_empty());
    }
    #[tokio::test]
    async fn open_existing_recupera_la_capa() {
        need_userns!();
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let st = d.path().join("st");
        let sb = Sandbox::create(spec(&repo, &st)).unwrap();
        sb.run(&sh("echo x > n"), &[]).await.unwrap();
        let again = Sandbox::open_existing(&st).unwrap();
        assert_eq!(again.changes().unwrap().len(), 1);
        assert!(Sandbox::open_existing(d.path()).is_err());
    }
    #[test]
    fn run_estado_dentro_de_root_es_error() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(Sandbox::create(spec(&repo, &repo.join("st"))).is_err());
    }
    #[tokio::test]
    async fn run_repo_bajo_tmp() {
        need_userns!();
        let d = tempfile::tempdir_in("/tmp").unwrap();
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let sb = Sandbox::create(spec(&repo, &d.path().join("st"))).unwrap();
        assert_eq!(sb.run(&sh("echo ok > x"), &[]).await.unwrap().code, 0);
        assert!(!repo.join("x").exists());
    }
}
