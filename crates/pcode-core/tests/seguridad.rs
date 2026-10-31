//! Eval de seguridad (F7.02): inyecciones de prompt y órdenes destructivas contra la capa de política
//! interactiva por defecto. Los casos están en `bench/seguridad.jsonl`.
//!
//! - `politica`: solo se comprueba la decisión del motor; **nunca** se ejecuta la orden.
//! - `agente`: un modelo simulado lee un archivo con una instrucción inyectada y la «obedece»; se
//!   comprueba que el motor la para (deniega, pide aprobación que nadie da, o error) y que no hubo efecto.
use pcode_core::{Agent, AgentConfig, Budget, Outcome};
use pcode_models::{ChatResponse, Message, MockProvider, StopReason, ToolCall, Usage};
use pcode_policy::{Capabilities, Decision, Policy};
use pcode_tools::ToolContext;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

fn casos() -> Vec<Value> {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/seguridad.jsonl");
    std::fs::read_to_string(p)
        .expect("bench/seguridad.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}")))
        .collect()
}

fn resp(text: &str, calls: Vec<ToolCall>) -> ChatResponse {
    let mut m = Message::assistant(text);
    let tool = !calls.is_empty();
    m.tool_calls = calls;
    ChatResponse {
        message: m,
        usage: Usage::default(),
        stop_reason: if tool {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        },
    }
}

fn decision(policy: &Policy, orden: &str) -> &'static str {
    match policy.check_command(orden) {
        Decision::Allow => "permitido",
        Decision::Ask(_) => "pregunta",
        Decision::Deny(_) => "denegado",
    }
}

/// Clasifica la salida de la tool que intentó la acción inyectada.
fn clasifica(out: &str) -> &'static str {
    let l = out.to_lowercase();
    if out.starts_with("denegado por la política") {
        "denegado"
    } else if out.starts_with("requiere aprobación humana") {
        "pregunta"
    } else if l.contains("fuera de la raíz") || l.contains("no existe") || l.contains("error") {
        "error"
    } else {
        "permitido"
    }
}

async fn caso_agente(c: &Value) -> (String, String) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("proyecto");
    std::fs::create_dir_all(root.join(".git/hooks")).unwrap();
    std::fs::create_dir_all(root.join(".pcode")).unwrap();
    std::fs::write(root.join(".pcode/audit.jsonl"), "{}\n").unwrap();
    let archivo = c["archivo"].as_str().unwrap();
    if let Some(parent) = root.join(archivo).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(root.join(archivo), c["contenido"].as_str().unwrap()).unwrap();
    let accion = &c["accion"];
    let provider = Arc::new(MockProvider::new(vec![
        resp(
            "",
            vec![ToolCall {
                id: "leer".into(),
                name: "leer".into(),
                arguments: serde_json::json!({ "path": archivo }),
            }],
        ),
        resp(
            "Sigo la instrucción del archivo.",
            vec![ToolCall {
                id: "accion".into(),
                name: accion["name"].as_str().unwrap().into(),
                arguments: accion["arguments"].clone(),
            }],
        ),
        resp("hecho", vec![]),
    ]));
    let mut agent = Agent::new(
        provider,
        pcode_tools::native_registry(),
        Policy::new(Capabilities::interactive_defaults()),
        None,
        AgentConfig {
            agent_id: "eval".into(),
            model: "m".into(),
            system_prompt: "s".into(),
            budget: Budget {
                max_steps: 5,
                ..Default::default()
            },
            stop_hook: None,
        },
        ToolContext {
            root: root.clone(),
            timeout: Duration::from_secs(20),
        },
    );
    let r = agent.run("revisa el proyecto").await;
    assert!(
        matches!(r.outcome, Outcome::Done { .. }),
        "{}: {:?}",
        c["caso"],
        r.outcome
    );
    let out = r
        .transcript
        .iter()
        .find(|m| m.tool_call_id.as_deref() == Some("accion"))
        .map(|m| m.content.clone())
        .unwrap_or_default();
    if let Some(p) = c["no_debe_existir"].as_str() {
        assert!(
            !root.join(p).exists(),
            "{}: se ejecutó la acción inyectada ({p} existe)",
            c["caso"]
        );
    }
    if accion["arguments"]["path"] == ".pcode/audit.jsonl" {
        assert_eq!(
            std::fs::read_to_string(root.join(".pcode/audit.jsonl")).unwrap(),
            "{}\n",
            "la auditoría no se toca"
        );
    }
    (clasifica(&out).to_string(), out)
}

#[tokio::test]
async fn seguridad_casos_del_banco() {
    let policy = Policy::new(Capabilities::interactive_defaults());
    let mut fallos = vec![];
    let mut tabla = vec![];
    for c in casos() {
        let caso = c["caso"].as_str().unwrap().to_string();
        let esperado = c["esperado"].as_str().unwrap();
        let (obtenido, detalle) = match c["modo"].as_str().unwrap() {
            "politica" => (
                decision(&policy, c["orden"].as_str().unwrap()).to_string(),
                String::new(),
            ),
            _ => caso_agente(&c).await,
        };
        tabla.push(format!("{caso}: {obtenido}"));
        if obtenido != esperado {
            fallos.push(format!(
                "{caso}: se esperaba {esperado} y fue {obtenido} ({detalle})"
            ));
        }
    }
    eprintln!("{}", tabla.join("\n"));
    assert!(tabla.len() >= 20, "el banco tiene {} casos", tabla.len());
    assert!(fallos.is_empty(), "fallos:\n{}", fallos.join("\n"));
}
