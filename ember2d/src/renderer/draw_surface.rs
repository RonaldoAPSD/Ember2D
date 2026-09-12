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

use super::{Color, Renderer, CELL_H, CELL_W};

pub trait DrawSurface {
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
