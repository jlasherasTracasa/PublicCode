//! Abrir otra carpeta desde el IDE: navegar por carpetas y lanzar `pcode ide <ruta>`.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Estado del selector de carpetas.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderPicker {
    pub dir: PathBuf,
    pub entries: Vec<String>,
    pub filter: String,
    pub selected: usize,
}

impl FolderPicker {
    /// Selector que empieza en `start`, con el filtro vacío y las subcarpetas leídas.
    pub fn new(start: &Path) -> FolderPicker {
        let mut picker = FolderPicker {
            dir: start.to_path_buf(),
            entries: vec![],
            filter: String::new(),
            selected: 0,
        };
        picker.refresh();
        picker
    }

    /// Vuelve a leer las subcarpetas de `dir` (sin ocultas, ordenadas sin mayúsculas) y pone la selección a 0.
    pub fn refresh(&mut self) {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        names.sort_by_key(|n| n.to_lowercase());
        self.entries = names;
        self.selected = 0;
    }

    /// Las entradas que contienen el filtro (sin distinguir mayúsculas), en orden.
    pub fn visible(&self) -> Vec<&str> {
        let f = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.to_lowercase().contains(&f))
            .map(String::as_str)
            .collect()
    }

    /// Entra en la subcarpeta `name`, quita el filtro y vuelve a leer.
    pub fn enter(&mut self, name: &str) {
        self.dir = self.dir.join(name);
        self.filter.clear();
        self.refresh();
    }

    /// Sube a la carpeta padre si la hay, y vuelve a leer.
    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            self.dir = parent;
            self.refresh();
        }
    }

    /// Suma `delta` a la selección, sin salirse de `0..visible().len()` (con lista vacía, 0).
    pub fn move_selection(&mut self, delta: i32) {
        let n = self.visible().len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        let max = (n - 1) as i64;
        self.selected = (self.selected as i64 + delta as i64).clamp(0, max) as usize;
    }

    /// La carpeta seleccionada, o `dir` si no hay ninguna visible.
    pub fn selected_dir(&self) -> PathBuf {
        match self.visible().get(self.selected) {
            Some(name) => self.dir.join(name),
            None => self.dir.clone(),
        }
    }
}

/// Proyectos recientes de un `recientes.txt` que aún existen.
pub fn parse_recent(text: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = PathBuf::from(line);
        if p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Proyectos recientes guardados por `pcode ide` (vacío si no hay configuración o archivo).
pub fn load_recent() -> Vec<PathBuf> {
    let file =
        pcode_config::user_config_path().and_then(|p| p.parent().map(|d| d.join("recientes.txt")));
    match file {
        Some(f) => std::fs::read_to_string(f)
            .map(|t| parse_recent(&t))
            .unwrap_or_default(),
        None => vec![],
    }
}

/// Orden `pcode ide <dir>`.
pub fn open_folder_command(pcode: &Path, dir: &Path) -> Command {
    let mut cmd = Command::new(pcode);
    cmd.arg("ide").arg(dir).stdin(Stdio::null());
    cmd
}

/// Abre `dir` en otra ventana del IDE (no espera).
pub fn open_folder(pcode: &Path, dir: &Path) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no es una carpeta",
        ));
    }
    open_folder_command(pcode, dir).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Carpeta temporal propia de cada test (sin `tempfile`, que no está en este crate).
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pcode-ide-folder-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lista_solo_subcarpetas_visibles() {
        let root = scratch("lista");
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::create_dir_all(root.join("A")).unwrap();
        std::fs::create_dir_all(root.join(".oculta")).unwrap();
        std::fs::write(root.join("c.txt"), "x").unwrap();
        let p = FolderPicker::new(&root);
        assert_eq!(p.entries, ["A", "b"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filtra_sin_mayusculas() {
        let root = scratch("filtro");
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::create_dir_all(root.join("A")).unwrap();
        let mut p = FolderPicker::new(&root);
        p.filter = "B".to_string();
        assert_eq!(p.visible(), ["b"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn entrar_y_subir() {
        let root = scratch("entrar");
        std::fs::create_dir_all(root.join("A")).unwrap();
        let mut p = FolderPicker::new(&root);
        p.filter = "a".to_string();
        p.enter("A");
        assert!(p.dir.ends_with("A"));
        assert!(p.filter.is_empty());
        p.up();
        assert_eq!(p.dir, root);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn seleccion_sin_salirse() {
        let root = scratch("seleccion");
        std::fs::create_dir_all(root.join("A")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        let mut p = FolderPicker::new(&root);
        p.move_selection(5);
        assert_eq!(p.selected, 1);
        p.move_selection(-9);
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_dir(), root.join("A"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn recientes_solo_los_que_existen() {
        let root = scratch("recientes");
        let text = format!(
            "{}\n/no/existe/pcode-ide-x\n\n{}\n",
            root.display(),
            root.display()
        );
        assert_eq!(parse_recent(&text), vec![root.clone()]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn orden_de_abrir() {
        let cmd = open_folder_command(Path::new("/x/pcode"), Path::new("/p"));
        assert_eq!(cmd.get_program(), "/x/pcode");
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, ["ide", "/p"]);
    }

    #[test]
    fn abrir_un_archivo_es_error() {
        let root = scratch("archivo");
        let file = root.join("c.txt");
        std::fs::write(&file, "x").unwrap();
        let e = open_folder(Path::new("/x/pcode"), &file).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
