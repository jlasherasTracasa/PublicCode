//! Puntos de control por turno (F8.15): deshacer, rehacer, límite y integración con el agente.
use pcode_core::{Agent, AgentConfig, Budget, Checkpoints, checkpoint::MAX_TURNS};
use pcode_models::{ChatResponse, Message, MockProvider, StopReason, ToolCall, Usage};
use pcode_policy::{Capabilities, Policy};
use pcode_tools::ToolContext;
use std::sync::Arc;
use std::time::Duration;

fn leer(root: &std::path::Path, p: &str) -> Option<String> {
    std::fs::read_to_string(root.join(p)).ok()
}

#[test]
fn deshacer_restaura_y_borra_creados() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    std::fs::write(root.join("a.txt"), "uno\ndos\n").unwrap();
    let mut cp = Checkpoints::open(root);
    cp.begin_turn("cambia a y crea b");
    cp.save("a.txt").unwrap();
    std::fs::write(root.join("a.txt"), "uno\nDOS\ntres\n").unwrap();
    cp.save("nuevo/b.txt").unwrap();
    std::fs::create_dir_all(root.join("nuevo")).unwrap();
    std::fs::write(root.join("nuevo/b.txt"), "b\n").unwrap();
    // una segunda escritura del mismo archivo en el turno no pisa el estado guardado
    cp.save("a.txt").unwrap();
    std::fs::write(root.join("a.txt"), "otra\n").unwrap();

    let cambios = cp.current_changes();
    assert_eq!(cambios.len(), 2);
    assert_eq!(cambios[0].path, "a.txt");
    assert!(cambios[1].created);

    let t = cp.undo().unwrap().expect("hay turno");
    assert_eq!(t.label, "cambia a y crea b");
    assert_eq!(t.files, vec!["a.txt", "nuevo/b.txt"]);
    assert_eq!(leer(root, "a.txt").as_deref(), Some("uno\ndos\n"));
    assert!(!root.join("nuevo/b.txt").exists(), "lo creado se borra");
    assert!(cp.undo().unwrap().is_none(), "no queda nada que deshacer");
}

#[test]
fn rehacer_vuelve_a_aplicar() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    std::fs::write(root.join("a.txt"), "v0\n").unwrap();
    let mut cp = Checkpoints::open(root);
    for v in ["v1", "v2"] {
        cp.begin_turn(v);
        cp.save("a.txt").unwrap();
        std::fs::write(root.join("a.txt"), format!("{v}\n")).unwrap();
    }
    cp.undo().unwrap();
    cp.undo().unwrap();
    assert_eq!(leer(root, "a.txt").as_deref(), Some("v0\n"));
    let r = cp.redo().unwrap().unwrap();
    assert_eq!(r.label, "v1", "rehace en orden");
    assert_eq!(leer(root, "a.txt").as_deref(), Some("v1\n"));
    cp.redo().unwrap();
    assert_eq!(leer(root, "a.txt").as_deref(), Some("v2\n"));
    assert!(cp.redo().unwrap().is_none());

    // un turno nuevo tras deshacer descarta lo que se podía rehacer
    cp.undo().unwrap();
    let mut cp2 = Checkpoints::open(root);
    cp2.begin_turn("otro camino");
    assert!(cp2.redo().unwrap().is_none());
    let l = cp2.list();
    assert_eq!(l.len(), 1);
    assert!(!l[0].undone);
}

#[test]
fn limite_de_50_turnos() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let mut cp = Checkpoints::open(root);
    for i in 0..(MAX_TURNS + 5) {
        cp.begin_turn(&format!("turno {i}"));
        cp.save("a.txt").unwrap();
        std::fs::write(root.join("a.txt"), i.to_string()).unwrap();
    }
    let l = cp.list();
    assert_eq!(l.len(), MAX_TURNS);
    assert_eq!(
        l[0].label,
        format!("turno {}", MAX_TURNS + 4),
        "el más reciente primero"
    );
    assert_eq!(
        l.last().unwrap().label,
        "turno 5",
        "se borran los más antiguos"
    );
}

#[test]
fn rutas_fuera_de_la_raiz_o_en_pcode_se_ignoran() {
    let d = tempfile::tempdir().unwrap();
    let mut cp = Checkpoints::open(d.path());
    cp.begin_turn("x");
    cp.save("../fuera.txt").unwrap();
    cp.save(".pcode/audit.jsonl").unwrap();
    assert!(cp.list().is_empty());
}

fn resp(calls: Vec<ToolCall>) -> ChatResponse {
    let mut m = Message::assistant(if calls.is_empty() { "hecho" } else { "" });
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

#[tokio::test]
async fn el_agente_guarda_antes_de_escribir() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    let provider = Arc::new(MockProvider::new(vec![
        resp(vec![ToolCall {
            id: "1".into(),
            name: "escribir".into(),
            arguments: serde_json::json!({"path": "main.rs", "content": "fn main() {\n    println!(\"hola\");\n}\n"}),
        }]),
        resp(vec![]),
    ]));
    let mut agent = Agent::new(
        provider,
        pcode_tools::native_registry(),
        Policy::new(Capabilities::interactive_defaults()),
        None,
        AgentConfig {
            agent_id: "t".into(),
            model: "m".into(),
            system_prompt: "s".into(),
            budget: Budget::default(),
            stop_hook: None,
        },
        ToolContext {
            root: root.clone(),
            timeout: Duration::from_secs(10),
        },
    )
    .with_checkpoints();
    agent.run("di hola").await;
    let c = agent.turn_changes();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].added, c[0].removed), (3, 1));
    let mut cp = Checkpoints::open(&root);
    cp.undo().unwrap().unwrap();
    assert_eq!(leer(&root, "main.rs").as_deref(), Some("fn main() {}\n"));
}
