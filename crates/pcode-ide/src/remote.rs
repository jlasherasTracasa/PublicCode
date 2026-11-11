//! Conectar a otro equipo desde el IDE: lista los servidores de `~/.ssh/config` con
//! `pcode remoto --listar` y abre otra ventana con `pcode remoto <servidor> --ide`.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Ruta del binario `pcode` que acompaña a este `pcode-ide` (mismo directorio); si no se sabe
/// dónde está el IDE, `pcode` a secas (se buscará en el `PATH`).
pub fn pcode_path(ide_exe: Option<&Path>) -> PathBuf {
    let name = if cfg!(windows) { "pcode.exe" } else { "pcode" };
    match ide_exe.and_then(Path::parent) {
        Some(dir) => dir.join(name),
        None => PathBuf::from(name),
    }
}

/// Servidores de la salida de `pcode remoto --listar` (uno por línea), sin vacíos ni repetidos.
pub fn parse_hosts(stdout: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for l in stdout.lines().map(str::trim) {
        if !l.is_empty() && !out.iter().any(|x| x == l) {
            out.push(l.to_string());
        }
    }
    out
}

/// Un servidor que se puede pasar a `pcode remoto` sin que se confunda con una opción.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && !host.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Servidores conocidos (vacío si `pcode` no está o falla).
pub fn list_hosts(pcode: &Path) -> Vec<String> {
    Command::new(pcode)
        .args(["remoto", "--listar"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_hosts(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Abre el IDE contra `host` en un proceso aparte (no se espera a que termine).
pub fn connect(pcode: &Path, host: &str) -> std::io::Result<()> {
    if !valid_host(host) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "servidor no válido",
        ));
    }
    Command::new(pcode)
        .args(["remoto", host, "--ide"])
        .stdin(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcode_al_lado_del_ide() {
        let name = if cfg!(windows) { "pcode.exe" } else { "pcode" };
        assert_eq!(
            pcode_path(Some(Path::new("/opt/pc/bin/pcode-ide"))),
            Path::new("/opt/pc/bin").join(name)
        );
        assert_eq!(pcode_path(None), PathBuf::from(name));
    }

    #[test]
    fn hosts_de_la_salida_de_listar() {
        assert_eq!(
            parse_hosts("servidor\n  gpu \n\nservidor\nana@casa\n"),
            ["servidor", "gpu", "ana@casa"]
        );
        assert!(parse_hosts("").is_empty());
    }

    #[test]
    fn servidor_valido() {
        assert!(valid_host("ana@casa"));
        assert!(!valid_host(""));
        assert!(!valid_host("-oProxyCommand=x"));
        assert!(!valid_host("a b"));
    }

    #[test]
    fn conectar_rechaza_un_servidor_que_parece_opcion() {
        let e = connect(Path::new("pcode"), "-oProxyCommand=x").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn lista_con_un_pcode_falso() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pcode-ide-remoto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("pcode");
        std::fs::write(
            &bin,
            "#!/bin/sh\n[ \"$1 $2\" = \"remoto --listar\" ] && printf 'uno\\ndos\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(list_hosts(&bin), ["uno", "dos"]);
        assert!(list_hosts(&dir.join("no-existe")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
