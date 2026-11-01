//! Lo que se le pide al modelo en cada paso: redactar una especificación a partir de una frase,
//! planificarla (diseño y tareas) e implementar una tarea. Funciones puras: se prueban sin modelo.
use crate::spec::Spec;
use crate::status::{PLAN_FILE, SPECS_DIR, TASKS_FILE, list};
use crate::tasks::{Drift, SpecTask, TaskList, parse_tasks, set_done};
use std::path::Path;

/// Máximo del diseño que se pasa a cada tarea (en caracteres).
const MAX_PLAN: usize = 12_000;

/// Instrucción para redactar una especificación.
pub const WRITER_SYSTEM: &str = "Redactas especificaciones para PublicCode. Una especificación dice QUÉ \
debe hacer el sistema y cómo se comprueba, nunca CÓMO se programa. Escribe en español, con requisitos \
pequeños, observables y comprobables.";

/// Instrucción para el planificador.
pub const PLANNER_SYSTEM: &str = "Eres el planificador de PublicCode. La especificación es la fuente de \
verdad: no añadas nada fuera de su alcance ni contradigas sus restricciones. Explora el repositorio con las \
tools de solo lectura (esquema, leer, buscar, listar) lo justo para saber dónde encaja cada cosa, y después \
escribe el diseño y las tareas.";

/// Instrucción para implementar una tarea.
pub const IMPLEMENTER_SYSTEM: &str = "Implementas una tarea de una especificación. La especificación es la \
fuente de verdad: cumple sus requisitos y restricciones sin añadir nada más. Escribe también los tests que \
comprueban los criterios de aceptación de los requisitos de la tarea (incluidos los casos no válidos y los \
límites). No cambies spec.md, plan.md ni tareas.md. Si la especificación es contradictoria o imposible, dilo \
y para en vez de improvisar.";

/// Lo que se le pide al modelo para redactar `spec.md` desde una descripción.
pub fn writer_prompt(name: &str, description: &str) -> String {
    format!(
        "Escribe la especificación «{name}» a partir de esta descripción:\n\n{description}\n\n\
         Formato exacto (Markdown):\n\
         # <Título>\n\n## Objetivo\n<2-3 frases: qué, para quién y por qué>\n\n## Requisitos\n\n\
         ### R1. <nombre corto>\nCUANDO <situación> EL SISTEMA DEBE <comportamiento observable>.\n\n\
         - <criterio de aceptación comprobable: entrada → resultado>\n\
         - <qué pasa con una entrada no válida o en el límite>\n\n\
         (más requisitos R2, R3…, pequeños e independientes)\n\n\
         ## Restricciones\n- <tecnología, compatibilidad, seguridad, rendimiento>\n\n\
         ## Fuera de alcance\n- <lo que no se hace ahora>\n\n\
         Reglas: no inventes detalles que la descripción no da; si algo no está claro, escribe lo razonable y \
         deja debajo una línea `<!-- Duda: … -->` para que la persona decida. Sin líneas `acepta:` salvo que \
         la descripción dé la orden. Responde solo con el contenido de spec.md."
    )
}

/// Qué cambió desde el plan anterior (para replanificar solo eso).
#[derive(Debug, Clone, Copy)]
pub struct Replan<'a> {
    /// `plan.md` anterior.
    pub plan: &'a str,
    /// `tareas.md` anterior.
    pub tasks: &'a str,
    /// Cambios en la especificación.
    pub drift: &'a Drift,
}

/// Lo que se le pide al planificador. `repair` lleva la respuesta anterior y sus problemas para
/// que los corrija.
pub fn planner_prompt(
    name: &str,
    spec_text: &str,
    replan: Option<Replan>,
    repair: Option<(&str, &[String])>,
) -> String {
    let mut p = format!(
        "Planifica la especificación «{name}» ({SPECS_DIR}/{name}/spec.md):\n\n```markdown\n{}\n```\n\n",
        spec_text.trim()
    );
    if let Some(r) = replan {
        let list = |v: &[String]| {
            if v.is_empty() {
                "ninguno".to_string()
            } else {
                v.join(", ")
            }
        };
        p.push_str(&format!(
            "Ya hay un plan. Desde entonces la especificación ha cambiado: requisitos cambiados: {}; nuevos: {}; \
             eliminados: {}.\nConserva tal cual, con su id y su [x], las tareas hechas de requisitos que no han \
             cambiado. Rehaz las de los requisitos cambiados y añade las de los nuevos (como pendientes, con ids \
             nuevos que no choquen con los que hay), y quita las de los eliminados. Ajusta el diseño solo en lo \
             que cambie.\n\nplan.md anterior:\n```markdown\n{}\n```\n\ntareas.md anterior:\n```markdown\n{}\n```\n\n",
            list(&r.drift.changed),
            list(&r.drift.added),
            list(&r.drift.removed),
            r.plan.trim(),
            r.tasks.trim()
        ));
    }
    p.push_str(&format!(
        "Escribe dos documentos:\n\n\
         1. {PLAN_FILE}: el diseño técnico, breve. Secciones: ## Enfoque, ## Archivos (qué se crea o cambia y \
         por qué), ## Interfaces (firmas, formatos, órdenes), ## Decisiones, ## Riesgos y ## Preguntas abiertas \
         (lo que la especificación no deja claro; «- ninguna» si no hay).\n\n\
         2. {TASKS_FILE}: tareas pequeñas, cada una hecha en un turno del agente y comprobada con una orden, en \
         este formato exacto:\n\n\
         # Tareas: <título>\n\n\
         - [ ] T1. <título>\n  - requisitos: R1\n  - toca: ruta/a.rs, ruta/b.rs\n  - depende: —\n  \
         - acepta: `<orden que termina en 0 solo si la tarea está bien hecha>`\n  - <qué hacer, 1-4 líneas>\n\n\
         Reglas:\n\
         - Cada requisito tiene al menos una tarea y cada tarea dice qué requisitos cubre.\n\
         - Los tests salen de los criterios de aceptación de la especificación (también los de seguridad: \
         entradas no válidas, límites, permisos) y los escribe la propia tarea; su `acepta` los ejecuta.\n\
         - `acepta` usa las órdenes de test del proyecto, es concreta (filtra por nombre de test), no usa red y \
         tarda segundos.\n\
         - Las dependencias van antes; nada fuera del alcance.\n\n\
         Responde SOLO con los dos documentos, así:\n\n<<<{PLAN_FILE}\n…\n>>>\n\n<<<{TASKS_FILE}\n…\n>>>\n"
    ));
    if let Some((prev, problems)) = repair {
        p.push_str(&format!(
            "\nTu respuesta anterior no se puede usar todavía. Corrige estos problemas y vuelve a responder con los \
             dos documentos completos:\n{}\n\nRespuesta anterior:\n{}\n",
            problems
                .iter()
                .map(|x| format!("- {x}"))
                .collect::<Vec<_>>()
                .join("\n"),
            prev.chars().take(20_000).collect::<String>()
        ));
    }
    p
}

/// Saca `plan.md` y `tareas.md` de la respuesta del planificador.
pub fn parse_planner_output(text: &str) -> Result<(String, String), String> {
    let mut docs: Vec<(String, Vec<&str>)> = vec![];
    let mut cur: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("<<<") {
            if let Some(c) = cur.take() {
                docs.push(c);
            }
            cur = Some((name.trim().to_string(), vec![]));
        } else if t == ">>>" {
            if let Some(c) = cur.take() {
                docs.push(c);
            }
        } else if let Some((_, v)) = cur.as_mut() {
            v.push(line);
        }
    }
    if let Some(c) = cur.take() {
        docs.push(c);
    }
    let get = |n: &str| {
        docs.iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| strip_fence(&v.join("\n")))
            .filter(|s| !s.trim().is_empty())
    };
    match (get(PLAN_FILE), get(TASKS_FILE)) {
        (Some(p), Some(t)) => Ok((p, t)),
        _ => Err(format!(
            "faltan los documentos: responde con «<<<{PLAN_FILE}» … «>>>» y «<<<{TASKS_FILE}» … «>>>»"
        )),
    }
}

/// Quita una valla de código que envuelva todo el texto (```markdown … ```).
pub fn strip_fence(text: &str) -> String {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```")
        && let Some(body) = rest.strip_suffix("```")
    {
        let body = body.split_once('\n').map(|x| x.1).unwrap_or("");
        return format!("{}\n", body.trim_end());
    }
    format!("{t}\n")
}

/// Lo que se le pide al agente para una tarea: la tarea, los requisitos que cubre (enteros), las
/// restricciones, el diseño y, si es un reintento, por qué falló.
pub fn task_prompt(
    name: &str,
    spec: &Spec,
    task: &SpecTask,
    plan: Option<&str>,
    failure: Option<&str>,
) -> String {
    let mut p = format!(
        "Implementa la tarea {} de la especificación «{}» ({SPECS_DIR}/{name}/).\n\n## Tarea {}. {}\n",
        task.id, spec.title, task.id, task.title
    );
    if !task.detail.is_empty() {
        p.push_str(&format!("{}\n", task.detail));
    }
    if !task.touches.is_empty() {
        p.push_str(&format!(
            "Archivos previstos: {}\n",
            task.touches.join(", ")
        ));
    }
    if let Some(a) = &task.accept {
        p.push_str(&format!(
            "Se da por hecha cuando `{a}` termina bien (se ejecuta al acabar; si falla, sigues).\n"
        ));
    }
    p.push_str("\n## Requisitos que cubre\n");
    for r in task.requirements.iter().filter_map(|r| spec.requirement(r)) {
        p.push_str(&format!("### {}. {}\n{}\n\n", r.id, r.title, r.body));
    }
    if !spec.constraints.is_empty() {
        p.push_str(&format!(
            "## Restricciones (no negociables)\n{}\n\n",
            spec.constraints
                .iter()
                .map(|c| format!("- {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !spec.out_of_scope.is_empty() {
        p.push_str(&format!(
            "## Fuera de alcance\n{}\n\n",
            spec.out_of_scope
                .iter()
                .map(|c| format!("- {c}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if let Some(plan) = plan.map(str::trim).filter(|p| !p.is_empty()) {
        let cut: String = plan.chars().take(MAX_PLAN).collect();
        p.push_str(&format!("## Diseño ({PLAN_FILE})\n{cut}\n\n"));
    }
    if let Some(f) = failure {
        p.push_str(&format!(
            "## El intento anterior no pasó la comprobación\n```\n{}\n```\nArregla la causa; no toques la orden de \
             comprobación ni desactives tests.\n",
            f.trim()
        ));
    }
    p
}

/// Tareas de `new_text` con su `[x]` corregido: hechas solo si ya lo estaban antes (mismo id) y
/// ninguno de sus requisitos ha cambiado. Así replanificar nunca da por hecho trabajo sin hacer.
pub fn merge_done(old: &TaskList, new_text: &str, drift: &Drift) -> String {
    let new = parse_tasks(new_text);
    let mut text = new_text.to_string();
    for t in &new.tasks {
        let touched = t.requirements.iter().any(|r| {
            drift.changed.contains(r) || drift.added.contains(r) || drift.removed.contains(r)
        });
        let was_done = old.get(&t.id).is_some_and(|o| o.done);
        let want = was_done && !touched;
        if t.done != want
            && let Some(s) = set_done(&text, &t.id, want)
        {
            text = s;
        }
    }
    text
}

/// Nota para el prompt del agente cuando el proyecto tiene especificaciones.
pub fn prompt_section(root: &Path) -> Option<String> {
    let specs = list(root);
    if specs.is_empty() {
        return None;
    }
    let items: Vec<String> = specs
        .iter()
        .take(12)
        .map(|s| {
            if s.tasks_total > 0 {
                format!(
                    "{} («{}», {} de {} tareas)",
                    s.name, s.title, s.tasks_done, s.tasks_total
                )
            } else {
                format!("{} («{}»)", s.name, s.title)
            }
        })
        .collect();
    Some(format!(
        "## Especificaciones\nEste proyecto se desarrolla a partir de especificaciones ({SPECS_DIR}/<nombre>/spec.md): \
         son la fuente de verdad. Antes de cambiar algo que cubra una, léela; no la contradigas, y si lo que te piden \
         la contradice, dilo y propón cambiar primero spec.md (después: `pcode spec planificar <nombre>`). \
         Especificaciones: {}.",
        items.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::parse_spec;

    const SPEC: &str = "# Facturas\n## Requisitos\n### R1. Exportar\nCUANDO exporto EL SISTEMA DEBE escribir CSV.\n- csv\n### R2. Vacío\n- cabecera\n## Restricciones\n- sin dependencias\n## Fuera de alcance\n- Excel\n";

    #[test]
    fn salida_del_planificador() {
        let r = "Aquí van:\n<<<plan.md\n# Plan\n## Enfoque\nx\n>>>\n\n<<<tareas.md\n```markdown\n# Tareas\n- [ ] T1. a\n```\n>>>\n";
        let (p, t) = parse_planner_output(r).unwrap();
        assert_eq!(p, "# Plan\n## Enfoque\nx\n");
        assert_eq!(t, "# Tareas\n- [ ] T1. a\n", "sin la valla de código");
        assert!(
            parse_planner_output("<<<plan.md\nx\n>>>")
                .unwrap_err()
                .contains("faltan los documentos")
        );
        // Sin el cierre final también vale.
        assert!(parse_planner_output("<<<plan.md\nx\n>>>\n<<<tareas.md\n- [ ] T1. a").is_ok());
    }

    #[test]
    fn prompt_de_tarea_con_requisitos_restricciones_y_fallo() {
        let s = parse_spec("facturas", SPEC).unwrap();
        let t = parse_tasks(
            "- [ ] T1. CSV\n  - requisitos: R1\n  - toca: src/csv.rs\n  - acepta: `cargo test csv`\n  - Añade to_csv.\n",
        );
        let p = task_prompt(
            "facturas",
            &s,
            &t.tasks[0],
            Some("# Plan\nusa serde"),
            Some("test csv FALLÓ"),
        );
        assert!(p.starts_with(
            "Implementa la tarea T1 de la especificación «Facturas» (specs/facturas/)."
        ));
        assert!(p.contains("Añade to_csv.") && p.contains("Archivos previstos: src/csv.rs"));
        assert!(p.contains("`cargo test csv`"));
        assert!(
            p.contains("### R1. Exportar\nCUANDO exporto"),
            "el requisito entero"
        );
        assert!(!p.contains("### R2"), "solo los suyos");
        assert!(p.contains("- sin dependencias") && p.contains("- Excel"));
        assert!(p.contains("usa serde") && p.contains("test csv FALLÓ"));
    }

    #[test]
    fn replanificar_conserva_solo_lo_hecho_que_no_cambio() {
        let old = parse_tasks("- [x] T1. a\n  - requisitos: R1\n- [x] T2. b\n  - requisitos: R2\n");
        let d = Drift {
            changed: vec!["R2".into()],
            added: vec!["R3".into()],
            removed: vec![],
        };
        // El modelo marca mal: T2 cambió (no puede seguir hecha) y T3 es nueva.
        let nuevo = "- [ ] T1. a\n  - requisitos: R1\n- [x] T2. b\n  - requisitos: R2\n- [x] T3. c\n  - requisitos: R3\n";
        let m = parse_tasks(&merge_done(&old, nuevo, &d));
        let hechas: Vec<bool> = m.tasks.iter().map(|t| t.done).collect();
        assert_eq!(hechas, [true, false, false]);
        let p = planner_prompt(
            "f",
            SPEC,
            Some(Replan {
                plan: "# P",
                tasks: "# T",
                drift: &d,
            }),
            None,
        );
        assert!(p.contains("requisitos cambiados: R2; nuevos: R3; eliminados: ninguno"));
        let p = planner_prompt(
            "f",
            SPEC,
            None,
            Some(("respuesta", &["R2 no tiene ninguna tarea".to_string()])),
        );
        assert!(p.contains("- R2 no tiene ninguna tarea") && p.contains("<<<plan.md"));
    }

    #[test]
    fn nota_para_el_agente() {
        let d = tempfile::tempdir().unwrap();
        assert!(prompt_section(d.path()).is_none());
        crate::status::create(d.path(), "facturas", SPEC).unwrap();
        let n = prompt_section(d.path()).unwrap();
        assert!(
            n.contains("fuente de verdad") && n.contains("facturas («Facturas»)"),
            "{n}"
        );
        assert!(writer_prompt("x", "exportar facturas").contains("<!-- Duda:"));
    }
}
