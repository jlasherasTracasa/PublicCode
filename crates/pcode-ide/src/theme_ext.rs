//! Temas de color de extensiones de VS Code: de su JSON (`colors`) a la paleta del IDE.
use crate::theme::Palette;
use eframe::egui::Color32;

/// Color de `#rgb`, `#rrggbb` o `#rrggbbaa`.
pub fn parse_color(s: &str) -> Option<Color32> {
    let hex = s.strip_prefix('#')?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // Cada componente de dos cifras (en `#rgb` se duplica cada cifra: 0xf -> 0xff).
    let par = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    let digito = |i: usize| {
        u8::from_str_radix(hex.get(i..i + 1)?, 16)
            .ok()
            .map(|d| d * 17)
    };
    match hex.len() {
        3 => Some(Color32::from_rgba_unmultiplied(
            digito(0)?,
            digito(1)?,
            digito(2)?,
            255,
        )),
        6 => Some(Color32::from_rgba_unmultiplied(
            par(0)?,
            par(2)?,
            par(4)?,
            255,
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            par(0)?,
            par(2)?,
            par(4)?,
            par(6)?,
        )),
        _ => None,
    }
}

/// Si un `uiTheme` de VS Code es oscuro.
pub fn is_dark(ui_theme: &str) -> bool {
    !matches!(ui_theme, "vs" | "hc-light")
}

/// Paleta a partir del JSON de un tema; lo que el tema no define se toma de `base`.
pub fn palette_from_theme(json: &str, base: &Palette) -> Option<Palette> {
    let v: serde_json::Value = serde_json::from_str(&pcode_extensions::strip_jsonc(json)).ok()?;
    let colors = v.get("colors")?.as_object()?;
    // Primer color válido de la lista de claves; si ninguno, el de `base`.
    let pick = |claves: &[&str], por_defecto: Color32| {
        claves
            .iter()
            .find_map(|k| colors.get(*k)?.as_str().and_then(parse_color))
            .unwrap_or(por_defecto)
    };
    Some(Palette {
        editor: pick(&["editor.background"], base.editor),
        side: pick(&["sideBar.background"], base.side),
        input: pick(&["input.background"], base.input),
        border: pick(&["panel.border", "sideBar.border"], base.border),
        fg: pick(&["editor.foreground", "foreground"], base.fg),
        fg_dim: pick(&["descriptionForeground"], base.fg_dim),
        accent: pick(&["focusBorder", "button.background"], base.accent),
        on_accent: pick(&["button.foreground"], base.on_accent),
        hover: pick(&["list.hoverBackground"], base.hover),
        active: pick(&["list.activeSelectionBackground"], base.active),
        selection: pick(&["editor.selectionBackground"], base.selection),
        line_number: pick(&["editorLineNumber.foreground"], base.line_number),
        error: pick(&["editorError.foreground"], base.error),
        warning: pick(&["editorWarning.foreground"], base.warning),
        git_modified: pick(
            &["gitDecoration.modifiedResourceForeground"],
            base.git_modified,
        ),
        git_added: pick(
            &["gitDecoration.untrackedResourceForeground"],
            base.git_added,
        ),
        git_deleted: pick(
            &["gitDecoration.deletedResourceForeground"],
            base.git_deleted,
        ),
        ..*base
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DARK;

    #[test]
    fn colores_hex() {
        assert_eq!(
            parse_color("#fff"),
            Some(Color32::from_rgba_unmultiplied(255, 255, 255, 255))
        );
        assert_eq!(
            parse_color("#1e1e2e"),
            Some(Color32::from_rgba_unmultiplied(0x1e, 0x1e, 0x2e, 255))
        );
        assert_eq!(
            parse_color("#1e1e2e80"),
            Some(Color32::from_rgba_unmultiplied(0x1e, 0x1e, 0x2e, 0x80))
        );
        assert_eq!(parse_color("#ABCDEF"), parse_color("#abcdef"));
        assert_eq!(parse_color("rojo"), None);
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("#gggggg"), None);
    }

    #[test]
    fn oscuro_segun_ui_theme() {
        assert!(!is_dark("vs"));
        assert!(!is_dark("hc-light"));
        assert!(is_dark("vs-dark"));
        assert!(is_dark("hc-black"));
        assert!(is_dark(""));
    }

    #[test]
    fn tema_con_comentarios() {
        let json = r##"{ // comentario
  "colors": { "editor.background": "#101010", "foreground": "#eeeeee" } }"##;
        let p = palette_from_theme(json, &DARK).expect("paleta");
        assert_eq!(p.editor, Color32::from_rgb(0x10, 0x10, 0x10));
        assert_eq!(p.fg, Color32::from_rgb(0xee, 0xee, 0xee));
        assert_eq!(p.side, DARK.side);
    }

    #[test]
    fn primera_clave_valida_gana() {
        let json = r##"{"colors": {
            "editor.foreground": "#010203",
            "foreground": "#040506"
        }}"##;
        let p = palette_from_theme(json, &DARK).expect("paleta");
        assert_eq!(p.fg, Color32::from_rgb(1, 2, 3));
    }

    #[test]
    fn sin_colors_no_hay_paleta() {
        assert!(palette_from_theme("{}", &DARK).is_none());
        assert!(palette_from_theme("esto no es json", &DARK).is_none());
    }
}
