//! `pcode spec`: el ciclo completo con un modelo simulado (borrador, plan con una reparación,
//! implementación tarea a tarea, verificación, cambio en la especificación y replanificación).
use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;

fn resp(content: &str, calls: serde_json::Value) -> serde_json::Value {
    json!({"message":{"role":"assistant","content":content,"tool_calls":calls},
           "usage":{"input_tokens":1,"output_tokens":1,"cached_input_tokens":0},"stop_reason":"EndTurn"})
}

/// `pcode` en `d` con el guion `respuestas` para el modelo simulado.
fn pcode(d: &std::path::Path, respuestas: serde_json::Value) -> Command {
    let s = d.join(".guion.json");
    std::fs::write(&s, respuestas.to_string()).unwrap();
    let mut c = Command::cargo_bin("pcode").unwrap();
    c.current_dir(d)
        .env("PCODE_MOCK_SCRIPT", &s)
        .env("NO_COLOR", "1");
    c
}

const SPEC: &str = "# Saludo

## Objetivo
Saludar y despedirse.

## Requisitos

### R1. Archivo de saludo
CUANDO se pide un saludo EL SISTEMA DEBE escribir saludo.txt con «hola».

- saludo.txt contiene hola

### R2. Despedida
- adios.txt existe
- acepta: `test -f adios.txt`

## Restricciones
- solo archivos de texto
";

fn plan(tareas: &str) -> String {
    format!(
        "<<<plan.md\n# Plan\n## Enfoque\nDos archivos de texto.\n## Preguntas abiertas\n- ninguna\n>>>\n\n<<<tareas.md\n{tareas}\n>>>\n"
    )
}

const T1: &str = "- [ ] T1. Escribir el saludo\n  - requisitos: R1\n  - toca: saludo.txt\n  - depende: —\n  - acepta: `grep -q hola saludo.txt`\n  - Crea saludo.txt.";
const T2: &str = "- [ ] T2. Escribir la despedida\n  - requisitos: R2\n  - depende: T1\n  - acepta: `test -f adios.txt`";

#[cfg(unix)]
#[test]
fn ciclo_completo_de_una_especificacion() {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    // Sin especificaciones.
    pcode(r, json!([]))
        .args(["spec"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No hay especificaciones"));

    // 1. Borrador desde una frase (el modelo lo devuelve dentro de una valla de código).
    pcode(
        r,
        json!([resp(&format!("```markdown\n{SPEC}```"), json!([]))]),
    )
    .args(["spec", "nueva", "saludo", "--desde", "saludar y despedirse"])
    .assert()
    .success()
    .stdout(predicate::str::contains("Creada specs/saludo/spec.md"));
    assert_eq!(
        std::fs::read_to_string(r.join("specs/saludo/spec.md")).unwrap(),
        SPEC
    );
    pcode(r, json!([]))
        .args(["spec", "nueva", "saludo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ya existe"));

    // 2. Plan: el primero deja R2 sin tarea; la validación lo detecta y la reparación lo arregla.
    pcode(
        r,
        json!([
            resp(
                "",
                json!([{"id":"1","name":"esquema","arguments":{"path":"."}}])
            ),
            resp(&plan(&format!("# Tareas: Saludo\n\n{T1}")), json!([])),
            resp(&plan(&format!("# Tareas: Saludo\n\n{T1}\n{T2}")), json!([])),
        ]),
    )
    .args(["spec", "planificar", "saludo"])
    .assert()
    .success()
    .stdout(predicate::str::contains("R2 no tiene ninguna tarea"))
    .stdout(predicate::str::contains("· esquema ."))
    .stdout(predicate::str::contains("2 tareas (0 ya hechas)"));
    let tareas = std::fs::read_to_string(r.join("specs/saludo/tareas.md")).unwrap();
    assert!(tareas.contains("<!-- spec: R1=") && tareas.contains("- [ ] T2."));
    assert!(r.join("specs/saludo/plan.md").is_file());
    pcode(r, json!([]))
        .args(["spec", "planificar", "saludo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("al día"));

    // 3. Implementar: una tarea tras otra, cada una hasta su `acepta`, y la verificación final.
    pcode(
        r,
        json!([
            resp("", json!([{"id":"1","name":"escribir","arguments":{"path":"saludo.txt","content":"hola\n"}}])),
            resp("Saludo escrito.", json!([])),
            resp("", json!([{"id":"2","name":"escribir","arguments":{"path":"adios.txt","content":"adiós\n"}}])),
            resp("Despedida escrita.", json!([])),
        ]),
    )
    .args(["spec", "implementar", "saludo"])
    .assert()
    .success()
    .stdout(predicate::str::contains("▶ T1 · Escribir el saludo"))
    .stdout(predicate::str::contains("· escribir saludo.txt"))
    .stdout(predicate::str::contains("✓ T1 hecha"))
    .stdout(predicate::str::contains("✓ T2 hecha"))
    .stdout(predicate::str::contains("✓ 2 de 2 requisitos verificados"));
    let tareas = std::fs::read_to_string(r.join("specs/saludo/tareas.md")).unwrap();
    assert!(tareas.contains("- [x] T1.") && tareas.contains("- [x] T2."));
    assert!(
        r.join(".pcode/checkpoints/000001/manifest.json").is_file(),
        "se puede deshacer"
    );
    let estado = pcode(r, json!([]))
        .args(["spec", "estado", "saludo", "--formato", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&estado.stdout).unwrap();
    assert_eq!(v["phase"], "done");
    assert_eq!(v["requirements"][0]["verdict"], "ok");
    pcode(r, json!([]))
        .args(["spec", "lista"])
        .assert()
        .success()
        .stdout(predicate::str::contains("saludo").and(predicate::str::contains("hecha")));

    // 4. Un requisito cambia: el plan se queda atrás y no se implementa hasta replanificar.
    std::fs::write(
        r.join("specs/saludo/spec.md"),
        SPEC.replace("con «hola».", "con «hola, mundo»."),
    )
    .unwrap();
    pcode(r, json!([]))
        .args(["spec", "estado", "saludo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("⚠ cambió"))
        .stdout(predicate::str::contains(
            "Siguiente paso: pcode spec planificar saludo",
        ));
    pcode(r, json!([]))
        .args(["spec", "implementar", "saludo"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("replanifica"));

    // 5. Replanificar: solo sigue hecha la tarea de lo que no cambió (aunque el modelo marque las dos).
    let t1 = T1
        .replace("- [ ]", "- [x]")
        .replace("grep -q hola", "grep -q 'hola, mundo'");
    let t2 = T2.replace("- [ ]", "- [x]");
    pcode(
        r,
        json!([resp(
            &plan(&format!("# Tareas: Saludo\n\n{t1}\n{t2}")),
            json!([])
        )]),
    )
    .args(["spec", "planificar", "saludo"])
    .assert()
    .success()
    .stdout(predicate::str::contains("Replanificando"))
    .stdout(predicate::str::contains("2 tareas (1 ya hechas)"));
    let tareas = std::fs::read_to_string(r.join("specs/saludo/tareas.md")).unwrap();
    assert!(
        tareas.contains("- [ ] T1.") && tareas.contains("- [x] T2."),
        "{tareas}"
    );
    // La verificación ve que R1 ya no se cumple.
    pcode(r, json!([]))
        .args(["spec", "verificar", "saludo"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("✗ R1"))
        .stdout(predicate::str::contains("✓ R2"));
}

#[test]
fn errores_de_uso() {
    let d = tempfile::tempdir().unwrap();
    pcode(d.path(), json!([]))
        .args(["spec", "estado", "nada"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pcode spec nueva nada"));
    pcode(d.path(), json!([]))
        .args(["spec", "nueva", "Mal Nombre"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nombre no válido"));
    pcode(d.path(), json!([]))
        .args(["spec", "nueva", "vacia"])
        .assert()
        .success();
    pcode(d.path(), json!([]))
        .args(["spec", "implementar", "vacia"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("No hay plan todavía"));
    // La plantilla no trae órdenes: verificar dice cómo añadirlas.
    pcode(d.path(), json!([]))
        .args(["spec", "verificar", "vacia"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("acepta"));
}
