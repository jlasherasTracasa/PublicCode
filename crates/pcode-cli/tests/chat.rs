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

#[test]
fn chat_jsonl_protocolo_con_aprobacion() {
    let d = tempfile::tempdir().unwrap();
    let out = pcode(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"bash","arguments":{"command":"git push origin main"}}])
            ),
            resp("hecho", json!([])),
            resp("segundo", json!([])),
        ]),
    )
    .args(["chat", "--formato", "jsonl"])
    .write_stdin("{\"mensaje\":\"sube\"}\n{\"aprobacion\":1,\"permitir\":false}\notro\n")
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let lines: Vec<serde_json::Value> = String::from_utf8_lossy(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}")))
        .collect();
    let tipos: Vec<&str> = lines.iter().map(|v| v["tipo"].as_str().unwrap()).collect();
    assert!(tipos.contains(&"aprobacion"), "{tipos:?}");
    let ap = lines.iter().find(|v| v["tipo"] == "aprobacion").unwrap();
    assert!(ap["motivo"].as_str().unwrap().contains("git push"));
    let res = lines.iter().find(|v| v["tipo"] == "resultado").unwrap();
    assert_eq!(
        res["error"], true,
        "la aprobación denegada llega como error"
    );
    assert_eq!(tipos.iter().filter(|t| **t == "fin").count(), 2);
    assert!(
        lines
            .iter()
            .any(|v| v["tipo"] == "texto" && v["texto"] == "segundo")
    );
}

#[cfg(unix)]
#[test]
fn mcp_tools_de_un_servidor_real_en_chat_y_en_pcode_mcp() {
    let d = tempfile::tempdir().unwrap();
    // Servidor MCP mínimo: los ids de las peticiones de pcode son 1, 2, 3…
    let srv = d.path().join("reloj.sh");
    std::fs::write(
        &srv,
        r#"read l
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}}}}'
read l
read l
echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hora","description":"Hora del servidor","inputSchema":{"type":"object"}}]}}'
read l
echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"son las 12:00"}]}}'
read l
"#,
    )
    .unwrap();
    std::fs::write(
        d.path().join("pcode.toml"),
        format!("[mcp.reloj]\norden = [\"sh\", \"{}\"]\n", srv.display()),
    )
    .unwrap();
    pcode(
        d.path(),
        json!([
            resp("", json!([{"id":"1","name":"reloj_hora","arguments":{}}])),
            resp("Son las doce.", json!([])),
        ]),
    )
    .args(["chat", "qué hora es"])
    .write_stdin("")
    .assert()
    .success()
    .stdout(predicate::str::contains("⏺ reloj_hora"))
    .stdout(predicate::str::contains("son las 12:00"))
    .stderr(predicate::str::contains("1 tools de MCP disponibles"));
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .arg("mcp")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ reloj: 1 tools"))
        .stdout(predicate::str::contains("reloj_hora"));
}

#[test]
fn chat_orden_de_shell_y_modo_plan() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("nota.txt"), "contenido de la nota").unwrap();
    pcode(d.path(), json!([resp("plan: nada que cambiar", json!([]))]))
        .arg("chat")
        .write_stdin("!echo hola-shell\n/plan\nrevisa @nota.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("hola-shell"))
        .stdout(predicate::str::contains("Modo plan"))
        .stdout(predicate::str::contains("plan: nada que cambiar"));
}

#[test]
fn preguntar_responde_sin_tools() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.rs"), "fn a() {}").unwrap();
    pcode(d.path(), json!([resp("a() no hace nada", json!([]))]))
        .args(["preguntar", "¿qué hace", "@a.rs?"])
        .write_stdin("")
        .assert()
        .success()
        .stdout("a() no hace nada\n");
    assert!(
        !d.path().join(".pcode").exists(),
        "preguntar no crea estado en el proyecto"
    );
}

#[test]
fn preguntar_lee_stdin_y_formato_json() {
    let d = tempfile::tempdir().unwrap();
    let out = pcode(d.path(), json!([resp("feat: añade a()", json!([]))]))
        .args(["preguntar", "--formato", "json", "mensaje de commit"])
        .write_stdin("diff --git a/a.rs b/a.rs\n+fn a() {}\n")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["texto"], "feat: añade a()");
    assert_eq!(v["entrada"], 3);
    pcode(d.path(), json!([]))
        .args(["preguntar"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("falta la pregunta"));
}

#[test]
fn jsonl_fin_lleva_los_cambios_del_turno_y_deshacer_los_revierte() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "uno\n").unwrap();
    let out = pcode(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"a.txt","content":"uno\ndos\n"}}])
            ),
            resp("listo", json!([])),
        ]),
    )
    .args(["chat", "--formato", "jsonl"])
    .write_stdin("{\"mensaje\":\"añade dos\"}\n")
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
    let fin = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["tipo"] == "fin")
        .expect("evento fin");
    assert_eq!(
        fin["cambios"],
        json!([{"ruta":"a.txt","mas":1,"menos":0,"creado":false,"borrado":false}])
    );
    pcode(d.path(), json!([]))
        .arg("chat")
        .write_stdin("/deshacer\n/salir\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Deshecho «añade dos»"));
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "uno\n"
    );
}
