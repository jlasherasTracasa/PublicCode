//! Binario `pcode`.
mod ask;
mod backend_run;
mod chat;
mod doctor;
mod flow_cmd;
mod launcher;
mod provider;
mod recent;
mod remote;
mod run;
mod spec_cmd;
mod update;

use clap::{Parser, Subcommand};

/// PublicCode: programación agéntica con varios agentes.
#[derive(Debug, Parser)]
#[command(name = "pcode", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
    /// Sin orden: abre la TUI retomando la última conversación del proyecto.
    #[arg(long = "continuar")]
    resume: bool,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Interfaz de terminal a pantalla completa (es lo que abre `pcode` solo, en una terminal).
    Tui(chat::ChatArgs),
    /// Conversación con el agente en modo línea (sirve también sin terminal, por stdin).
    Chat(chat::ChatArgs),
    /// Ejecuta una tarea con el bucle del agente.
    Run(run::RunArgs),
    /// Pregunta al modelo sin tools ni cambios (`@archivo` adjunta; lo que llegue por stdin, también).
    #[command(alias = "ask")]
    Preguntar(ask::AskArgs),
    /// Arranca el demonio (API JSON-RPC, eventos y página web); por defecto solo en loopback.
    Daemon {
        /// Puerto.
        #[arg(long, default_value_t = 7878)]
        puerto: u16,
        /// Dirección IP en la que escuchar (p. ej. `0.0.0.0` para la app Android en la red local).
        #[arg(long, default_value = "127.0.0.1")]
        escuchar: std::net::IpAddr,
    },
    /// Flujos de tareas: DAG, aceptación, reintentos e integración.
    #[command(subcommand)]
    Flow(flow_cmd::FlowCmd),
    /// Desarrollo guiado por especificaciones: escribes `specs/<nombre>/spec.md` y PublicCode
    /// planifica, implementa y verifica (sin subcomando, lista las especificaciones).
    Spec {
        #[command(subcommand)]
        cmd: Option<spec_cmd::SpecCmd>,
    },
    /// PublicCode Studio: editor, git, terminal y agente en el navegador, solo en este equipo.
    Studio {
        /// Puerto (0 = uno libre).
        #[arg(long, default_value_t = 0)]
        puerto: u16,
        /// No abrir el navegador; solo imprimir la dirección.
        #[arg(long = "no-abrir")]
        no_open: bool,
        /// Terminar cuando se cierre la entrada estándar (lo usa `pcode remoto` y el IDE nativo).
        #[arg(long = "hasta-eof", hide = true)]
        until_eof: bool,
    },
    /// IDE nativo (sin navegador): abre `pcode-ide` sobre el Studio de este proyecto.
    Ide {
        /// Carpeta del proyecto (por defecto, la actual; desde el menú, la última abierta).
        ruta: Option<std::path::PathBuf>,
        /// Puerto del demonio (0 = uno libre).
        #[arg(long, default_value_t = 0)]
        puerto: u16,
    },
    /// PublicCode en el menú de aplicaciones (Linux).
    #[command(subcommand)]
    Lanzador(launcher::LauncherCmd),
    /// Extensiones de VS Code (solo temas, fragmentos y lenguajes; nada ejecutable).
    #[command(subcommand)]
    Extensiones(ExtCmd),
    /// Trabaja en otro equipo por SSH: Studio (o la TUI, o el IDE) con lo pesado en el servidor.
    Remoto(remote::RemoteArgs),
    /// Servidores MCP configurados y las tools que aporta cada uno.
    Mcp {
        /// Configuración del proyecto.
        #[arg(long = "config", default_value = "pcode.toml")]
        config: std::path::PathBuf,
    },
    /// Comprueba la instalación: configuración, modelo, claves, git, sandbox y agentes externos.
    Doctor {
        /// Configuración del proyecto.
        #[arg(long = "config", default_value = "pcode.toml")]
        config: std::path::PathBuf,
    },
    /// Configuración global del usuario.
    #[command(subcommand)]
    Config(doctor::ConfigCmd),
    /// Descarga e instala la última versión publicada.
    Actualizar {
        /// Solo comprueba si hay versión nueva.
        #[arg(long = "comprobar")]
        check: bool,
    },
    /// Mini sandboxes (overlay copy-on-write sin privilegios).
    #[command(subcommand)]
    Sandbox(backend_run::SandboxCmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    if matches!(
        cli.command,
        Some(Cmd::Flow(_)) | Some(Cmd::Sandbox(_)) | Some(Cmd::Run(_)) | Some(Cmd::Spec { .. })
    ) {
        choose_sandbox_level();
    }
    match cli.command {
        Some(Cmd::Tui(a)) => std::process::exit(chat::tui(a).await?),
        Some(Cmd::Chat(a)) => std::process::exit(chat::chat(a).await?),
        Some(Cmd::Run(a)) => std::process::exit(run::run(a).await?),
        Some(Cmd::Preguntar(a)) => std::process::exit(ask::ask(a).await?),
        Some(Cmd::Daemon { puerto, escuchar }) => {
            // Un token fijo (PCODE_DAEMON_TOKEN) evita volver a emparejar el móvil en cada arranque.
            let token = std::env::var("PCODE_DAEMON_TOKEN")
                .ok()
                .filter(|t| t.len() >= 32)
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                });
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            apply_lsp_config(&st);
            let host = if escuchar.is_unspecified() {
                "<IP-de-este-equipo>".to_string()
            } else {
                escuchar.to_string()
            };
            eprintln!("pcoded: http://{host}:{puerto}/?t={token}");
            if !escuchar.is_loopback() {
                eprintln!(
                    "aviso: escuchando fuera de loopback sin TLS; úsalo solo en una red de confianza o tras un túnel (WireGuard, Tailscale)"
                );
            }
            pcode_server::serve(st, std::net::SocketAddr::new(escuchar, puerto)).await?;
            Ok(())
        }
        Some(Cmd::Studio {
            puerto,
            no_open,
            until_eof,
        }) => {
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            apply_lsp_config(&st);
            let l = tokio::net::TcpListener::bind(("127.0.0.1", puerto)).await?;
            let url = format!("http://{}/?t={token}", l.local_addr()?);
            eprintln!("PublicCode Studio: {url}\n(Ctrl+C para cerrar)");
            if !no_open {
                open_browser(&url);
            }
            if until_eof {
                tokio::select! {
                    r = pcode_server::serve_listener(st, l) => r?,
                    _ = stdin_closed() => {}
                }
            } else {
                pcode_server::serve_listener(st, l).await?;
            }
            Ok(())
        }
        Some(Cmd::Ide { ruta, puerto }) => {
            let cwd = std::env::current_dir()?;
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let recent = recent::load();
            let dir = recent::choose(ruta.as_deref(), &cwd, home.as_deref(), &recent);
            let dir = if dir.is_file() {
                dir.parent().map(|d| d.to_path_buf()).unwrap_or(dir)
            } else {
                dir
            };
            std::env::set_current_dir(&dir)?;
            if home.as_deref() != Some(dir.as_path()) {
                let _ = recent::save(&recent::push(
                    &recent,
                    &dir.canonicalize().unwrap_or(dir.clone()),
                ));
            }
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let st = pcode_server::AppState::new(
                std::env::current_dir()?,
                std::env::current_exe()?,
                Some(token.clone()),
            );
            apply_lsp_config(&st);
            let l = tokio::net::TcpListener::bind(("127.0.0.1", puerto)).await?;
            let url = format!("http://{}/?t={token}", l.local_addr()?);
            let Some(ide) = spawn_ide(&url, false) else {
                eprintln!(
                    "No se encontró pcode-ide (instálalo junto a pcode). Mientras, usa: pcode studio"
                );
                std::process::exit(2);
            };
            // El demonio vive lo que viva la ventana del IDE. Si no hay OpenGL, se queda sirviendo
            // Studio en el navegador hasta Ctrl+C.
            let ventana = async {
                let en_navegador = tokio::task::spawn_blocking(move || supervise_ide(ide, &url))
                    .await
                    .unwrap_or(false);
                if en_navegador {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                r = pcode_server::serve_listener(st, l) => r?,
                _ = ventana => {}
            }
            Ok(())
        }
        Some(Cmd::Lanzador(c)) => std::process::exit(launcher::launcher_cmd(c)?),
        Some(Cmd::Extensiones(c)) => std::process::exit(ext_cmd(c)?),
        Some(Cmd::Remoto(a)) => std::process::exit(remote::remote(a).await?),
        Some(Cmd::Mcp { config }) => std::process::exit(doctor::mcp_cmd(&config).await?),
        Some(Cmd::Doctor { config }) => std::process::exit(doctor::doctor(&config)),
        Some(Cmd::Config(c)) => std::process::exit(doctor::config_cmd(c).await?),
        Some(Cmd::Actualizar { check }) => std::process::exit(update::update(check).await?),
        Some(Cmd::Flow(c)) => std::process::exit(flow_cmd::flow_cmd(c).await?),
        Some(Cmd::Spec { cmd }) => std::process::exit(spec_cmd::spec_cmd(cmd).await?),
        Some(Cmd::Sandbox(c)) => std::process::exit(backend_run::sandbox_cmd(c).await?),
        None => {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                let mut a = chat::ChatArgs::default_args();
                a.resume = cli.resume;
                std::process::exit(chat::tui(a).await?);
            }
            use clap::CommandFactory;
            // Si la salida se cierra antes (p. ej. `| head`), no es un error.
            let _ = Cli::command().print_help();
            println!();
            Ok(())
        }
    }
}

/// Elige el nivel de las mini sandboxes: N1 (overlay en namespaces de usuario) si se puede, o
/// N0 (copia de trabajo con `git worktree`) si se pide (`PCODE_SANDBOX=N0` o
/// `[sandbox] nivel_por_defecto = "N0"`) o si el sistema no tiene namespaces (Windows).
fn choose_sandbox_level() {
    let wanted = std::env::var("PCODE_SANDBOX").ok().or_else(|| {
        pcode_config::Config::load_layered(std::path::Path::new("pcode.toml"))
            .ok()
            .map(|(c, _)| c.sandbox.default_level)
    });
    let n0 = wanted.as_deref() == Some("N0") || !pcode_sandbox::userns_available();
    pcode_sandbox::set_default_n0(n0);
    if n0 {
        tracing::info!("sandbox N0 (git worktree): sin aislamiento de red ni de disco");
    }
}

/// Aplica `[lsp.*]` y `[recursos]` de la configuración (global y del proyecto) al demonio.
fn apply_lsp_config(st: &pcode_server::AppState) {
    if let Ok((cfg, _)) = pcode_config::Config::load_layered(std::path::Path::new("pcode.toml")) {
        let r = cfg.resources.resolve(pcode_config::SystemInfo::detect());
        st.lsp.set_limits(pcode_lsp::LspLimits {
            max: r.lsp_max,
            idle: (r.lsp_idle_min > 0).then(|| std::time::Duration::from_secs(r.lsp_idle_min * 60)),
            light: r.light,
        });
        st.lsp.set_overrides(
            cfg.lsp
                .into_iter()
                .filter(|(_, l)| !l.command.is_empty())
                .map(|(k, l)| (k, l.command))
                .collect(),
        );
    }
}

/// Subórdenes de `pcode extensiones`.
#[derive(Debug, Subcommand)]
enum ExtCmd {
    /// Instala desde un `.vsix` o una carpeta de extensión (p. ej. de `~/.vscode/extensions`).
    Instalar {
        /// Archivo `.vsix` o carpeta.
        origen: std::path::PathBuf,
    },
    /// Importa lo declarativo de todas las extensiones de VS Code instaladas.
    ImportarVscode,
    /// Lista las instaladas.
    Listar,
    /// Quita una (`editor.nombre`).
    Quitar {
        /// Identificador.
        id: String,
    },
}

fn ext_cmd(c: ExtCmd) -> anyhow::Result<i32> {
    let root = pcode_extensions::default_root()
        .ok_or_else(|| anyhow::anyhow!("no se encuentra la carpeta de datos del usuario"))?;
    let show = |m: &pcode_extensions::Manifest, w: &[String]| {
        println!(
            "✓ {} {} · {} temas · {} fragmentos · {} lenguajes",
            m.id,
            m.version,
            m.themes.len(),
            m.snippets.len(),
            m.languages.len()
        );
        for x in w {
            println!("  ⚠ {x}");
        }
    };
    match c {
        ExtCmd::Instalar { origen } => {
            let (m, w) = pcode_extensions::install(&origen, &root)?;
            show(&m, &w);
        }
        ExtCmd::ImportarVscode => {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default();
            let mut n = 0;
            for e in std::fs::read_dir(home.join(".vscode/extensions"))
                .into_iter()
                .flatten()
                .flatten()
            {
                if !e.path().join("package.json").is_file() {
                    continue;
                }
                match pcode_extensions::install(&e.path(), &root) {
                    Ok((m, w)) => {
                        show(&m, &w);
                        n += 1;
                    }
                    Err(err) => println!("✗ {}: {err}", e.file_name().to_string_lossy()),
                }
            }
            println!("{n} extensiones importadas en {}", root.display());
        }
        ExtCmd::Listar => {
            for m in pcode_extensions::list(&root) {
                show(&m, &[]);
            }
        }
        ExtCmd::Quitar { id } => {
            if !pcode_extensions::uninstall(&root, &id)? {
                eprintln!("no está instalada: {id}");
                return Ok(2);
            }
            println!("quitada: {id}");
        }
    }
    Ok(0)
}

/// Termina cuando la entrada estándar llega al final (o falla).
async fn stdin_closed() {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 256];
    let mut stdin = tokio::io::stdin();
    while let Ok(n) = stdin.read(&mut buf).await {
        if n == 0 {
            break;
        }
    }
}

/// Ruta de `pcode-ide`: al lado de este binario o en el `PATH`.
fn ide_path() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) {
        "pcode-ide.exe"
    } else {
        "pcode-ide"
    };
    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)));
    here.filter(|p| p.is_file()).or_else(|| {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|d| d.join(name))
                .find(|p| p.is_file())
        })
    })
}

/// Lanza el IDE nativo contra `url` (None si no está instalado).
///
/// Con `software` se fuerza el render por software (`LIBGL_ALWAYS_SOFTWARE=1`, llvmpipe), para
/// equipos sin aceleración o con el controlador roto.
fn spawn_ide(url: &str, software: bool) -> Option<std::process::Child> {
    let mut cmd = std::process::Command::new(ide_path()?);
    cmd.args(["--url", url, "--hasta-eof"])
        .stdin(std::process::Stdio::piped());
    if software {
        cmd.env("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    let mut child = cmd.spawn().ok()?;
    // El extremo de escritura vive lo que viva este proceso: si muere (incluso con SIGKILL), el
    // sistema lo cierra y el IDE ve el final de su entrada y termina.
    if let Some(w) = child.stdin.take() {
        std::mem::forget(w);
    }
    Some(child)
}

/// Código con el que `pcode-ide` sale si no pudo abrir la ventana con OpenGL.
const IDE_NO_GL: i32 = 3;

/// ¿Se relanza el IDE con render por software? Solo una vez, y solo si salió con [`IDE_NO_GL`]
/// enseguida (un fallo tardío no es de arranque: sería reabrir una ventana que el usuario usaba).
fn should_retry_software(code: Option<i32>, elapsed: std::time::Duration, tried: bool) -> bool {
    !tried && code == Some(IDE_NO_GL) && elapsed < std::time::Duration::from_secs(5)
}

/// Espera a que el IDE termine. Si no pudo abrir la ventana, lo relanza una vez por software y,
/// si tampoco, abre Studio en el navegador. Devuelve `true` si acabó en el navegador.
fn supervise_ide(mut ide: std::process::Child, url: &str) -> bool {
    let mut tried = false;
    loop {
        let t0 = std::time::Instant::now();
        let code = ide.wait().ok().and_then(|s| s.code());
        if should_retry_software(code, t0.elapsed(), tried) {
            tried = true;
            eprintln!("Sin aceleración OpenGL: se reintenta con render por software.");
            match spawn_ide(url, true) {
                Some(c) => {
                    ide = c;
                    continue;
                }
                None => return false,
            }
        }
        if tried && code == Some(IDE_NO_GL) {
            eprintln!("Sin OpenGL: se abre Studio en el navegador");
            eprintln!("{url}");
            open_browser(url);
            return true;
        }
        return false;
    }
}

/// Abre el IDE nativo; si no está, el navegador.
fn open_ide(url: &str) {
    if spawn_ide(url, false).is_none() {
        eprintln!("No se encontró pcode-ide: se abre el navegador.");
        open_browser(url);
    }
}

/// Abre `url` en el navegador del sistema (sin esperar; si falla, basta con la dirección impresa).
fn open_browser(url: &str) {
    let r = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    if r.is_err() {
        eprintln!("No se pudo abrir el navegador: abre la dirección a mano.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn reintento_software_solo_una_vez_y_si_fallo_rapido() {
        let pronto = Duration::from_secs(1);
        assert!(should_retry_software(Some(IDE_NO_GL), pronto, false));
        // Ya se probó por software: no se insiste.
        assert!(!should_retry_software(Some(IDE_NO_GL), pronto, true));
        // Otro código de salida (cierre normal, uso incorrecto, señal) no es falta de OpenGL.
        assert!(!should_retry_software(Some(0), pronto, false));
        assert!(!should_retry_software(Some(2), pronto, false));
        assert!(!should_retry_software(None, pronto, false));
        // Un fallo tardío no es de arranque.
        assert!(!should_retry_software(
            Some(IDE_NO_GL),
            Duration::from_secs(5),
            false
        ));
    }
}
