//! `pcode --version`: la versión lleva el commit corto con el que se compiló el binario.
use assert_cmd::Command;

/// Commit corto (7 cifras) del repo donde está el paquete, o `None` si no hay git.
fn commit_esperado() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn salida_version() -> String {
    let out = Command::cargo_bin("pcode")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success(), "pcode --version debe terminar bien");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn version_incluye_el_commit() {
    let salida = salida_version();
    let version = env!("CARGO_PKG_VERSION");
    match commit_esperado() {
        Some(commit) => {
            let esperada = format!("pcode {version} ({commit})\n");
            assert_eq!(salida, esperada);
        }
        None => {
            // Sin git: solo se exige el número de versión.
            assert!(
                salida.starts_with(&format!("pcode {version}")),
                "salida: {salida}"
            );
        }
    }
}

#[test]
fn version_empieza_por_el_numero() {
    let salida = salida_version();
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        salida.starts_with(&format!("pcode {version}")),
        "salida: {salida}"
    );
    assert!(
        salida.ends_with('\n'),
        "la salida debe acabar en salto de línea"
    );
}
