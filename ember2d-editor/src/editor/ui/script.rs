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
            let cursor_x = line_x + (cursor.0).min(line.len());
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
            renderer.draw_str(col, y, &line[i..], Color::Grey, bg);
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
            let word = &line[start..i];
            let color = if keywords.contains(&word) { Color::Cyan } else { Color::White };
            renderer.draw_str(col, y, word, color, bg);
            col += word.len();
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
