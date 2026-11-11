//! Conversión entre índices de carácter del editor y posiciones LSP (línea, unidades UTF-16).

/// (línea, carácter UTF-16), desde 0, del carácter número `char_index`.
pub fn to_lsp(text: &str, char_index: usize) -> (u32, u32) {
    let (mut line, mut col) = (0u32, 0u32);
    for c in text.chars().take(char_index) {
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += c.len_utf16() as u32;
        }
    }
    (line, col)
}

/// Índice de carácter de una posición LSP; satura al final de la línea o del texto.
pub fn from_lsp(text: &str, line: u32, character: u32) -> usize {
    let mut idx = 0;
    let mut cur = 0u32;
    let mut lines = text.chars().peekable();
    while cur < line {
        match lines.next() {
            Some('\n') => cur += 1,
            Some(_) => {}
            None => return idx,
        }
        idx += 1;
    }
    let mut units = 0u32;
    for c in lines {
        if c == '\n' || units >= character {
            break;
        }
        units += c.len_utf16() as u32;
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_ida_y_vuelta() {
        let t = "ab\ncde\nf";
        assert_eq!(to_lsp(t, 4), (1, 1));
        assert_eq!(from_lsp(t, 1, 1), 4);
    }

    #[test]
    fn emoji_cuenta_dos_unidades() {
        let t = "añ😀b\nx";
        assert_eq!(to_lsp(t, 3), (0, 4));
        assert_eq!(from_lsp(t, 0, 4), 3);
    }

    #[test]
    fn satura_al_final() {
        let t = "ab\ncd";
        assert_eq!(from_lsp(t, 0, 99), 2);
        assert_eq!(from_lsp(t, 9, 0), 5);
    }
}
