// editor/ui/rect.rs — UiRect: a pixel-space rectangle for editor layout
// (Phase 7 Part 1b, docs/ember2d-phase7-plan.md).
//
// The editor's entire UI is quantized to 8×16 character cells today —
// `Panel.x/y/w/h` are `i32`/`usize` cell coordinates, and every hit-test
// (`on_title_bar`, `on_close_btn`, `on_resize_handle`) works in that same
// grid. `UiRect` is the pixel-space replacement Part 1c migrates `Panel`/
// `PanelManager` onto — plain `f32` pixels, no cell grid baked into the
// type itself.
//
// `from_cells` is what makes that migration survivable: because a cell is
// exactly 8×16 pixels, converting an existing panel's cell rect through it
// is a lossless multiply — Part 1's "appearance must not change" property.
// Every panel starts life going through `from_cells`; panels migrate to
// arbitrary pixel positions one at a time after that, and `from_cells`
// itself (along with the cell-based coordinate system it bridges from)
// goes away entirely once Part 4's restyle lands.

/// Width/height of one character cell in pixels — re-exported as `f32`
/// from `ember2d::renderer`'s own `CELL_W`/`CELL_H` (Phase 7 Part 1e,
/// docs/ember2d-phase7-plan.md, E2). This used to be a local duplicate of
/// the same two literals `editor/ui/canvas.rs::grid_to_pixel` and
/// `editor/panel.rs` also hardcoded independently; now there's one
/// definition and three call sites that read it.
const CELL_W: f32 = ember2d::renderer::CELL_W as f32;
const CELL_H: f32 = ember2d::renderer::CELL_H as f32;

/// A pixel-space rectangle. `f32` for layout math; rounding to whole pixels
/// happens at draw time, in the renderer primitives that consume one
/// (`fill_rect_px`/`draw_texture_px`/`draw_nine_slice`, Part 1a) — not here,
/// so intermediate layout math (insetting, splitting) never accumulates
/// rounding error before the final draw call snaps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl UiRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        UiRect { x, y, w, h }
    }

    /// A cell rect converted to pixels — see this module's header comment
    /// for why this conversion is lossless and why every panel starts life
    /// going through it.
    pub fn from_cells(cx: i32, cy: i32, cw: usize, ch: usize) -> Self {
        UiRect {
            x: cx as f32 * CELL_W,
            y: cy as f32 * CELL_H,
            w: cw as f32 * CELL_W,
            h: ch as f32 * CELL_H,
        }
    }

    pub fn right(self) -> f32 { self.x + self.w }
    pub fn bottom(self) -> f32 { self.y + self.h }

    /// Half-open: the far edge (`right()`/`bottom()`) is excluded, matching
    /// `ember2d_sim::math::Rect::contains_point`'s own convention.
    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Shrink by `by` pixels on every side. A rect too small to hold the
    /// inset clamps to zero size (staying centered) rather than flipping to
    /// a negative width/height.
    pub fn inset(self, by: f32) -> Self {
        let w = (self.w - by * 2.0).max(0.0);
        let h = (self.h - by * 2.0).max(0.0);
        UiRect::new(self.x + by, self.y + by, w, h)
    }

    /// Split off a `w`-pixel-wide strip from the left edge. Returns
    /// `(strip, remainder)`. `w` is clamped to this rect's own width, so
    /// the remainder never goes negative.
    pub fn split_left(self, w: f32) -> (Self, Self) {
        let w = w.clamp(0.0, self.w);
        (UiRect::new(self.x, self.y, w, self.h), UiRect::new(self.x + w, self.y, self.w - w, self.h))
    }

    /// Split off a `w`-pixel-wide strip from the right edge. Returns
    /// `(strip, remainder)` — the remainder is what's left on the LEFT
    /// side, matching `split_left`'s "peeled piece first" convention.
    pub fn split_right(self, w: f32) -> (Self, Self) {
        let w = w.clamp(0.0, self.w);
        (UiRect::new(self.right() - w, self.y, w, self.h), UiRect::new(self.x, self.y, self.w - w, self.h))
    }

    /// Split off an `h`-pixel-tall strip from the top edge. Returns
    /// `(strip, remainder)`.
    pub fn split_top(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (UiRect::new(self.x, self.y, self.w, h), UiRect::new(self.x, self.y + h, self.w, self.h - h))
    }

    /// Split off an `h`-pixel-tall strip from the bottom edge. Returns
    /// `(strip, remainder)` — the remainder is what's left on TOP, matching
    /// `split_right`'s convention.
    pub fn split_bottom(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (UiRect::new(self.x, self.bottom() - h, self.w, h), UiRect::new(self.x, self.y, self.w, self.h - h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_cells_multiplies_by_the_exact_cell_size() {
        let r = UiRect::from_cells(2, 3, 10, 5);
        assert_eq!(r, UiRect::new(16.0, 48.0, 80.0, 80.0));
    }

    #[test]
    fn from_cells_matches_the_old_cell_math_for_shapes_like_the_current_panels() {
        // Part 1's "appearance must not change" property, pinned: converting
        // any (cx, cy, cw, ch) cell rect through UiRect must reproduce the
        // exact pixel geometry the old i32/usize `Panel` fields already
        // implied — a plain cell-size multiply, lossless since a cell is
        // exactly 8x16 pixels. Shapes below mirror PanelManager::new's own
        // Hierarchy/Inspector/Console constants (panel.rs), not copies of
        // them (Part 1c is what actually migrates those).
        for &(cx, cy, cw, ch) in &[
            (0i32, 2i32, 14usize, 20usize), // Hierarchy-shaped
            (36, 2, 30, 20),                // Inspector-shaped
            (0, 15, 80, 9),                 // Console-shaped
        ] {
            let r = UiRect::from_cells(cx, cy, cw, ch);
            assert_eq!(r.x, cx as f32 * 8.0);
            assert_eq!(r.y, cy as f32 * 16.0);
            assert_eq!(r.w, cw as f32 * 8.0);
            assert_eq!(r.h, ch as f32 * 16.0);
        }
    }

    #[test]
    fn contains_is_a_half_open_rect() {
        let r = UiRect::new(10.0, 10.0, 5.0, 5.0);
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(14.9, 14.9));
        assert!(!r.contains(15.0, 15.0), "the far edge is exclusive");
        assert!(!r.contains(9.9, 10.0));
    }

    #[test]
    fn inset_shrinks_symmetrically_and_clamps_at_zero() {
        let r = UiRect::new(0.0, 0.0, 20.0, 10.0).inset(2.0);
        assert_eq!(r, UiRect::new(2.0, 2.0, 16.0, 6.0));

        let tiny = UiRect::new(0.0, 0.0, 2.0, 2.0).inset(5.0);
        assert_eq!((tiny.w, tiny.h), (0.0, 0.0), "an inset larger than the rect must clamp to zero, not go negative");
    }

    #[test]
    fn split_left_and_split_right_partition_the_rect_without_overlap() {
        let r = UiRect::new(0.0, 0.0, 100.0, 50.0);

        let (left, rest) = r.split_left(30.0);
        assert_eq!(left, UiRect::new(0.0, 0.0, 30.0, 50.0));
        assert_eq!(rest, UiRect::new(30.0, 0.0, 70.0, 50.0));

        let (right, rest) = r.split_right(30.0);
        assert_eq!(right, UiRect::new(70.0, 0.0, 30.0, 50.0));
        assert_eq!(rest, UiRect::new(0.0, 0.0, 70.0, 50.0));
    }

    #[test]
    fn split_top_and_split_bottom_partition_the_rect_without_overlap() {
        let r = UiRect::new(0.0, 0.0, 100.0, 50.0);

        let (top, rest) = r.split_top(10.0);
        assert_eq!(top, UiRect::new(0.0, 0.0, 100.0, 10.0));
        assert_eq!(rest, UiRect::new(0.0, 10.0, 100.0, 40.0));

        let (bottom, rest) = r.split_bottom(10.0);
        assert_eq!(bottom, UiRect::new(0.0, 40.0, 100.0, 10.0));
        assert_eq!(rest, UiRect::new(0.0, 0.0, 100.0, 40.0));
    }

    #[test]
    fn split_clamps_a_strip_wider_or_taller_than_the_rect_itself() {
        let r = UiRect::new(0.0, 0.0, 10.0, 10.0);

        let (strip, rest) = r.split_left(50.0);
        assert_eq!(strip, UiRect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!((rest.w, rest.h), (0.0, 10.0));

        let (strip, rest) = r.split_top(50.0);
        assert_eq!(strip, UiRect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!((rest.w, rest.h), (10.0, 0.0));
    }
}
