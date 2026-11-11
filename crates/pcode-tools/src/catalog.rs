//! Catálogo de tools: cada tool anunciada cuesta tokens en cada petición, así que con muchas tools de
//! MCP se anuncian solo las nativas y `buscar_tools`. Lo que el agente encuentra con ella queda
//! visible desde ese momento. Lógica pura: no hay E/S.
use crate::bm25::Bm25;
use pcode_models::ToolSpec;
use serde_json::json;
use std::collections::{BTreeSet, HashSet};

/// Nombre de la tool de búsqueda de tools.
pub const SEARCH_TOOL: &str = "buscar_tools";
/// A partir de cuántas tools se dejan de anunciar todas.
pub const DEFAULT_MAX_VISIBLE: usize = 20;

/// Si la tool se puede dejar sin anunciar (las de MCP).
pub fn is_deferrable(spec: &ToolSpec) -> bool {
    spec.description.starts_with("[MCP ")
}

/// Descripción sin el prefijo `[MCP …] `, en minúsculas y con los espacios colapsados.
fn normalized(description: &str) -> String {
    let sin_prefijo = description
        .strip_prefix("[MCP ")
        .and_then(|r| r.find("] ").map(|i| &r[i + 2..]))
        .unwrap_or(description);
    sin_prefijo
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Quita las tools de MCP repetidas (misma descripción y mismos parámetros en otro servidor).
pub fn dedupe(specs: Vec<ToolSpec>) -> Vec<ToolSpec> {
    let mut vistas: HashSet<(String, String)> = HashSet::new();
    specs
        .into_iter()
        .filter(|s| {
            if !is_deferrable(s) {
                return true;
            }
            let norm = normalized(&s.description);
            if norm.is_empty() {
                return true;
            }
            vistas.insert((norm, s.parameters.to_string()))
        })
        .collect()
}

/// Especificación de `buscar_tools`.
pub fn search_spec() -> ToolSpec {
    ToolSpec {
        name: SEARCH_TOOL.into(),
        description: "Hay más tools de las que ves. Describe lo que necesitas (p. ej. \"crear una \
            incidencia\") y te devuelvo las que sirven; quedan disponibles a partir de la siguiente \
            respuesta."
            .into(),
        parameters: json!({"type":"object","properties":{"consulta":{"type":"string"}},"required":["consulta"]}),
    }
}

/// Qué tools se anuncian al modelo en cada petición.
#[derive(Debug, Clone)]
pub struct ToolCatalog {
    max_visible: usize,
    unlocked: BTreeSet<String>,
}

impl ToolCatalog {
    /// Catálogo sin tools desbloqueadas.
    pub fn new(max_visible: usize) -> Self {
        Self {
            max_visible,
            unlocked: BTreeSet::new(),
        }
    }

    /// Las specs que se anuncian ahora.
    pub fn visible(&self, specs: Vec<ToolSpec>) -> Vec<ToolSpec> {
        if specs.len() <= self.max_visible {
            return specs;
        }
        let mut out: Vec<ToolSpec> = specs
            .into_iter()
            .filter(|s| !is_deferrable(s) || self.unlocked.contains(&s.name))
            .collect();
        out.push(search_spec());
        out
    }

    /// Busca entre las tools sin anunciar, desbloquea las mejores y devuelve el texto del resultado.
    pub fn search(&mut self, specs: &[ToolSpec], query: &str, k: usize) -> String {
        if query.trim().is_empty() {
            return "falta `consulta`".into();
        }
        let candidatas: Vec<&ToolSpec> = specs.iter().filter(|s| is_deferrable(s)).collect();
        let docs: Vec<String> = candidatas
            .iter()
            .map(|s| format!("{} {}", s.name, s.description))
            .collect();
        let resultados = Bm25::new(&docs).search(query, k.clamp(1, 10));
        if resultados.is_empty() {
            return format!("ninguna tool coincide con «{query}»");
        }
        let mut lineas = vec!["Tools disponibles a partir de ahora:".to_string()];
        for (idx, _) in resultados {
            let s = candidatas[idx];
            self.unlocked.insert(s.name.clone());
            let desc: String = s.description.chars().take(200).collect();
            lineas.push(format!("- {}: {desc}", s.name));
        }
        lineas.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, desc: &str, params: serde_json::Value) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: desc.into(),
            parameters: params,
        }
    }

    fn params() -> serde_json::Value {
        json!({"type":"object","properties":{"titulo":{"type":"string"}}})
    }

    /// Las cinco tools del ejemplo, con la de `gl` repetida (misma descripción normalizada).
    fn ejemplo() -> Vec<ToolSpec> {
        vec![
            spec("leer", "Lee un archivo", params()),
            spec("bash", "Ejecuta una orden", params()),
            spec(
                "gh_crear_incidencia",
                "[MCP gh] Crea una incidencia en el repositorio",
                params(),
            ),
            spec(
                "gl_crear_incidencia",
                "[MCP gl] Crea una  incidencia en el repositorio",
                params(),
            ),
            spec(
                "clima_hoy",
                "[MCP clima] Tiempo de hoy en una ciudad",
                params(),
            ),
        ]
    }

    fn nombres(v: &[ToolSpec]) -> Vec<&str> {
        v.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn pocas_tools_todas_visibles() {
        let specs = ejemplo();
        let c = ToolCatalog::new(20);
        let vis = c.visible(specs.clone());
        assert_eq!(vis.len(), 5);
        assert!(!nombres(&vis).contains(&SEARCH_TOOL));
    }

    #[test]
    fn muchas_tools_solo_nativas_y_buscar() {
        let c = ToolCatalog::new(3);
        let vis = c.visible(ejemplo());
        assert_eq!(nombres(&vis), vec!["leer", "bash", SEARCH_TOOL]);
    }

    #[test]
    fn buscar_desbloquea() {
        let d = dedupe(ejemplo());
        let mut c = ToolCatalog::new(3);
        assert_eq!(
            nombres(&c.visible(d.clone())),
            vec!["leer", "bash", SEARCH_TOOL]
        );
        let texto = c.search(&d, "tiempo ciudad", 5);
        assert_eq!(
            texto,
            "Tools disponibles a partir de ahora:\n- clima_hoy: [MCP clima] Tiempo de hoy en una ciudad"
        );
        assert_eq!(
            nombres(&c.visible(d)),
            vec!["leer", "bash", "clima_hoy", SEARCH_TOOL]
        );
    }

    #[test]
    fn buscar_sin_coincidencias_no_desbloquea() {
        let d = dedupe(ejemplo());
        let mut c = ToolCatalog::new(3);
        let texto = c.search(&d, "zzz", 5);
        assert_eq!(texto, "ninguna tool coincide con «zzz»");
        assert_eq!(nombres(&c.visible(d)), vec!["leer", "bash", SEARCH_TOOL]);
    }

    #[test]
    fn dedupe_prefiere_la_primera() {
        let d = dedupe(ejemplo());
        assert_eq!(
            nombres(&d),
            vec!["leer", "bash", "gh_crear_incidencia", "clima_hoy"]
        );
    }

    #[test]
    fn dedupe_no_toca_nativas_ni_parametros_distintos() {
        let otros = json!({"type":"object","properties":{"cuerpo":{"type":"string"}}});
        let specs = vec![
            spec("leer", "Lee un archivo", params()),
            spec("leer2", "Lee un archivo", params()),
            spec("gh_x", "[MCP gh] Crea algo", params()),
            spec("gl_x", "[MCP gl] Crea algo", otros),
        ];
        let d = dedupe(specs);
        assert_eq!(nombres(&d), vec!["leer", "leer2", "gh_x", "gl_x"]);
    }
}
