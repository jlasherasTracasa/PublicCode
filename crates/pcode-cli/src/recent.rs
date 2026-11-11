//! Proyectos recientes de `pcode ide` (`recientes.txt` junto a la configuración global).
use std::path::{Path, PathBuf};

/// Cuántos se recuerdan.
pub const MAX_RECENT: usize = 10;

/// Rutas de un `recientes.txt` (una por línea; sin vacías).
pub fn parse(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// `p` arriba, sin repetir, como mucho [`MAX_RECENT`].
pub fn push(list: &[PathBuf], p: &Path) -> Vec<PathBuf> {
    let mut v = vec![p.to_path_buf()];
    v.extend(list.iter().filter(|x| x.as_path() != p).cloned());
    v.truncate(MAX_RECENT);
    v
}

/// Texto del archivo.
pub fn render(list: &[PathBuf]) -> String {
    list.iter().map(|p| format!("{}\n", p.display())).collect()
}

fn file() -> Option<PathBuf> {
    pcode_config::user_config_path().and_then(|p| p.parent().map(|d| d.join("recientes.txt")))
}

/// Recientes que aún existen.
pub fn load() -> Vec<PathBuf> {
    file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| parse(&t).into_iter().filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

/// Guarda la lista.
pub fn save(list: &[PathBuf]) -> std::io::Result<()> {
    let Some(f) = file() else { return Ok(()) };
    if let Some(d) = f.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(f, render(list))
}

/// Carpeta en la que abrir el IDE: la pedida; si no hay y estamos en `$HOME`, la más reciente.
pub fn choose(
    asked: Option<&Path>,
    cwd: &Path,
    home: Option<&Path>,
    recent: &[PathBuf],
) -> PathBuf {
    match asked {
        Some(p) => p.to_path_buf(),
        None if home == Some(cwd) => recent.first().cloned().unwrap_or_else(|| cwd.to_path_buf()),
        None => cwd.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recientes_sin_repetir_y_el_ultimo_primero() {
        let l = parse("/a\n/b\n");
        let l = push(&l, Path::new("/b"));
        assert_eq!(l, [PathBuf::from("/b"), PathBuf::from("/a")]);
        assert_eq!(render(&l), "/b\n/a\n");
    }

    #[test]
    fn recientes_como_mucho_diez() {
        let mut l = vec![];
        for i in 0..15 {
            l = push(&l, Path::new(&format!("/p{i}")));
        }
        assert_eq!(l.len(), MAX_RECENT);
        assert_eq!(l[0], PathBuf::from("/p14"));
    }

    #[test]
    fn recientes_ignora_lineas_vacias() {
        assert_eq!(parse("\n  \n/x\n"), [PathBuf::from("/x")]);
    }

    #[test]
    fn carpeta_del_ide() {
        let home = Path::new("/home/ana");
        let rec = vec![PathBuf::from("/home/ana/web")];
        assert_eq!(
            choose(None, home, Some(home), &rec),
            PathBuf::from("/home/ana/web")
        );
        assert_eq!(
            choose(None, Path::new("/srv/x"), Some(home), &rec),
            PathBuf::from("/srv/x")
        );
        assert_eq!(
            choose(Some(Path::new("/y")), home, Some(home), &rec),
            PathBuf::from("/y")
        );
        assert_eq!(
            choose(None, home, Some(home), &[]),
            PathBuf::from("/home/ana")
        );
    }
}
