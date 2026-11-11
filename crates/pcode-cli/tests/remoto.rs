//! `pcode remoto` de punta a punta con un `ssh` falso (Python) que ejecuta la orden «remota» en
//! este equipo, con otro `HOME`, y reenvía los puertos de `-L` de verdad. Solo en Unix.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SSH_FALSO: &str = r#"#!/usr/bin/env python3
# ssh falso: interpreta -L (reenvía el puerto) y ejecuta la orden con sh en este equipo.
import socket, subprocess, sys, threading
args, fw, i = sys.argv[1:], [], 0
while i < len(args):
    a = args[i]
    if a in ("-o", "-L", "-p", "-i"):
        if a == "-L":
            fw.append(args[i + 1])
        i += 2
    elif a.startswith("-"):
        i += 1
    else:
        break
cmd = " ".join(args[i + 1:])
def pipe(a, b):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            b.sendall(d)
    except OSError:
        pass
    finally:
        for s in (a, b):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
def forward(spec, ready):
    lh, lp, rh, rp = spec.split(":")
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((lh, int(lp)))
    srv.listen(16)
    ready.set()
    for _ in range(1000):
        c, _ = srv.accept()
        try:
            r = socket.create_connection((rh, int(rp)), timeout=10)
        except OSError:
            c.close()
            continue
        threading.Thread(target=pipe, args=(c, r), daemon=True).start()
        threading.Thread(target=pipe, args=(r, c), daemon=True).start()
for f in fw:
    ev = threading.Event()
    threading.Thread(target=forward, args=(f, ev), daemon=True).start()
    ev.wait(5)
sys.exit(subprocess.call(["sh", "-c", cmd]))
"#;

fn http(port: u16, req: &str) -> Option<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    s.write_all(req.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn python3() -> bool {
    Command::new("python3").arg("--version").output().is_ok()
}

#[test]
fn remoto_instala_abre_tunel_y_cierra_al_salir() {
    if !python3() {
        eprintln!("sin python3: se salta");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let home = d.path().join("home");
    let proj = home.join("proyecto");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("hola.py"), "print('hola')\n").unwrap();
    let ssh = d.path().join("ssh");
    std::fs::write(&ssh, SSH_FALSO).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_pcode"))
        .args([
            "remoto",
            "servidor",
            "--instalar",
            "--no-abrir",
            "--ruta",
            "~/proyecto",
        ])
        .env("PCODE_SSH", &ssh)
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let err = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let mut url = None;
    let mut log = vec![];
    let deadline = Instant::now() + Duration::from_secs(60);
    while url.is_none() && Instant::now() < deadline {
        if let Ok(l) = rx.recv_timeout(Duration::from_millis(200)) {
            if l.contains("túnel SSH") {
                url = l
                    .split_whitespace()
                    .find(|w| w.starts_with("http://"))
                    .map(String::from);
            }
            log.push(l);
        }
    }
    let url = url.unwrap_or_else(|| {
        let _ = child.kill();
        panic!("no salió la dirección: {log:?}")
    });
    // Se instaló el binario en el «servidor».
    assert!(home.join(".local/bin/pcode").is_file(), "{log:?}");
    assert!(log.iter().any(|l| l.contains("Copiando")), "{log:?}");

    let rest = url.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, query) = rest.split_once('/').unwrap();
    let port: u16 = port.parse().unwrap();
    let token = query.strip_prefix("?t=").unwrap();
    let index = http(
        port,
        &format!("GET /?t={token} HTTP/1.0\r\nHost: x\r\n\r\n"),
    )
    .unwrap();
    assert!(
        index.starts_with("HTTP/1.0 200") || index.starts_with("HTTP/1.1 200"),
        "{}",
        &index[..80.min(index.len())]
    );
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"server.info","params":{}}"#;
    let info = http(
        port,
        &format!(
            "POST /rpc HTTP/1.0\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
    .unwrap();
    assert!(
        info.contains("proyecto"),
        "el Studio remoto trabaja en ~/proyecto: {info}"
    );

    // Al salir pcode remoto se cierra el túnel y, con él, el Studio remoto.
    child.kill().unwrap();
    child.wait().unwrap();
    let closed = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        TcpStream::connect(("127.0.0.1", port)).is_err()
    });
    assert!(closed, "el túnel siguió abierto");
    assert!(!Path::new(&home.join(".local/bin/.pcode.nuevo")).exists());
}

#[test]
fn remoto_sin_pcode_y_sin_instalar_explica_que_hacer() {
    if !python3() {
        eprintln!("sin python3: se salta");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let ssh = d.path().join("ssh");
    std::fs::write(&ssh, SSH_FALSO).unwrap();
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pcode"))
        .args(["remoto", "servidor", "--no-abrir"])
        .env("PCODE_SSH", &ssh)
        .env("HOME", d.path())
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(e.contains("--instalar"), "{e}");
    let mal = Command::new(env!("CARGO_BIN_EXE_pcode"))
        .args(["remoto", "-oProxyCommand=x"])
        .output()
        .unwrap();
    assert_ne!(mal.status.code(), Some(0));
}
