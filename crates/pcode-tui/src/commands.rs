//! Catálogo de órdenes `/` de la TUI: una tabla única con nombre, alias y ayuda.
//!
//! Esta tabla sirve para listar las órdenes (paleta, ayuda). No ejecuta nada: la ejecución sigue
//! en `App::submit`.
use crate::i18n::Lang;

/// Una orden `/` de la TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// Nombre en español (sin la barra).
    pub name: &'static str,
    /// Alias en inglés.
    pub alias: &'static str,
    /// Qué hace, en español.
    pub es: &'static str,
    /// Qué hace, en inglés.
    pub en: &'static str,
    /// La ejecuta quien arranca la TUI (no `App`): se usa en tareas posteriores.
    pub external: bool,
}

/// Todas las órdenes, en el orden en que se listan.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "ayuda",
        alias: "help",
        es: "muestra la ayuda de las órdenes",
        en: "show the command help",
        external: false,
    },
    CommandSpec {
        name: "nuevo",
        alias: "new",
        es: "olvida el historial y empieza otra conversación",
        en: "forget the history and start again",
        external: false,
    },
    CommandSpec {
        name: "modelo",
        alias: "model",
        es: "muestra o cambia el modelo activo",
        en: "show or change the active model",
        external: false,
    },
    CommandSpec {
        name: "uso",
        alias: "usage",
        es: "muestra el consumo de tokens y contexto",
        en: "show token and context usage",
        external: false,
    },
    CommandSpec {
        name: "compactar",
        alias: "compact",
        es: "resume el historial para ahorrar contexto",
        en: "summarize history to save context",
        external: false,
    },
    CommandSpec {
        name: "deshacer",
        alias: "undo",
        es: "deshace el último cambio del agente",
        en: "undo the agent's last change",
        external: false,
    },
    CommandSpec {
        name: "rehacer",
        alias: "redo",
        es: "rehace el último cambio deshecho",
        en: "redo the last undone change",
        external: false,
    },
    CommandSpec {
        name: "plan",
        alias: "plan",
        es: "pasa al modo plan (solo lectura)",
        en: "switch to plan mode (read-only)",
        external: false,
    },
    CommandSpec {
        name: "construir",
        alias: "build",
        es: "vuelve al modo normal de edición",
        en: "back to normal editing mode",
        external: false,
    },
    CommandSpec {
        name: "recuerda",
        alias: "remember",
        es: "guarda un dato en la memoria",
        en: "save a fact to memory",
        external: false,
    },
    CommandSpec {
        name: "memoria",
        alias: "memory",
        es: "muestra o gestiona la memoria",
        en: "show or manage memory",
        external: false,
    },
    CommandSpec {
        name: "olvida",
        alias: "forget",
        es: "borra un dato de la memoria",
        en: "delete a fact from memory",
        external: false,
    },
    CommandSpec {
        name: "salir",
        alias: "quit",
        es: "sale de la TUI",
        en: "exit the TUI",
        external: false,
    },
];

/// Orden por nombre o alias exactos.
pub fn find(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.alias == name)
}

/// Nombre que se muestra en `lang`.
pub fn display_name(spec: &CommandSpec, lang: Lang) -> &'static str {
    match lang {
        Lang::Es => spec.name,
        Lang::En => spec.alias,
    }
}

/// Ayuda en `lang`.
pub fn help(spec: &CommandSpec, lang: Lang) -> &'static str {
    match lang {
        Lang::Es => spec.es,
        Lang::En => spec.en,
    }
}

/// Órdenes cuyo nombre o alias empieza por `prefix` (sin barra).
///
/// `prefix` se pasa a minúsculas. Con `prefix` vacío devuelve todas, en el orden de [`COMMANDS`].
/// `lang` se acepta por uniformidad con el resto de funciones de la tabla.
pub fn matching(prefix: &str, lang: Lang) -> Vec<&'static CommandSpec> {
    let _ = lang;
    let prefix = prefix.to_lowercase();
    COMMANDS
        .iter()
        .filter(|spec| spec.name.starts_with(&prefix) || spec.alias.starts_with(&prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogo_incluye_las_ordenes_basicas_sin_repetir() {
        let nombres = [
            "ayuda",
            "nuevo",
            "modelo",
            "uso",
            "compactar",
            "deshacer",
            "rehacer",
            "plan",
            "construir",
            "recuerda",
            "memoria",
            "olvida",
            "salir",
        ];
        for nombre in nombres {
            assert!(find(nombre).is_some(), "falta la orden {nombre}");
        }
        for (i, a) in COMMANDS.iter().enumerate() {
            for b in &COMMANDS[i + 1..] {
                assert_ne!(a.name, b.name, "nombre repetido");
                assert_ne!(a.alias, b.alias, "alias repetido");
                assert_ne!(a.name, b.alias, "nombre y alias repetidos");
                assert_ne!(a.alias, b.name, "alias y nombre repetidos");
            }
        }
    }

    #[test]
    fn find_por_nombre_y_por_alias() {
        let por_nombre = find("salir").expect("salir existe");
        let por_alias = find("quit").expect("quit existe");
        assert_eq!(por_nombre, por_alias);
        assert!(find("nada").is_none());
    }

    #[test]
    fn matching_por_prefijo_y_alias() {
        let nombres = |v: Vec<&CommandSpec>| v.iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(nombres(matching("desh", Lang::Es)), vec!["deshacer"]);
        assert_eq!(nombres(matching("reh", Lang::Es)), vec!["rehacer"]);
        assert_eq!(nombres(matching("hel", Lang::Es)), vec!["ayuda"]);
        let todas = matching("", Lang::Es);
        assert_eq!(todas.len(), COMMANDS.len());
        assert_eq!(todas[0].name, "ayuda");
    }

    #[test]
    fn ayuda_no_vacia_en_los_dos_idiomas() {
        for spec in COMMANDS {
            assert!(!spec.es.is_empty(), "{} sin ayuda en español", spec.name);
            assert!(!spec.en.is_empty(), "{} sin ayuda en inglés", spec.name);
            assert!(
                spec.es.chars().count() <= 50,
                "{} demasiado largo (es)",
                spec.name
            );
            assert!(
                spec.en.chars().count() <= 50,
                "{} demasiado largo (en)",
                spec.name
            );
        }
    }
}
