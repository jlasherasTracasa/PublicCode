//! Hook de parada: no hay «terminado» sin la aceptación en verde.
use std::path::Path;
use std::time::Duration;

/// Configuración del hook.
#[derive(Debug, Clone)]
pub struct StopHook {
    /// Orden de aceptación.
    pub command: String,
    /// Rondas máximas.
    pub max_rounds: u32,
    /// Tiempo máximo por ejecución.
    pub timeout: Duration,
    /// Restricciones que el mensaje final debe confirmar con `- [x] item`.
    pub checklist: Vec<String>,
}

/// Ejecuta la aceptación; devuelve (éxito, salida).
pub async fn run_acceptance(command: &str, root: &Path, timeout: Duration) -> (bool, String) {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let Ok(child) = child else {
        return (false, "no se pudo lanzar la aceptación".into());
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Err(_) => (
            false,
            format!("tiempo agotado tras {} s", timeout.as_secs()),
        ),
        Ok(Err(e)) => (false, format!("error de E/S: {e}")),
        Ok(Ok(o)) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn run_acceptance_ok_y_fallo() {
        let d = tempfile::tempdir().unwrap();
        assert!(
            run_acceptance("true", d.path(), Duration::from_secs(5))
                .await
                .0
        );
        assert!(
            !run_acceptance("echo mal; false", d.path(), Duration::from_secs(5))
                .await
                .0
        );
    }
    #[tokio::test]
    async fn run_acceptance_timeout() {
        let d = tempfile::tempdir().unwrap();
        let (ok, out) = run_acceptance("sleep 5", d.path(), Duration::from_millis(200)).await;
        assert!(!ok && out.contains("tiempo agotado"));
    }
}
