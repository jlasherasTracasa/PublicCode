//! Tool `guion`: ejecuta varias órdenes de shell seguidas en una sola llamada (F4.12).
//!
//! Esta parte es lógica pura: la especificación de la tool, la lectura de los pasos, el código de
//! salida de `bash` y el resumen que se devuelve al modelo. La ejecución va en otra tarea.

use pcode_models::ToolSpec;
use serde_json::{Value, json};

/// Nombre de la tool.
pub const SCRIPT_TOOL: &str = "guion";
/// Pasos máximos de un guion.
pub const MAX_STEPS: usize = 20;

/// Resultado de un paso del guion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    /// Orden ejecutada.
    pub command: String,
    /// Código de salida.
    pub code: i32,
    /// Salida del paso.
    pub output: String,
    /// Motivo si la política denegó la orden.
    pub denied: Option<String>,
}

/// Especificación de la tool `guion`.
pub fn script_spec() -> ToolSpec {
    ToolSpec {
        name: SCRIPT_TOOL.into(),
        description: "Ejecuta varias órdenes de shell seguidas en una sola llamada y devuelve el \
            resultado de cada una. Úsala para secuencias fijas (formatear, lint, tests). Por defecto \
            se para en la primera que falla."
            .into(),
        parameters: json!({"type":"object","properties":{"pasos":{"type":"array","items":{"type":"string"}},"parar_en_error":{"type":"boolean"}},"required":["pasos"]}),
    }
}

/// Pasos y `parar_en_error` de los argumentos de una llamada.
pub fn parse_steps(args: &Value) -> Result<(Vec<String>, bool), String> {
    const FALTA: &str = "falta `pasos` (lista de órdenes)";
    let lista = args
        .get("pasos")
        .and_then(Value::as_array)
        .ok_or_else(|| FALTA.to_string())?;
    let pasos: Vec<String> = lista
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if pasos.is_empty() {
        return Err(FALTA.to_string());
    }
    if pasos.len() > MAX_STEPS {
        return Err(format!("demasiados pasos (máximo {MAX_STEPS})"));
    }
    let parar = args
        .get("parar_en_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok((pasos, parar))
}

/// Código de salida que informa la tool `bash` en su primera línea (`exit=N`), o -1.
pub fn exit_code(output: &str) -> i32 {
    output
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("exit="))
        .and_then(|n| n.trim().parse::<i32>().ok())
        .unwrap_or(-1)
}

/// Resumen del guion para el modelo.
pub fn render(results: &[StepResult], total: usize) -> String {
    let mut lineas: Vec<String> = Vec::new();
    let mut ok = 0usize;
    for (idx, r) in results.iter().enumerate() {
        let i = idx + 1;
        let cabecera = if let Some(motivo) = &r.denied {
            format!("[{i}/{total}] ⛔ {}: {motivo}", r.command)
        } else if r.code == 0 {
            ok += 1;
            format!("[{i}/{total}] ✓ {}", r.command)
        } else {
            format!("[{i}/{total}] ✗ {} (código {})", r.command, r.code)
        };
        lineas.push(cabecera);
        let salida = r.output.trim_end();
        if !salida.is_empty() {
            lineas.push(salida.to_string());
        }
    }
    if results.len() < total {
        lineas.push(format!("no ejecutados: {} pasos", total - results.len()));
    }
    lineas.push(format!("guion: {ok} de {total} pasos correctos"));
    lineas.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paso(command: &str, code: i32, output: &str) -> StepResult {
        StepResult {
            command: command.into(),
            code,
            output: output.into(),
            denied: None,
        }
    }

    #[test]
    fn formato_spec_con_pasos_obligatorios() {
        let spec = script_spec();
        assert_eq!(spec.name, "guion");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "pasos"));
    }

    #[test]
    fn formato_lee_pasos_y_descarta_vacios() {
        let args = json!({"pasos": [" cargo fmt --all ", "", "cargo test"]});
        assert_eq!(
            parse_steps(&args),
            Ok((
                vec!["cargo fmt --all".to_string(), "cargo test".to_string()],
                true
            ))
        );
        let args = json!({"pasos": ["ls"], "parar_en_error": false});
        assert!(!parse_steps(&args).unwrap().1);
    }

    #[test]
    fn formato_errores_de_pasos() {
        let falta = "falta `pasos` (lista de órdenes)".to_string();
        assert_eq!(parse_steps(&json!({})), Err(falta.clone()));
        assert_eq!(parse_steps(&json!({"pasos": "ls"})), Err(falta));
        let muchos: Vec<String> = (0..21).map(|i| format!("echo {i}")).collect();
        assert_eq!(
            parse_steps(&json!({"pasos": muchos})),
            Err("demasiados pasos (máximo 20)".to_string())
        );
    }

    #[test]
    fn formato_codigo_de_salida() {
        assert_eq!(exit_code("exit=101\n--- stdout ---\n"), 101);
        assert_eq!(exit_code("exit=0\n--- stdout ---\nhola"), 0);
        assert_eq!(exit_code("tiempo agotado tras 5 s"), -1);
        assert_eq!(exit_code(""), -1);
    }

    #[test]
    fn formato_render_del_ejemplo() {
        let results = vec![
            paso("cargo fmt --all", 0, ""),
            paso("cargo test", 101, "test a ... FAILED"),
        ];
        assert_eq!(
            render(&results, 3),
            "[1/3] ✓ cargo fmt --all\n[2/3] ✗ cargo test (código 101)\ntest a ... FAILED\nno ejecutados: 1 pasos\nguion: 1 de 3 pasos correctos"
        );
    }

    #[test]
    fn formato_render_denegado() {
        let results = vec![StepResult {
            command: "git push".into(),
            code: 0,
            output: String::new(),
            denied: Some("orden denegada: git push".into()),
        }];
        assert_eq!(
            render(&results, 1),
            "[1/1] ⛔ git push: orden denegada: git push\nguion: 0 de 1 pasos correctos"
        );
    }
}
