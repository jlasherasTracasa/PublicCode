//! Buscar ejecutables en el `PATH`, igual en Linux y en Windows.
use std::ffi::OsStr;
use std::path::PathBuf;

/// Busca un ejecutable en el `PATH` (con las extensiones de `PATHEXT` en Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    which_in(name, std::env::var_os("PATH")?.as_os_str())
}

/// Como [`which`], pero en la lista de directorios `path` (con el formato de `PATH`).
pub fn which_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    // Una ruta (absoluta o con separador) no se busca: se comprueba tal cual.
    if name.contains('/') || name.contains('\\') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|e| e.to_lowercase())
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(path).find_map(|dir| {
        exts.iter()
            .map(|e| dir.join(format!("{name}{e}")))
            .find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encuentra_en_la_lista_y_no_fuera() {
        let d = tempfile::tempdir().unwrap();
        let exe = if cfg!(windows) {
            "agente.exe"
        } else {
            "agente"
        };
        std::fs::write(d.path().join(exe), "").unwrap();
        let path = std::env::join_paths([d.path()]).unwrap();
        assert_eq!(which_in("agente", &path), Some(d.path().join(exe)));
        assert_eq!(which_in("otro", &path), None);
        let full = d.path().join(exe);
        assert_eq!(
            which_in(full.to_str().unwrap(), OsStr::new("")),
            Some(full.clone())
        );
    }
}
