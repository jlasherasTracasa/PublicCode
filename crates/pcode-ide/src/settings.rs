//! Ajustes del IDE en `ide.toml`, junto a la configuración de usuario.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Ajustes persistentes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdeSettings {
    /// Tema oscuro.
    #[serde(rename = "oscuro")]
    pub dark: bool,
    /// Tamaño de letra (8 a 32).
    #[serde(rename = "letra")]
    pub font_size: f32,
    /// Panel del agente visible.
    #[serde(rename = "agente")]
    pub show_agent: bool,
    /// Ancho del explorador.
    #[serde(rename = "ancho_explorador")]
    pub explorer_width: f32,
    /// Tema de una extensión: `<id>/<ruta>`; vacío = el del IDE.
    #[serde(rename = "tema_extension")]
    pub ext_theme: String,
}

impl Default for IdeSettings {
    fn default() -> Self {
        IdeSettings {
            dark: true,
            font_size: 14.0,
            show_agent: true,
            explorer_width: 240.0,
            ext_theme: String::new(),
        }
    }
}

impl IdeSettings {
    /// Lee ajustes; con un TOML roto devuelve los de por defecto.
    pub fn parse(s: &str) -> IdeSettings {
        let mut v: IdeSettings = toml::from_str(s).unwrap_or_default();
        v.font_size = if v.font_size.is_finite() {
            v.font_size.clamp(8.0, 32.0)
        } else {
            14.0
        };
        v
    }

    /// TOML de los ajustes.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// Dónde se guardan.
    pub fn path() -> Option<PathBuf> {
        pcode_config::user_config_path().and_then(|p| p.parent().map(|d| d.join("ide.toml")))
    }

    /// Guarda con escritura atómica (`.tmp` y `rename`).
    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, self.to_toml()).is_ok() {
            let _ = std::fs::rename(tmp, p);
        }
    }

    /// Lee del disco (o los de por defecto).
    pub fn load() -> IdeSettings {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valores_por_defecto() {
        let d = IdeSettings::default();
        assert!(d.dark && d.show_agent);
        assert_eq!(d.font_size, 14.0);
    }

    #[test]
    fn toml_roto_no_falla() {
        assert_eq!(
            IdeSettings::parse("esto [no es toml"),
            IdeSettings::default()
        );
    }

    #[test]
    fn letra_acotada() {
        assert_eq!(IdeSettings::parse("letra = 500.0").font_size, 32.0);
        assert_eq!(IdeSettings::parse("letra = 1.0").font_size, 8.0);
    }

    #[test]
    fn ida_y_vuelta() {
        let s = IdeSettings {
            dark: false,
            font_size: 18.0,
            show_agent: false,
            explorer_width: 300.0,
            ext_theme: String::new(),
        };
        assert_eq!(IdeSettings::parse(&s.to_toml()), s);
    }

    #[test]
    fn tema_de_extension_se_guarda_y_se_lee() {
        let s = IdeSettings {
            ext_theme: "ana.temas/themes/noche.json".to_string(),
            ..IdeSettings::default()
        };
        assert_eq!(IdeSettings::parse(&s.to_toml()), s);
        assert_eq!(IdeSettings::parse("oscuro = true").ext_theme, "");
    }
}
