//! Sección `## Contrato` de una skill.
use crate::registry::{Skill, SkillError};

/// Un campo de entrada o salida.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Nombre.
    pub name: String,
    /// Tipo.
    pub ty: String,
    /// Descripción.
    pub doc: String,
}

/// Contrato tipado de una skill.
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    /// Entradas.
    pub inputs: Vec<Field>,
    /// Salidas.
    pub outputs: Vec<Field>,
    /// Ejemplo de invocación.
    pub example: String,
    /// Checkpoints de estado.
    pub checkpoints: Vec<String>,
}

fn section<'a>(body: &'a str, heading: &str, level: usize) -> Option<&'a str> {
    let marker = format!("{} {}", "#".repeat(level), heading);
    let start = body
        .lines()
        .scan(0usize, |pos, l| {
            let p = *pos;
            *pos += l.len() + 1;
            Some((p, l))
        })
        .find(|(_, l)| l.trim() == marker)?;
    let from = start.0 + start.1.len() + 1;
    let rest = body.get(from..).unwrap_or("");
    let mut end = rest.len();
    let mut pos = 0;
    for l in rest.lines() {
        let lvl = l.chars().take_while(|c| *c == '#').count();
        if lvl > 0 && lvl <= level && l.chars().nth(lvl) == Some(' ') {
            end = pos;
            break;
        }
        pos += l.len() + 1;
    }
    Some(&rest[..end.min(rest.len())])
}

fn fields(s: &str) -> Vec<Field> {
    s.lines()
        .filter_map(|l| l.trim().strip_prefix("- "))
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let (ty, doc) = rest
                .split_once('—')
                .or_else(|| rest.split_once(" - "))
                .unwrap_or((rest, ""));
            Some(Field {
                name: name.trim().into(),
                ty: ty.trim().into(),
                doc: doc.trim().into(),
            })
        })
        .collect()
}

/// Parsea el contrato (None si no hay sección; error si falta el ejemplo).
pub fn parse_contract(body: &str) -> Result<Option<Contract>, SkillError> {
    let Some(c) = section(body, "Contrato", 2) else {
        return Ok(None);
    };
    let example = section(c, "Ejemplo", 3)
        .map(|e| e.trim().to_string())
        .unwrap_or_default();
    if example.is_empty() {
        return Err(SkillError::Invalid(
            "contrato".into(),
            "falta `### Ejemplo`: un contrato sin plantilla concreta no ayuda".into(),
        ));
    }
    Ok(Some(Contract {
        inputs: section(c, "Entradas", 3).map(fields).unwrap_or_default(),
        outputs: section(c, "Salidas", 3).map(fields).unwrap_or_default(),
        example,
        checkpoints: section(c, "Checkpoints", 3)
            .map(|s| {
                s.lines()
                    .filter_map(|l| l.trim().strip_prefix("- "))
                    .map(|x| x.trim().to_string())
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

/// Avisos legibles sobre una skill.
pub fn lint(skill: &Skill) -> Vec<String> {
    let mut w = vec![];
    if skill.description.chars().count() > 300 {
        w.push(format!(
            "{}: la descripción supera los 300 caracteres",
            skill.name
        ));
    }
    match parse_contract(&skill.body) {
        Ok(Some(c)) if c.inputs.is_empty() => {
            w.push(format!("{}: el contrato no declara entradas", skill.name))
        }
        Err(e) => w.push(format!("{}: {e}", skill.name)),
        _ => {}
    }
    if skill
        .provenance
        .as_deref()
        .is_some_and(|p| p.starts_with("destilada"))
        && skill.do_not_use_when.is_none()
    {
        w.push(format!(
            "{}: una skill aprendida debe declarar `no_usar_cuando`",
            skill.name
        ));
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    const OK: &str = "# S\n\n## Contrato\n\n### Entradas\n- path: string — archivo a leer\n- n: int — líneas\n\n### Salidas\n- texto: string — contenido\n\n### Ejemplo\n```\nleer(path=\"a\", n=3)\n```\n\n### Checkpoints\n- leido\n\n## Otra\nx\n";
    fn sk(body: &str, prov: Option<&str>, nu: Option<&str>) -> Skill {
        Skill {
            name: "s".into(),
            description: "d".into(),
            depends: vec![],
            do_not_use_when: nu.map(String::from),
            provenance: prov.map(String::from),
            dir: ".".into(),
            body: body.into(),
        }
    }
    #[test]
    fn contrato_completo() {
        let c = parse_contract(OK).unwrap().unwrap();
        assert_eq!(c.inputs.len(), 2);
        assert_eq!(
            c.inputs[0],
            Field {
                name: "path".into(),
                ty: "string".into(),
                doc: "archivo a leer".into()
            }
        );
        assert_eq!(c.outputs.len(), 1);
        assert!(c.example.contains("leer(path"));
        assert_eq!(c.checkpoints, ["leido"]);
    }
    #[test]
    fn sin_contrato_es_none() {
        assert!(parse_contract("# S\ntexto").unwrap().is_none());
    }
    #[test]
    fn contrato_sin_ejemplo_es_error() {
        assert!(parse_contract("## Contrato\n### Entradas\n- a: int — x\n").is_err());
    }
    #[test]
    fn checkpoints() {
        assert_eq!(parse_contract(OK).unwrap().unwrap().checkpoints.len(), 1);
    }
    #[test]
    fn lint_aprendida_sin_no_usar_cuando() {
        let w = lint(&sk("", Some("destilada de T1"), None));
        assert!(w.iter().any(|x| x.contains("no_usar_cuando")));
        assert!(lint(&sk("", Some("destilada de T1"), Some("nunca"))).is_empty());
    }
}
