//! Capacidades y política por capas (atenuación).
use crate::shell::simple_commands;
use globset::{Glob, GlobSet, GlobSetBuilder};
use pcode_tools::Effect;

/// Resultado de una comprobación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Permitido.
    Allow,
    /// Requiere aprobación humana.
    Ask(String),
    /// Denegado.
    Deny(String),
}

/// Capacidades de una capa. Una lista de permitidos vacía significa «no restringe».
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// Rutas legibles.
    #[serde(rename = "leer")]
    pub read: Vec<String>,
    /// Rutas escribibles.
    #[serde(rename = "escribir")]
    pub write: Vec<String>,
    /// Rutas no escribibles.
    #[serde(rename = "no_escribir")]
    pub deny_write: Vec<String>,
    /// Órdenes denegadas.
    #[serde(rename = "denegar_ordenes")]
    pub deny_commands: Vec<String>,
    /// Órdenes que piden confirmación humana antes de ejecutarse.
    #[serde(rename = "preguntar_ordenes")]
    pub ask_commands: Vec<String>,
    /// Órdenes permitidas.
    #[serde(rename = "permitir_ordenes")]
    pub allow_commands: Vec<String>,
    /// Dominios permitidos (red cerrada por defecto).
    #[serde(rename = "red_permitir")]
    pub net_allow: Vec<String>,
    /// Preguntar por efectos irreversibles.
    #[serde(rename = "preguntar_irreversibles")]
    pub ask_irreversible: bool,
    /// Tools que no se anuncian.
    #[serde(rename = "tools_ocultas")]
    pub hidden_tools: Vec<String>,
    /// Efectos denegados.
    #[serde(rename = "efectos_denegados")]
    pub deny_effects: Vec<Effect>,
}

fn set(pats: &[String]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in pats {
        if let Ok(g) = Glob::new(p) {
            b.add(g);
        }
    }
    b.build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap())
}

fn norm(p: &str) -> String {
    p.trim_start_matches("./").to_string()
}

impl Capabilities {
    /// Lectura.
    pub fn check_read(&self, rel: &str) -> Decision {
        if self.read.is_empty() || set(&self.read).is_match(norm(rel)) {
            Decision::Allow
        } else {
            Decision::Deny(format!("lectura no permitida: {rel}"))
        }
    }
    /// Escritura.
    pub fn check_write(&self, rel: &str) -> Decision {
        let r = norm(rel);
        if set(&self.deny_write).is_match(&r) {
            return Decision::Deny(format!("escritura prohibida en {rel}"));
        }
        if !self.write.is_empty() && !set(&self.write).is_match(&r) {
            return Decision::Deny(format!("escritura no permitida en {rel}"));
        }
        Decision::Allow
    }
    /// Orden de shell (cada orden simple se comprueba).
    pub fn check_command(&self, command: &str) -> Decision {
        let cmds = match simple_commands(command) {
            Ok(c) => c,
            Err(e) => return Decision::Deny(e),
        };
        let deny = set(&self.deny_commands);
        let allow = set(&self.allow_commands);
        let ask = set(&self.ask_commands);
        let mut ask_reason = None;
        for c in cmds {
            let c = c.split_whitespace().collect::<Vec<_>>().join(" ");
            if deny.is_match(&c) {
                return Decision::Deny(format!("orden denegada: {c}"));
            }
            if !self.allow_commands.is_empty() && !allow.is_match(&c) {
                return Decision::Deny(format!("orden no permitida: {c}"));
            }
            if ask_reason.is_none() && ask.is_match(&c) {
                ask_reason = Some(format!("orden con efectos difíciles de deshacer: {c}"));
            }
        }
        ask_reason.map_or(Decision::Allow, Decision::Ask)
    }

    /// Capa segura para el uso interactivo (`pcode`, `pcode chat`): deniega lo catastrófico y
    /// pide confirmación para lo destructivo o lo que sale de la máquina. La configuración del
    /// usuario se añade encima como otra capa y nunca la amplía.
    pub fn interactive_defaults() -> Capabilities {
        let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect();
        Capabilities {
            deny_commands: v(&[
                "rm -rf /",
                "rm -rf /*",
                "rm -rf ~",
                "rm -rf ~/",
                "rm -fr /",
                "mkfs*",
                "dd * of=/dev/sd*",
                "dd * of=/dev/nvme*",
                "format [a-zA-Z]:*",
                "shutdown*",
                "reboot",
            ]),
            ask_commands: v(&[
                "git push*",
                "git reset --hard*",
                "git clean*",
                "git checkout -- *",
                "git restore *",
                "git rebase*",
                "git stash drop*",
                "git branch -D*",
                "rm -r*",
                "rm -f*",
                "rmdir /s*",
                "rd /s*",
                "del /s*",
                "Remove-Item*-Recurse*",
                "npm publish*",
                "cargo publish*",
                "docker system prune*",
                "kubectl delete*",
                "terraform apply*",
                "terraform destroy*",
            ]),
            // El agente no toca git por dentro (hooks = ejecución de código) ni su propia auditoría.
            deny_write: v(&[".git/**", "**/.git/**", ".pcode/**"]),
            ask_irreversible: true,
            ..Default::default()
        }
    }
    /// Dominio de red.
    pub fn check_domain(&self, host: &str) -> Decision {
        if set(&self.net_allow).is_match(host) {
            Decision::Allow
        } else {
            Decision::Deny(format!("dominio no permitido: {host}"))
        }
    }
    /// Efectos de una tool.
    pub fn check_effects(&self, effects: &[Effect]) -> Decision {
        if let Some(e) = effects.iter().find(|e| self.deny_effects.contains(e)) {
            return Decision::Deny(format!("efecto denegado: {e:?}"));
        }
        if self.ask_irreversible && effects.contains(&Effect::Irreversible) {
            return Decision::Ask("acción irreversible".into());
        }
        Decision::Allow
    }
}

/// Pila de capas: la decisión es la más restrictiva.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    layers: Vec<Capabilities>,
}

fn rank(d: &Decision) -> u8 {
    match d {
        Decision::Allow => 0,
        Decision::Ask(_) => 1,
        Decision::Deny(_) => 2,
    }
}

impl Policy {
    /// Política con una capa base.
    pub fn new(base: Capabilities) -> Self {
        Policy { layers: vec![base] }
    }
    /// Política hija: una capa más (nunca amplía).
    pub fn child(&self, caps: Capabilities) -> Policy {
        let mut l = self.layers.clone();
        l.push(caps);
        Policy { layers: l }
    }
    fn worst(&self, f: impl Fn(&Capabilities) -> Decision) -> Decision {
        self.layers
            .iter()
            .map(f)
            .max_by_key(rank)
            .unwrap_or(Decision::Allow)
    }
    /// Lectura.
    pub fn check_read(&self, p: &str) -> Decision {
        self.worst(|c| c.check_read(p))
    }
    /// Escritura.
    pub fn check_write(&self, p: &str) -> Decision {
        self.worst(|c| c.check_write(p))
    }
    /// Orden de shell.
    pub fn check_command(&self, cmd: &str) -> Decision {
        self.worst(|c| c.check_command(cmd))
    }
    /// Dominio.
    pub fn check_domain(&self, h: &str) -> Decision {
        self.worst(|c| c.check_domain(h))
    }
    /// Efectos.
    pub fn check_effects(&self, e: &[Effect]) -> Decision {
        self.worst(|c| c.check_effects(e))
    }
    /// Si la tool se anuncia al modelo.
    pub fn tool_visible(&self, name: &str, effects: &[Effect]) -> bool {
        !self.layers.iter().any(|c| {
            c.hidden_tools.iter().any(|h| h == name)
                || effects.iter().any(|e| c.deny_effects.contains(e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caps(f: impl FnOnce(&mut Capabilities)) -> Capabilities {
        let mut c = Capabilities::default();
        f(&mut c);
        c
    }
    #[test]
    fn interactivo_pregunta_lo_destructivo_y_deniega_lo_catastrofico() {
        let p = Policy::new(Capabilities::interactive_defaults());
        assert!(matches!(
            p.check_command("cargo test && git status"),
            Decision::Allow
        ));
        assert!(matches!(
            p.check_command("git push origin main"),
            Decision::Ask(_)
        ));
        assert!(matches!(
            p.check_command("cd x && rm -rf build"),
            Decision::Ask(_)
        ));
        assert!(matches!(
            p.check_command("sudo rm -rf /"),
            Decision::Deny(_)
        ));
        assert!(matches!(
            p.check_command("bash -c 'mkfs.ext4 /dev/sda1'"),
            Decision::Deny(_)
        ));
        assert!(matches!(
            p.check_command("cmd /C \"rmdir /s /q build\""),
            Decision::Ask(_)
        ));
        // La capa del usuario puede endurecer, nunca ablandar.
        let user = Capabilities {
            deny_commands: vec!["git push*".into()],
            ..Default::default()
        };
        assert!(matches!(
            p.child(user).check_command("git push"),
            Decision::Deny(_)
        ));
    }
    #[test]
    fn write_permitida_y_denegada() {
        let c = caps(|c| {
            c.write = vec!["**".into()];
            c.deny_write = vec!["docs/**".into()];
        });
        assert_eq!(c.check_write("src/a.rs"), Decision::Allow);
        assert!(matches!(c.check_write("docs/x.md"), Decision::Deny(_)));
    }
    #[test]
    fn capa_vacia_no_restringe_escritura() {
        let p = Policy::new(Capabilities::default()).child(Capabilities::default());
        assert_eq!(p.check_write("src/a.rs"), Decision::Allow);
    }
    #[test]
    fn deny_commands_git_stash() {
        let c = caps(|c| c.deny_commands = vec!["git stash*".into()]);
        assert!(matches!(
            c.check_command("bash -c 'git stash'"),
            Decision::Deny(_)
        ));
        assert!(matches!(
            c.check_command("true && git stash pop"),
            Decision::Deny(_)
        ));
        assert_eq!(c.check_command("git status"), Decision::Allow);
    }
    #[test]
    fn allow_commands_restringe() {
        let c = caps(|c| c.allow_commands = vec!["cargo *".into()]);
        assert_eq!(c.check_command("cargo test"), Decision::Allow);
        assert!(matches!(c.check_command("curl x"), Decision::Deny(_)));
    }
    #[test]
    fn dominio_comodin() {
        let c = caps(|c| c.net_allow = vec!["*.crates.io".into()]);
        assert_eq!(c.check_domain("static.crates.io"), Decision::Allow);
        assert!(matches!(c.check_domain("evil.com"), Decision::Deny(_)));
        assert!(matches!(
            Capabilities::default().check_domain("a.com"),
            Decision::Deny(_)
        ));
    }
    #[test]
    fn irreversible_pregunta() {
        let c = caps(|c| c.ask_irreversible = true);
        assert!(matches!(
            c.check_effects(&[Effect::Irreversible]),
            Decision::Ask(_)
        ));
    }
    #[test]
    fn politica_hijo_no_amplia() {
        let padre = Policy::new(caps(|c| c.deny_effects = vec![Effect::Network]));
        let hijo = padre.child(caps(|c| c.net_allow = vec!["*".into()]));
        assert!(matches!(
            hijo.check_effects(&[Effect::Network]),
            Decision::Deny(_)
        ));
    }
    #[test]
    fn deny_gana_a_ask() {
        let p = Policy::new(caps(|c| c.ask_irreversible = true))
            .child(caps(|c| c.deny_effects = vec![Effect::Irreversible]));
        assert!(matches!(
            p.check_effects(&[Effect::Irreversible]),
            Decision::Deny(_)
        ));
    }
    #[test]
    fn tool_oculta_no_visible() {
        let p = Policy::new(caps(|c| c.hidden_tools = vec!["bash".into()]));
        assert!(!p.tool_visible("bash", &[Effect::Exec]));
        assert!(p.tool_visible("leer", &[Effect::Read]));
    }
    #[test]
    fn deserializa_desde_toml() {
        let c: Capabilities =
            toml::from_str("denegar_ordenes = [\"git push*\"]\nescribir = [\"**\"]").unwrap();
        assert_eq!(c.deny_commands.len(), 1);
    }
}
