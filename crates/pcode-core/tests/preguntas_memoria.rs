//! Preguntas de aclaración (F4.11), memoria del proyecto (F8.19), verificación antes de terminar
//! (F8.46) y compactación que conserva las restricciones.
use pcode_core::memory::{self, MAX_PROMPT_BYTES};
use pcode_core::{
    Agent, AgentConfig, Approver, AskMode, Budget, COMPACT_PROMPT, Outcome, VERIFY_NOTE,
};
use pcode_models::{ChatResponse, Message, MockProvider, Role, StopReason, ToolCall, Usage};
use pcode_policy::{Capabilities, Policy};
use pcode_tools::ToolContext;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

fn resp(text: &str, calls: Vec<(&str, serde_json::Value)>) -> ChatResponse {
    let mut m = Message::assistant(text);
    let tool = !calls.is_empty();
    m.tool_calls = calls
        .into_iter()
        .enumerate()
        .map(|(i, (n, a))| ToolCall {
            id: format!("c{i}-{n}"),
            name: n.into(),
            arguments: a,
        })
        .collect();
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

struct Responde(bool);
#[async_trait::async_trait]
impl Approver for Responde {
    async fn ask(&self, _action: &str, _reason: &str) -> bool {
        self.0
    }
}

fn agent(root: &std::path::Path, script: Vec<ChatResponse>) -> (Agent, Arc<MockProvider>) {
    let p = Arc::new(MockProvider::new(script));
    let a = Agent::new(
        p.clone(),
        pcode_tools::native_registry(),
        Policy::new(Capabilities::interactive_defaults()),
        None,
        AgentConfig {
            agent_id: "t".into(),
            model: "m".into(),
            system_prompt: "SISTEMA".into(),
            budget: Budget::default(),
            stop_hook: None,
        },
        ToolContext {
            root: root.to_path_buf(),
            timeout: Duration::from_secs(20),
        },
    );
    (a, p)
}

fn tools_ofrecidas(p: &MockProvider) -> Vec<String> {
    p.requests()[0]
        .tools
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

// ── F4.11: preguntar ──

#[tokio::test]
async fn interactivo_lee_respuesta() {
    let d = tempfile::tempdir().unwrap();
    let (a, p) = agent(
        d.path(),
        vec![
            resp(
                "",
                vec![(
                    "preguntar",
                    json!({"pregunta": "¿Mantengo la API pública?", "opciones": ["sí", "no"]}),
                )],
            ),
            resp("Hecho, la mantengo.", vec![]),
        ],
    );
    let mut a = a.with_questions(AskMode::Interactive);
    let r = a.run_turn(vec![], "refactoriza el módulo").await;
    let Outcome::Done { final_message } = &r.outcome else {
        panic!("{:?}", r.outcome)
    };
    assert_eq!(
        final_message,
        "❓ ¿Mantengo la API pública?\n  1. sí\n  2. no"
    );
    assert!(tools_ofrecidas(&p).contains(&"preguntar".to_string()));
    // La transcripción alterna bien: llamada, resultado y la pregunta como mensaje del asistente.
    let last = r.transcript.last().unwrap();
    assert_eq!(
        (last.role, last.content.as_str()),
        (Role::Assistant, final_message.as_str())
    );
    // La respuesta del usuario llega en el siguiente turno, con la pregunta en el historial.
    let r2 = a.run_turn(r.transcript, "sí, mantenla").await;
    assert!(matches!(r2.outcome, Outcome::Done { .. }));
    let req = &p.requests()[1];
    let n = req.messages.len();
    assert_eq!(req.messages[n - 1].content, "sí, mantenla");
    assert!(req.messages[n - 2].content.starts_with("❓"));
    assert_eq!(a.questions_asked(), ["¿Mantengo la API pública?"]);
}

#[tokio::test]
async fn presupuesto_agotado() {
    let d = tempfile::tempdir().unwrap();
    let q = |t: &str| resp("", vec![("preguntar", json!({ "pregunta": t }))]);
    let (a, p) = agent(
        d.path(),
        vec![
            q("¿uno?"),
            q("¿dos?"),
            q("¿tres?"),
            resp("decidido", vec![]),
        ],
    );
    let mut a = a.with_questions(AskMode::Bench);
    let r = a.run("haz algo").await;
    assert!(matches!(r.outcome, Outcome::Done { .. }));
    assert_eq!(
        a.questions_asked().len(),
        2,
        "solo cuentan las del presupuesto"
    );
    let tercera = p.requests()[3]
        .messages
        .last()
        .map(|m| m.content.clone())
        .unwrap();
    assert!(tercera.starts_with("Sin más preguntas"), "{tercera}");
}

#[tokio::test]
async fn bench_responde_y_cuenta() {
    let d = tempfile::tempdir().unwrap();
    let (a, p) = agent(
        d.path(),
        vec![
            resp(
                "",
                vec![("preguntar", json!({"pregunta": "¿tabs o espacios?"}))],
            ),
            resp("espacios", vec![]),
        ],
    );
    let mut a = a.with_questions(AskMode::Bench);
    let r = a.run("formatea").await;
    assert!(matches!(r.outcome, Outcome::Done { .. }));
    assert_eq!(a.questions_asked(), ["¿tabs o espacios?"]);
    assert!(
        p.requests()[1]
            .messages
            .last()
            .unwrap()
            .content
            .contains("decide con lo que sabes")
    );
}

#[tokio::test]
async fn flow_escala() {
    let d = tempfile::tempdir().unwrap();
    let (a, _) = agent(
        d.path(),
        vec![resp(
            "",
            vec![("preguntar", json!({"pregunta": "¿Qué base de datos?"}))],
        )],
    );
    let mut a = a.with_questions(AskMode::Flow);
    let r = a.run("añade persistencia").await;
    assert_eq!(
        r.outcome,
        Outcome::Failed("pregunta pendiente: ¿Qué base de datos?".into())
    );
}

#[tokio::test]
async fn sin_modo_no_se_ofrece_preguntar() {
    let d = tempfile::tempdir().unwrap();
    let (mut a, p) = agent(d.path(), vec![resp("ok", vec![])]);
    a.run("x").await;
    let t = tools_ofrecidas(&p);
    assert!(!t.contains(&"preguntar".to_string()) && !t.contains(&"recordar".to_string()));
    assert!(
        t.contains(&"esquema".to_string()),
        "la tool esquema es nativa"
    );
}

// ── F8.19: memoria del proyecto ──

#[test]
fn recuerda_añade_nota() {
    let d = tempfile::tempdir().unwrap();
    assert!(memory::notes(d.path()).is_empty());
    let n = memory::remember(d.path(), "  los commits,\n en inglés ", false).unwrap();
    assert!(n.ends_with("] los commits, en inglés"), "{n}");
    memory::remember(d.path(), "no tocar la API v1", true).unwrap();
    let notas = memory::notes(d.path());
    assert_eq!(notas.len(), 2);
    assert!(notas[1].contains(" · agente] no tocar la API v1"));
    let texto = std::fs::read_to_string(memory::memory_path(d.path())).unwrap();
    assert!(texto.starts_with("# Memoria del proyecto"));
    assert_eq!(
        memory::forget(d.path(), 1).unwrap().unwrap(),
        notas[0],
        "olvida la primera"
    );
    assert_eq!(memory::notes(d.path()).len(), 1);
    assert!(memory::forget(d.path(), 9).unwrap().is_none());
    assert!(memory::remember(d.path(), "   ", false).is_err());
}

#[test]
fn fechas_civiles() {
    assert_eq!(memory::ymd(0), "1970-01-01");
    assert_eq!(memory::ymd(951_782_400), "2000-02-29");
    assert_eq!(memory::ymd(1_793_404_800), "2026-10-31");
    assert_eq!(memory::ymd(1_793_491_200), "2026-11-01");
}

#[test]
fn prompt_incluye_memoria_recortada() {
    let d = tempfile::tempdir().unwrap();
    for i in 0..400 {
        memory::remember(
            d.path(),
            &format!("decisión número {i} con algo de texto"),
            false,
        )
        .unwrap();
    }
    let s = memory::prompt_section(d.path()).unwrap();
    assert!(s.len() <= MAX_PROMPT_BYTES + 200, "{}", s.len());
    let primera = s.lines().nth(1).unwrap();
    assert!(
        primera.contains("decisión número 399"),
        "la más reciente primero: {primera}"
    );
    assert!(!s.contains("decisión número 0 "), "las antiguas no caben");
    assert!(s.contains("notas antiguas no caben"));
}

#[tokio::test]
async fn sin_memoria_no_añade_nada() {
    let d = tempfile::tempdir().unwrap();
    assert!(memory::prompt_section(d.path()).is_none());
    let (a, p) = agent(d.path(), vec![resp("ok", vec![])]);
    let mut a = a.with_memory();
    a.run("x").await;
    assert_eq!(p.requests()[0].messages[0].content, "SISTEMA");
}

#[tokio::test]
async fn la_memoria_entra_en_cada_turno_y_se_actualiza() {
    let d = tempfile::tempdir().unwrap();
    memory::remember(d.path(), "usa tabs", false).unwrap();
    let (a, p) = agent(d.path(), vec![resp("uno", vec![]), resp("dos", vec![])]);
    let mut a = a.with_memory();
    let r = a.run_turn(vec![], "hola").await;
    assert!(p.requests()[0].messages[0].content.contains("- ["));
    assert!(p.requests()[0].messages[0].content.contains("usa tabs"));
    memory::remember(d.path(), "y los commits en inglés", false).unwrap();
    a.run_turn(r.transcript, "otra").await;
    let sys = &p.requests()[1].messages[0].content;
    assert!(sys.starts_with("SISTEMA") && sys.contains("y los commits en inglés"));
}

#[tokio::test]
async fn recordar_requiere_aprobacion() {
    for (aprueba, guardado) in [(true, true), (false, false)] {
        let d = tempfile::tempdir().unwrap();
        let (a, p) = agent(
            d.path(),
            vec![
                resp(
                    "",
                    vec![("recordar", json!({"texto": "los tests con pytest -q"}))],
                ),
                resp("vale", vec![]),
            ],
        );
        let mut a = a.with_memory().with_approver(Arc::new(Responde(aprueba)));
        a.run("recuerda que los tests van con pytest -q").await;
        assert!(tools_ofrecidas(&p).contains(&"recordar".to_string()));
        assert_eq!(!memory::notes(d.path()).is_empty(), guardado);
        let res = p.requests()[1].messages.last().unwrap().content.clone();
        assert!(
            res.contains(if aprueba { "guardado" } else { "no quiso" }),
            "{res}"
        );
    }
}

// ── F8.46: verificar antes de terminar ──

#[tokio::test]
async fn verifica_antes_de_terminar_una_vez() {
    let d = tempfile::tempdir().unwrap();
    let (a, p) = agent(
        d.path(),
        vec![
            resp(
                "",
                vec![("escribir", json!({"path": "a.py", "content": "x = 1\n"}))],
            ),
            resp("Listo.", vec![]),
            resp("No hay tests en el proyecto: no hace falta.", vec![]),
        ],
    );
    let mut a = a.with_verify_reminder();
    let r = a.run("crea a.py").await;
    let Outcome::Done { final_message } = &r.outcome else {
        panic!("{:?}", r.outcome)
    };
    assert!(final_message.starts_with("No hay tests"));
    assert_eq!(p.requests().len(), 3, "un solo recordatorio");
    assert_eq!(
        p.requests()[2].messages.last().unwrap().content,
        VERIFY_NOTE
    );
}

#[tokio::test]
async fn sin_recordatorio_si_ya_comprobo_o_no_cambio_nada() {
    let d = tempfile::tempdir().unwrap();
    let orden = if cfg!(windows) { "echo ok" } else { "true" };
    let (a, p) = agent(
        d.path(),
        vec![
            resp(
                "",
                vec![("escribir", json!({"path": "a.py", "content": "x = 1\n"}))],
            ),
            resp("", vec![("bash", json!({ "command": orden }))]),
            resp("Listo y comprobado.", vec![]),
        ],
    );
    let mut a = a.with_verify_reminder();
    a.run("crea a.py y compruébalo").await;
    assert_eq!(p.requests().len(), 3);
    let (b, q) = agent(d.path(), vec![resp("Solo te explico el código.", vec![])]);
    let mut b = b.with_verify_reminder();
    b.run("explica a.py").await;
    assert_eq!(q.requests().len(), 1);
}

// ── Compactación ──

#[test]
fn compactar_conserva_restricciones_rutas_y_errores() {
    for k in ["LITERALMENTE", "restricciones", "rutas exactas", "errores"] {
        assert!(COMPACT_PROMPT.contains(k), "falta «{k}»");
    }
}
