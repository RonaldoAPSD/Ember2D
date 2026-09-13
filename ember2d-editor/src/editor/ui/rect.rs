// editor/ui/rect.rs — UiRect: a pixel-space rectangle for editor layout
// (Phase 7 Part 1b, docs/ember2d-phase7-plan.md).
//
// `UiRect` is the pixel-space type `Panel`/`PanelManager` (Part 1c) and
// every themed `ui::draw_*` function (7D-2, docs/ember2d-master-plan.md
// §5.4) build their geometry from — plain `f32` pixels, no cell grid
// baked into the type itself.
//
// `from_cells(cx, cy, cw, ch) -> Self` — a lossless `* CELL_W`/`* CELL_H`
// multiply — used to live here as the one bridge every panel and every
// `ui::draw_*` call built its geometry through while the whole editor was
// still cell-quantized (Part 1's "appearance must not change" property).
// Deleted once its last real caller (the color picker's hue bar/SV map,
// `ui/panels/modals.rs` — cell-grid BY DESIGN, not unmigrated chrome; see
// that file's own comment) converted to constructing the same pixel rect
// directly instead of through a named helper. The cell-based coordinate
// system itself didn't go away with it: `Panel::cell_x`/`content_x` (and
// siblings, `panel/mod.rs`) are still very much alive for the genuinely
// cell-grid subsystems that were never "chrome that hasn't migrated
// yet" — the viewport, the script editor's per-character math, the node
// graph's cell-addressed hit-testing — see that file's own header comment
// for the full account.

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

    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

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
        (
            UiRect::new(self.x, self.y, w, self.h),
            UiRect::new(self.x + w, self.y, self.w - w, self.h),
        )
    }

    /// Split off a `w`-pixel-wide strip from the right edge. Returns
    /// `(strip, remainder)` — the remainder is what's left on the LEFT
    /// side, matching `split_left`'s "peeled piece first" convention.
    pub fn split_right(self, w: f32) -> (Self, Self) {
        let w = w.clamp(0.0, self.w);
        (
            UiRect::new(self.right() - w, self.y, w, self.h),
            UiRect::new(self.x, self.y, self.w - w, self.h),
        )
    }

    /// Split off an `h`-pixel-tall strip from the top edge. Returns
    /// `(strip, remainder)`.
    pub fn split_top(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (
            UiRect::new(self.x, self.y, self.w, h),
            UiRect::new(self.x, self.y + h, self.w, self.h - h),
        )
    }

    /// Split off an `h`-pixel-tall strip from the bottom edge. Returns
    /// `(strip, remainder)` — the remainder is what's left on TOP, matching
    /// `split_right`'s convention.
    pub fn split_bottom(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (
            UiRect::new(self.x, self.bottom() - h, self.w, h),
            UiRect::new(self.x, self.y, self.w, self.h - h),
        )
    }
}

/// `UiRect` is this crate's own pixel-rect type; `ember2d_sim::math::Rect`
/// is what `DrawSurface`'s pixel-native methods (`fill_rect_px`,
/// `draw_nine_slice_px`, `draw_text_row`) take, since they're defined at
/// the `ember2d`/`ember2d_sim` level and know nothing about `ui::UiRect`.
/// Identical `{x, y, w, h}` shape — this is the one place that fact is
/// load-bearing, so a panel's own `content_rect()` (`UiRect`) can feed
/// straight into a themed draw call with `.into()` instead of every call
/// site re-typing the same four-field copy (docs/ember2d-master-plan.md
/// §5.4, the `UiRect::from_cells` removal).
impl From<UiRect> for ember2d_sim::math::Rect {
    fn from(r: UiRect) -> Self {
        ember2d_sim::math::Rect::new(r.x, r.y, r.w, r.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            (tiny.w, tiny.h),
            (0.0, 0.0),
            "an inset larger than the rect must clamp to zero, not go negative"
        );
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
