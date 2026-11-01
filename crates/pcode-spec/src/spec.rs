//! `spec.md`: lo único que escribe la persona. Título, objetivo, requisitos con sus criterios de
//! aceptación (algunos ejecutables con `acepta:`), restricciones y lo que queda fuera.
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Un criterio de aceptación de un requisito.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Criterion {
    /// El criterio tal como está escrito.
    pub text: String,
    /// La orden que lo comprueba (`- acepta: \`cargo test x\``), si es ejecutable.
    pub command: Option<String>,
}

/// Un requisito (`### R1. Título`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Requirement {
    /// Identificador (`R1`, `RF-2`…).
    pub id: String,
    /// Título.
    pub title: String,
    /// El texto completo de la sección (sin comentarios HTML).
    pub body: String,
    /// Criterios de aceptación.
    pub criteria: Vec<Criterion>,
    /// Huella del requisito: cambia si cambia su texto (para detectar la deriva del plan).
    pub hash: String,
}

impl Requirement {
    /// Órdenes ejecutables de sus criterios.
    pub fn commands(&self) -> Vec<&str> {
        self.criteria
            .iter()
            .filter_map(|c| c.command.as_deref())
            .collect()
    }
}

/// Una especificación ya leída.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Spec {
    /// Nombre (el de su carpeta en `specs/`).
    pub name: String,
    /// Título (la primera línea `# …`).
    pub title: String,
    /// Sección «Objetivo».
    pub objective: String,
    /// Requisitos en orden.
    pub requirements: Vec<Requirement>,
    /// Sección «Restricciones» (una por viñeta).
    pub constraints: Vec<String>,
    /// Sección «Fuera de alcance» (una por viñeta).
    pub out_of_scope: Vec<String>,
    /// Avisos que no impiden planificar (requisitos sin criterios…).
    pub warnings: Vec<String>,
}

impl Spec {
    /// El requisito `id`.
    pub fn requirement(&self, id: &str) -> Option<&Requirement> {
        self.requirements.iter().find(|r| r.id == id)
    }
}

/// Quita los comentarios HTML (`<!-- … -->`, también de varias líneas).
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("<!--") {
        out.push_str(&rest[..i]);
        match rest[i..].find("-->") {
            Some(j) => rest = &rest[i + j + 3..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Huella corta (8 caracteres) de un texto. Solo cuentan las palabras: cambiar espacios o
/// saltos de línea (reajustar un párrafo) no cambia la huella.
pub fn fingerprint(text: &str) -> String {
    let norm = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let h = Sha256::digest(norm.as_bytes());
    hex::encode(&h[..4])
}

/// ¿Es `s` un identificador de requisito o de tarea (`R1`, `RF-2`, `T3`, `R1.2`)?
pub fn is_id(s: &str) -> bool {
    let letters = s.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let rest = s[letters..].trim_start_matches('-');
    (1..=6).contains(&letters)
        && !rest.is_empty()
        && rest.chars().next().is_some_and(|c| c.is_ascii_digit())
        && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !rest.ends_with('.')
}

/// Separa `"R1. Título"` (o `R1 · Título`, `R1: Título`, `R1 - Título`) en id y título.
pub fn split_id(heading: &str) -> Option<(String, String)> {
    let h = heading.trim();
    let end = h
        .find(|c: char| c.is_whitespace() || c == ':' || c == '·' || c == '—')
        .unwrap_or(h.len());
    let id = h[..end].trim_end_matches(['.', ':']);
    if !is_id(id) {
        return None;
    }
    let title = h[end..]
        .trim_start_matches(|c: char| c.is_whitespace() || ".:·-—–".contains(c))
        .trim();
    Some((id.to_string(), title.to_string()))
}

/// El texto de una viñeta (`- `, `* `, `- [ ] `), o `None` si la línea no lo es.
pub fn bullet(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
    let rest = rest
        .strip_prefix("[ ] ")
        .or_else(|| rest.strip_prefix("[x] "))
        .or_else(|| rest.strip_prefix("[X] "))
        .unwrap_or(rest);
    Some(rest.trim())
}

/// La orden de `acepta: \`cmd\`` (sin las comillas invertidas), si la línea lo es.
pub fn accept_command(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let rest = ["acepta:", "aceptación:", "accept:"]
        .iter()
        .find_map(|p| lower.starts_with(p).then(|| &text[p.len()..]))?;
    let cmd = rest.trim().trim_matches('`').trim();
    (!cmd.is_empty()).then(|| cmd.to_string())
}

fn section_kind(heading: &str) -> &'static str {
    let h = heading.trim().to_lowercase();
    if h.starts_with("objetivo") || h.starts_with("goal") || h.starts_with("contexto") {
        "objetivo"
    } else if h.starts_with("requisito") || h.starts_with("requirement") {
        "requisitos"
    } else if h.starts_with("restricci") || h.starts_with("constraint") {
        "restricciones"
    } else if h.starts_with("fuera de alcance") || h.starts_with("out of scope") {
        "fuera"
    } else {
        "otra"
    }
}

/// Lee una especificación. Falla si no tiene requisitos, si un `###` de «Requisitos» no tiene
/// identificador o si hay identificadores repetidos.
pub fn parse_spec(name: &str, text: &str) -> Result<Spec, String> {
    let clean = strip_comments(&text.replace("\r\n", "\n"));
    let mut spec = Spec {
        name: name.to_string(),
        title: String::new(),
        objective: String::new(),
        requirements: vec![],
        constraints: vec![],
        out_of_scope: vec![],
        warnings: vec![],
    };
    let mut section = "otra";
    let mut current: Option<(String, String, Vec<String>)> = None;
    let mut errors = vec![];
    let finish = |cur: &mut Option<(String, String, Vec<String>)>, spec: &mut Spec| {
        if let Some((id, title, lines)) = cur.take() {
            let body = lines.join("\n").trim().to_string();
            let criteria: Vec<Criterion> = lines
                .iter()
                .filter_map(|l| bullet(l))
                .filter(|t| !t.is_empty())
                .map(|t| Criterion {
                    text: t.to_string(),
                    command: accept_command(t),
                })
                .collect();
            let hash = fingerprint(&format!("{title}\n{body}"));
            spec.requirements.push(Requirement {
                id,
                title,
                body,
                criteria,
                hash,
            });
        }
    };
    for line in clean.lines() {
        let t = line.trim_end();
        if let Some(h) = t.strip_prefix("# ") {
            finish(&mut current, &mut spec);
            if spec.title.is_empty() {
                spec.title = h.trim().to_string();
            }
            section = "otra";
        } else if let Some(h) = t.strip_prefix("## ") {
            finish(&mut current, &mut spec);
            section = section_kind(h);
        } else if let Some(h) = t.strip_prefix("### ") {
            finish(&mut current, &mut spec);
            match split_id(h) {
                Some((id, title)) => current = Some((id, title, vec![])),
                None if section == "requisitos" => errors.push(format!(
                    "«### {}»: falta el identificador del requisito (escribe «### R1. {}»)",
                    h.trim(),
                    h.trim()
                )),
                None => {}
            }
        } else if let Some((_, _, lines)) = current.as_mut() {
            lines.push(t.to_string());
        } else {
            match section {
                "objetivo" => {
                    spec.objective.push_str(t);
                    spec.objective.push('\n');
                }
                "restricciones" => spec.constraints.extend(bullet(t).map(String::from)),
                "fuera" => spec.out_of_scope.extend(bullet(t).map(String::from)),
                _ => {}
            }
        }
    }
    finish(&mut current, &mut spec);
    spec.objective = spec.objective.trim().to_string();
    if spec.title.is_empty() {
        spec.title = name.to_string();
    }
    let mut seen = std::collections::HashSet::new();
    for r in &spec.requirements {
        if !seen.insert(r.id.clone()) {
            errors.push(format!("el requisito {} está repetido", r.id));
        }
    }
    if spec.requirements.is_empty() && errors.is_empty() {
        errors.push("no hay requisitos: añade «### R1. …» bajo «## Requisitos»".into());
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    for r in &spec.requirements {
        if r.criteria.is_empty() {
            spec.warnings.push(format!(
                "{} no tiene criterios de aceptación: añade viñetas con lo que debe cumplirse",
                r.id
            ));
        }
    }
    Ok(spec)
}

/// Plantilla de `spec.md` para una especificación nueva.
pub fn template(title: &str) -> String {
    format!(
        "# {title}

<!-- Especificación de PublicCode: es lo único que escribes. Desde aquí:
     pcode spec planificar <nombre>   → plan.md (diseño) y tareas.md (pasos con su comprobación)
     pcode spec implementar <nombre>  → el agente escribe el código y los tests, tarea a tarea
     pcode spec verificar <nombre>    → comprueba todos los criterios y dice qué requisito cumple
     Si cambias un requisito, vuelve a planificar: solo se rehace lo que cambió. -->

## Objetivo

Qué se quiere conseguir, para quién y por qué. Dos o tres frases.

## Requisitos

### R1. Nombre corto del primer requisito

CUANDO <situación o acción> EL SISTEMA DEBE <comportamiento observable>.

- Un criterio de aceptación comprobable (entrada → resultado esperado)
- Qué pasa con una entrada no válida o en el límite
<!-- Si ya sabes cómo comprobarlo, añade «- acepta: `cargo test nombre_del_test`»; si no, el plan crea el test. -->

## Restricciones

- Por ejemplo: sin dependencias nuevas; funciona en Linux y Windows

## Fuera de alcance

- Lo que no se hace ahora
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const EJEMPLO: &str = "# Exportar facturas a CSV

<!-- comentario
     de varias líneas con ### R9. que no cuenta -->

## Objetivo
Que contabilidad saque las facturas del mes sin copiar a mano.

## Requisitos

### R1. Exportar un mes
CUANDO el usuario ejecuta `facturas exportar --mes 2026-10` EL SISTEMA DEBE escribir un CSV.

- [ ] la cabecera es `numero,fecha,cliente,total`
- acepta: `cargo test exportar_csv`

### R2 · Mes sin facturas
- el CSV solo lleva la cabecera
* Acepta: cargo test mes_vacio

## Notas
texto libre que no es un requisito
### Detalle
- tampoco cuenta

## Restricciones
- sin dependencias nuevas

## Fuera de alcance
- Excel
";

    #[test]
    fn lee_requisitos_criterios_y_secciones() {
        let s = parse_spec("exportar", EJEMPLO).unwrap();
        assert_eq!(s.title, "Exportar facturas a CSV");
        assert!(s.objective.starts_with("Que contabilidad"));
        let ids: Vec<&str> = s.requirements.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["R1", "R2"]);
        let r1 = &s.requirements[0];
        assert_eq!(r1.title, "Exportar un mes");
        assert_eq!(r1.criteria.len(), 2);
        assert_eq!(r1.commands(), ["cargo test exportar_csv"]);
        assert_eq!(s.requirements[1].title, "Mes sin facturas");
        assert_eq!(s.requirements[1].commands(), ["cargo test mes_vacio"]);
        assert_eq!(s.constraints, ["sin dependencias nuevas"]);
        assert_eq!(s.out_of_scope, ["Excel"]);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn la_huella_cambia_con_el_texto_y_no_con_espacios() {
        let a = parse_spec("e", EJEMPLO).unwrap();
        let b = parse_spec(
            "e",
            &EJEMPLO.replace("escribir un CSV.", "escribir  un\n   CSV.\n"),
        )
        .unwrap();
        assert_eq!(a.requirements[0].hash, b.requirements[0].hash);
        let c = parse_spec("e", &EJEMPLO.replace("escribir un CSV", "escribir un TSV")).unwrap();
        assert_ne!(a.requirements[0].hash, c.requirements[0].hash);
        assert_eq!(a.requirements[1].hash, c.requirements[1].hash);
    }

    #[test]
    fn errores_claros() {
        let e = parse_spec("x", "# X\n\n## Requisitos\n\n### Exportar\n- algo\n").unwrap_err();
        assert!(
            e.contains("falta el identificador") && e.contains("### R1. Exportar"),
            "{e}"
        );
        let e = parse_spec("x", "# X\n## Objetivo\nnada\n").unwrap_err();
        assert!(e.contains("no hay requisitos"));
        let e = parse_spec("x", "## Requisitos\n### R1. a\n- b\n### R1. c\n- d\n").unwrap_err();
        assert!(e.contains("R1 está repetido"));
        let s = parse_spec("x", "## Requisitos\n### R1. sin criterios\ntexto\n").unwrap();
        assert!(s.warnings[0].contains("R1 no tiene criterios"));
        assert_eq!(s.title, "x");
    }

    #[test]
    fn identificadores() {
        for ok in ["R1", "RF-2", "T10", "R1.2", "REQ3"] {
            assert!(is_id(ok), "{ok}");
        }
        for no in ["R", "1", "Exportar", "R1.", "ABCDEFG1", "R1a"] {
            assert!(!is_id(no), "{no}");
        }
        assert_eq!(
            split_id("R3: Mes vacío"),
            Some(("R3".into(), "Mes vacío".into()))
        );
        assert_eq!(split_id("T2. Título"), Some(("T2".into(), "Título".into())));
        assert_eq!(split_id("Notas"), None);
    }

    #[test]
    fn la_plantilla_es_una_spec_valida() {
        let s = parse_spec("nueva", &template("Mi función")).unwrap();
        assert_eq!(s.title, "Mi función");
        assert_eq!(s.requirements.len(), 1);
        assert_eq!(s.requirements[0].criteria.len(), 2);
        assert!(s.requirements[0].commands().is_empty());
    }
}
