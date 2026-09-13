// editor/ui/script.rs — Script editor rendering and Rhai syntax highlighting.
//
// Converted to POINTS (7D-3 checkpoint 5, docs/ember2d-master-plan.md §5.4):
// every rect this draws is now real pixels from `ScriptLayout::compute`
// (`ui/script_layout.rs`), not the engine's fixed 8×16 cell grid — the
// script editor scales with the editor's UI scale like every other chrome
// surface, drawn through the theme's `code_font` (a monospace face) instead
// of the literal bitmap glyph pipeline. Routed through `UiPainter` (7D-3
// checkpoint 7) — `UiPainter::text_mono` is exactly the fixed-per-character-
// pitch text draw this file needs, so it no longer keeps its own local
// wrapper around `draw_text_run`.
//
// R73 (§3 in the master plan) fixed here two ways: `clip` restricts the
// text area to its own rect (a long line or a wide identifier can no
// longer bleed into a neighboring panel), and selection highlighting is
// now a single per-line background FILL over the exact `(start, end)`
// character span BEFORE any text draws — not a per-TOKEN color decision
// the highlighter used to make while drawing each token's own background,
// which highlighted a whole token even when the selection boundary
// actually landed mid-token.

use ember2d::renderer::{color::Color, Font, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::{Rect, Vec2};

use super::script_layout::ScriptLayout;

/// A script buffer position, `(char_idx, line_idx)` — same shape as
/// `EditorState::script_cursor` (7C-8, master plan §5.3).
pub type ScriptPos = (usize, usize);

#[allow(clippy::too_many_arguments)]
pub fn draw_script_editor(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    path: Option<&str>,
    buffer: &[String],
    cursor: ScriptPos,
    scroll: usize,
    // 7C-8 (master plan §5.3): columns scrolled off the left — the
    // horizontal twin of `scroll` above.
    hscroll: usize,
    unsaved: bool,
    content: Rect,
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
    // The buffer's own background is the theme's `InputBg` role — a script
    // buffer is an editable text surface, same reasoning `chrome.rs`'s
    // `draw_text_input`/`draw_inspector` (`dock.rs`) apply to their own
    // editable fields. The header/find-bar bars use the same
    // black-on-accent "text-on-accent" treatment as `dock.rs`'s panel
    // headers (`draw_hierarchy`, `draw_console`) — no themed role for that
    // combination exists yet (7D-1's own documented gap).
    let bg_col = theme.role_color(PaletteRole::InputBg);
    let accent = theme.role_color(PaletteRole::Accent);
    let danger = theme.role_color(PaletteRole::Danger);
    let dim = theme.role_color(PaletteRole::TextDim);
    let selection_bg = theme.role_color(PaletteRole::Selection);
    let text_px = theme.font_sizes.body;

    let layout = ScriptLayout::compute(theme, font, content, buffer.len(), error.is_some(), find_query.is_some());
    painter.fill(content, bg_col);

    let title = match path {
        Some(p) => format!(" EDIT: {}{}", p, if unsaved { "*" } else { "" }),
        None => " (no script open) ".to_string(),
    };
    painter.fill(layout.header, accent);
    painter.clip(Some(layout.header));
    let header_baseline = layout.header.y + painter.ascent(font, text_px);
    painter.text(font, &title, Vec2::new(layout.header.x, header_baseline), text_px, Color::Black);
    painter.clip(None);

    if let Some(query) = find_query {
        let find_rect = layout.find_bar.expect("find_query.is_some() implies ScriptLayout reserved a find_bar rect");
        painter.fill(find_rect, accent);
        let text = format!(" Find: {}_", query);
        let baseline = find_rect.y + painter.ascent(font, text_px);
        painter.clip(Some(find_rect));
        painter.text(font, &text, Vec2::new(find_rect.x, baseline), text_px, Color::Black);
        painter.clip(None);
    }

    if buffer.is_empty() && path.is_none() {
        let baseline = layout.text.y + painter.ascent(font, text_px);
        painter.text(
            font,
            "Select a .rhai file from the Files panel to edit.",
            Vec2::new(layout.text.x, baseline),
            text_px,
            dim,
        );
        return;
    }

    let comment_starts = block_comment_starts(buffer);
    let err_line = error.map(|(line, _)| line);

    // R73: clips every source line to the text area's own rect — a long
    // line or a wide identifier can no longer paint past this panel's own
    // bounds into whatever's drawn next to it.
    painter.clip(Some(layout.text));

    for (i, line) in buffer.iter().enumerate().skip(scroll).take(layout.visible_rows) {
        let row_y = layout.text.y + (i - scroll) as f32 * layout.row_h;
        let row_rect = Rect::new(layout.text.x, row_y, layout.text.w, layout.row_h);

        if err_line == Some(i) {
            painter.fill(row_rect, danger);
        }

        let num_str = format!("{:>width$} ", i + 1, width = layout.gutter_digits);
        let num_baseline = row_y + painter.ascent(font, text_px);
        painter.text_mono( font, &num_str, Vec2::new(layout.text.x, num_baseline), text_px, layout.char_w, dim);

        let line_char_count = line.chars().count();
        let visible_cols = layout.visible_cols();
        // 7C-8 (master plan §5.3): a trailing `…` when the line has more
        // content than fits after `hscroll` — shrinks the highlighter's
        // own visible width by one column to leave room for it, rather
        // than the marker overwriting whatever character was there.
        let clipped = line_char_count > hscroll + visible_cols;
        let highlight_cols = if clipped { visible_cols.saturating_sub(1) } else { visible_cols };
        // The horizontally-scrolled-off prefix is dropped BEFORE
        // highlighting, not skipped character-by-character inside it —
        // `draw_highlighted_rhai`'s comment/string parsing state simply
        // restarts at column `hscroll`, the same "good enough, not a real
        // parser" simplification `block_comment_starts` already makes
        // across line boundaries, just applied across the hscroll cut
        // instead.
        let visible_line: String = line.chars().skip(hscroll).collect();

        // R73: selection is now an exact per-character background FILL,
        // not a per-token color decision the highlighter used to make
        // while drawing each token — a selection edge landing mid-token no
        // longer highlights the whole token.
        if let Some((s, e)) = line_selection_range(selection, i) {
            let s = s.saturating_sub(hscroll).min(highlight_cols);
            let e = e.saturating_sub(hscroll).min(highlight_cols);
            if e > s {
                let sel_rect = Rect::new(
                    layout.line_x + s as f32 * layout.char_w,
                    row_y,
                    (e - s) as f32 * layout.char_w,
                    layout.row_h,
                );
                painter.fill(sel_rect, selection_bg);
            }
        }

        draw_highlighted_rhai(
            painter,
            font,
            layout.line_x,
            row_y,
            text_px,
            layout.char_w,
            &visible_line,
            highlight_cols,
            comment_starts[i],
        );
        if clipped {
            let marker_x = layout.line_x + highlight_cols as f32 * layout.char_w;
            painter.text_mono( font, "\u{2026}", Vec2::new(marker_x, num_baseline), text_px, layout.char_w, dim);
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
            let cursor_x = layout.line_x + visible_col as f32 * layout.char_w;
            if cursor_x < layout.text.x + layout.text.w {
                let char_at_cursor = line.chars().nth(cursor.0).unwrap_or(' ');
                painter.fill(Rect::new(cursor_x, row_y, layout.char_w, layout.row_h), accent);
                painter.text(
                    font,
                    &char_at_cursor.to_string(),
                    Vec2::new(cursor_x, num_baseline),
                    text_px,
                    Color::Black,
                );
            }
        }
    }
    painter.clip(None);

    if let (Some((line, msg)), Some(error_rect)) = (error, layout.error) {
        let text = format!(" ERROR Line {}: {}", line + 1, msg);
        let baseline = error_rect.y + painter.ascent(font, text_px);
        painter.fill(error_rect, danger);
        painter.clip(Some(error_rect));
        painter.text(font, &text, Vec2::new(error_rect.x, baseline), text_px, theme.role_color(PaletteRole::TitleText));
        painter.clip(None);
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
/// across lines. Skips string contents so a `/*` inside one doesn't start
/// a real comment, the same naive way `draw_highlighted_rhai`'s own
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

/// The Rhai token colors below (keyword Cyan, string Yellow, number
/// Magenta, ...) stay literal, un-themed — 7D-2 (master plan §5.4): this
/// is Rhai syntax highlighting, a semantic signal about the CONTENT being
/// edited, not decorative editor chrome, same reasoning `dock.rs` applies
/// to console log levels and file-browser icon kinds.
///
/// R73: no longer takes a selection range or draws a per-character
/// background at all — every character's background was already painted
/// by `draw_script_editor`'s own selection-fill pass (or the row's base
/// fill) before this runs; this only ever draws FOREGROUND glyphs now,
/// batched into one `UiPainter::text_mono` call per same-colored run (a whole
/// identifier/keyword, a whole string, a whole line comment) rather than
/// one call per character, which both draws faster and can never
/// misalign a run's own characters from each other.
#[allow(clippy::too_many_arguments)]
fn draw_highlighted_rhai(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    x: f32,
    y: f32,
    text_px: f32,
    char_w: f32,
    line: &str,
    max_cols: usize,
    starts_in_block_comment: bool,
) {
    let keywords = [
        "let", "const", "fn", "if", "else", "while", "loop", "for", "in", "return", "break",
        "continue", "true", "false", "import", "as", "export", "switch", "do", "until", "throw",
        "try", "catch", "private", "global",
    ];

    let baseline_y = y + painter.ascent(font, text_px);
    let mut col = 0usize;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut in_block_comment = starts_in_block_comment;

    macro_rules! at {
        ($col:expr) => {
            Vec2::new(x + $col as f32 * char_w, baseline_y)
        };
    }

    while i < chars.len() && col < max_cols {
        let ch = chars[i];

        // Block comments (may span lines — `starts_in_block_comment`
        // above carries the state in; `block_comment_starts` carries it
        // out to whatever line comes next).
        if in_block_comment {
            let start = i;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            if i < chars.len() {
                i += 2; // the closing `*/`
                in_block_comment = false;
            } else {
                i = chars.len(); // still open at the end of this line
            }
            let run: String = chars[start..i].iter().collect();
            let run_cols = run.chars().count();
            painter.text_mono( font, &run, at!(col), text_px, char_w, Color::Grey);
            col += run_cols;
            continue;
        }
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            in_block_comment = true;
            continue;
        }

        // Comments
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            let rest: String = chars[i..].iter().collect();
            painter.text_mono( font, &rest, at!(col), text_px, char_w, Color::Grey);
            return;
        }

        // Strings
        if ch == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                i += 1; // the closing quote
            }
            let run: String = chars[start..i].iter().collect();
            let run_cols = run.chars().count();
            painter.text_mono( font, &run, at!(col), text_px, char_w, Color::Yellow);
            col += run_cols;
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() {
            painter.text_mono( font, &ch.to_string(), at!(col), text_px, char_w, Color::Magenta);
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
            let word: String = chars[start..i].iter().collect();
            let color = if keywords.contains(&word.as_str()) { Color::Cyan } else { Color::White };
            let word_cols = word.chars().count();
            painter.text_mono( font, &word, at!(col), text_px, char_w, color);
            col += word_cols;
            continue;
        }

        // Operators / Punctuation
        let color = match ch {
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '^' => Color::Yellow,
            '(' | ')' | '[' | ']' | '{' | '}' => Color::Magenta,
            ',' | ';' | ':' | '.' => Color::Grey,
            _ => Color::White,
        };
        painter.text_mono( font, &ch.to_string(), at!(col), text_px, char_w, color);
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
