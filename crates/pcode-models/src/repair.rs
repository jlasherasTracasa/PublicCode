//! Reparación de argumentos JSON mal formados (modelos abiertos).
use serde_json::Value;

fn obj(s: &str) -> Option<Value> {
    serde_json::from_str::<Value>(s)
        .ok()
        .filter(|v| v.is_object())
}

fn strip_fence(s: &str) -> String {
    let t = s.trim();
    if let Some(r) = t.strip_prefix("```") {
        let r = r.strip_prefix("json").unwrap_or(r);
        return r.trim().trim_end_matches("```").trim().to_string();
    }
    t.to_string()
}

fn trailing_commas(s: &str) -> String {
    regex_lite(s)
}

fn regex_lite(s: &str) -> String {
    // Quita comas antes de `}` o `]` ignorando espacios.
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut in_str = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' && (i == 0 || chars[i - 1] != '\\') {
            in_str = !in_str;
        }
        if !in_str && c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Intenta reparar `raw` hasta obtener un objeto JSON.
pub fn repair_json_arguments(raw: &str) -> Option<Value> {
    if let Some(v) = obj(raw) {
        return Some(v);
    }
    let s = strip_fence(raw);
    if let Some(v) = obj(&s) {
        return Some(v);
    }
    let cut = match (s.find('{'), s.rfind('}')) {
        (Some(a), Some(b)) if b > a => s[a..=b].to_string(),
        _ => s.clone(),
    };
    if let Some(v) = obj(&cut) {
        return Some(v);
    }
    let nc = trailing_commas(&cut);
    if let Some(v) = obj(&nc) {
        return Some(v);
    }
    if !nc.contains('"') {
        let q = nc.replace('\'', "\"");
        if let Some(v) = obj(&q) {
            return Some(v);
        }
    }
    // comillas simples con comas finales
    if !cut.contains('"') {
        let q = trailing_commas(&cut.replace('\'', "\""));
        if let Some(v) = obj(&q) {
            return Some(v);
        }
    }
    if let Ok(Value::String(inner)) = serde_json::from_str::<Value>(raw.trim()) {
        return obj(&inner);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repair_estricto() {
        assert!(repair_json_arguments(r#"{"a":1}"#).is_some());
    }
    #[test]
    fn repair_valla_de_codigo() {
        assert!(repair_json_arguments("```json\n{\"a\":1}\n```").is_some());
    }
    #[test]
    fn repair_texto_alrededor() {
        assert!(repair_json_arguments("claro: {\"a\":1} listo").is_some());
    }
    #[test]
    fn repair_coma_final() {
        assert!(repair_json_arguments(r#"{"a":1,}"#).is_some());
    }
    #[test]
    fn repair_comillas_simples() {
        assert!(repair_json_arguments("{'a': 'b'}").is_some());
    }
    #[test]
    fn repair_doble_codificacion() {
        assert!(repair_json_arguments(r#""{\"a\":1}""#).is_some());
    }
    #[test]
    fn repair_basura_es_none() {
        assert!(repair_json_arguments("no es json").is_none());
    }
}
