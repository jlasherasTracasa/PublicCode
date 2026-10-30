//! Clasificador de ejecuciones de GitHub Actions (sin tareas fantasma).
use serde_json::Value;

/// Veredicto.
#[derive(Debug, PartialEq)]
pub enum CiVerdict {
    /// Verde.
    Green,
    /// Roja de verdad: (job, paso).
    Red { failing: Vec<(String, String)> },
    /// Cancelada o fallo sin ningún paso fallido.
    NotActionable(String),
    /// Fallo de infraestructura.
    Infra(String),
    /// En curso.
    InProgress,
}

/// Clasifica un run con la respuesta de `/actions/runs/{id}/jobs`.
pub fn classify(run: &Value, jobs: &Value) -> CiVerdict {
    if run.get("status").and_then(Value::as_str) != Some("completed") {
        return CiVerdict::InProgress;
    }
    match run.get("conclusion").and_then(Value::as_str) {
        Some("success") => CiVerdict::Green,
        Some("cancelled") => CiVerdict::NotActionable("run cancelado".into()),
        Some("startup_failure") => CiVerdict::Infra("startup_failure".into()),
        Some("failure") | Some("timed_out") => {
            let mut failing = vec![];
            let mut infra = false;
            for j in jobs
                .get("jobs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = j.get("name").and_then(Value::as_str).unwrap_or("?");
                if j.get("conclusion").and_then(Value::as_str) == Some("startup_failure") {
                    infra = true;
                }
                for s in j
                    .get("steps")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if s.get("conclusion").and_then(Value::as_str) == Some("failure") {
                        failing.push((
                            name.to_string(),
                            s.get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("?")
                                .to_string(),
                        ));
                    }
                }
            }
            if !failing.is_empty() {
                CiVerdict::Red { failing }
            } else if infra {
                CiVerdict::Infra("job sin arrancar".into())
            } else {
                CiVerdict::NotActionable("failure sin ningún paso fallido".into())
            }
        }
        other => CiVerdict::NotActionable(format!("conclusión {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn run(status: &str, c: Option<&str>) -> Value {
        json!({"status": status, "conclusion": c})
    }
    #[test]
    fn ci_verde() {
        assert_eq!(
            classify(&run("completed", Some("success")), &json!({})),
            CiVerdict::Green
        );
    }
    #[test]
    fn ci_roja_con_paso() {
        let jobs = json!({"jobs":[{"name":"rust","conclusion":"failure","steps":[{"name":"clippy","conclusion":"failure"},{"name":"test","conclusion":"skipped"}]}]});
        assert_eq!(
            classify(&run("completed", Some("failure")), &jobs),
            CiVerdict::Red {
                failing: vec![("rust".into(), "clippy".into())]
            }
        );
    }
    #[test]
    fn ci_cancelada_no_accionable() {
        assert!(matches!(
            classify(&run("completed", Some("cancelled")), &json!({})),
            CiVerdict::NotActionable(_)
        ));
    }
    #[test]
    fn ci_failure_sin_pasos_no_accionable() {
        let jobs = json!({"jobs":[{"name":"repo","conclusion":"failure","steps":[{"name":"x","conclusion":"success"}]}]});
        assert!(matches!(
            classify(&run("completed", Some("failure")), &jobs),
            CiVerdict::NotActionable(_)
        ));
    }
    #[test]
    fn ci_startup_failure_infra() {
        assert!(matches!(
            classify(&run("completed", Some("startup_failure")), &json!({})),
            CiVerdict::Infra(_)
        ));
    }
    #[test]
    fn ci_en_curso() {
        assert_eq!(
            classify(&run("in_progress", None), &json!({})),
            CiVerdict::InProgress
        );
    }
}
