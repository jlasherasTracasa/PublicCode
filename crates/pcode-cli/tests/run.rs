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
        .stdout(predicate::str::contains(format!(
            "pcode {}",
            env!("CARGO_PKG_VERSION")
        )));
}

fn userns() -> bool {
    std::process::Command::new("unshare")
        .args(["--user", "--map-root-user", "--mount", "true"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn backend_modelo_en_sandbox_no_toca_el_repo_hasta_aplicar() {
    if !userns() {
        eprintln!("sin namespaces de usuario: test omitido");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("pcode.toml"),
        "[backends.glm]\ntipo = \"modelo\"\nmodelo = \"m\"\n",
    )
    .unwrap();
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"nuevo.txt","content":"hi"}}])
            ),
            resp("hecho en sandbox", json!([]))
        ]),
    );
    // sin --aplicar-si-acepta: el repo real no cambia
    pcode(d.path(), &s)
        .args(["run", "x", "--backend", "glm"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hecho en sandbox"))
        .stderr(predicate::str::contains("nuevo.txt"));
    assert!(!d.path().join("nuevo.txt").exists());
}

#[test]
fn backend_con_aceptacion_aplica_los_cambios() {
    if !userns() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("pcode.toml"),
        "[backends.glm]\ntipo = \"modelo\"\n",
    )
    .unwrap();
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"ok.txt","content":"1"}}])
            ),
            resp("listo", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .args([
            "run",
            "x",
            "--backend",
            "glm",
            "--acepta",
            "test -f ok.txt",
            "--aplicar-si-acepta",
        ])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(d.path().join("ok.txt")).unwrap(),
        "1"
    );
}

/// Copilot (y los demás agentes integrados) funciona sin declararlo en `[backends]`; con
/// `--aplicar` sus cambios llegan al proyecto con un punto de control, y si el agente falla no se
/// aplica nada.
#[cfg(unix)]
#[test]
fn copilot_integrado_aplica_con_punto_de_control() {
    use std::os::unix::fs::PermissionsExt;
    if !userns() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let fake = bin.path().join("copilot");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho \"$*\" > de_copilot.txt\necho hecho por copilot\nexit ${COPILOT_FALSO_SALIDA:-0}\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = |salida: &str| {
        let mut c = Command::cargo_bin("pcode").unwrap();
        c.current_dir(d.path())
            .env("PATH", &path)
            .env("COPILOT_FALSO_SALIDA", salida)
            .args([
                "run",
                "saluda",
                "--backend",
                "copilot",
                "--modelo",
                "m1",
                "--aplicar",
            ]);
        c
    };
    // Si el agente falla, el proyecto no cambia.
    run("3").assert().code(3);
    assert!(!d.path().join("de_copilot.txt").exists());
    run("0")
        .assert()
        .success()
        .stdout(predicate::str::contains("hecho por copilot"))
        .stderr(predicate::str::contains("1 cambios aplicados"));
    let args = std::fs::read_to_string(d.path().join("de_copilot.txt")).unwrap();
    assert!(
        args.contains("-p saluda") && args.contains("-s") && args.contains("--model m1"),
        "{args}"
    );
    let manifest =
        std::fs::read_to_string(d.path().join(".pcode/checkpoints/000001/manifest.json")).unwrap();
    assert!(manifest.contains("copilot: saluda") && manifest.contains("de_copilot.txt"));
}

#[test]
fn backend_desconocido_es_error() {
    let d = tempfile::tempdir().unwrap();
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .args(["run", "x", "--backend", "nada"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("backend desconocido"));
}

#[test]
fn sandbox_ejecutar_cambios_aplicar() {
    if !userns() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let st = tempfile::tempdir().unwrap();
    let est = st.path().join("e");
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .args([
            "sandbox",
            "ejecutar",
            "--estado",
            est.to_str().unwrap(),
            "--",
            "sh",
            "-c",
            "echo hola > f.txt",
        ])
        .assert()
        .success();
    assert!(!d.path().join("f.txt").exists());
    Command::cargo_bin("pcode")
        .unwrap()
        .args(["sandbox", "cambios", est.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("f.txt"));
    Command::cargo_bin("pcode")
        .unwrap()
        .args(["sandbox", "aplicar", est.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(d.path().join("f.txt")).unwrap(),
        "hola\n"
    );
}

#[test]
fn flow_run_integra_tareas_con_dependencias() {
    if !userns() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let g = |a: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(a)
            .output()
            .unwrap()
    };
    g(&["init", "-q", "-b", "main"]);
    std::fs::write(
        d.path().join("pcode.toml"),
        "[backends.glm]\ntipo = \"modelo\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.path().join("tareas")).unwrap();
    std::fs::write(
        d.path().join("tareas/uno.md"),
        "<!-- acepta: test -f uno.txt -->\n<!-- modo: aislado -->\n# Uno\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("tareas/dos.md"),
        "<!-- acepta: test -f dos.txt -->\n<!-- modo: aislado -->\n<!-- depende: uno -->\n# Dos\n",
    )
    .unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "init"]);
    // Cada `pcode run` dentro de la sandbox arranca el guion desde el principio: escribe ambos archivos.
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"uno.txt","content":"1"}},
                            {"id":"2","name":"escribir","arguments":{"path":"dos.txt","content":"2"}}])
            ),
            resp("listo", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .args(["flow", "run", "--backend", "glm", "--paralelo", "1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("integradas: 2"));
    assert!(d.path().join("uno.txt").exists() && d.path().join("dos.txt").exists());
    let log = String::from_utf8_lossy(&g(&["log", "--format=%s"]).stdout).to_string();
    assert!(log.contains("uno: Uno"), "{log}");
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .args(["flow", "estado"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("| uno | integrada | 1 |")
                .and(predicate::str::contains("| dos | integrada | 1 |")),
        );
}

#[test]
fn n0_flow_run_integra_tareas_con_dependencias() {
    let d = tempfile::tempdir().unwrap();
    let g = |a: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(a)
            .output()
            .unwrap()
    };
    g(&["init", "-q", "-b", "main"]);
    std::fs::write(
        d.path().join("pcode.toml"),
        "[backends.glm]\ntipo = \"modelo\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(d.path().join("tareas")).unwrap();
    std::fs::write(
        d.path().join("tareas/uno.md"),
        "<!-- acepta: test -f uno.txt -->\n<!-- modo: aislado -->\n# Uno\n",
    )
    .unwrap();
    std::fs::write(
        d.path().join("tareas/dos.md"),
        "<!-- acepta: test -f dos.txt -->\n<!-- modo: aislado -->\n<!-- depende: uno -->\n# Dos\n",
    )
    .unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "init"]);
    // Cada `pcode run` dentro de la sandbox arranca el guion desde el principio: escribe ambos archivos.
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"uno.txt","content":"1"}},
                            {"id":"2","name":"escribir","arguments":{"path":"dos.txt","content":"2"}}])
            ),
            resp("listo", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .env("PCODE_SANDBOX", "N0")
        .args(["flow", "run", "--backend", "glm", "--paralelo", "1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("integradas: 2"));
    assert!(d.path().join("uno.txt").exists() && d.path().join("dos.txt").exists());
    let log = String::from_utf8_lossy(&g(&["log", "--format=%s"]).stdout).to_string();
    assert!(log.contains("uno: Uno"), "{log}");
    Command::cargo_bin("pcode")
        .unwrap()
        .current_dir(d.path())
        .args(["flow", "estado"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("| uno | integrada | 1 |")
                .and(predicate::str::contains("| dos | integrada | 1 |")),
        );
}

#[test]
fn n0_backend_y_aplicar_con_git_worktree() {
    let d = tempfile::tempdir().unwrap();
    let g = |a: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(d.path())
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(a)
                .status()
                .unwrap()
                .success()
        );
    };
    g(&["init", "-q"]);
    std::fs::write(
        d.path().join("pcode.toml"),
        "[backends.glm]\ntipo = \"modelo\"\n",
    )
    .unwrap();
    g(&["add", "pcode.toml"]);
    g(&["commit", "-qm", "init"]);
    let s = script(
        d.path(),
        json!([
            resp(
                "",
                json!([{"id":"1","name":"escribir","arguments":{"path":"n0.txt","content":"desde N0"}}])
            ),
            resp("listo en N0", json!([]))
        ]),
    );
    pcode(d.path(), &s)
        .env("PCODE_SANDBOX", "N0")
        .args([
            "run",
            "x",
            "--backend",
            "glm",
            "--acepta",
            "git --version",
            "--aplicar-si-acepta",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("listo en N0"));
    assert_eq!(
        std::fs::read_to_string(d.path().join("n0.txt")).unwrap(),
        "desde N0"
    );
    let wt = std::process::Command::new("git")
        .arg("-C")
        .arg(d.path())
        .args(["worktree", "list"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&wt.stdout).lines().count(),
        1,
        "sin worktrees colgados"
    );
}
