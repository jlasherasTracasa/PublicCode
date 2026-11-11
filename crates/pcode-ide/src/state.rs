//! Estado del IDE sin interfaz (se prueba sin ventana): pestañas, búsqueda difusa, árbol.
use crate::highlight::{Lang, lang_of};
use std::collections::{BTreeMap, BTreeSet};

/// Un archivo abierto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// Ruta relativa al proyecto.
    pub path: String,
    /// Texto en el editor.
    pub text: String,
    /// Texto tal como está en disco (para saber si hay cambios sin guardar).
    pub saved: String,
    /// Lenguaje para el resaltado.
    pub lang: Lang,
}

impl Tab {
    /// Si tiene cambios sin guardar.
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Nombre corto para la pestaña.
    pub fn title(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// Pestañas abiertas y cuál está activa.
#[derive(Debug, Default)]
pub struct Tabs {
    /// Pestañas en orden.
    pub list: Vec<Tab>,
    /// Índice de la activa (si hay alguna).
    pub active: usize,
}

impl Tabs {
    /// Abre `path` (o activa su pestaña si ya estaba) y devuelve su índice.
    pub fn open(&mut self, path: &str, text: String) -> usize {
        if let Some(i) = self.list.iter().position(|t| t.path == path) {
            self.active = i;
            return i;
        }
        self.list.push(Tab {
            path: path.to_string(),
            saved: text.clone(),
            text,
            lang: lang_of(path),
        });
        self.active = self.list.len() - 1;
        self.active
    }

    /// Cierra la pestaña `i`; la activa pasa a la de al lado.
    pub fn close(&mut self, i: usize) -> Option<Tab> {
        if i >= self.list.len() {
            return None;
        }
        let t = self.list.remove(i);
        if self.active >= self.list.len() {
            self.active = self.list.len().saturating_sub(1);
        } else if self.active > i {
            self.active -= 1;
        }
        Some(t)
    }

    /// La pestaña activa.
    pub fn current(&self) -> Option<&Tab> {
        self.list.get(self.active)
    }

    /// La pestaña activa, para editarla.
    pub fn current_mut(&mut self) -> Option<&mut Tab> {
        self.list.get_mut(self.active)
    }

    /// Marca `path` como guardado con `text` (lo que se escribió en disco).
    pub fn mark_saved(&mut self, path: &str, text: &str) {
        if let Some(t) = self.list.iter_mut().find(|t| t.path == path) {
            t.saved = text.to_string();
        }
    }

    /// Si alguna pestaña tiene cambios sin guardar.
    pub fn any_dirty(&self) -> bool {
        self.list.iter().any(Tab::dirty)
    }
}

/// Puntuación difusa de `query` en `cand` (mayor es mejor; None si no están todas las letras en
/// orden). Premia letras seguidas, el principio de un nombre y coincidir en el nombre del archivo.
pub fn fuzzy_score(query: &str, cand: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name_start = cand.rfind('/').map_or(0, |i| i + 1);
    let lc: Vec<char> = cand.chars().flat_map(char::to_lowercase).collect();
    let mut score = 0;
    let mut pos = 0usize;
    let mut prev: Option<usize> = None;
    for q in query.chars().flat_map(char::to_lowercase) {
        if q == ' ' {
            continue;
        }
        let i = (pos..lc.len()).find(|&i| lc[i] == q)?;
        score += 1;
        if prev == Some(i.wrapping_sub(1)) {
            score += 5;
        }
        if i == 0 || matches!(lc.get(i.wrapping_sub(1)), Some('/' | '_' | '-' | '.')) {
            score += 3;
        }
        if i >= name_start {
            score += 2;
        }
        prev = Some(i);
        pos = i + 1;
    }
    Some(score * 100 - cand.len() as i32)
}

/// Los `limit` archivos que mejor coinciden con `query`, de mejor a peor.
pub fn quick_open(query: &str, files: &[String], limit: usize) -> Vec<String> {
    let mut v: Vec<(i32, &String)> = files
        .iter()
        .filter_map(|f| fuzzy_score(query, f).map(|s| (s, f)))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    v.into_iter().take(limit).map(|(_, f)| f.clone()).collect()
}

/// Una entrada del explorador (lo que devuelve `fs.list`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Entry {
    /// Nombre.
    pub name: String,
    /// Ruta relativa.
    pub path: String,
    /// Si es carpeta.
    pub dir: bool,
}

/// Árbol del explorador: el contenido de cada carpeta se pide al desplegarla.
#[derive(Debug, Default)]
pub struct Tree {
    /// Carpeta → entradas (carpetas primero, luego por nombre).
    pub children: BTreeMap<String, Vec<Entry>>,
    /// Carpetas desplegadas.
    pub expanded: BTreeSet<String>,
}

impl Tree {
    /// Guarda el contenido de `dir`, ordenado.
    pub fn set(&mut self, dir: &str, mut entries: Vec<Entry>) {
        entries.sort_by(|a, b| {
            b.dir
                .cmp(&a.dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.children.insert(dir.to_string(), entries);
    }

    /// Despliega o pliega `dir`; devuelve `true` si hay que pedir su contenido.
    pub fn toggle(&mut self, dir: &str) -> bool {
        if !self.expanded.remove(dir) {
            self.expanded.insert(dir.to_string());
            return !self.children.contains_key(dir);
        }
        false
    }

    /// Olvida el contenido (tras cambios en disco); devuelve las carpetas que hay que volver a pedir.
    pub fn invalidate(&mut self) -> Vec<String> {
        self.children.clear();
        let mut v: Vec<String> = vec![String::new()];
        v.extend(self.expanded.iter().cloned());
        v
    }
}

/// Números de línea (`1\n2\n…`) alineados a la derecha.
pub fn gutter(text: &str) -> String {
    let n = text.split('\n').count();
    let w = n.to_string().len();
    (1..=n)
        .map(|i| format!("{i:>w$}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Byte donde empieza la línea `line` (1 = primera; 0 se trata como 1; pasado el final, `text.len()`).
pub fn line_to_byte(text: &str, line: usize) -> usize {
    let mut pos = 0;
    for _ in 1..line.max(1) {
        match text[pos..].find('\n') {
            Some(i) => pos += i + 1,
            None => return text.len(),
        }
    }
    pos
}

/// Memoria residente de este proceso en MB (Linux; None en otros sistemas).
pub fn own_rss_mb() -> Option<f32> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f32 = s
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pestanas_abrir_cerrar_y_cambios() {
        let mut t = Tabs::default();
        assert_eq!(t.open("src/a.rs", "uno".into()), 0);
        assert_eq!(t.open("b.py", "dos".into()), 1);
        assert_eq!(t.open("src/a.rs", "otro".into()), 0, "reutiliza la pestaña");
        assert_eq!(t.current().unwrap().text, "uno");
        assert_eq!(t.current().unwrap().title(), "a.rs");
        assert_eq!(t.current().unwrap().lang, Lang::Rust);
        t.current_mut().unwrap().text.push('!');
        assert!(t.any_dirty());
        t.mark_saved("src/a.rs", "uno!");
        assert!(!t.any_dirty());
        t.active = 1;
        t.close(0);
        assert_eq!(t.current().unwrap().path, "b.py");
        t.close(0);
        assert!(t.current().is_none());
        assert!(t.close(5).is_none());
    }

    #[test]
    fn busqueda_difusa() {
        let files: Vec<String> = [
            "crates/pcode-ide/src/main.rs",
            "crates/pcode-cli/src/main.rs",
            "docs/PLAN.md",
            "README.md",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            quick_open("idemain", &files, 1),
            vec!["crates/pcode-ide/src/main.rs"]
        );
        assert_eq!(quick_open("plan", &files, 5)[0], "docs/PLAN.md");
        assert!(quick_open("zzz", &files, 5).is_empty());
        assert_eq!(quick_open("", &files, 2).len(), 2);
        assert!(fuzzy_score("rdm", "README.md").is_some());
        assert!(fuzzy_score("mr", "README.md").is_none(), "en orden");
    }

    #[test]
    fn arbol_perezoso() {
        let mut t = Tree::default();
        t.set(
            "",
            vec![
                Entry {
                    name: "b.rs".into(),
                    path: "b.rs".into(),
                    dir: false,
                },
                Entry {
                    name: "src".into(),
                    path: "src".into(),
                    dir: true,
                },
                Entry {
                    name: "A.md".into(),
                    path: "A.md".into(),
                    dir: false,
                },
            ],
        );
        let nombres: Vec<&str> = t.children[""].iter().map(|e| e.name.as_str()).collect();
        assert_eq!(nombres, ["src", "A.md", "b.rs"]);
        assert!(t.toggle("src"), "hay que pedir src");
        assert!(!t.toggle("src"), "plegar no pide nada");
        t.set("src", vec![]);
        assert!(!t.toggle("src"), "ya se tenía");
        assert_eq!(t.invalidate(), vec!["".to_string(), "src".to_string()]);
    }

    #[test]
    fn numeros_de_linea() {
        assert_eq!(gutter("a\nb\nc"), "1\n2\n3");
        assert_eq!(gutter(""), "1");
        assert_eq!(gutter("a\n"), "1\n2");
    }

    #[test]
    fn numeros_de_linea_alineados() {
        let t = "x\n".repeat(9);
        assert!(gutter(&t).starts_with(" 1\n 2"));
    }

    #[test]
    fn ir_a_linea() {
        let t = "ab\ncd\nef";
        assert_eq!(line_to_byte(t, 1), 0);
        assert_eq!(line_to_byte(t, 2), 3);
        assert_eq!(line_to_byte(t, 0), 0);
        assert_eq!(line_to_byte(t, 99), t.len());
    }

    #[test]
    fn memoria_propia() {
        if cfg!(target_os = "linux") {
            assert!(own_rss_mb().unwrap() > 0.0);
        }
    }
}
