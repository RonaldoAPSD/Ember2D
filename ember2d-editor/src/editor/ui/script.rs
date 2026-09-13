// editor/ui/script.rs — Script editor rendering and Rhai syntax highlighting.

use ember2d::renderer::{color::Color, DrawSurface};

/// A script buffer position, `(char_idx, line_idx)` — same shape as
/// `EditorState::script_cursor` (7C-8, master plan §5.3).
pub type ScriptPos = (usize, usize);

#[allow(clippy::too_many_arguments)]
pub fn draw_script_editor(
    renderer: &mut dyn DrawSurface,
    path: Option<&str>,
    buffer: &[String],
    cursor: ScriptPos,
    scroll: usize,
    // 7C-8 (master plan §5.3): columns scrolled off the left — the
    // horizontal twin of `scroll` above.
    hscroll: usize,
    unsaved: bool,
    cx: usize,
    cy: usize,
    cw: usize,
    ch: usize,
    // 7C-7 (master plan §5.3, R18): `(0-based line, message)` from the
    // last `check_script_syntax` — shown both as a highlighted background
    // on the erroring line and a reserved message row at the bottom of
    // this panel, so the docked and fullscreen script editors (the two
    // callers) get identical error display without either needing its own
    // copy of this logic.
    error: Option<(usize, &str)>,
    // 7C-8 (master plan §5.3): the active selection, normalized so
    // `start <= end` in reading order — `EditorState::script_selection`'s
    // own doc comment explains the normalization.
    selection: Option<(ScriptPos, ScriptPos)>,
    // 7C-8 (master plan §5.3): `Some(query)` while the Ctrl+F find bar is
    // open — consumes the row right after the header, the same way
    // `error` consumes one at the bottom.
    find_query: Option<&str>,
) {
    let bg_col = Color::Black;
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, bg_col);

    let title = match path {
        Some(p) => format!(" EDIT: {}{}", p, if unsaved { "*" } else { "" }),
        None => " (no script open) ".to_string(),
    };
    let header = format!(" {:<width$}", title, width = cw.saturating_sub(1));
    renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);

    let find_rows = usize::from(find_query.is_some());
    let text_start = cy + 1 + find_rows;
    if let Some(query) = find_query {
        let text = format!(" Find: {}_", query);
        let text: String = format!("{:<width$}", text, width = cw).chars().take(cw).collect();
        renderer.draw_str(cx, cy + 1, &text, Color::Black, Color::Yellow);
    }
    // Reserve the bottom row for the error message when there is one —
    // matches the header's own "consume one row" shape above.
    let max_visible = ch.saturating_sub(find_rows + if error.is_some() { 2 } else { 1 });

    if buffer.is_empty() && path.is_none() {
        renderer.draw_str(
            cx + 1,
            text_start,
            "Select a .rhai file from the Files panel to edit.",
            Color::Grey,
            bg_col,
        );
        return;
    }

    let comment_starts = block_comment_starts(buffer);
    let err_line = error.map(|(line, _)| line);

    let gutter_w = 4;
    let max_line_w = cw.saturating_sub(gutter_w);
    let line_x = cx + gutter_w;
    for (i, line) in buffer.iter().enumerate().skip(scroll).take(max_visible) {
        let row = text_start + (i - scroll);
        if row >= cy + ch {
            break;
        }

        let line_bg = if err_line == Some(i) {
            renderer.draw_rect_filled(cx, row, cw, 1, ' ', Color::White, Color::DarkRed);
            Color::DarkRed
        } else {
            bg_col
        };

        let num_str = format!("{:3} ", i + 1);
        renderer.draw_str(cx, row, &num_str, Color::DarkGrey, line_bg);

        let line_char_count = line.chars().count();
        // 7C-8 (master plan §5.3): a trailing `…` when the line has more
        // content than fits after `hscroll` — shrinks the highlighter's
        // own visible width by one column to leave room for it, rather
        // than the marker overwriting whatever character was there.
        let clipped = line_char_count > hscroll + max_line_w;
        let highlight_w = if clipped { max_line_w.saturating_sub(1) } else { max_line_w };
        // The horizontally-scrolled-off prefix is dropped BEFORE
        // highlighting, not skipped character-by-character inside it —
        // `draw_highlighted_rhai`'s comment/string parsing state simply
        // restarts at column `hscroll`, the same "good enough, not a real
        // parser" simplification `block_comment_starts` already makes
        // across line boundaries, just applied across the hscroll cut
        // instead. `sel_range` is adjusted to match: it's computed in
        // absolute line-column terms, but `draw_highlighted_rhai` now
        // only ever sees the visible (post-hscroll) substring.
        let visible_line: String = line.chars().skip(hscroll).collect();
        let sel_range = line_selection_range(selection, i)
            .map(|(s, e)| (s.saturating_sub(hscroll), e.saturating_sub(hscroll)));

        draw_highlighted_rhai(
            renderer,
            line_x,
            row,
            &visible_line,
            highlight_w,
            line_bg,
            comment_starts[i],
            sel_range,
        );
        if clipped {
            renderer.draw_char(line_x + highlight_w, row, '\u{2026}', Color::DarkGrey, line_bg);
        }

        if i == cursor.1 && cursor.0 >= hscroll {
            // R11 (7A-2, docs/ember2d-master-plan.md): `cursor.0` is a
            // character index (script_editor.rs's own `char_byte_offset`
            // doc comment) — clamping it against `line.len()` (bytes) let
            // it land past the line's actual character count on any
            // multi-byte line, which `chars().nth(cursor.0)` below would
            // then draw as a space instead of the real character at the
            // cursor.
            let visible_col = (cursor.0 - hscroll).min(line_char_count.saturating_sub(hscroll));
            let cursor_x = line_x + visible_col;
            if cursor_x < cx + cw {
                let char_at_cursor = line.chars().nth(cursor.0).unwrap_or(' ');
                renderer.draw_char(cursor_x, row, char_at_cursor, Color::Black, Color::Cyan);
            }
        }
    }

    if let Some((line, msg)) = error {
        let row = cy + ch - 1;
        let text = format!(" ERROR Line {}: {}", line + 1, msg);
        let text: String = format!("{:<width$}", text, width = cw).chars().take(cw).collect();
        renderer.draw_str(cx, row, &text, Color::White, Color::DarkRed);
    }
}

/// This line's own `(start_char, end_char)` slice of a buffer-wide
/// selection, if any of it falls on line `line_idx` (7C-8, master plan
/// §5.3) — the full line's width when the selection spans past both
/// edges, clamped to just the selected columns on the first/last line.
fn line_selection_range(
    selection: Option<(ScriptPos, ScriptPos)>,
    line_idx: usize,
) -> Option<(usize, usize)> {
    let ((sc, sl), (ec, el)) = selection?;
    if line_idx < sl || line_idx > el {
        return None;
    }
    let start = if line_idx == sl { sc } else { 0 };
    let end = if line_idx == el { ec } else { usize::MAX };
    Some((start, end))
}

/// Which lines of the buffer START already inside a `/* */` block comment
/// (7C-7, master plan §5.3): `draw_highlighted_rhai` only ever sees one
/// line at a time — and, with the buffer scrolled, not even from the top
/// — so this scans the WHOLE buffer once per render to carry that state
/// across lines. Skips string contents (so a `/*` inside one doesn't
/// start a real comment) the same naive way `draw_highlighted_rhai`'s own
/// pre-existing string case already does: no backslash-escape awareness,
/// so a string containing an escaped quote (`"say \"hi\""`) ends the
/// "string" early at that inner `"`. Good enough for a syntax
/// highlighter, not a real parser; `check_script_syntax`'s real
/// `rhai::Engine::compile` is the actual source of truth for whether the
/// script is valid.
fn block_comment_starts(buffer: &[String]) -> Vec<bool> {
    let mut starts = Vec::with_capacity(buffer.len());
    let mut in_comment = false;
    for line in buffer {
        starts.push(in_comment);
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if in_comment {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    in_comment = false;
                    i += 2;
                } else {
                    i += 1;
                }
            } else if chars[i] == '"' {
                // Skip string contents so a `/*` inside one doesn't count.
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                i += 1; // the closing quote, or past the end
            } else if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
                break; // rest of the line is a line comment
            } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                in_comment = true;
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    starts
}

/// The background to draw a token starting at char index `i` with — the
/// selection color if `i` falls inside `sel_range`, `default` otherwise
/// (7C-8, master plan §5.3). Applied per TOKEN (using its first
/// character's index), not per character within multi-char tokens
/// (identifiers, strings, line comments) — a selection boundary that
/// lands mid-token highlights the whole token rather than splitting it,
/// the same kind of approximation `block_comment_starts` already accepts
/// at line boundaries.
fn sel_bg(i: usize, sel_range: Option<(usize, usize)>, default: Color) -> Color {
    match sel_range {
        Some((s, e)) if i >= s && i < e => Color::DarkBlue,
        _ => default,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_highlighted_rhai(
    renderer: &mut dyn DrawSurface,
    x: usize,
    y: usize,
    line: &str,
    max_w: usize,
    bg: Color,
    starts_in_block_comment: bool,
    sel_range: Option<(usize, usize)>,
) {
    let keywords = [
        "let", "const", "fn", "if", "else", "while", "loop", "for", "in", "return", "break",
        "continue", "true", "false", "import", "as", "export", "switch", "do", "until", "throw",
        "try", "catch", "private", "global",
    ];

    let mut col = x;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut in_block_comment = starts_in_block_comment;

    while i < chars.len() && (col - x) < max_w {
        let ch = chars[i];
        let bg = sel_bg(i, sel_range, bg);

        // Block comments (may span lines — `starts_in_block_comment`
        // above carries the state in; `block_comment_starts` carries it
        // out to whatever line comes next).
        if in_block_comment {
            renderer.draw_char(col, y, ch, Color::Grey, bg);
            col += 1;
            i += 1;
            if ch == '*' && chars.get(i) == Some(&'/') {
                renderer.draw_char(col, y, chars[i], Color::Grey, bg);
                col += 1;
                i += 1;
                in_block_comment = false;
            }
            continue;
        }
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            in_block_comment = true;
            continue;
        }

        // Comments
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
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
            col += 1;
            i += 1;
            while i < chars.len() && (col - x) < max_w {
                let s_ch = chars[i];
                renderer.draw_char(col, y, s_ch, Color::Yellow, bg);
                col += 1;
                i += 1;
                if s_ch == '"' {
                    break;
                }
            }
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() {
            renderer.draw_char(col, y, ch, Color::Magenta, bg);
            col += 1;
            i += 1;
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
        col += 1;
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::block_comment_starts;

    #[test]
    fn a_block_comment_spanning_three_lines_is_tracked_correctly() {
        let buffer: Vec<String> = ["let x = 1;", "/* still", "a comment */", "let y = 2;"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let starts = block_comment_starts(&buffer);
        assert_eq!(
            starts,
            vec![false, false, true, false],
            "only line 2 (\"a comment */\") starts already inside the block comment"
        );
    }

    #[test]
    fn an_unterminated_block_comment_stays_open_to_the_end_of_the_buffer() {
        let buffer: Vec<String> =
            ["/* never closes", "still going", "still going"].iter().map(|s| s.to_string()).collect();
        let starts = block_comment_starts(&buffer);
        assert_eq!(starts, vec![false, true, true]);
    }

    #[test]
    fn a_line_comment_does_not_start_a_block_comment() {
        let buffer: Vec<String> =
            ["// this /* is not a block comment", "let x = 1;"].iter().map(|s| s.to_string()).collect();
        let starts = block_comment_starts(&buffer);
        assert_eq!(starts, vec![false, false]);
    }

    #[test]
    fn a_slash_star_inside_a_string_does_not_start_a_real_comment() {
        let buffer: Vec<String> =
            ["let s = \"/*\";", "let x = 1;"].iter().map(|s| s.to_string()).collect();
        let starts = block_comment_starts(&buffer);
        assert_eq!(starts, vec![false, false]);
    }
}
