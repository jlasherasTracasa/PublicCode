//! Trait `Tool`, efectos, registro y rutas seguras.
use pcode_models::ToolSpec;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Efecto de una tool.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Effect {
    /// Lee.
    Read,
    /// Escribe.
    Write,
    /// Usa la red.
    Network,
    /// Ejecuta procesos.
    Exec,
    /// Irreversible.
    Irreversible,
}

/// Privilegio de un conjunto de efectos (máximo; 0 si no hay).
pub fn privilege(effects: &[Effect]) -> u8 {
    effects
        .iter()
        .map(|e| match e {
            Effect::Read => 0,
            Effect::Write => 1,
            Effect::Network => 2,
            Effect::Exec => 3,
            Effect::Irreversible => 4,
        })
        .max()
        .unwrap_or(0)
}

/// Contexto de ejecución de una tool.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Raíz del proyecto.
    pub root: PathBuf,
    /// Tiempo máximo.
    pub timeout: Duration,
}

/// Salida de una tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// Contenido.
    pub content: String,
    /// Si es un error.
    pub is_error: bool,
}

impl ToolOutput {
    /// Salida correcta.
    pub fn ok(s: impl Into<String>) -> Self {
        ToolOutput {
            content: s.into(),
            is_error: false,
        }
    }
    /// Salida de error.
    pub fn error(s: impl Into<String>) -> Self {
        ToolOutput {
            content: s.into(),
            is_error: true,
        }
    }
}

/// Una tool.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Descripción para el modelo.
    fn spec(&self) -> ToolSpec;
    /// Efectos declarados.
    fn effects(&self) -> &'static [Effect];
    /// Ejecuta la tool.
    async fn call(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput;
}

/// Registro de tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Registro vacío.
    pub fn new() -> Self {
        Self::default()
    }
    /// Registra una tool; error si el nombre ya existe.
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), String> {
        let n = tool.spec().name;
        if self.get(&n).is_some() {
            return Err(format!("tool duplicada: {n}"));
        }
        self.tools.push(tool);
        Ok(())
    }
    /// Busca por nombre.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.spec().name == name).cloned()
    }
    /// Specs permitidas, de menor a mayor privilegio y luego por nombre.
    pub fn specs_filtered(&self, allow: impl Fn(&str, &[Effect]) -> bool) -> Vec<ToolSpec> {
        let mut v: Vec<(u8, ToolSpec)> = self
            .tools
            .iter()
            .filter(|t| allow(&t.spec().name, t.effects()))
            .map(|t| (privilege(t.effects()), t.spec()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        v.into_iter().map(|x| x.1).collect()
    }
}

/// Resuelve `p` dentro de `root` sin tocar el disco.
pub fn resolve_in_root(root: &Path, p: &str) -> Result<PathBuf, String> {
    let pp = Path::new(p);
    let joined = if pp.is_absolute() {
        pp.to_path_buf()
    } else {
        root.join(pp)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return Err(format!("ruta fuera de la raíz: {p}"));
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.starts_with(root) {
        Ok(out)
    } else {
        Err(format!("ruta fuera de la raíz: {p}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct T(&'static str, &'static [Effect]);
    #[async_trait::async_trait]
    impl Tool for T {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: self.0.into(),
                description: String::new(),
                parameters: serde_json::json!({}),
            }
        }
        fn effects(&self) -> &'static [Effect] {
            self.1
        }
        async fn call(&self, _: serde_json::Value, _: &ToolContext) -> ToolOutput {
            ToolOutput::ok("")
        }
    }
    #[test]
    fn privilegio_es_el_maximo() {
        assert_eq!(privilege(&[]), 0);
        assert_eq!(privilege(&[Effect::Read, Effect::Exec]), 3);
    }
    #[test]
    fn registro_rechaza_duplicados() {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(T("a", &[]))).unwrap();
        assert!(r.register(Arc::new(T("a", &[]))).is_err());
    }
    #[test]
    fn specs_ordenadas_por_privilegio() {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(T("z_bash", &[Effect::Exec]))).unwrap();
        r.register(Arc::new(T("b_leer", &[Effect::Read]))).unwrap();
        r.register(Arc::new(T("a_leer", &[Effect::Read]))).unwrap();
        let n: Vec<_> = r
            .specs_filtered(|_, _| true)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(n, ["a_leer", "b_leer", "z_bash"]);
    }
    #[test]
    fn specs_filtradas_ocultan_las_no_permitidas() {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(T("a", &[Effect::Read]))).unwrap();
        r.register(Arc::new(T("b", &[Effect::Exec]))).unwrap();
        assert_eq!(r.specs_filtered(|n, _| n == "a").len(), 1);
    }
    #[test]
    fn resolve_relativa_ok() {
        assert_eq!(
            resolve_in_root(Path::new("/r"), "a/./b").unwrap(),
            Path::new("/r/a/b")
        );
    }
    #[test]
    fn resolve_escape_con_puntos_es_error() {
        assert!(resolve_in_root(Path::new("/r"), "../x").is_err());
        assert!(resolve_in_root(Path::new("/r"), "a/../../x").is_err());
    }
    #[test]
    fn resolve_absoluta_fuera_es_error() {
        assert!(resolve_in_root(Path::new("/r"), "/etc/passwd").is_err());
        assert!(resolve_in_root(Path::new("/r"), "/r/a").is_ok());
    }
}
