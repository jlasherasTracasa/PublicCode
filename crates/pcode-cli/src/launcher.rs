//! `pcode lanzador instalar|quitar`: PublicCode en el menú de aplicaciones, como VS Code.
use std::path::{Path, PathBuf};

/// Entrada de escritorio (fuente única: `packaging/linux/pcode.desktop`).
pub const DESKTOP: &str = include_str!("../../../packaging/linux/pcode.desktop");
/// Icono.
pub const ICON: &str = include_str!("../../../packaging/linux/pcode.svg");

/// Subórdenes.
#[derive(Debug, clap::Subcommand)]
pub enum LauncherCmd {
    /// Añade PublicCode al menú de aplicaciones (y su icono).
    Instalar,
    /// Lo quita del menú.
    Quitar,
}

/// `$XDG_DATA_HOME` o `~/.local/share`.
pub fn data_dir(xdg_data_home: Option<&str>, home: &str) -> PathBuf {
    match xdg_data_home.filter(|x| !x.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => Path::new(home).join(".local/share"),
    }
}

/// La entrada con la ruta absoluta de este `pcode` en cada `Exec=` (entre comillas si hace falta).
pub fn desktop_entry(pcode_bin: &str) -> String {
    let q = if pcode_bin.contains([' ', '"', '\\']) {
        format!(
            "\"{}\"",
            pcode_bin.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        pcode_bin.to_string()
    };
    DESKTOP
        .lines()
        .map(|l| match l.strip_prefix("Exec=pcode") {
            Some(rest) => format!("Exec={q}{rest}"),
            None => match l.strip_prefix("Exec=x-terminal-emulator -e pcode") {
                Some(rest) => format!("Exec=x-terminal-emulator -e {q}{rest}"),
                None => l.to_string(),
            },
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn files(data: &Path) -> (PathBuf, PathBuf) {
    (
        data.join("applications/pcode.desktop"),
        data.join("icons/hicolor/scalable/apps/pcode.svg"),
    )
}

/// Escribe la entrada y el icono; devuelve las rutas.
pub fn install(data: &Path, pcode_bin: &str) -> std::io::Result<Vec<PathBuf>> {
    let (d, i) = files(data);
    for (p, text) in [(&d, desktop_entry(pcode_bin)), (&i, ICON.to_string())] {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, text)?;
    }
    Ok(vec![d, i])
}

/// Borra la entrada y el icono; devuelve cuántos había.
pub fn uninstall(data: &Path) -> std::io::Result<usize> {
    let (d, i) = files(data);
    let mut n = 0;
    for p in [d, i] {
        if p.exists() {
            std::fs::remove_file(p)?;
            n += 1;
        }
    }
    Ok(n)
}

/// `pcode lanzador …`.
pub fn launcher_cmd(c: LauncherCmd) -> anyhow::Result<i32> {
    if !cfg!(target_os = "linux") {
        println!("El lanzador del menú es para Linux; en Windows lo crea el instalador.");
        return Ok(0);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let data = data_dir(std::env::var("XDG_DATA_HOME").ok().as_deref(), &home);
    match c {
        LauncherCmd::Instalar => {
            let bin = std::env::current_exe()?.display().to_string();
            for p in install(&data, &bin)? {
                println!("✓ {}", p.display());
            }
            // Refresca el menú si la herramienta existe (no es obligatorio).
            let _ = std::process::Command::new("update-desktop-database")
                .arg(data.join("applications"))
                .output();
            println!("PublicCode ya está en el menú de aplicaciones.");
        }
        LauncherCmd::Quitar => println!("{} archivos quitados", uninstall(&data)?),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrada_con_ruta_absoluta() {
        let e = desktop_entry("/home/ana/.local/bin/pcode");
        assert!(e.contains("Exec=/home/ana/.local/bin/pcode ide %F\n"));
        assert!(e.contains("Exec=/home/ana/.local/bin/pcode studio\n"));
        assert!(e.contains("Exec=x-terminal-emulator -e /home/ana/.local/bin/pcode\n"));
        assert!(e.contains("Icon=pcode"));
        assert!(
            desktop_entry("/opt/mi pcode/pcode").contains("Exec=\"/opt/mi pcode/pcode\" ide %F")
        );
    }

    #[test]
    fn accion_modo_opencode_con_ruta_absoluta() {
        let e = desktop_entry("/home/ana/.local/bin/pcode");
        assert!(e.contains("Actions=studio;tui;opencode;"));
        assert!(e.contains("[Desktop Action opencode]"));
        assert!(e.contains(
            "Exec=x-terminal-emulator -e /home/ana/.local/bin/pcode tui --tema opencode\n"
        ));
    }

    #[test]
    fn instala_y_quita_en_xdg_data_home() {
        let d = tempfile::tempdir().unwrap();
        let data = data_dir(Some(d.path().to_str().unwrap()), "/no/usado");
        assert_eq!(data, d.path());
        assert_eq!(
            data_dir(None, "/home/ana"),
            Path::new("/home/ana/.local/share")
        );
        let p = install(&data, "/x/pcode").unwrap();
        assert!(p.iter().all(|f| f.is_file()));
        assert!(std::fs::read_to_string(&p[1]).unwrap().contains("<svg"));
        assert_eq!(uninstall(&data).unwrap(), 2);
        assert_eq!(uninstall(&data).unwrap(), 0);
    }
}
