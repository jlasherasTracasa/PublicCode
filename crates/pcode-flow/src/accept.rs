//! Aceptación en dos etapas: rápida (N1) y completa (contenedor o N1).
use std::path::{Path, PathBuf};

fn shell_argv(cmd: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd".into(), "/C".into(), cmd.into()]
    } else {
        vec!["sh".into(), "-c".into(), cmd.into()]
    }
}
use std::time::Duration;

/// Especificación de la aceptación.
pub struct AcceptSpec {
    /// Orden rápida opcional.
    pub quick: Option<String>,
    /// Orden completa.
    pub full: String,
    /// Imagen de contenedor para la etapa completa.
    pub image: Option<String>,
    /// Tiempo máximo por etapa.
    pub timeout: Duration,
}

/// Resultado.
#[derive(Debug)]
pub struct AcceptOutcome {
    /// Si pasó.
    pub ok: bool,
    /// Etapa en la que terminó.
    pub stage: &'static str,
    /// Líneas clave.
    pub key_lines: Vec<String>,
    /// Dónde está la salida completa.
    pub output_path: PathBuf,
}

/// Orden de docker para la etapa completa.
pub fn docker_argv(image: &str, workdir: &Path, cmd: &str) -> Vec<String> {
    vec![
        "docker".into(),
        "run".into(),
        "--rm".into(),
        "-v".into(),
        format!("{}:/work", workdir.display()),
        "-w".into(),
        "/work".into(),
        image.into(),
        "sh".into(),
        "-c".into(),
        cmd.into(),
    ]
}

/// Líneas que importan (errores, fallos, resúmenes de tests).
pub fn key_lines(output: &str, max: usize) -> Vec<String> {
    const KEYS: &[&str] = &["error", "FAILED", "panicked", "test result", "TESTS DONE"];
    output
        .lines()
        .filter(|l| KEYS.iter().any(|k| l.contains(k)))
        .take(max)
        .map(String::from)
        .collect()
}

async fn sh(argv: &[String], dir: &Path, timeout: Duration) -> (bool, String) {
    let Some((prog, args)) = argv.split_first() else {
        return (false, "orden vacía".into());
    };
    let child = tokio::process::Command::new(prog)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let Ok(child) = child else {
        return (false, format!("no se pudo lanzar {prog}"));
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Err(_) => (
            false,
            format!("tiempo agotado tras {} s", timeout.as_secs()),
        ),
        Ok(Err(e)) => (false, e.to_string()),
        Ok(Ok(o)) => (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        ),
    }
}

/// Ejecuta la aceptación sobre `tree` y guarda la salida en `report`.
pub async fn run_accept(spec: &AcceptSpec, tree: &Path, report: &Path) -> AcceptOutcome {
    let mut all = String::new();
    let finish = |ok: bool, stage: &'static str, all: &str| {
        if let Some(p) = report.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = std::fs::write(report, all);
        AcceptOutcome {
            ok,
            stage,
            key_lines: key_lines(all, 20),
            output_path: report.to_path_buf(),
        }
    };
    if let Some(q) = &spec.quick {
        let (ok, out) = sh(&shell_argv(q), tree, spec.timeout).await;
        all.push_str(&format!("=== rápida ===\n{out}\n"));
        if !ok {
            return finish(false, "rápida", &all);
        }
    }
    let argv = match &spec.image {
        Some(img) => docker_argv(img, tree, &spec.full),
        None => shell_argv(&spec.full),
    };
    let (ok, out) = sh(&argv, tree, spec.timeout).await;
    all.push_str(&format!("=== completa ===\n{out}\n"));
    finish(ok, "completa", &all)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn docker_argv_exacto() {
        let v = docker_argv("img:1", Path::new("/t"), "cargo test");
        assert_eq!(
            v.join(" "),
            "docker run --rm -v /t:/work -w /work img:1 sh -c cargo test"
        );
    }
    #[test]
    fn key_lines_extrae() {
        let k = key_lines(
            "ok\nerror[E1]: mal\nnada\ntest result: FAILED. 1 failed\n",
            10,
        );
        assert_eq!(k.len(), 2);
    }
    #[tokio::test]
    async fn rapida_falla_no_ejecuta_completa() {
        let d = tempfile::tempdir().unwrap();
        let spec = AcceptSpec {
            quick: Some("false".into()),
            full: "touch completa".into(),
            image: None,
            timeout: Duration::from_secs(5),
        };
        let o = run_accept(&spec, d.path(), &d.path().join("r/out.txt")).await;
        assert!(!o.ok && o.stage == "rápida");
        assert!(!d.path().join("completa").exists());
    }
    #[tokio::test]
    async fn completa_sin_imagen_usa_shell() {
        let d = tempfile::tempdir().unwrap();
        let spec = AcceptSpec {
            quick: Some("true".into()),
            full: "echo test result: ok".into(),
            image: None,
            timeout: Duration::from_secs(5),
        };
        let o = run_accept(&spec, d.path(), &d.path().join("r/out.txt")).await;
        assert!(o.ok && o.stage == "completa" && o.key_lines.len() == 1);
        assert!(
            std::fs::read_to_string(o.output_path)
                .unwrap()
                .contains("=== completa ===")
        );
    }
}
