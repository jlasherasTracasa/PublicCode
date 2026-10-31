//! Puntos de control por turno: antes de que una tool escriba un archivo se guarda cómo estaba (o que
//! no existía) en `.pcode/checkpoints/<turno>/`. `undo` deja el proyecto como antes del último turno
//! (y borra lo que el turno creó); `redo` lo vuelve a aplicar. Se guardan como mucho [`MAX_TURNS`].
//!
//! Solo cubre lo que escriben las tools de archivos (`escribir`, `editar`); lo que cambie una orden
//! de shell no se registra.
use serde_json::{Value, json};
use std::io;
use std::path::{Path, PathBuf};

/// Turnos que se conservan; al pasar de aquí se borran los más antiguos.
pub const MAX_TURNS: usize = 50;

/// Un turno con puntos de control.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnInfo {
    /// Número de turno (creciente).
    pub turn: u64,
    /// Segundos desde 1970 en que empezó.
    pub time: u64,
    /// Primeras palabras de la petición del usuario.
    pub label: String,
    /// Archivos que tocó, relativos a la raíz.
    pub files: Vec<String>,
    /// Si está deshecho.
    pub undone: bool,
}

/// Cambio de un archivo en un turno.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    /// Ruta relativa a la raíz.
    pub path: String,
    /// Líneas añadidas.
    pub added: usize,
    /// Líneas quitadas.
    pub removed: usize,
    /// El turno lo creó.
    pub created: bool,
    /// El turno lo borró.
    pub deleted: bool,
}

/// Puntos de control de un proyecto.
#[derive(Debug, Clone)]
pub struct Checkpoints {
    root: PathBuf,
    dir: PathBuf,
    current: Option<(u64, String, u64)>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn label_of(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 80 {
        format!("{}…", one.chars().take(80).collect::<String>())
    } else {
        one
    }
}

fn read_opt(p: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(p) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Deja `path` con `content` (o lo borra si es `None`).
fn put(path: &Path, content: Option<&[u8]>) -> io::Result<()> {
    match content {
        Some(c) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, c)
        }
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

fn line_counts(before: &[u8], after: &[u8]) -> (usize, usize) {
    let a = String::from_utf8_lossy(before);
    let b = String::from_utf8_lossy(after);
    let diff = similar::TextDiff::from_lines(a.as_ref(), b.as_ref());
    let (mut add, mut del) = (0, 0);
    for c in diff.iter_all_changes() {
        match c.tag() {
            similar::ChangeTag::Insert => add += 1,
            similar::ChangeTag::Delete => del += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    (add, del)
}

impl Checkpoints {
    /// Puntos de control del proyecto en `root` (no crea nada hasta que se guarda algo).
    pub fn open(root: &Path) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        Checkpoints {
            dir: root.join(".pcode").join("checkpoints"),
            root,
            current: None,
        }
    }

    fn turn_dir(&self, turn: u64) -> PathBuf {
        self.dir.join(format!("{turn:06}"))
    }

    fn turns(&self) -> Vec<u64> {
        let mut v: Vec<u64> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .collect();
        v.sort_unstable();
        v
    }

    fn manifest(&self, turn: u64) -> Option<Value> {
        let s = std::fs::read_to_string(self.turn_dir(turn).join("manifest.json")).ok()?;
        serde_json::from_str(&s).ok()
    }

    fn write_manifest(&self, turn: u64, m: &Value) -> io::Result<()> {
        std::fs::write(
            self.turn_dir(turn).join("manifest.json"),
            serde_json::to_vec_pretty(m)?,
        )
    }

    fn info(&self, turn: u64, m: &Value) -> TurnInfo {
        TurnInfo {
            turn,
            time: m["time"].as_u64().unwrap_or(0),
            label: m["label"].as_str().unwrap_or("").to_string(),
            files: m["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f["path"].as_str().map(String::from))
                .collect(),
            undone: m["undone"].as_bool().unwrap_or(false),
        }
    }

    /// Empieza un turno nuevo. Descarta los turnos deshechos (ya no se pueden rehacer). El
    /// directorio del turno solo se crea si el turno llega a escribir algo.
    pub fn begin_turn(&mut self, label: &str) -> u64 {
        for t in self.turns() {
            if self.manifest(t).is_none_or(|m| m["undone"] == true) {
                let _ = std::fs::remove_dir_all(self.turn_dir(t));
            }
        }
        let next = self.turns().last().copied().unwrap_or(0) + 1;
        self.current = Some((next, label_of(label), now()));
        next
    }

    /// Ruta relativa a la raíz de `path` (relativa o absoluta), o `None` si está fuera.
    fn relative(&self, path: &str) -> Option<String> {
        let full = pcode_tools::resolve_in_root(&self.root, path).ok()?;
        let rel = full.strip_prefix(&self.root).ok()?;
        let s = rel.to_string_lossy().replace('\\', "/");
        (!s.is_empty() && !s.starts_with(".pcode/")).then_some(s)
    }

    /// Guarda cómo está `path` antes de que el turno actual lo cambie (solo la primera vez en el
    /// turno). Si no hay turno empezado, empieza uno sin etiqueta.
    pub fn save(&mut self, path: &str) -> io::Result<()> {
        let Some(rel) = self.relative(path) else {
            return Ok(());
        };
        if self.current.is_none() {
            self.begin_turn("");
        }
        let (turn, label, time) = self.current.clone().unwrap_or_default();
        let dir = self.turn_dir(turn);
        let mut m = self.manifest(turn).unwrap_or_else(
            || json!({"turn": turn, "time": time, "label": label, "undone": false, "files": []}),
        );
        let files = m["files"].as_array().cloned().unwrap_or_default();
        if files.iter().any(|f| f["path"] == rel.as_str()) {
            return Ok(());
        }
        std::fs::create_dir_all(dir.join("antes"))?;
        let idx = files.len();
        let prev = read_opt(&self.root.join(&rel))?;
        if let Some(b) = &prev {
            std::fs::write(dir.join("antes").join(idx.to_string()), b)?;
        }
        let mut files = files;
        files.push(json!({"path": rel, "existed": prev.is_some()}));
        m["files"] = Value::Array(files);
        self.write_manifest(turn, &m)?;
        self.prune();
        Ok(())
    }

    fn prune(&self) {
        let t = self.turns();
        if t.len() > MAX_TURNS {
            for old in &t[..t.len() - MAX_TURNS] {
                let _ = std::fs::remove_dir_all(self.turn_dir(*old));
            }
        }
    }

    /// Turnos guardados, del más reciente al más antiguo.
    pub fn list(&self) -> Vec<TurnInfo> {
        self.turns()
            .into_iter()
            .rev()
            .filter_map(|t| self.manifest(t).map(|m| self.info(t, &m)))
            .collect()
    }

    /// Qué cambió cada archivo en `turn` (si está deshecho, lo que cambiaría al rehacerlo).
    pub fn changes(&self, turn: u64) -> Vec<FileChange> {
        let Some(m) = self.manifest(turn) else {
            return vec![];
        };
        let dir = self.turn_dir(turn);
        let undone = m["undone"] == true;
        let mut out = vec![];
        for (i, f) in m["files"].as_array().into_iter().flatten().enumerate() {
            let path = f["path"].as_str().unwrap_or_default().to_string();
            let before = if f["existed"] == true {
                std::fs::read(dir.join("antes").join(i.to_string())).ok()
            } else {
                None
            };
            let after = if undone {
                read_opt(&dir.join("despues").join(i.to_string()))
                    .ok()
                    .flatten()
            } else {
                read_opt(&self.root.join(&path)).ok().flatten()
            };
            let (added, removed) = line_counts(
                before.as_deref().unwrap_or_default(),
                after.as_deref().unwrap_or_default(),
            );
            out.push(FileChange {
                path,
                added,
                removed,
                created: before.is_none() && after.is_some(),
                deleted: before.is_some() && after.is_none(),
            });
        }
        out
    }

    /// Cambios del turno en curso (vacío si aún no ha escrito nada).
    pub fn current_changes(&self) -> Vec<FileChange> {
        self.current
            .as_ref()
            .map(|(t, _, _)| self.changes(*t))
            .unwrap_or_default()
    }

    /// Número del turno en curso.
    pub fn current_turn(&self) -> Option<u64> {
        self.current.as_ref().map(|(t, _, _)| *t)
    }

    /// Deshace el último turno aplicado: restaura cada archivo como estaba antes y borra los que
    /// creó. Devuelve el turno deshecho, o `None` si no hay nada que deshacer.
    pub fn undo(&mut self) -> io::Result<Option<TurnInfo>> {
        let Some((turn, mut m)) = self
            .turns()
            .into_iter()
            .rev()
            .filter_map(|t| self.manifest(t).map(|m| (t, m)))
            .find(|(_, m)| m["undone"] != true)
        else {
            return Ok(None);
        };
        let dir = self.turn_dir(turn);
        std::fs::create_dir_all(dir.join("despues"))?;
        let files = m["files"].as_array().cloned().unwrap_or_default();
        for (i, f) in files.iter().enumerate() {
            let rel = f["path"].as_str().unwrap_or_default();
            let target = self.root.join(rel);
            let after = dir.join("despues").join(i.to_string());
            match read_opt(&target)? {
                Some(b) => std::fs::write(&after, b)?,
                None => put(&after, None)?,
            }
            let before = if f["existed"] == true {
                Some(std::fs::read(dir.join("antes").join(i.to_string()))?)
            } else {
                None
            };
            put(&target, before.as_deref())?;
        }
        m["undone"] = Value::Bool(true);
        self.write_manifest(turn, &m)?;
        if self.current_turn() == Some(turn) {
            self.current = None;
        }
        Ok(Some(self.info(turn, &m)))
    }

    /// Vuelve a aplicar el turno deshecho más antiguo (el siguiente en orden). Devuelve el turno, o
    /// `None` si no hay nada que rehacer.
    pub fn redo(&mut self) -> io::Result<Option<TurnInfo>> {
        let Some((turn, mut m)) = self
            .turns()
            .into_iter()
            .filter_map(|t| self.manifest(t).map(|m| (t, m)))
            .find(|(_, m)| m["undone"] == true)
        else {
            return Ok(None);
        };
        let dir = self.turn_dir(turn);
        let files = m["files"].as_array().cloned().unwrap_or_default();
        for (i, f) in files.iter().enumerate() {
            let rel = f["path"].as_str().unwrap_or_default();
            let after = read_opt(&dir.join("despues").join(i.to_string()))?;
            put(&self.root.join(rel), after.as_deref())?;
        }
        let _ = std::fs::remove_dir_all(dir.join("despues"));
        m["undone"] = Value::Bool(false);
        self.write_manifest(turn, &m)?;
        Ok(Some(self.info(turn, &m)))
    }
}
