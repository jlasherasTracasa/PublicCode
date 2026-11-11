//! `pcode remoto`: trabajar en otro equipo por SSH, como Remote-SSH de VS Code pero sin instalar
//! nada pesado allí. En el servidor corre el mismo binario `pcode` (Studio solo en loopback) y aquí
//! se abre un túnel `ssh -L`; los servidores de lenguaje, las compilaciones y los agentes gastan
//! la memoria del servidor, no la de este equipo. Si el servidor no tiene `pcode`, `--instalar`
//! copia este binario por la misma conexión (no necesita Internet allí).
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

/// Tiempo máximo para conectar (incluye escribir la contraseña de SSH si la pide).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);
/// Intentos con otro puerto remoto si el elegido está ocupado.
const PORT_TRIES: u32 = 3;

/// Argumentos de `pcode remoto`.
#[derive(Debug, clap::Args)]
pub struct RemoteArgs {
    /// Destino SSH: `usuario@servidor` o un `Host` de `~/.ssh/config`.
    pub destino: Option<String>,
    /// Lista los servidores de `~/.ssh/config` y sale.
    #[arg(long = "listar")]
    pub list: bool,
    /// Carpeta del proyecto en el servidor (relativa a la de inicio, absoluta o con `~/`).
    #[arg(long = "ruta", default_value = ".")]
    pub ruta: String,
    /// Puerto local del túnel (0 = uno libre).
    #[arg(long = "puerto-local", default_value_t = 0)]
    pub local_port: u16,
    /// No abrir el navegador ni el IDE: solo imprimir la dirección.
    #[arg(long = "no-abrir")]
    pub no_open: bool,
    /// Copia este `pcode` a `~/.local/bin` del servidor si allí falta o tiene otra versión.
    #[arg(long = "instalar")]
    pub install: bool,
    /// Abre la TUI en el servidor (`ssh -t`) en vez de Studio.
    #[arg(long = "tui", conflicts_with = "ide")]
    pub tui: bool,
    /// Abre el IDE nativo (`pcode-ide`) en vez del navegador.
    #[arg(long = "ide")]
    pub ide: bool,
    /// Reenvía también este puerto del servidor a este equipo (p. ej. 11434 para usar su Ollama).
    #[arg(long = "reenviar")]
    pub forward: Vec<u16>,
}

/// Lo que se sabe del servidor tras la sonda.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probe {
    /// `uname -s` (p. ej. `Linux`).
    pub os: String,
    /// `uname -m` (p. ej. `x86_64`).
    pub arch: String,
    /// Ruta absoluta de `pcode` allí, si lo tiene.
    pub bin: Option<String>,
    /// Lo que imprime `pcode --version` allí.
    pub version: Option<String>,
}

/// Sonda en `sh`: sistema, arquitectura y dónde está `pcode` (también en `~/.local/bin`, que las
/// sesiones no interactivas de SSH no suelen tener en el `PATH`).
pub const PROBE_SCRIPT: &str = r#"p=$(command -v pcode 2>/dev/null); if [ -z "$p" ] && [ -x "$HOME/.local/bin/pcode" ]; then p="$HOME/.local/bin/pcode"; fi; echo "SO=$(uname -s)"; echo "ARQ=$(uname -m)"; echo "BIN=$p"; if [ -n "$p" ]; then echo "VER=$("$p" --version 2>/dev/null)"; fi"#;

/// Copia lo que llega por stdin a `~/.local/bin/pcode` (con renombrado atómico) e imprime la ruta.
pub const INSTALL_SCRIPT: &str = r#"set -e; d="$HOME/.local/bin"; mkdir -p "$d"; cat > "$d/.pcode.nuevo"; chmod 755 "$d/.pcode.nuevo"; mv -f "$d/.pcode.nuevo" "$d/pcode"; echo "$d/pcode""#;

/// Lee la salida de [`PROBE_SCRIPT`].
pub fn parse_probe(out: &str) -> Probe {
    let mut p = Probe::default();
    for l in out.lines() {
        let l = l.trim_end_matches('\r');
        let non_empty = |v: &str| (!v.trim().is_empty()).then(|| v.trim().to_string());
        if let Some(v) = l.strip_prefix("SO=") {
            p.os = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("ARQ=") {
            p.arch = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("BIN=") {
            p.bin = non_empty(v);
        } else if let Some(v) = l.strip_prefix("VER=") {
            p.version = non_empty(v);
        }
    }
    p
}

/// Si este binario sirve en el servidor: mismo sistema (Linux) y misma arquitectura.
pub fn same_target(p: &Probe, local_os: &str, local_arch: &str) -> bool {
    let arch = match p.arch.as_str() {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        a => a,
    };
    local_os == "linux" && p.os == "Linux" && arch == local_arch
}

/// `cd` a la carpeta del proyecto con las comillas de `sh` (`~/` se expande; `.` no hace nada).
pub fn cd_command(dir: &str) -> String {
    match dir {
        "" | "." => String::new(),
        "~" => "cd \"$HOME\" && ".into(),
        d => match d.strip_prefix("~/") {
            Some(rest) => format!("cd \"$HOME\"/{} && ", shell_words::quote(rest)),
            None => format!("cd {} && ", shell_words::quote(d)),
        },
    }
}

/// Orden remota de Studio: solo en loopback, en `port`, y se cierra cuando se corta la conexión.
pub fn remote_studio_command(bin: &str, dir: &str, port: u16) -> String {
    format!(
        "{}exec {} studio --no-abrir --puerto {port} --hasta-eof",
        cd_command(dir),
        shell_words::quote(bin)
    )
}

/// Argumentos de `ssh` para el túnel de Studio (y los puertos extra) y la orden remota.
pub fn ssh_tunnel_args(
    dest: &str,
    local: u16,
    remote: u16,
    forwards: &[u16],
    cmd: &str,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-L".into(),
        format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
    ];
    for f in forwards {
        a.push("-L".into());
        a.push(format!("127.0.0.1:{f}:127.0.0.1:{f}"));
    }
    a.push(dest.into());
    a.push(cmd.into());
    a
}

/// Si una línea del Studio remoto trae su dirección, la misma con el puerto local del túnel.
pub fn tunnel_url(line: &str, local: u16) -> Option<String> {
    let i = line.find("/?t=")?;
    if !line[..i].contains("http://127.0.0.1:") {
        return None;
    }
    let token: String = line[i + 4..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    (token.len() >= 32).then(|| format!("http://127.0.0.1:{local}/?t={token}"))
}

/// Nombres de las líneas `Host` de un `~/.ssh/config` (sin comodines, sin repetir).
pub fn ssh_hosts(config: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for l in config.lines() {
        let mut w = l.split_whitespace();
        if !w.next().is_some_and(|k| k.eq_ignore_ascii_case("host")) {
            continue;
        }
        for h in w {
            if h.contains(['*', '?', '!']) || out.iter().any(|x| x == h) {
                continue;
            }
            out.push(h.to_string());
        }
    }
    out
}

/// Un destino de SSH razonable (que no se pueda confundir con una opción).
pub fn valid_destination(d: &str) -> bool {
    !d.is_empty() && !d.starts_with('-') && !d.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Un puerto remoto entre 20000 y 59999, distinto en cada intento.
fn random_port() -> u16 {
    let b = uuid::Uuid::new_v4();
    let n = u16::from_le_bytes([b.as_bytes()[0], b.as_bytes()[1]]);
    20000 + n % 40000
}

/// Un puerto libre en loopback.
fn free_local_port() -> anyhow::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

fn ssh_bin() -> String {
    std::env::var("PCODE_SSH").unwrap_or_else(|_| "ssh".into())
}

/// Ejecuta una orden en el servidor con `stdin` dado y devuelve su salida estándar.
async fn ssh_output(dest: &str, script: &str, stdin: Option<Vec<u8>>) -> anyhow::Result<String> {
    let mut c = tokio::process::Command::new(ssh_bin());
    c.arg(dest)
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .kill_on_drop(true);
    let mut child = c
        .spawn()
        .map_err(|e| anyhow::anyhow!("no se pudo lanzar ssh: {e}"))?;
    if let (Some(data), Some(mut w)) = (stdin, child.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        w.write_all(&data).await?;
        w.shutdown().await?;
    }
    let out = tokio::time::timeout(CONNECT_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("el servidor no contestó a tiempo"))??;
    if !out.status.success() {
        anyhow::bail!("ssh {dest} falló ({})", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `pcode remoto`.
pub async fn remote(mut a: RemoteArgs) -> anyhow::Result<i32> {
    if a.list {
        let cfg = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".ssh/config"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        for h in ssh_hosts(&cfg) {
            println!("{h}");
        }
        return Ok(0);
    }
    let Some(dest) = a.destino.take() else {
        eprintln!("indica el servidor: pcode remoto usuario@servidor (o --listar)");
        return Ok(2);
    };
    if !valid_destination(&dest) {
        eprintln!("destino SSH no válido: «{dest}»");
        return Ok(2);
    }

    let local_version = format!("pcode {}", env!("CARGO_PKG_VERSION"));
    eprintln!("Conectando con {}…", dest);
    let probe = parse_probe(&ssh_output(&dest, PROBE_SCRIPT, None).await?);
    let outdated = probe.version.as_deref() != Some(local_version.as_str());
    let bin = match (&probe.bin, a.install) {
        (Some(b), false) => {
            if outdated {
                eprintln!(
                    "aviso: el servidor tiene {} y aquí {local_version}; --instalar lo iguala",
                    probe
                        .version
                        .as_deref()
                        .unwrap_or("una versión desconocida")
                );
            }
            b.clone()
        }
        (Some(b), true) if !outdated => b.clone(),
        (_, true) => {
            if !same_target(&probe, std::env::consts::OS, std::env::consts::ARCH) {
                eprintln!(
                    "El servidor es {} {} y este pcode es {} {}: instálalo allí con install.sh",
                    probe.os,
                    probe.arch,
                    std::env::consts::OS,
                    std::env::consts::ARCH
                );
                return Ok(2);
            }
            eprintln!("Copiando {local_version} al servidor…");
            let me = tokio::fs::read(std::env::current_exe()?).await?;
            let path = ssh_output(&dest, INSTALL_SCRIPT, Some(me)).await?;
            path.lines().last().unwrap_or_default().trim().to_string()
        }
        (None, false) => {
            eprintln!(
                "El servidor no tiene pcode. Repite con --instalar para copiarle este binario, o instálalo allí con install.sh."
            );
            return Ok(2);
        }
    };
    if a.tui {
        let cmd = format!("{}exec {}", cd_command(&a.ruta), shell_words::quote(&bin));
        let st = tokio::process::Command::new(ssh_bin())
            .args(["-t", &dest, &cmd])
            .status()
            .await?;
        return Ok(st.code().unwrap_or(2));
    }
    let local = if a.local_port == 0 {
        free_local_port()?
    } else {
        a.local_port
    };
    for attempt in 1..=PORT_TRIES {
        let rport = random_port();
        let cmd = remote_studio_command(&bin, &a.ruta, rport);
        let mut child = tokio::process::Command::new(ssh_bin())
            .args(ssh_tunnel_args(&dest, local, rport, &a.forward, &cmd))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| anyhow::anyhow!("no se pudo lanzar ssh: {e}"))?;
        // El stdin del túnel se queda abierto: al cerrarse (salir de aquí), el Studio remoto acaba.
        let _keep = child.stdin.take();
        let mut lines = BufReader::new(child.stderr.take().expect("stderr")).lines();
        let mut seen = vec![];
        let found = tokio::time::timeout(CONNECT_TIMEOUT, async {
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(u) = tunnel_url(&l, local) {
                    return Some(u);
                }
                if seen.len() < 50 {
                    seen.push(l);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
        let Some(url) = found else {
            let _ = child.kill().await;
            let busy = seen
                .iter()
                .any(|l| l.contains("in use") || l.contains("en uso"));
            if busy && attempt < PORT_TRIES {
                continue;
            }
            for l in &seen {
                eprintln!("{l}");
            }
            eprintln!("No se pudo arrancar Studio en {}", dest);
            return Ok(2);
        };
        wait_port(local).await;
        eprintln!(
            "PublicCode Studio en {} (túnel SSH): {url}\n(Ctrl+C para cerrar)",
            dest
        );
        if !a.no_open {
            if a.ide {
                crate::open_ide(&url);
            } else {
                crate::open_browser(&url);
            }
        }
        // Lo que diga el Studio remoto sigue saliendo aquí.
        let pump = tokio::spawn(async move {
            while let Ok(Some(l)) = lines.next_line().await {
                eprintln!("[remoto] {l}");
            }
        });
        let code = tokio::select! {
            st = child.wait() => st.ok().and_then(|s| s.code()).unwrap_or(0),
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                0
            }
        };
        pump.abort();
        return Ok(code);
    }
    Ok(2)
}

/// Espera (como mucho 10 s) a que el túnel acepte conexiones.
async fn wait_port(port: u16) {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_de_ssh_config() {
        let c = "Host 37.27.19.116\n  HostName 37.27.19.116\nhost web db\nHost web\n";
        assert_eq!(ssh_hosts(c), ["37.27.19.116", "web", "db"]);
    }

    #[test]
    fn hosts_de_ssh_config_sin_comodines() {
        assert_eq!(ssh_hosts("Host *\nHost a* b !c\n"), ["b"]);
    }

    #[test]
    fn sonda_del_servidor() {
        let p = parse_probe(
            "SO=Linux\r\nARQ=x86_64\nBIN=/home/ana/.local/bin/pcode\nVER=pcode 1.1.0\n",
        );
        assert_eq!(p.os, "Linux");
        assert_eq!(p.bin.as_deref(), Some("/home/ana/.local/bin/pcode"));
        assert_eq!(p.version.as_deref(), Some("pcode 1.1.0"));
        assert!(same_target(&p, "linux", "x86_64"));
        assert!(!same_target(&p, "linux", "aarch64"));
        assert!(!same_target(&p, "windows", "x86_64"));
        let vacio = parse_probe("SO=Linux\nARQ=arm64\nBIN=\n");
        assert_eq!(vacio.bin, None);
        assert!(same_target(&vacio, "linux", "aarch64"));
    }

    #[test]
    fn orden_remota_con_comillas() {
        assert_eq!(
            remote_studio_command("/opt/pcode", ".", 23456),
            "exec /opt/pcode studio --no-abrir --puerto 23456 --hasta-eof"
        );
        assert_eq!(
            remote_studio_command("/home/a b/pcode", "~/mis proyectos/web", 2),
            "cd \"$HOME\"/'mis proyectos/web' && exec '/home/a b/pcode' studio --no-abrir --puerto 2 --hasta-eof"
        );
        assert_eq!(cd_command("/srv/x;rm -rf ~"), "cd '/srv/x;rm -rf ~' && ");
    }

    #[test]
    fn tunel_y_url_local() {
        let a = ssh_tunnel_args("ana@srv", 4000, 25000, &[11434], "exec pcode");
        assert_eq!(
            a,
            [
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "ServerAliveInterval=30",
                "-L",
                "127.0.0.1:4000:127.0.0.1:25000",
                "-L",
                "127.0.0.1:11434:127.0.0.1:11434",
                "ana@srv",
                "exec pcode"
            ]
        );
        let tok = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            tunnel_url(
                &format!("PublicCode Studio: http://127.0.0.1:25000/?t={tok}"),
                4000
            ),
            Some(format!("http://127.0.0.1:4000/?t={tok}"))
        );
        assert_eq!(tunnel_url("http://127.0.0.1:1/?t=corto", 4000), None);
        assert_eq!(tunnel_url("otra cosa", 4000), None);
        assert!(valid_destination("ana@srv.example"));
        assert!(!valid_destination("-oProxyCommand=x"));
        assert!(!valid_destination("a b"));
        let p = random_port();
        assert!((20000..60000).contains(&p));
    }
}
