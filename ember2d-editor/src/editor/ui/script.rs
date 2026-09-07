// editor/ui/script.rs — Script editor rendering and Rhai syntax highlighting.

use ember2d::renderer::{color::Color, Renderer};

pub fn draw_script_editor(renderer: &mut Renderer, path: Option<&str>, buffer: &[String], cursor: (usize, usize), scroll: usize, unsaved: bool, cx: usize, cy: usize, cw: usize, ch: usize) {
    let bg_col = Color::Black;
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, bg_col);

    let title = match path {
        Some(p) => format!(" EDIT: {}{}", p, if unsaved { "*" } else { "" }),
        None    => " (no script open) ".to_string(),
    };
    let header = format!(" {:<width$}", title, width = cw.saturating_sub(1));
    renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);

    let text_start = cy + 1;
    let max_visible = ch.saturating_sub(1);

    if buffer.is_empty() && path.is_none() {
        renderer.draw_str(cx + 1, text_start, "Select a .rhai file from the Files panel to edit.", Color::Grey, bg_col);
        return;
    }

    let gutter_w = 4;
    for (i, line) in buffer.iter().enumerate().skip(scroll).take(max_visible) {
        let row = text_start + (i - scroll);
        if row >= cy + ch { break; }

        let num_str = format!("{:3} ", i + 1);
        renderer.draw_str(cx, row, &num_str, Color::DarkGrey, Color::Black);

        let line_x = cx + gutter_w;
        let max_line_w = cw.saturating_sub(gutter_w);
        draw_highlighted_rhai(renderer, line_x, row, line, max_line_w, bg_col);

        if i == cursor.1 {
            // R11 (7A-2, docs/ember2d-master-plan.md): `cursor.0` is a
            // character index (script_editor.rs's own `char_byte_offset`
            // doc comment) — clamping it against `line.len()` (bytes) let
            // it land past the line's actual character count on any
            // multi-byte line, which `chars().nth(cursor.0)` below would
            // then draw as a space instead of the real character at the
            // cursor.
            let cursor_x = line_x + (cursor.0).min(line.chars().count());
            if cursor_x < cx + cw {
                let char_at_cursor = line.chars().nth(cursor.0).unwrap_or(' ');
                renderer.draw_char(cursor_x, row, char_at_cursor, Color::Black, Color::Cyan);
            }
        }
    }
}

fn draw_highlighted_rhai(renderer: &mut Renderer, x: usize, y: usize, line: &str, max_w: usize, bg: Color) {
    let keywords = [
        "let", "const", "fn", "if", "else", "while", "loop", "for", "in",
        "return", "break", "continue", "true", "false", "import", "as", "export"
    ];

    let mut col = x;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();

    while i < chars.len() && (col - x) < max_w {
        let ch = chars[i];

        // Comments
        if ch == '/' && i + 1 < chars.len() && chars[i+1] == '/' {
            // R11 (7A-2, docs/ember2d-master-plan.md): `i` is an index into
            // `chars` (a `Vec<char>`), not a byte offset — slicing the
            // original `line: &str` with it (`&line[i..]`) panicked the
            // moment any multi-byte character appeared before the `//`.
            // Building the rest-of-line `String` from `chars` instead is
            // always char-safe, at the (negligible, once-per-comment)
            // allocation cost.
            let rest: String = chars[i..].iter().collect();
            renderer.draw_str(col, y, &rest, Color::Grey, bg);
            return;
        }

        // Strings
        if ch == '"' {
            renderer.draw_char(col, y, ch, Color::Yellow, bg);
            col += 1; i += 1;
            while i < chars.len() && (col - x) < max_w {
                let s_ch = chars[i];
                renderer.draw_char(col, y, s_ch, Color::Yellow, bg);
                col += 1; i += 1;
                if s_ch == '"' { break; }
            }
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() {
            renderer.draw_char(col, y, ch, Color::Magenta, bg);
            col += 1; i += 1;
            continue;
        }

        // Identifiers / Keywords
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            // R11: same char-index-into-byte-slice bug as the comment case
            // above — `start`/`i` index `chars`, not `line`'s bytes. Safe
            // here in practice too (identifiers are ASCII-only by the
            // `is_ascii_alphanumeric`/`is_ascii_alphabetic` checks above,
            // so `line[start..i]` would happen to land on char boundaries
            // whenever it didn't panic outright on an earlier multi-byte
            // character elsewhere in the line) — built from `chars` anyway
            // for the same reason as the comment case: don't rely on a
            // downstream character class to keep an upstream slice safe.
            let word: String = chars[start..i].iter().collect();
            let color = if keywords.contains(&word.as_str()) { Color::Cyan } else { Color::White };
            renderer.draw_str(col, y, &word, color, bg);
            col += word.chars().count();
            continue;
        }

        // Operators / Punctuation
        let color = match ch {
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '^' => Color::Yellow,
            '(' | ')' | '[' | ']' | '{' | '}' => Color::Magenta,
            ',' | ';' | ':' | '.' => Color::Grey,
            _ => Color::White,
        };
        renderer.draw_char(col, y, ch, color, bg);
        col += 1; i += 1;
    }
}
