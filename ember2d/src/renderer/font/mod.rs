// renderer/font/mod.rs — Font abstraction (Phase 7 Part 2, additive per
// that phase's guardrail, docs/ember2d-phase7-plan.md §0.6).
//
// Split into a directory module (rather than one flat `font.rs`) from the
// start, anticipating what Part 2's own plan calls for: the `Font` trait
// plus TWO real implementations and the dynamic atlas the second one
// needs — `bitmap.rs` (`BitmapFont`, wraps the existing font8x8 atlas),
// `ttf.rs` (`TtfFont`, rasterizes via `fontdue`), `atlas.rs` (`GlyphAtlas`,
// the shelf-packed dynamic texture `TtfFont` rasterizes into on a cache
// miss). Neither implementation is wired into any live UI draw call yet
// — that's Part 2c's `measure_text` audit and Part 4's actual restyle.

use super::texture::{Texture, TextureId};
use ember2d_sim::math::{Rect, Vec2};

mod atlas;
mod bitmap;
mod ttf;

pub use atlas::GlyphAtlas;
pub use bitmap::BitmapFont;
pub use ttf::TtfFont;

/// Which `Font` `ui_font_from_env` picked. `Renderer::draw_str` matches on
/// this to decide whether it can take the original, byte-for-byte-
/// unchanged font8x8 fast path (`Bitmap`) or must go through the generic
/// `Font`-trait `draw_text_px` path instead (`Ttf`) — see that function's
/// own doc comment (7B-5, docs/ember2d-master-plan.md §5.2) for why those
/// two aren't interchangeable yet: `BitmapFont`'s `Font` implementation
/// models a native glyph as a literal 8×8 square, but the dedicated
/// font8x8 GPU path (`WgpuBackend::draw_char`) has always stretched that
/// same 8×8 bitmap 2x vertically to fill the 8×16 `CELL_W`×`CELL_H` cell —
/// routing the default case through `draw_text_px` would render every
/// glyph visibly smaller than it always has been, which is a real
/// regression, not a refactor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiFontKind {
    Bitmap,
    Ttf,
}

/// This process's active UI font, chosen once from the `EMBER_UI_FONT`
/// debug env var (7B-5, docs/ember2d-master-plan.md §5.2): `"ttf"` selects
/// the bundled Cascadia Mono `TtfFont` at 16px; anything else (unset,
/// typo'd, the bundled font somehow failing to parse) falls back to
/// `BitmapFont` at its native, `CELL_W`-sized 8px. Takes the env var's
/// value as a plain `Option<&str>` rather than reading
/// `std::env::var` itself, so tests can exercise both branches directly
/// instead of mutating process-global env state (racy under `cargo test`'s
/// parallel test execution) — `ui_font_from_env` below is the real
/// entry point every non-test caller uses.
fn ui_font_for(value: Option<&str>) -> (Box<dyn Font>, f32, UiFontKind) {
    const CASCADIA_MONO: &[u8] = include_bytes!("../../../assets/fonts/CascadiaMono.ttf");
    const TTF_PX: f32 = 16.0;

    if value == Some("ttf") {
        match TtfFont::from_bytes(CASCADIA_MONO, 0) {
            Ok(font) => return (Box::new(font), TTF_PX, UiFontKind::Ttf),
            Err(e) => eprintln!(
                "EMBER_UI_FONT=ttf requested but the bundled font failed to parse ({e}) — falling back to BitmapFont"
            ),
        }
    }
    (Box::new(BitmapFont::new()), super::CELL_W as f32, UiFontKind::Bitmap)
}

/// See `ui_font_for` — this just supplies it the real `EMBER_UI_FONT`
/// env var. `Renderer::new` is the one caller.
pub fn ui_font_from_env() -> (Box<dyn Font>, f32, UiFontKind) {
    ui_font_for(std::env::var("EMBER_UI_FONT").ok().as_deref())
}

/// One glyph's atlas placement and pen-advance metrics, resolved for one
/// specific `(char, px)` request. Deliberately doesn't carry which texture
/// it came from — that's `Font::texture_id`, since it's the same answer
/// for every glyph a given `Font` impl ever returns, not worth repeating
/// per glyph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphInfo {
    /// This glyph's sub-rectangle within `Font::texture_id`'s atlas, in
    /// that texture's own pixels. Zero-sized (not absent) for a glyph with
    /// nothing to draw, e.g. a space — it still has a real `advance`.
    pub atlas_rect: Rect,
    /// Offset from the pen position (sitting ON the baseline, at the
    /// glyph's nominal left edge) to the glyph quad's top-left corner, in
    /// the same pixel units as `atlas_rect`, in this engine's y-DOWN
    /// screen convention. `Vec2::ZERO` for a font with no per-glyph
    /// bearing (every `BitmapFont` glyph fills its whole advance cell with
    /// no side-bearing to shift for).
    pub offset: Vec2,
    /// How far the pen moves horizontally after drawing this glyph.
    pub advance: f32,
    /// R50 (§3 in the master plan, fixed 7D-3): the size to actually DRAW
    /// this glyph at, in the same pixel units as `atlas_rect`/`offset` —
    /// NOT necessarily the same as `atlas_rect`'s own `(w, h)`. For
    /// `TtfFont` the two always agree (`fontdue` rasterizes each glyph at
    /// exactly the requested size, so the atlas rect already IS the drawn
    /// size). For `BitmapFont` they never do at a non-native request: the
    /// atlas rect stays a fixed 8×8 native cell no matter what size was
    /// asked for (only `advance`/`offset` scale) — before this field
    /// existed, `draw_text_run`'s dest rect used `atlas_rect`'s size
    /// directly, which drew every `BitmapFont` glyph at its literal native
    /// size regardless of the requested `px`, a gap that predated 7D-3 but
    /// went unnoticed because `draw_text_px`/`draw_text_run` had no live
    /// `BitmapFont` caller before this step (chrome text always used a
    /// `TtfFont` theme font, and the default bitmap UI-font path drew
    /// through the separate, dedicated `draw_char` pipeline instead).
    pub size: Vec2,
}

/// A source of glyph metrics and atlas placements at arbitrary pixel
/// sizes. UI code calls this trait and never branches on which
/// implementation it got (Phase 7 Part 2, docs/ember2d-phase7-plan.md
/// §0.3) — a pixel-styled theme hands out a `BitmapFont`, a modern one a
/// `TtfFont`, and every call site that centers, right-aligns, wraps, or
/// positions a text cursor goes through `measure`/`glyph` either way.
pub trait Font {
    /// The atlas texture every `GlyphInfo` this `Font` returns lives in.
    /// Not part of the plan's own trait sketch (docs/ember2d-phase7-plan.md
    /// §2a) — added here because a `GlyphInfo` alone can't be turned into
    /// an actual textured-quad draw call without knowing which texture its
    /// `atlas_rect` addresses.
    fn texture_id(&self) -> TextureId;

    /// The full pixel dimensions of `texture_id`'s atlas — needed
    /// alongside it (Phase 7 Part 2d, docs/ember2d-phase7-plan.md) to turn
    /// a `GlyphInfo::atlas_rect` (in that texture's own pixels) into a
    /// normalized UV rect for an actual draw call. Same rationale as
    /// `texture_id` for being outside the plan's own trait sketch.
    fn texture_size(&self) -> (u32, u32);

    /// The real, CPU-side backing `Texture` for `texture_id`'s atlas, if
    /// this `Font` owns one (Phase 7 Part 2d, docs/ember2d-phase7-plan.md).
    /// `TtfFont` does (`GlyphAtlas::texture`, a real growable buffer this
    /// same struct rasterizes new glyphs into). `BitmapFont` doesn't — its
    /// atlas is baked once into the GPU by `WgpuBackend::new` and was
    /// never represented as an `ember2d::renderer::Texture` value at all
    /// — hence `None` by default: a caller that actually uploads a `Font`'s
    /// atlas (`Renderer::draw_text_px`) needs real pixels only for the
    /// kind of `Font` whose atlas can change after its first upload;
    /// `BitmapFont`'s never does; there's nothing to upload, ever.
    fn atlas_texture(&self) -> Option<&Texture> {
        None
    }

    /// Whether this `Font`'s atlas texture has changed since the last
    /// call, clearing the flag either way (Phase 7 Part 2d). `BitmapFont`
    /// never needs to override this — its atlas is immutable — so `false`
    /// is the correct default, not just a placeholder.
    fn take_dirty(&mut self) -> bool {
        false
    }

    /// Discard every cached glyph and rebuild this `Font`'s atlas at
    /// `w`×`h` (7D-3, docs/ember2d-master-plan.md §5.4) — called when the
    /// editor's UI scale changes, since every glyph a chrome font had
    /// cached was rasterized at the OLD scale's physical size and is now
    /// the wrong size for anything drawn from here on. `TtfFont` overrides
    /// this with a fresh `GlyphAtlas` (a new texture id — the old one
    /// simply falls out of use and is eventually evicted by the backend's
    /// own LRU budget, R26, the same way a theme switch's old chrome
    /// texture already does, `theme_loader::switch_theme`'s own doc
    /// comment). `BitmapFont` needs no override: its atlas is the engine's
    /// one shared, size-independent font8x8 texture — "resizing" it makes
    /// no sense, so the default no-op is correct, not a placeholder.
    fn reset_atlas(&mut self, _w: u32, _h: u32) {}

    /// Rasterize (or fetch from cache) one glyph at one pixel size.
    /// Returns its atlas sub-rect and layout metrics, or `None` if this
    /// font has no glyph for `ch` at all — `TtfFont` also returns `None`
    /// if its atlas is full (see `GlyphAtlas`'s own doc comment); that's
    /// a distinct condition from "no such glyph" but the plan defers
    /// solving atlas exhaustion until it's ever actually observed, so
    /// there's no third outcome to plumb through yet.
    fn glyph(&mut self, ch: char, px: f32) -> Option<GlyphInfo>;

    /// Width and height of `text` at `px`, WITHOUT drawing it. Operates on
    /// a single line — a caller with multi-line text splits on `\n` first
    /// and combines each line's width with `line_height`, the same way
    /// every existing multi-line `draw_str` call site already splits
    /// before calling it per line.
    fn measure(&mut self, text: &str, px: f32) -> (f32, f32);

    /// Vertical distance from one line's baseline to the next, at `px`.
    fn line_height(&self, px: f32) -> f32;

    /// Vertical distance from the baseline up to the top of a full-height
    /// glyph, at `px` — what a caller that thinks in top-left-anchored
    /// boxes (every existing cell-based draw call) adds to a box's top
    /// edge to find where to place the baseline `glyph`'s `offset` is
    /// relative to.
    fn ascent(&self, px: f32) -> f32;

    /// Split `text` into lines no wider than `max_w` at `px`, breaking
    /// only on whitespace (Phase 7 Part 2c, docs/ember2d-phase7-plan.md —
    /// "exactly what docs/ember2d-rpg-demo-feasibility.md §2.5 flags as
    /// missing for dialogue"). A default method, not per-implementation:
    /// it's built entirely out of `measure`, so every `Font` impl gets it
    /// for free, present and future.
    ///
    /// `text` is assumed to be a single paragraph with no embedded `\n` —
    /// same convention as `measure`'s own doc comment; a caller with
    /// multiple paragraphs wraps each one separately and joins the
    /// results. Whitespace runs (spaces, tabs, newlines-if-any-slip-through)
    /// collapse to a single space between words, standard word-wrap
    /// behavior. A single word wider than `max_w` on its own still gets
    /// its own line rather than being split mid-word — this only wraps
    /// between words, it doesn't hyphenate.
    ///
    /// Empty (or all-whitespace) input returns one empty line, not zero
    /// lines — a text box showing "nothing" still has one line's worth of
    /// height to reserve, not none.
    fn wrap_text(&mut self, text: &str, px: f32, max_w: f32) -> Vec<String> {
        if text.trim().is_empty() {
            return vec![String::new()];
        }

        let mut lines = Vec::new();
        let mut current = String::new();
        let mut current_w = 0.0f32;

        for word in text.split_whitespace() {
            let (word_w, _) = self.measure(word, px);
            if current.is_empty() {
                current = word.to_string();
                current_w = word_w;
                continue;
            }
            let (space_w, _) = self.measure(" ", px);
            if current_w + space_w + word_w <= max_w {
                current.push(' ');
                current.push_str(word);
                current_w += space_w + word_w;
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
                current_w = word_w;
            }
        }
        lines.push(current);
        lines
    }

    /// Truncate `text` to fit within `max_w` pixels at `px`, appending a
    /// truncation marker if it had to cut (Phase 7 Part 2c,
    /// docs/ember2d-phase7-plan.md — "truncation with `…`"). Returns
    /// `text` unchanged if it already fits.
    ///
    /// Uses `".."` rather than the plan's literal `…` (U+2026): every
    /// `Font` impl must be able to draw its own truncation marker, and
    /// `BitmapFont` wraps a pure-ASCII atlas (`glyph` returns `None` for
    /// anything outside it) — a real ellipsis character would either fail
    /// to render or silently wrap onto an unrelated glyph. `".."` matches
    /// the ASCII-safe truncation marker `ui/panels/chrome.rs`'s
    /// `draw_text_input` already used before this method existed.
    fn truncate_to_width(&mut self, text: &str, px: f32, max_w: f32) -> String {
        let (full_w, _) = self.measure(text, px);
        if full_w <= max_w {
            return text.to_string();
        }

        const MARKER: &str = "..";
        let (marker_w, _) = self.measure(MARKER, px);
        let budget = (max_w - marker_w).max(0.0);

        let mut out = String::new();
        let mut w = 0.0f32;
        for ch in text.chars() {
            let cw = self.glyph(ch, px).map(|g| g.advance).unwrap_or(0.0);
            if w + cw > budget {
                break;
            }
            out.push(ch);
            w += cw;
        }
        out.push_str(MARKER);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `BitmapFont` at 8px makes expected widths trivial to hand-compute
    // (every char and the space between words is exactly 8px) — the
    // point of these tests is `wrap_text`'s own line-breaking logic, not
    // any particular `Font` implementation's metrics.

    #[test]
    fn wrap_text_of_empty_input_returns_one_empty_line() {
        let mut font = BitmapFont::new();
        assert_eq!(font.wrap_text("", 8.0, 100.0), vec![String::new()]);
        assert_eq!(
            font.wrap_text("   ", 8.0, 100.0),
            vec![String::new()],
            "all-whitespace input is also empty"
        );
    }

    #[test]
    fn wrap_text_keeps_an_exact_fit_on_one_line() {
        let mut font = BitmapFont::new();
        // "ab cd" @ 8px = 16 + 8 (space) + 16 = 40, exactly `max_w` —
        // must NOT wrap just because it's flush against the limit.
        assert_eq!(font.wrap_text("ab cd", 8.0, 40.0), vec!["ab cd".to_string()]);
        // One past exact-fit must wrap.
        assert_eq!(font.wrap_text("ab cde", 8.0, 40.0), vec!["ab".to_string(), "cde".to_string()]);
    }

    #[test]
    fn wrap_text_never_splits_a_single_word_wider_than_max_w() {
        let mut font = BitmapFont::new();
        let long_word = "supercalifragilisticexpialidocious"; // far wider than 40px @ 8px/char
        assert_eq!(font.wrap_text(long_word, 8.0, 40.0), vec![long_word.to_string()]);
    }

    #[test]
    fn wrap_text_a_long_word_mixed_with_short_ones_only_breaks_between_words() {
        let mut font = BitmapFont::new();
        let long_word = "supercalifragilisticexpialidocious";
        let text = format!("a {} b", long_word);
        assert_eq!(
            font.wrap_text(&text, 8.0, 40.0),
            vec!["a".to_string(), long_word.to_string(), "b".to_string()],
        );
    }

    #[test]
    fn wrap_text_breaks_into_multiple_lines_at_the_width_limit() {
        let mut font = BitmapFont::new();
        // Word widths @ 8px: the=24, quick=40, brown=40, fox=24, jumps=40; space=8.
        let lines = font.wrap_text("the quick brown fox jumps", 8.0, 100.0);
        assert_eq!(
            lines,
            vec!["the quick".to_string(), "brown fox".to_string(), "jumps".to_string()]
        );
    }

    #[test]
    fn truncate_to_width_returns_text_unchanged_when_it_already_fits() {
        let mut font = BitmapFont::new();
        // "hello" @ 8px = 40, exactly `max_w` — fitting exactly is not truncating.
        assert_eq!(font.truncate_to_width("hello", 8.0, 40.0), "hello");
    }

    #[test]
    fn truncate_to_width_cuts_and_appends_the_ascii_marker() {
        let mut font = BitmapFont::new();
        // "hello world" @ 8px = 88px. max_w=48 leaves 48-16(marker ".." = 2*8)=32px = 4 chars of budget.
        assert_eq!(font.truncate_to_width("hello world", 8.0, 48.0), "hell..");
    }

    #[test]
    fn truncate_to_width_never_exceeds_the_input_length() {
        let mut font = BitmapFont::new();
        // Even a huge budget doesn't grow short text or add a marker it doesn't need.
        assert_eq!(font.truncate_to_width("hi", 8.0, 1000.0), "hi");
    }

    #[test]
    fn truncate_to_width_degrades_to_just_the_marker_when_nothing_else_fits() {
        let mut font = BitmapFont::new();
        assert_eq!(font.truncate_to_width("hello world", 8.0, 1.0), "..");
    }

    // ── Tests: `ui_font_for` (7B-5, docs/ember2d-master-plan.md §5.2) ──────

    #[test]
    fn ui_font_for_defaults_to_bitmap_at_cell_w_when_unset() {
        let (font, px, kind) = ui_font_for(None);
        assert_eq!(kind, UiFontKind::Bitmap);
        assert_eq!(px, super::super::CELL_W as f32);
        assert_eq!(font.texture_id(), BitmapFont::new().texture_id());
    }

    #[test]
    fn ui_font_for_falls_back_to_bitmap_for_any_value_other_than_ttf() {
        let (_, _, kind) = ui_font_for(Some("bitmap"));
        assert_eq!(kind, UiFontKind::Bitmap, "an unrecognized value must not silently pick ttf");
        let (_, _, kind) = ui_font_for(Some(""));
        assert_eq!(kind, UiFontKind::Bitmap);
    }

    #[test]
    fn ui_font_for_selects_the_bundled_ttf_font_at_16px() {
        let (mut font, px, kind) = ui_font_for(Some("ttf"));
        assert_eq!(kind, UiFontKind::Ttf);
        assert_eq!(px, 16.0);
        // A real glyph resolves — confirms the bundled Cascadia Mono bytes
        // actually parsed, not just that the branch was taken.
        assert!(font.glyph('A', px).is_some());
    }
}
