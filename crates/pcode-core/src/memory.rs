//! Memoria del proyecto (F8.19): decisiones y restricciones que tienen que durar más que una
//! conversación, en `.pcode/memoria.md`, y que el agente lee en cada turno.
//!
//! - El usuario añade notas con `/recuerda` (TUI y chat) o editando el archivo (Studio).
//! - El agente puede **proponer** una nota con la tool `recordar`, pero solo se guarda si el usuario
//!   la aprueba: la memoria persistente es una superficie de ataque (AMIA, «Query-Efficient Memory
//!   Injection Attacks on LLM Agents», 2026.findings-emnlp.1312).
//! - Entra en el prompt de sistema de cada turno (lo más reciente primero, como mucho
//!   [`MAX_PROMPT_BYTES`]), así que sobrevive a la compactación: SLUMP («When the Specification
//!   Emerges», 2026.findings-emnlp.76) muestra que los compromisos de diseño revelados a lo largo de
//!   una sesión se pierden, y que un estado externo del proyecto recupera la mayor parte.
use pcode_models::ToolSpec;
use serde_json::json;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Archivo de la memoria, relativo a la raíz del proyecto.
pub const MEMORY_FILE: &str = ".pcode/memoria.md";
/// Tamaño máximo de la memoria que entra en el prompt.
pub const MAX_PROMPT_BYTES: usize = 8 * 1024;
/// Nombre de la tool con la que el agente propone recordar algo.
pub const REMEMBER_TOOL: &str = "recordar";

const HEADER: &str = "# Memoria del proyecto\n\n\
Decisiones y restricciones que el agente lee en cada turno. Una por línea; se pueden editar a mano.\n\n";

/// Ruta del archivo de memoria.
pub fn memory_path(root: &Path) -> PathBuf {
    root.join(MEMORY_FILE)
}

/// Fecha `AAAA-MM-DD` (UTC) de un instante en segundos desde 1970.
pub fn ymd(secs: u64) -> String {
    // Algoritmo de días civiles de Howard Hinnant.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn today() -> String {
    ymd(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0))
}

/// Notas guardadas, en el orden del archivo (la más antigua primero).
pub fn notes(root: &Path) -> Vec<String> {
    std::fs::read_to_string(memory_path(root))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("- "))
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Añade una nota con la fecha de hoy. `origin` distingue lo que propuso el agente.
pub fn remember(root: &Path, text: &str, from_agent: bool) -> io::Result<String> {
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nota vacía"));
    }
    let note = format!(
        "[{}{}] {one}",
        today(),
        if from_agent { " · agente" } else { "" }
    );
    let p = memory_path(root);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let new = !p.exists();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)?;
    if new {
        f.write_all(HEADER.as_bytes())?;
    }
    writeln!(f, "- {note}")?;
    Ok(note)
}

/// Borra la nota número `n` (desde 1, en el orden de [`notes`]) y la devuelve.
pub fn forget(root: &Path, n: usize) -> io::Result<Option<String>> {
    let p = memory_path(root);
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Ok(None);
    };
    let mut seen = 0;
    let mut removed = None;
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| {
            if let Some(rest) = l.strip_prefix("- ")
                && !rest.trim().is_empty()
            {
                seen += 1;
                if seen == n {
                    removed = Some(rest.trim().to_string());
                    return false;
                }
            }
            true
        })
        .collect();
    if removed.is_some() {
        std::fs::write(&p, format!("{}\n", kept.join("\n")))?;
    }
    Ok(removed)
}

/// Sección para el prompt de sistema: las notas, la más reciente primero, hasta
/// [`MAX_PROMPT_BYTES`]. `None` si no hay memoria.
pub fn prompt_section(root: &Path) -> Option<String> {
    let all = notes(root);
    if all.is_empty() {
        return None;
    }
    let mut out = String::from(
        "# Memoria del proyecto (decisiones y restricciones que debes respetar; la más reciente primero)\n",
    );
    let mut omitted = 0;
    for n in all.iter().rev() {
        let line = format!("- {n}\n");
        if out.len() + line.len() > MAX_PROMPT_BYTES {
            omitted += 1;
            continue;
        }
        out.push_str(&line);
    }
    if omitted > 0 {
        out.push_str(&format!(
            "[{omitted} notas antiguas no caben: están en {MEMORY_FILE}]\n"
        ));
    }
    Some(out)
}

/// Especificación de la tool con la que el agente propone recordar algo (requiere aprobación).
pub fn remember_spec() -> ToolSpec {
    ToolSpec {
        name: REMEMBER_TOOL.into(),
        description: "Propón guardar en la memoria del proyecto una decisión o restricción que el usuario ha \
            dejado clara y que debe durar más allá de esta conversación (p. ej. «los commits, en inglés»). \
            El usuario tiene que aprobarla. No guardes datos temporales ni secretos."
            .into(),
        parameters: json!({"type":"object","properties":{"texto":{"type":"string"}},"required":["texto"]}),
    }
}
