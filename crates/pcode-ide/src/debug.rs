//! Estado del depurador en el IDE: puntos de ruptura, pila, variables y salida. El adaptador
//! corre en el demonio (`debug.*`); aquí solo se guarda lo que llega y lo que se va a pedir.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Líneas de salida que se guardan como mucho.
pub const MAX_OUTPUT: usize = 1000;

/// Un marco de la pila.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Id del marco (para pedir sus variables).
    pub id: i64,
    /// Función.
    pub name: String,
    /// Archivo (relativo al proyecto si está dentro).
    pub path: String,
    /// Línea (desde 1).
    pub line: u32,
}

/// Una variable local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    /// Nombre.
    pub name: String,
    /// Valor como texto.
    pub value: String,
    /// Tipo.
    pub kind: String,
}

/// En qué está la depuración.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// No hay ninguna en marcha.
    #[default]
    Idle,
    /// El programa corre.
    Running,
    /// Parado en un punto de ruptura o tras un paso.
    Stopped,
}

/// Todo lo que el panel de depuración enseña.
#[derive(Debug, Default)]
pub struct DebugState {
    /// Fase.
    pub phase: Phase,
    /// Motivo de la última parada (`breakpoint`, `step`, `exception`…).
    pub reason: String,
    /// Pila de la última parada.
    pub frames: Vec<Frame>,
    /// Marco elegido en la pila.
    pub selected: usize,
    /// Variables del marco elegido.
    pub vars: Vec<Var>,
    /// Salida del programa.
    pub output: Vec<String>,
    /// Código de salida de la última ejecución (si se supo).
    pub exit: Option<i32>,
    breakpoints: BTreeMap<String, BTreeSet<u32>>,
    /// La última línea de `output` no ha terminado (el siguiente trozo la continúa).
    open_line: bool,
}

impl DebugState {
    /// Pone o quita el punto de ruptura de `line` (desde 1); devuelve las líneas del archivo.
    pub fn toggle_breakpoint(&mut self, path: &str, line: u32) -> Vec<u32> {
        let set = self.breakpoints.entry(path.to_string()).or_default();
        if !set.remove(&line) {
            set.insert(line);
        }
        let lines: Vec<u32> = set.iter().copied().collect();
        if lines.is_empty() {
            self.breakpoints.remove(path);
        }
        lines
    }

    /// Si `line` (desde 1) de `path` tiene punto de ruptura.
    pub fn has_breakpoint(&self, path: &str, line: u32) -> bool {
        self.breakpoints
            .get(path)
            .is_some_and(|s| s.contains(&line))
    }

    /// Todos los puntos de ruptura como los pide `debug.start`: `{ruta: [líneas]}`.
    pub fn breakpoints_json(&self) -> Value {
        Value::Object(
            self.breakpoints
                .iter()
                .map(|(p, s)| (p.clone(), json!(s.iter().collect::<Vec<_>>())))
                .collect(),
        )
    }

    /// Dónde está parado el programa en el marco elegido: (ruta, línea desde 1).
    pub fn location(&self) -> Option<(&str, u32)> {
        if self.phase != Phase::Stopped {
            return None;
        }
        self.frames
            .get(self.selected)
            .map(|f| (f.path.as_str(), f.line))
    }

    /// Al empezar una depuración: se olvida la anterior (los puntos de ruptura se conservan).
    pub fn started(&mut self) {
        self.phase = Phase::Running;
        self.reason.clear();
        self.frames.clear();
        self.vars.clear();
        self.output.clear();
        self.open_line = false;
        self.selected = 0;
        self.exit = None;
    }

    /// Tras `continue` o un paso: el programa vuelve a correr.
    pub fn resumed(&mut self) {
        if self.phase == Phase::Stopped {
            self.phase = Phase::Running;
            self.vars.clear();
        }
    }

    /// Guarda la respuesta de `debug.variables`.
    pub fn set_vars(&mut self, v: &Value) {
        self.vars = v
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| Var {
                name: x["name"].as_str().unwrap_or_default().to_string(),
                value: x["value"].as_str().unwrap_or_default().to_string(),
                kind: x["kind"].as_str().unwrap_or_default().to_string(),
            })
            .collect();
    }

    /// Aplica un evento del demonio. Devuelve `true` si era de depuración.
    pub fn apply(&mut self, ev: &Value) -> bool {
        match ev["tipo"].as_str() {
            Some("debugStopped") => {
                self.phase = Phase::Stopped;
                self.reason = ev["reason"].as_str().unwrap_or_default().to_string();
                self.frames = ev["frames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|f| Frame {
                        id: f["id"].as_i64().unwrap_or(0),
                        name: f["name"].as_str().unwrap_or_default().to_string(),
                        path: f["path"].as_str().unwrap_or_default().to_string(),
                        line: f["line"].as_u64().unwrap_or(0) as u32,
                    })
                    .collect();
                self.selected = 0;
                self.vars.clear();
                true
            }
            Some("debugOutput") => {
                // La salida llega en trozos que no coinciden con las líneas: un trozo sigue la
                // última línea si esta no acabó en salto.
                let text = ev["text"].as_str().unwrap_or_default();
                for piece in text.split_inclusive('\n') {
                    let line = piece.trim_end_matches(['\n', '\r']);
                    match self.output.last_mut() {
                        Some(last) if self.open_line => last.push_str(line),
                        _ => self.output.push(line.to_string()),
                    }
                    self.open_line = !piece.ends_with('\n');
                }
                if self.output.len() > MAX_OUTPUT {
                    let extra = self.output.len() - MAX_OUTPUT;
                    self.output.drain(..extra);
                }
                true
            }
            Some("debugExited") => {
                self.phase = Phase::Idle;
                self.exit = ev["code"].as_i64().map(|c| c as i32);
                self.frames.clear();
                self.vars.clear();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puntos_de_ruptura_se_ponen_y_se_quitan() {
        let mut d = DebugState::default();
        assert_eq!(d.toggle_breakpoint("a.py", 7), [7]);
        assert_eq!(d.toggle_breakpoint("a.py", 3), [3, 7]);
        assert!(d.has_breakpoint("a.py", 7) && !d.has_breakpoint("b.py", 7));
        assert_eq!(d.breakpoints_json(), json!({"a.py": [3, 7]}));
        assert_eq!(d.toggle_breakpoint("a.py", 7), [3]);
        assert!(d.toggle_breakpoint("a.py", 3).is_empty());
        assert_eq!(d.breakpoints_json(), json!({}));
    }

    #[test]
    fn parada_pila_variables_y_final() {
        let mut d = DebugState::default();
        d.toggle_breakpoint("a.py", 2);
        d.started();
        assert_eq!((d.phase, d.location()), (Phase::Running, None));
        assert!(d.apply(
            &json!({"tipo": "debugStopped", "reason": "breakpoint", "frames": [
            {"id": 7, "name": "main", "path": "a.py", "line": 2},
            {"id": 8, "name": "<module>", "path": "a.py", "line": 9}]})
        ));
        assert_eq!(d.phase, Phase::Stopped);
        assert_eq!(d.location(), Some(("a.py", 2)));
        d.selected = 1;
        assert_eq!(d.location(), Some(("a.py", 9)));
        d.set_vars(&json!([{"name": "x", "value": "42", "kind": "int"}]));
        assert_eq!(d.vars[0].value, "42");
        d.resumed();
        assert!(d.vars.is_empty() && d.phase == Phase::Running);
        assert!(
            d.apply(&json!({"tipo": "debugOutput", "category": "stdout", "text": "uno\ndos\n"}))
        );
        assert_eq!(d.output, ["uno", "dos"]);
        // Trozos que parten una línea (así los manda debugpy con `print("resultado", 42)`).
        d.apply(&json!({"tipo": "debugOutput", "text": "resultado"}));
        d.apply(&json!({"tipo": "debugOutput", "text": " 42\nfin"}));
        assert_eq!(d.output, ["uno", "dos", "resultado 42", "fin"]);
        assert!(d.apply(&json!({"tipo": "debugExited", "code": 3})));
        assert_eq!((d.phase, d.exit), (Phase::Idle, Some(3)));
        // Los puntos de ruptura sobreviven a la sesión.
        assert!(d.has_breakpoint("a.py", 2));
        assert!(!d.apply(&json!({"tipo": "filesChanged"})));
    }

    #[test]
    fn la_salida_no_crece_sin_limite() {
        let mut d = DebugState::default();
        let texto = "x\n".repeat(MAX_OUTPUT + 50);
        d.apply(&json!({"tipo": "debugOutput", "text": texto}));
        assert_eq!(d.output.len(), MAX_OUTPUT);
    }
}
