use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;

fn resp(content: &str, calls: serde_json::Value) -> serde_json::Value {
    json!({"message":{"role":"assistant","content":content,"tool_calls":calls},
           "usage":{"input_tokens":3,"output_tokens":2,"cached_input_tokens":0},"stop_reason":"EndTurn"})
}
fn pcode(d: &std::path::Path, script: serde_json::Value) -> Command {
    let p = d.join("script.json");
    std::fs::write(&p, script.to_string()).unwrap();
    let mut c = Command::cargo_bin("pcode").unwrap();
    c.current_dir(d)
        .env("PCODE_MOCK_SCRIPT", p)
        .env("NO_COLOR", "1");
    c
}

#[test]
fn chat_dos_mensajes_por_stdin_con_tool_y_comandos() {
    let d = tempfile::tempdir().unwrap();
    let out = pcode(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"a.txt","content":"x"}}])
            ),
            resp("creado a.txt", json!([])),
            resp("segunda respuesta", json!([])),
        ]),
    )
    .arg("chat")
    .write_stdin("crea a.txt\n/uso\notra cosa\n/salir\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("⏺ escribir"))
    .stdout(predicate::str::contains("creado a.txt"))
    .stdout(predicate::str::contains("segunda respuesta"))
    .stdout(predicate::str::contains("tokens de entrada"))
    .get_output()
    .stdout
    .clone();
    assert!(
        !String::from_utf8_lossy(&out).contains('\x1b'),
        "NO_COLOR sin colores"
    );
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "x"
    );
    assert!(d.path().join(".pcode/pcode.db").exists());
}

#[test]
fn chat_con_prompt_inicial_y_sin_stdin() {
    let d = tempfile::tempdir().unwrap();
    pcode(d.path(), json!([resp("hola desde el mock", json!([]))]))
        .args(["chat", "di", "hola"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("hola desde el mock"));
}

#[test]
fn chat_continuar_retoma_la_conversacion() {
    let d = tempfile::tempdir().unwrap();
    pcode(d.path(), json!([resp("uno", json!([]))]))
        .args(["chat", "primero"])
        .write_stdin("")
        .assert()
        .success();
    pcode(d.path(), json!([resp("dos", json!([]))]))
        .args(["chat", "--continuar", "segundo"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(predicate::str::contains("Retomando la sesión"))
        .stdout(predicate::str::contains("dos"));
}

#[test]
fn chat_sin_modelo_configurado_explica_que_hacer() {
    let d = tempfile::tempdir().unwrap();
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .env_remove("PCODE_MOCK_SCRIPT")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .args(["chat", "hola"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ANTHROPIC_API_KEY"));
}

#[test]
fn chat_deniega_en_no_interactivo_lo_que_pide_confirmacion() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("pcode.toml"),
        "[politica]\ndenegar_ordenes = [\"git push\"]\n",
    )
    .unwrap();
    pcode(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"bash","arguments":{"command":"git push"}}])
            ),
            resp("no pude", json!([])),
        ]),
    )
    .args(["chat", "sube"])
    .write_stdin("")
    .assert()
    .success()
    .stdout(predicate::str::contains("denegado por la política"));
}
