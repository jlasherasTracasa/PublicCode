//! Auditoría append-only con hash encadenado.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Una entrada del registro.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// Número de secuencia (desde 1).
    pub seq: u64,
    /// Marca de tiempo en ms.
    pub ts_ms: u64,
    /// Quién actúa.
    pub actor: String,
    /// De dónde viene la acción.
    pub provenance: String,
    /// Acción.
    pub action: String,
    /// Detalle.
    pub detail: serde_json::Value,
    /// Decisión.
    pub decision: String,
    /// Hash de la entrada anterior.
    pub prev_hash: String,
    /// Hash de esta entrada.
    pub hash: String,
}

/// Errores de auditoría.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// E/S.
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    /// Entrada rota.
    #[error("entrada {0} corrupta o manipulada")]
    Broken(u64),
}

const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fn compute(e: &AuditEntry) -> String {
    let mut m = BTreeMap::new();
    m.insert("seq", serde_json::json!(e.seq));
    m.insert("ts_ms", serde_json::json!(e.ts_ms));
    m.insert("actor", serde_json::json!(e.actor));
    m.insert("provenance", serde_json::json!(e.provenance));
    m.insert("action", serde_json::json!(e.action));
    m.insert("detail", e.detail.clone());
    m.insert("decision", serde_json::json!(e.decision));
    let canon = serde_json::to_string(&m).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(format!("{}\n{}", e.prev_hash, canon));
    hex::encode(h.finalize())
}

/// Registro de auditoría.
pub struct AuditLog {
    path: PathBuf,
    seq: u64,
    last_hash: String,
}

impl AuditLog {
    /// Abre o crea el registro, continuando el anterior.
    pub fn open(path: &Path) -> Result<AuditLog, AuditError> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let (mut seq, mut last) = (0, ZERO.to_string());
        if let Ok(s) = std::fs::read_to_string(path) {
            for l in s.lines().filter(|l| !l.trim().is_empty()) {
                let e: AuditEntry =
                    serde_json::from_str(l).map_err(|_| AuditError::Broken(seq + 1))?;
                seq = e.seq;
                last = e.hash;
            }
        }
        Ok(AuditLog {
            path: path.to_path_buf(),
            seq,
            last_hash: last,
        })
    }
    /// Añade una entrada.
    pub fn append(
        &mut self,
        actor: &str,
        provenance: &str,
        action: &str,
        detail: serde_json::Value,
        decision: &str,
    ) -> Result<AuditEntry, AuditError> {
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut e = AuditEntry {
            seq: self.seq + 1,
            ts_ms,
            actor: actor.into(),
            provenance: provenance.into(),
            action: action.into(),
            detail,
            decision: decision.into(),
            prev_hash: self.last_hash.clone(),
            hash: String::new(),
        };
        e.hash = compute(&e);
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", serde_json::to_string(&e).unwrap_or_default())?;
        self.seq = e.seq;
        self.last_hash = e.hash.clone();
        Ok(e)
    }
    /// Verifica la cadena; devuelve el número de entradas.
    pub fn verify(path: &Path) -> Result<u64, AuditError> {
        let s = std::fs::read_to_string(path)?;
        let (mut n, mut prev) = (0u64, ZERO.to_string());
        for l in s.lines().filter(|l| !l.trim().is_empty()) {
            let e: AuditEntry = serde_json::from_str(l).map_err(|_| AuditError::Broken(n + 1))?;
            if e.seq != n + 1 || e.prev_hash != prev || compute(&e) != e.hash {
                return Err(AuditError::Broken(n + 1));
            }
            prev = e.hash;
            n += 1;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    use serde_json::json;
    fn log(d: &tempfile::TempDir) -> (AuditLog, PathBuf) {
        let p = d.path().join("a.jsonl");
        (AuditLog::open(&p).unwrap(), p)
    }
    #[test]
    fn audit_encadena() {
        let d = tempfile::tempdir().unwrap();
        let (mut l, _) = log(&d);
        let a = l
            .append("x", "usuario", "leer", json!({}), "allow")
            .unwrap();
        let b = l
            .append("x", "usuario", "leer", json!({}), "allow")
            .unwrap();
        assert_eq!(b.prev_hash, a.hash);
        assert_eq!(a.prev_hash, ZERO);
    }
    #[test]
    fn audit_continua_tras_reabrir() {
        let d = tempfile::tempdir().unwrap();
        let (mut l, p) = log(&d);
        l.append("x", "u", "a", json!({}), "allow").unwrap();
        let mut l2 = AuditLog::open(&p).unwrap();
        assert_eq!(l2.append("x", "u", "b", json!({}), "allow").unwrap().seq, 2);
        assert_eq!(AuditLog::verify(&p).unwrap(), 2);
    }
    #[test]
    fn audit_verify_ok() {
        let d = tempfile::tempdir().unwrap();
        let (mut l, p) = log(&d);
        for _ in 0..3 {
            l.append("x", "u", "a", json!({"k":1}), "allow").unwrap();
        }
        assert_eq!(AuditLog::verify(&p).unwrap(), 3);
    }
    #[test]
    fn audit_detecta_manipulacion() {
        let d = tempfile::tempdir().unwrap();
        let (mut l, p) = log(&d);
        for _ in 0..3 {
            l.append("x", "u", "a", json!({}), "allow").unwrap();
        }
        let s = std::fs::read_to_string(&p).unwrap();
        let mut lines: Vec<String> = s.lines().map(String::from).collect();
        lines[1] = lines[1].replace("\"allow\"", "\"deny\"");
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();
        assert!(matches!(AuditLog::verify(&p), Err(AuditError::Broken(2))));
    }
    #[test]
    fn audit_detecta_linea_borrada() {
        let d = tempfile::tempdir().unwrap();
        let (mut l, p) = log(&d);
        for _ in 0..3 {
            l.append("x", "u", "a", json!({}), "allow").unwrap();
        }
        let s = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        std::fs::write(&p, format!("{}\n{}\n", lines[0], lines[2])).unwrap();
        assert!(AuditLog::verify(&p).is_err());
    }
}
