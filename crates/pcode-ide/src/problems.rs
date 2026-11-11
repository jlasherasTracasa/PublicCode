//! Diagnósticos del LSP recibidos por eventos.
use serde_json::Value;
use std::collections::BTreeMap;

/// Un diagnóstico de un archivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Ruta relativa.
    pub path: String,
    /// Línea (desde 0).
    pub line: u32,
    /// Carácter (UTF-16).
    pub character: u32,
    /// 1 = error, 2 = aviso.
    pub severity: u8,
    /// Mensaje.
    pub message: String,
    /// Quién lo dice.
    pub source: String,
}

/// Problemas por archivo (cada evento reemplaza los anteriores del archivo).
#[derive(Debug, Default)]
pub struct Problems {
    /// Ruta → diagnósticos.
    pub by_path: BTreeMap<String, Vec<Problem>>,
}

impl Problems {
    /// Aplica un evento `diagnostics`; `false` si es otro evento.
    pub fn apply(&mut self, ev: &Value) -> bool {
        if ev["tipo"] != "diagnostics" {
            return false;
        }
        let path = ev["path"].as_str().unwrap_or_default().to_string();
        let items: Vec<Problem> = ev["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| Problem {
                path: path.clone(),
                line: d["line"].as_u64().unwrap_or(0) as u32,
                character: d["character"].as_u64().unwrap_or(0) as u32,
                severity: d["severity"].as_u64().unwrap_or(1) as u8,
                message: d["message"].as_str().unwrap_or_default().to_string(),
                source: d["source"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
        if items.is_empty() {
            self.by_path.remove(&path);
        } else {
            self.by_path.insert(path, items);
        }
        true
    }

    /// (errores, avisos).
    pub fn counts(&self) -> (usize, usize) {
        let all = self.by_path.values().flatten();
        let e = all.clone().filter(|p| p.severity == 1).count();
        let w = all.filter(|p| p.severity == 2).count();
        (e, w)
    }

    /// Lista plana por ruta y línea.
    pub fn sorted(&self) -> Vec<Problem> {
        let mut v: Vec<Problem> = self.by_path.values().flatten().cloned().collect();
        v.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(path: &str, n: usize, sev: u8) -> Value {
        json!({"tipo":"diagnostics","path":path,"items":(0..n).map(|i| json!({"line":i,"character":0,"severity":sev,"message":"m","source":"s"})).collect::<Vec<_>>()})
    }

    #[test]
    fn diagnosticos_reemplazan_por_ruta() {
        let mut p = Problems::default();
        assert!(p.apply(&ev("a.rs", 3, 1)));
        assert!(p.apply(&ev("a.rs", 1, 1)));
        assert_eq!(p.by_path["a.rs"].len(), 1);
    }

    #[test]
    fn lista_vacia_borra() {
        let mut p = Problems::default();
        p.apply(&ev("a.rs", 2, 1));
        p.apply(&ev("a.rs", 0, 1));
        assert!(p.by_path.is_empty());
    }

    #[test]
    fn cuenta_errores_y_avisos() {
        let mut p = Problems::default();
        p.apply(&ev("b.rs", 2, 1));
        p.apply(&ev("a.rs", 1, 2));
        assert_eq!(p.counts(), (2, 1));
        assert_eq!(p.sorted()[0].path, "a.rs");
    }

    #[test]
    fn otros_eventos_no_cuentan() {
        let mut p = Problems::default();
        assert!(!p.apply(&json!({"tipo":"filesChanged"})));
    }
}
