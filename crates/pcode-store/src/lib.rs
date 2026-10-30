//! Sesiones y eventos en SQLite.
use rusqlite::{Connection, params};
use std::path::Path;

/// Errores del almacén.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite.
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    /// JSON.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Un evento.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Id.
    pub id: i64,
    /// Sesión.
    pub session_id: i64,
    /// Marca de tiempo (ms).
    pub ts_ms: i64,
    /// Tipo.
    pub kind: String,
    /// Contenido.
    pub payload: serde_json::Value,
}

/// Una sesión.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    /// Id.
    pub id: i64,
    /// Creación (ms).
    pub created_ms: i64,
    /// Modelo.
    pub model: String,
    /// Tarea.
    pub task: String,
    /// Resultado.
    pub outcome: Option<String>,
}

/// Almacén.
pub struct Store {
    conn: Connection,
}

const VERSION: i64 = 1;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl Store {
    fn migrate(conn: Connection) -> Result<Store, StoreError> {
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sessions(id INTEGER PRIMARY KEY, created_ms INTEGER NOT NULL, model TEXT NOT NULL, task TEXT NOT NULL, outcome TEXT);
                 CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY, session_id INTEGER NOT NULL REFERENCES sessions(id), ts_ms INTEGER NOT NULL, kind TEXT NOT NULL, payload TEXT NOT NULL);
                 CREATE INDEX IF NOT EXISTS events_session ON events(session_id, id);",
            )?;
            conn.execute_batch(&format!("PRAGMA user_version = {VERSION}"))?;
        }
        Ok(Store { conn })
    }
    /// Abre (y migra) un archivo.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        Self::migrate(Connection::open(path)?)
    }
    /// Base en memoria.
    pub fn open_in_memory() -> Result<Store, StoreError> {
        Self::migrate(Connection::open_in_memory()?)
    }
    /// Crea una sesión.
    pub fn create_session(&self, model: &str, task: &str) -> Result<i64, StoreError> {
        self.conn.execute(
            "INSERT INTO sessions(created_ms, model, task) VALUES (?1, ?2, ?3)",
            params![now_ms(), model, task],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Cierra una sesión.
    pub fn finish_session(&self, id: i64, outcome: &str) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE sessions SET outcome = ?1 WHERE id = ?2",
            params![outcome, id],
        )?;
        Ok(())
    }
    /// Añade un evento.
    pub fn append_event(
        &self,
        session_id: i64,
        kind: &str,
        payload: &serde_json::Value,
    ) -> Result<i64, StoreError> {
        self.conn.execute(
            "INSERT INTO events(session_id, ts_ms, kind, payload) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, now_ms(), kind, serde_json::to_string(payload)?],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Eventos de una sesión, en orden.
    pub fn events(&self, session_id: i64) -> Result<Vec<Event>, StoreError> {
        let mut st = self
            .conn
            .prepare("SELECT id, session_id, ts_ms, kind, payload FROM events WHERE session_id = ?1 ORDER BY id")?;
        let rows = st.query_map(params![session_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = vec![];
        for r in rows {
            let (id, session_id, ts_ms, kind, p) = r?;
            out.push(Event {
                id,
                session_id,
                ts_ms,
                kind,
                payload: serde_json::from_str(&p)?,
            });
        }
        Ok(out)
    }
    /// Últimas sesiones.
    pub fn sessions(&self, limit: u32) -> Result<Vec<SessionRow>, StoreError> {
        let mut st = self.conn.prepare(
            "SELECT id, created_ms, model, task, outcome FROM sessions ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = st.query_map(params![limit], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                created_ms: r.get(1)?,
                model: r.get(2)?,
                task: r.get(3)?,
                outcome: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn store_crea_y_lista_sesiones() {
        let s = Store::open_in_memory().unwrap();
        let a = s.create_session("m", "t1").unwrap();
        let b = s.create_session("m", "t2").unwrap();
        let l = s.sessions(10).unwrap();
        assert_eq!((l[0].id, l[1].id), (b, a));
    }
    #[test]
    fn store_eventos_en_orden() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_session("m", "t").unwrap();
        s.append_event(id, "a", &json!(1)).unwrap();
        s.append_event(id, "b", &json!({"x": 2})).unwrap();
        let e = s.events(id).unwrap();
        assert_eq!((e[0].kind.as_str(), e[1].kind.as_str()), ("a", "b"));
        assert_eq!(e[1].payload["x"], 2);
    }
    #[test]
    fn store_finish_session() {
        let s = Store::open_in_memory().unwrap();
        let id = s.create_session("m", "t").unwrap();
        s.finish_session(id, "done").unwrap();
        assert_eq!(s.sessions(1).unwrap()[0].outcome.as_deref(), Some("done"));
    }
    #[test]
    fn store_reabrir_conserva_datos() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x/p.db");
        Store::open(&p).unwrap().create_session("m", "t").unwrap();
        assert_eq!(Store::open(&p).unwrap().sessions(5).unwrap().len(), 1);
    }
    #[test]
    fn store_migracion_idempotente() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("p.db");
        Store::open(&p).unwrap();
        Store::open(&p).unwrap();
    }
}
