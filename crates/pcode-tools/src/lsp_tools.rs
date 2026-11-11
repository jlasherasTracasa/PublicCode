//! Formato de las tools de LSP: el modelo da una línea (desde 1) y el nombre del símbolo, y aquí se
//! calcula la posición que pide el protocolo. También guarda los últimos diagnósticos publicados.
use crate::registry::{Effect, Tool, ToolContext, ToolOutput, resolve_in_root};
use pcode_lsp::{Diagnostic, DiagnosticsSink, Location, LspClient, LspManager, rel_path};
use pcode_models::ToolSpec;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Máximo de posiciones o diagnósticos que se muestran.
const MAX_LISTA: usize = 50;
/// Espera por defecto de `diagnosticos` y tope máximo, en milisegundos.
const ESPERA_DEFECTO_MS: u64 = 5000;
const ESPERA_MAXIMA_MS: u64 = 30_000;

/// Posición LSP (línea desde 0, columna UTF-16) de `symbol` en la línea `line` (desde 1) de `text`.
pub fn find_position(text: &str, line: u32, symbol: &str) -> Result<(u32, u32), String> {
    let lineas: Vec<&str> = text.lines().collect();
    let n = lineas.len();
    if line == 0 || line as usize > n {
        return Err(format!(
            "la línea {line} no existe (el archivo tiene {n} líneas)"
        ));
    }
    let linea = lineas[line as usize - 1];
    match linea.find(symbol) {
        Some(i) => {
            let columna = linea[..i].encode_utf16().count() as u32;
            Ok((line - 1, columna))
        }
        None => {
            let texto: String = linea.trim().chars().take(100).collect();
            Err(format!("«{symbol}» no aparece en la línea {line}: {texto}"))
        }
    }
}

/// Línea `line` (desde 0) del archivo `root/path`, sin espacios en los extremos y recortada.
fn linea_de(root: &Path, path: &str, line: u32) -> Option<String> {
    let texto = std::fs::read_to_string(root.join(path)).ok()?;
    let linea = texto.lines().nth(line as usize)?;
    Some(linea.trim().chars().take(120).collect())
}

/// Posiciones como texto `ruta:línea:columna  código`.
pub fn render_locations(root: &Path, locs: &[Location]) -> String {
    if locs.is_empty() {
        return "sin resultados".into();
    }
    let mut out: Vec<String> = locs
        .iter()
        .take(MAX_LISTA)
        .map(|l| {
            let base = format!("{}:{}:{}", l.path, l.line + 1, l.character + 1);
            match linea_de(root, &l.path, l.line) {
                Some(t) if !t.is_empty() => format!("{base}  {t}"),
                _ => base,
            }
        })
        .collect();
    if locs.len() > MAX_LISTA {
        out.push(format!("… y {} más", locs.len() - MAX_LISTA));
    }
    out.join("\n")
}

/// Diagnósticos de un archivo como texto, los errores primero.
pub fn render_diagnostics(path: &str, diags: &[Diagnostic]) -> String {
    if diags.is_empty() {
        return format!("{path}: sin diagnósticos");
    }
    let errores = diags.iter().filter(|d| d.severity == 1).count();
    let avisos = diags.iter().filter(|d| d.severity == 2).count();
    let mut ordenados: Vec<&Diagnostic> = diags.iter().collect();
    ordenados.sort_by_key(|d| (d.severity, d.line));
    let mut out = vec![format!("{path}: {errores} errores, {avisos} avisos")];
    for d in ordenados.into_iter().take(MAX_LISTA) {
        let sev = match d.severity {
            1 => "error",
            2 => "aviso",
            3 => "info",
            _ => "pista",
        };
        let primera = d.message.lines().next().unwrap_or("");
        let fuente = if d.source.is_empty() {
            String::new()
        } else {
            format!(" [{}]", d.source)
        };
        out.push(format!(
            "{path}:{}:{} {sev} {primera}{fuente}",
            d.line + 1,
            d.character + 1
        ));
    }
    out.join("\n")
}

/// Últimos diagnósticos publicados por los servidores, por ruta.
#[derive(Clone, Default)]
pub struct DiagStore(Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>);

impl DiagStore {
    /// Almacén vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Acceso al mapa; un cerrojo envenenado no impide seguir.
    fn guardado(&self) -> MutexGuard<'_, HashMap<String, Vec<Diagnostic>>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sumidero para `LspManager::new` que guarda lo que llega.
    pub fn sink(&self) -> DiagnosticsSink {
        let store = self.clone();
        Arc::new(move |path: String, diags: Vec<Diagnostic>| {
            store.guardado().insert(path, diags);
        })
    }

    /// Saca los diagnósticos de una ruta, si han llegado.
    pub fn take(&self, path: &str) -> Option<Vec<Diagnostic>> {
        self.guardado().remove(path)
    }

    /// Olvida los diagnósticos de una ruta.
    pub fn clear(&self, path: &str) {
        self.guardado().remove(path);
    }
}

/// Argumento de texto obligatorio.
fn arg_str<'a>(args: &'a Value, nombre: &str) -> Result<&'a str, ToolOutput> {
    args.get(nombre)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolOutput::error(format!("falta `{nombre}`")))
}

/// Argumento `line` obligatorio (entero desde 1).
fn arg_linea(args: &Value) -> Result<u32, ToolOutput> {
    args.get("line")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| ToolOutput::error("falta `line` (un entero desde 1)"))
}

/// Ruta relativa al proyecto y texto del archivo.
async fn leer(ctx: &ToolContext, path: &str) -> Result<(String, String), String> {
    let full = resolve_in_root(&ctx.root, path)?;
    let rel = rel_path(&ctx.root, &full);
    let texto = tokio::fs::read_to_string(&full)
        .await
        .map_err(|e| format!("no se pudo leer {path}: {e}"))?;
    Ok((rel, texto))
}

/// Cliente del servidor del lenguaje de `rel`, o el error para el modelo.
async fn cliente(manager: &LspManager, rel: &str) -> Result<Arc<LspClient>, ToolOutput> {
    manager
        .client_for(rel)
        .await
        .map_err(|e| ToolOutput::error(format!("{e}; mientras tanto compila con `bash`")))
}

/// Tool `diagnosticos`.
pub struct Diagnostics {
    manager: Arc<LspManager>,
    store: DiagStore,
}

#[async_trait::async_trait]
impl Tool for Diagnostics {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "diagnosticos".into(),
            description: "Errores y avisos del servidor de lenguaje para un archivo, sin compilar el proyecto."
                .into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"espera_ms":{"type":"integer"}},"required":["path"]}),
        }
    }

    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }

    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let path = match arg_str(&args, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };
        let (rel, texto) = match leer(ctx, path).await {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(e),
        };
        let client = match cliente(&self.manager, &rel).await {
            Ok(c) => c,
            Err(e) => return e,
        };
        self.store.clear(&rel);
        if let Err(e) = client.sync(&rel, &texto).await {
            return ToolOutput::error(e);
        }
        if let Err(e) = client.saved(&rel).await {
            return ToolOutput::error(e);
        }
        let espera = args
            .get("espera_ms")
            .and_then(Value::as_u64)
            .unwrap_or(ESPERA_DEFECTO_MS)
            .min(ESPERA_MAXIMA_MS);
        let limite = Instant::now() + Duration::from_millis(espera);
        loop {
            if let Some(d) = self.store.take(&rel) {
                return ToolOutput::ok(render_diagnostics(&rel, &d));
            }
            if Instant::now() >= limite {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        ToolOutput::ok(format!(
            "{rel}: el servidor aún no ha publicado diagnósticos; repite la llamada en unos segundos"
        ))
    }
}

/// Tool `definicion`.
#[allow(dead_code)] // El almacén se guarda para que las tres tools tengan la misma forma; solo `diagnosticos` lo lee.
pub struct Definition {
    manager: Arc<LspManager>,
    store: DiagStore,
}

#[async_trait::async_trait]
impl Tool for Definition {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "definicion".into(),
            description: "Dónde se define el símbolo `symbol` que aparece en la línea `line` de `path` (exacto, mejor que buscar texto)."
                .into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"line":{"type":"integer"},"symbol":{"type":"string"}},"required":["path","line","symbol"]}),
        }
    }

    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }

    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let path = match arg_str(&args, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };
        let symbol = match arg_str(&args, "symbol") {
            Ok(s) => s,
            Err(e) => return e,
        };
        let linea = match arg_linea(&args) {
            Ok(l) => l,
            Err(e) => return e,
        };
        let (rel, texto) = match leer(ctx, path).await {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(e),
        };
        let (l, c) = match find_position(&texto, linea, symbol) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };
        let client = match cliente(&self.manager, &rel).await {
            Ok(c) => c,
            Err(e) => return e,
        };
        if let Err(e) = client.sync(&rel, &texto).await {
            return ToolOutput::error(e);
        }
        match client.definition(&rel, l, c).await {
            Ok(locs) => ToolOutput::ok(render_locations(&ctx.root, &locs)),
            Err(e) => ToolOutput::error(e),
        }
    }
}

/// Tool `referencias`.
#[allow(dead_code)] // El almacén se guarda para que las tres tools tengan la misma forma; solo `diagnosticos` lo lee.
pub struct References {
    manager: Arc<LspManager>,
    store: DiagStore,
}

#[async_trait::async_trait]
impl Tool for References {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "referencias".into(),
            description:
                "Todos los usos del símbolo `symbol` que aparece en la línea `line` de `path`."
                    .into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"},"line":{"type":"integer"},"symbol":{"type":"string"}},"required":["path","line","symbol"]}),
        }
    }

    fn effects(&self) -> &'static [Effect] {
        &[Effect::Read]
    }

    async fn call(&self, args: Value, ctx: &ToolContext) -> ToolOutput {
        let path = match arg_str(&args, "path") {
            Ok(p) => p,
            Err(e) => return e,
        };
        let symbol = match arg_str(&args, "symbol") {
            Ok(s) => s,
            Err(e) => return e,
        };
        let linea = match arg_linea(&args) {
            Ok(l) => l,
            Err(e) => return e,
        };
        let (rel, texto) = match leer(ctx, path).await {
            Ok(v) => v,
            Err(e) => return ToolOutput::error(e),
        };
        let (l, c) = match find_position(&texto, linea, symbol) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };
        let client = match cliente(&self.manager, &rel).await {
            Ok(c) => c,
            Err(e) => return e,
        };
        if let Err(e) = client.sync(&rel, &texto).await {
            return ToolOutput::error(e);
        }
        match client.references(&rel, l, c).await {
            Ok(locs) => ToolOutput::ok(render_locations(&ctx.root, &locs)),
            Err(e) => ToolOutput::error(e),
        }
    }
}

/// Las tres tools sobre un gestor ya creado (su sumidero debe ser `store.sink()`).
pub fn lsp_tools(manager: Arc<LspManager>, store: DiagStore) -> Vec<Arc<dyn Tool>> {
    let diag: Arc<dyn Tool> = Arc::new(Diagnostics {
        manager: manager.clone(),
        store: store.clone(),
    });
    let def: Arc<dyn Tool> = Arc::new(Definition {
        manager: manager.clone(),
        store: store.clone(),
    });
    let refs: Arc<dyn Tool> = Arc::new(References { manager, store });
    vec![diag, def, refs]
}

/// Las tres tools para un proyecto; `overrides` es la orden por grupo (`[lsp.<grupo>]` de la configuración).
pub fn lsp_tools_for(root: &Path, overrides: HashMap<String, Vec<String>>) -> Vec<Arc<dyn Tool>> {
    let store = DiagStore::new();
    let m = LspManager::new(root, overrides, store.sink());
    lsp_tools(Arc::new(m), store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(line: u32, character: u32, severity: u8, message: &str, source: &str) -> Diagnostic {
        Diagnostic {
            line,
            character,
            end_line: line,
            end_character: character + 1,
            severity,
            message: message.into(),
            source: source.into(),
        }
    }

    #[test]
    fn formato_posicion_en_utf16() {
        assert_eq!(
            find_position("fn main() {\n    let año = doble(1);\n}\n", 2, "doble"),
            Ok((1, 14))
        );
        assert_eq!(
            find_position("fn main() {}\n", 1, "doble"),
            Err("«doble» no aparece en la línea 1: fn main() {}".into())
        );
        assert_eq!(
            find_position("fn main() {}\n", 9, "main"),
            Err("la línea 9 no existe (el archivo tiene 1 líneas)".into())
        );
    }

    #[test]
    fn formato_posiciones_con_el_codigo() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/lib.rs"), "pub fn doble() {}\n").unwrap();
        let locs = vec![Location {
            path: "src/lib.rs".into(),
            line: 0,
            character: 7,
        }];
        assert_eq!(
            render_locations(d.path(), &locs),
            "src/lib.rs:1:8  pub fn doble() {}"
        );
        let sin_archivo = vec![Location {
            path: "x.rs".into(),
            line: 0,
            character: 0,
        }];
        assert_eq!(render_locations(d.path(), &sin_archivo), "x.rs:1:1");
        assert_eq!(render_locations(d.path(), &[]), "sin resultados");
    }

    #[test]
    fn formato_diagnosticos_errores_primero() {
        let ds = vec![
            diag(4, 0, 2, "sin usar", "rustc"),
            diag(1, 4, 1, "tipo incorrecto\nmás", ""),
        ];
        assert_eq!(
            render_diagnostics("a.rs", &ds),
            "a.rs: 1 errores, 1 avisos\na.rs:2:5 error tipo incorrecto\na.rs:5:1 aviso sin usar [rustc]"
        );
        assert_eq!(render_diagnostics("a.rs", &[]), "a.rs: sin diagnósticos");
    }

    #[test]
    fn formato_almacen_guarda_lo_ultimo() {
        let store = DiagStore::new();
        let sink = store.sink();
        sink("a.rs".into(), vec![diag(0, 0, 1, "uno", "")]);
        sink("a.rs".into(), vec![diag(1, 0, 2, "dos", "")]);
        let got = store.take("a.rs").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].message, "dos");
        assert!(store.take("a.rs").is_none());
        sink("b.rs".into(), vec![diag(0, 0, 1, "x", "")]);
        store.clear("b.rs");
        assert!(store.take("b.rs").is_none());
    }
}
