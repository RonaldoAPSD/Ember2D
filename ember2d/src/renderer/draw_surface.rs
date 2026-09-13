// renderer/draw_surface.rs — the headless-testing seam 7C-5
// (docs/ember2d-master-plan.md §5.3) needed: every editor `draw_*`
// function used to take a concrete `&mut Renderer`, which requires a real
// wgpu `Surface`/`Device`/`Window` (`Renderer::new`, `renderer/mod.rs`) —
// there is no way to construct one headlessly, so there was no way to
// drive the editor's drawing (and, via `UiFrame::push` inside those same
// calls, its 7C-1 hit-testing) from a test at all.
//
// `DrawSurface` is the narrow trait that closes that gap: exactly the
// `Renderer` methods/fields the editor's `draw_*` functions actually call
// (verified by grep across `ember2d-editor/src`, not guessed), so
// `NullRenderer` below can stand in for a real `Renderer` without ever
// touching wgpu/winit. This is deliberately NOT a revival of the
// `RenderBackend` trait 7B-3 removed (see `backend.rs`'s own comment on
// that decision) — that one abstracted over multiple GPU backends behind
// the same live window, which had no real second implementor; this one
// abstracts over "is there a real window at all," which a headless test
// genuinely needs. `Renderer` keeps every one of these as a normal
// inherent method/field, unchanged — this trait only adds a second,
// test-only implementor.

use super::{Color, Font, Renderer, Texture, CELL_H, CELL_W};
use ember2d_sim::math::{Rect, Vec2};

pub trait DrawSurface {
    /// 7D-2 (docs/ember2d-master-plan.md §5.4): the theme-chrome twin of
    /// `draw_rect_filled` — draws `texture` as a 9-slice into `dest`,
    /// corners/edges unstretched by `border`. `src` is this 9-slice's own
    /// sub-rect of `texture` — a theme's chrome atlas packs every
    /// `SliceRole` into ONE shared texture (`NineSlice::src`, 7D-1), so
    /// this can't assume the whole texture is the 9-slice the way
    /// `Renderer::draw_nine_slice`'s very first caller did. Takes an
    /// already-resolved `&Texture`, not a `TextureId`/`AssetManager`
    /// lookup, so this trait stays exactly as narrow as
    /// `draw_char`/`draw_rect_filled` above — resolving a theme's
    /// `chrome: TextureId` into pixels is the caller's job (`EditorState`
    /// owns its own resolved copy; see its own doc comment on why that's
    /// simpler than keeping an `AssetManager` alive just to re-resolve
    /// one id every frame).
    fn draw_nine_slice_px(&mut self, dest: Rect, texture: &Texture, src: Rect, border: (f32, f32, f32, f32), tint: Color);
    /// The theme-chrome twin of `draw_str` — draws `text` through an
    /// arbitrary caller-owned `Font` (a theme's own loaded Cascadia
    /// instance, not `Renderer::ui_font`) at a real pixel position/size,
    /// baseline-positioned like `Renderer::draw_text_px` itself (see that
    /// method's own doc comment). Returns the horizontal advance, matching
    /// `Renderer::draw_text_px` — `NullRenderer`'s own impl still computes
    /// this via `Font::measure` rather than returning a dummy `0.0`, since
    /// title-centering math that runs headlessly (7C-5 tests) needs a real
    /// width to center against, not just the side effect of drawing.
    fn draw_text_px(&mut self, font: &mut dyn Font, text: &str, pos: Vec2, px: f32, color: Color) -> f32;
    /// The pixel-space twin of `draw_rect_filled` (docs/ember2d-master-plan.md
    /// §5.4, the `UiRect::from_cells` removal) — a solid color fill at an
    /// arbitrary pixel rect, not snapped to the character-cell grid.
    /// Mirrors `Renderer::fill_rect_px` exactly (backed by the 1×1 white
    /// texture, same instanced path glyphs/sprites use). Panel content
    /// converting off `from_cells` uses this for a text row's background —
    /// `draw_text_px` only ever draws the glyphs themselves, no fill.
    fn fill_rect_px(&mut self, rect: Rect, color: Color);
    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color);
    /// Same argument count as `Renderer::draw_char_scaled_pixels` (which
    /// this mirrors) already carries unsuppressed at the `v0.5.7b`
    /// baseline (`renderer/mod.rs`) — allowed here too so this trait
    /// declaration doesn't add a second, redundant warning for the exact
    /// same shape.
    #[allow(clippy::too_many_arguments)]
    fn draw_char_scaled_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        scale: f32,
    );
    fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color);
    fn draw_rect_outline(&mut self, x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color);
    /// Same argument count as `Renderer::draw_rect_filled` (mirrored here)
    /// already carries unsuppressed at the `v0.5.7b` baseline —
    /// see `draw_char_scaled_pixels`'s own comment above.
    #[allow(clippy::too_many_arguments)]
    fn draw_rect_filled(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ch: char,
        fg: Color,
        bg: Color,
    );
    fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>);
    /// Screen size in cells — a plain accessor since `Renderer::width`/
    /// `height` are public fields, not methods, and a trait can't expose a
    /// field. The `renderer.width()` call-site form only appears where a
    /// draw function needed to become generic over this trait; every
    /// concrete, `ember2d`-internal use of `Renderer::width` (the field)
    /// is untouched.
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn pixel_width(&self) -> usize;
    fn pixel_height(&self) -> usize;
}

impl DrawSurface for Renderer {
    fn draw_nine_slice_px(&mut self, dest: Rect, texture: &Texture, src: Rect, border: (f32, f32, f32, f32), tint: Color) {
        Renderer::draw_nine_slice(self, dest, texture, src, border, tint);
    }
    fn draw_text_px(&mut self, font: &mut dyn Font, text: &str, pos: Vec2, px: f32, color: Color) -> f32 {
        Renderer::draw_text_px(self, font, text, pos, px, color)
    }
    fn fill_rect_px(&mut self, rect: Rect, color: Color) {
        Renderer::fill_rect_px(self, rect, color);
    }
    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        Renderer::draw_char(self, x, y, ch, fg, bg);
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_char_scaled_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        scale: f32,
    ) {
        Renderer::draw_char_scaled_pixels(self, px, py, ch, fg, bg, scale);
    }
    fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        Renderer::draw_str(self, x, y, s, fg, bg);
    }
    fn draw_rect_outline(&mut self, x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color) {
        Renderer::draw_rect_outline(self, x, y, w, h, fg, bg);
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_rect_filled(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ch: char,
        fg: Color,
        bg: Color,
    ) {
        Renderer::draw_rect_filled(self, x, y, w, h, ch, fg, bg);
    }
    fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>) {
        Renderer::set_scissor(self, rect);
    }
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn pixel_width(&self) -> usize {
        self.pixel_width
    }
    fn pixel_height(&self) -> usize {
        self.pixel_height
    }
}

/// A `DrawSurface` with no window, no GPU, and no side effects — every draw
/// call is a no-op. Exists purely so a headless test can call the editor's
/// real `draw_*` functions (which register `UiFrame` hits as a side effect
/// of drawing, 7C-1) without a live wgpu surface. Screen size is fixed at
/// construction, mirroring `Renderer`'s own `width`/`height`/`pixel_width`/
/// `pixel_height` relationship (`renderer/mod.rs`'s `compute_layout`:
/// `pixel_* == *_in_cells * CELL_*`) rather than letting the two drift
/// independently.
pub struct NullRenderer {
    width: usize,
    height: usize,
    pixel_width: usize,
    pixel_height: usize,
}

impl NullRenderer {
    pub fn new(pixel_width: usize, pixel_height: usize) -> Self {
        NullRenderer {
            width: pixel_width / CELL_W,
            height: pixel_height / CELL_H,
            pixel_width,
            pixel_height,
        }
    }
}

impl DrawSurface for NullRenderer {
    fn draw_nine_slice_px(&mut self, _dest: Rect, _texture: &Texture, _src: Rect, _border: (f32, f32, f32, f32), _tint: Color) {}
    fn draw_text_px(&mut self, font: &mut dyn Font, text: &str, _pos: Vec2, px: f32, _color: Color) -> f32 {
        // No-op drawing, but a REAL measured width — headless tests that
        // center title text against this return value (7D-2) need the
        // same number a real render would produce, not a dummy `0.0`.
        font.measure(text, px).0
    }
    fn fill_rect_px(&mut self, _rect: Rect, _color: Color) {}
    fn draw_char(&mut self, _x: usize, _y: usize, _ch: char, _fg: Color, _bg: Color) {}
    #[allow(clippy::too_many_arguments)]
    fn draw_char_scaled_pixels(
        &mut self,
        _px: i32,
        _py: i32,
        _ch: char,
        _fg: Color,
        _bg: Color,
        _scale: f32,
    ) {
    }
    fn draw_str(&mut self, _x: usize, _y: usize, _s: &str, _fg: Color, _bg: Color) {}
    fn draw_rect_outline(
        &mut self,
        _x: usize,
        _y: usize,
        _w: usize,
        _h: usize,
        _fg: Color,
        _bg: Color,
    ) {
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_rect_filled(
        &mut self,
        _x: usize,
        _y: usize,
        _w: usize,
        _h: usize,
        _ch: char,
        _fg: Color,
        _bg: Color,
    ) {
    }
    fn set_scissor(&mut self, _rect: Option<(u32, u32, u32, u32)>) {}
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn pixel_width(&self) -> usize {
        self.pixel_width
    }
    fn pixel_height(&self) -> usize {
        self.pixel_height
    }
}
