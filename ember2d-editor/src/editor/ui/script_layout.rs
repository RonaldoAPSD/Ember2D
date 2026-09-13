// editor/ui/script_layout.rs — ScriptLayout: the one geometry the script
// editor's draw pass and BOTH of its input paths (the docked panel's
// first-click handler, `input/panels/file_and_script.rs`, and the
// focused/fullscreen handler, `input/script_editor.rs`) compute from
// (7D-3, docs/ember2d-master-plan.md §5.4).
//
// THE BUG THIS CLOSES (R67/R68, §3 in the master plan): before this, the
// docked panel's first click and its own focused-panel click each
// recomputed the header height, the gutter width, and the text area
// bounds independently — a fixed `gutter_w = 4` baked into three separate
// places, none of which knew the gutter actually needs a 5th column once
// the file passes 999 lines. `ScriptLayout::compute` is a pure function of
// its own inputs (the content rect, the theme, the font, the line count,
// and whether an error/find row is reserved) — called fresh in draw() and
// in each input handler, it always returns the exact same rects, the same
// "single source of truth" fix already applied to every other panel in
// this step (`ChromeMetrics`/`Panel::content_rect`), just as a per-call
// pure function instead of a stored value (nothing here is expensive
// enough to need caching between calls).

use ember2d::renderer::Font;
use ember2d::theme::Theme;
use ember2d_sim::math::Rect;

pub struct ScriptLayout {
    /// The header strip — the script's path/unsaved-marker row.
    pub header: Rect,
    /// The Ctrl+F find bar, one row right below the header, when open.
    pub find_bar: Option<Rect>,
    /// The reserved error-message row at the very bottom, when a live
    /// syntax check has one to show.
    pub error: Option<Rect>,
    /// The actual source-text area: below the header/find bar, above the
    /// error row, gutter included.
    pub text: Rect,
    /// The line-number gutter's width, in points — R68: sized from the
    /// buffer's own real line count (`digits(line_count) + 1` chars wide),
    /// not a fixed `4` that silently misaligned past line 999.
    pub gutter_w: f32,
    /// How many digits `gutter_w` was sized for (`gutter_w / char_w - 1`,
    /// stored directly rather than recovered from a float division) — the
    /// line-number column's own `{:>width$}` field width.
    pub gutter_digits: usize,
    /// Where source characters (after the gutter) start, in points.
    pub line_x: f32,
    pub row_h: f32,
    /// The code font's own fixed per-character advance, in points —
    /// `UiPainter::text_mono`'s `pitch_pt`, so a drawn character's column
    /// always matches this same layout's own column math.
    pub char_w: f32,
    /// How many source rows the text area actually fits.
    pub visible_rows: usize,
}

impl ScriptLayout {
    pub fn compute(
        theme: &Theme,
        font: &mut dyn Font,
        content: Rect,
        line_count: usize,
        has_error: bool,
        has_find: bool,
    ) -> Self {
        let row_h = theme.metrics.row_h;
        let text_px = theme.font_sizes.body;
        let char_w = font.measure("M", text_px).0.max(1.0);

        // R68: was a fixed `4` — now wide enough for the highest line
        // number this buffer actually has, plus one column of padding, so
        // a file long enough to need a 4-digit line number (1000+ lines)
        // never misaligns the gutter from the source text beside it.
        let digits = line_count.max(1).to_string().len();
        let gutter_w = (digits as f32 + 1.0) * char_w;

        let header = Rect::new(content.x, content.y, content.w, row_h);
        let mut y = content.y + row_h;
        let find_bar = if has_find {
            let r = Rect::new(content.x, y, content.w, row_h);
            y += row_h;
            Some(r)
        } else {
            None
        };

        let error = if has_error {
            Some(Rect::new(content.x, content.y + content.h - row_h, content.w, row_h))
        } else {
            None
        };
        let error_h = if has_error { row_h } else { 0.0 };

        let text_h = (content.y + content.h - y - error_h).max(0.0);
        let text = Rect::new(content.x, y, content.w, text_h);
        let visible_rows = (text.h / row_h).floor().max(0.0) as usize;
        let line_x = content.x + gutter_w;

        ScriptLayout {
            header,
            find_bar,
            error,
            text,
            gutter_w,
            gutter_digits: digits,
            line_x,
            row_h,
            char_w,
            visible_rows,
        }
    }

    /// How many source columns (after the gutter) the text area fits.
    pub fn visible_cols(&self) -> usize {
        ((self.text.w - self.gutter_w) / self.char_w).floor().max(0.0) as usize
    }

    /// The `(char, line)` a click at `(px, py)` lands on, or `None` if it
    /// falls outside the text area entirely (the header, the gutter... no
    /// — the gutter still resolves to column 0, matching a click anywhere
    /// on a row placing the cursor at at least the row's start) or past
    /// the end of the buffer. `scroll`/`hscroll` are the current vertical/
    /// horizontal scroll offsets; `buffer` clamps the result to each
    /// line's own real character count (R11's own reasoning: a char
    /// index, never a byte index).
    pub fn hit(
        &self,
        px: f32,
        py: f32,
        scroll: usize,
        hscroll: usize,
        buffer: &[String],
    ) -> Option<(usize, usize)> {
        if px < self.text.x || px >= self.text.x + self.text.w {
            return None;
        }
        if py < self.text.y || py >= self.text.y + self.text.h {
            return None;
        }
        let row_in_view = ((py - self.text.y) / self.row_h).floor().max(0.0) as usize;
        let line = scroll + row_in_view;
        if line >= buffer.len() {
            return None;
        }
        let col_f = (px - self.line_x) / self.char_w + hscroll as f32;
        let col = (col_f.max(0.0).round() as usize).min(buffer[line].chars().count());
        Some((col, line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d::renderer::BitmapFont;
    use ember2d::theme::{FontChoice, FontSizes, Metrics};
    use std::collections::BTreeMap;

    /// A minimal, directly-constructed `Theme` (mirrors `Theme::fallback`'s
    /// own field values) — no `AssetManager` needed, since nothing here
    /// ever draws the chrome texture, only reads `metrics`/`font_sizes`.
    fn test_theme() -> Theme {
        Theme {
            name: "test".to_string(),
            palette: BTreeMap::new(),
            chrome: ember2d::renderer::TextureId(0),
            slices: BTreeMap::new(),
            font: FontChoice::Bitmap,
            code_font: None,
            font_sizes: FontSizes { small: 8.0, body: 8.0, heading: 16.0 },
            metrics: Metrics { padding: 4.0, border: 1.0, row_h: 16.0, min_target: 16.0 },
        }
    }

    #[test]
    fn gutter_widens_for_a_four_digit_line_count() {
        // R68: a 3-digit budget (the old fixed `4` == 3 digits + 1 space)
        // is exactly enough for 999 lines but not 1000.
        let theme = test_theme();
        let mut font = BitmapFont::new();
        let content = Rect::new(0.0, 0.0, 400.0, 400.0);
        let short = ScriptLayout::compute(&theme, &mut font, content, 42, false, false);
        let long = ScriptLayout::compute(&theme, &mut font, content, 1000, false, false);
        assert!(long.gutter_w > short.gutter_w, "a 1000-line buffer must get a wider gutter than a 42-line one");
    }

    #[test]
    fn hit_outside_the_text_area_returns_none() {
        let theme = test_theme();
        let mut font = BitmapFont::new();
        let content = Rect::new(0.0, 0.0, 400.0, 400.0);
        let layout = ScriptLayout::compute(&theme, &mut font, content, 10, false, false);
        let buffer = vec!["hello".to_string()];
        assert_eq!(layout.hit(0.0, 0.0, 0, 0, &buffer), None, "the header row is not the text area");
    }

    #[test]
    fn hit_past_the_last_line_returns_none() {
        let theme = test_theme();
        let mut font = BitmapFont::new();
        let content = Rect::new(0.0, 0.0, 400.0, 400.0);
        let layout = ScriptLayout::compute(&theme, &mut font, content, 1, false, false);
        let buffer = vec!["hello".to_string()];
        let far_below = layout.text.y + layout.text.h - 1.0;
        assert_eq!(layout.hit(layout.line_x, far_below, 0, 0, &buffer), None);
    }

    #[test]
    fn hit_clamps_the_column_to_the_lines_own_character_count() {
        // R11's own reasoning, reused here: clicking past the end of a
        // short line must clamp to its character count, never its byte
        // length or some larger fixed budget.
        let theme = test_theme();
        let mut font = BitmapFont::new();
        let content = Rect::new(0.0, 0.0, 400.0, 400.0);
        let layout = ScriptLayout::compute(&theme, &mut font, content, 1, false, false);
        let buffer = vec!["ab".to_string()];
        let far_right = layout.line_x + 5.0 * layout.char_w;
        let hit = layout.hit(far_right, layout.text.y + 1.0, 0, 0, &buffer);
        assert_eq!(hit, Some((2, 0)), "must clamp to the 2-character line, not overshoot");
    }
}
