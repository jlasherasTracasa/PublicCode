use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;

fn script(d: &std::path::Path, v: serde_json::Value) -> std::path::PathBuf {
    let p = d.join("script.json");
    std::fs::write(&p, v.to_string()).unwrap();
    p
}
fn resp(content: &str, calls: serde_json::Value) -> serde_json::Value {
    json!({"message":{"role":"assistant","content":content,"tool_calls":calls},
           "usage":{"input_tokens":1,"output_tokens":1,"cached_input_tokens":0},"stop_reason":"EndTurn"})
}
fn pcode(d: &std::path::Path, s: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("pcode").unwrap();
    c.current_dir(d).env("PCODE_MOCK_SCRIPT", s);
    c
}

#[test]
fn run_con_mock_escribe_archivo() {
    let d = tempfile::tempdir().unwrap();
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"hola.txt","content":"hi"}}])
            ),
            resp("listo", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .args(["run", "haz algo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("listo"));
    assert_eq!(
        std::fs::read_to_string(d.path().join("hola.txt")).unwrap(),
        "hi"
    );
}
#[test]
fn run_con_acepta_que_falla_sale_con_2() {
    let d = tempfile::tempdir().unwrap();
    let s = script(
        d.path(),
        json!([
            resp("a", json!([])),
            resp("b", json!([])),
            resp("c", json!([])),
            resp("d", json!([])),
            resp("e", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .args(["run", "x", "--acepta", "false"])
        .assert()
        .code(2);
}
#[test]
fn run_crea_audit_y_db() {
    let d = tempfile::tempdir().unwrap();
    let s = script(d.path(), json!([resp("ok", json!([]))]));
    pcode(d.path(), &s).args(["run", "x"]).assert().success();
    assert!(d.path().join(".pcode/pcode.db").exists());
    assert!(d.path().join(".pcode").exists());
}
#[test]
fn run_politica_deniega_y_lo_dice() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("pcode.toml"),
        "[politica]\nno_escribir = [\"docs/**\"]\n",
    )
    .unwrap();
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"docs/a.md","content":"x"}}])
            ),
            resp("ok", json!([]))
        ]),
    );
    pcode(d.path(), &s).args(["run", "x"]).assert().success();
    assert!(!d.path().join("docs/a.md").exists());
}
#[test]
fn version_se_imprime() {
    Command::cargo_bin("pcode")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("pcode 0.1.0"));
}
