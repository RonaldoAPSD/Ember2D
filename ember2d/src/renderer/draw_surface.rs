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
//
// 7D-3 (docs/ember2d-master-plan.md §5.4) additions: `set_scissor` and
// `draw_nine_slice_px` moved to `f32`/`border_scale` (see each's own doc
// comment); `draw_text_run`/`TextRun` replace `draw_text_px` as the
// REQUIRED method (`draw_text_px` is now a default method built on top —
// every existing call site keeps working unchanged); `draw_char_px` and
// `display_scale` are new, needed by `UiPainter`/`UiSpace`.

use super::{draw_log::DrawOp, Color, Font, Renderer, Texture, CELL_H, CELL_W};
use ember2d_sim::math::{Rect, Vec2};

/// A display's render (DPI-integer) scale and its raw, un-rounded OS scale
/// factor — see `Renderer.os_scale_factor`'s own doc comment for why both
/// are kept (`UiScaleChoice::Auto` needs the real DPI reading `render_scale`
/// already rounded away). `NullRenderer` defaults to `(1, 1.0)` and can be
/// overridden via `with_display` for a test that needs a specific R or a
/// specific "OS" scale factor to test `Auto` resolution against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayScale {
    pub render_scale: u32,
    pub os_scale_factor: f32,
}

/// One text-drawing request (7D-3, docs/ember2d-master-plan.md §5.4) — see
/// `draw_text_run`'s own doc comment for the full contract. `raster_px` is
/// the size to actually rasterize/measure the font at (for chrome, this is
/// already `points * ui_scale`, i.e. physical pixels — `UiPainter` computes
/// it, never this type); `texel_scale` is how many LOGICAL pixels one atlas
/// texel (one unit of `raster_px`) occupies when drawn (`1.0` for every
/// pre-7D-3 caller, `ui_scale / render_scale` for chrome text drawn through
/// `UiPainter`); `pitch`, if set, overrides the font's own per-glyph advance
/// with a fixed logical-pixel step (the script editor's monospace grid).
pub struct TextRun<'a> {
    pub text: &'a str,
    pub origin: Vec2,
    pub raster_px: f32,
    pub texel_scale: f32,
    pub pitch: Option<f32>,
    pub color: Color,
}

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
    /// one id every frame). `border_scale` (7D-3, docs/ember2d-master-plan.md
    /// §5.4) — see `Renderer::draw_nine_slice`'s own doc comment; `1.0`
    /// reproduces every pre-7D-3 caller's behavior.
    #[allow(clippy::too_many_arguments)]
    fn draw_nine_slice_px(
        &mut self,
        dest: Rect,
        texture: &Texture,
        src: Rect,
        border: (f32, f32, f32, f32),
        border_scale: f32,
        tint: Color,
    );
    /// Step 8-2 (docs/ember2d-master-plan.md §5.7): draw `texture` (or its
    /// `src` texel sub-rect) stretched to fill `dest`, logical pixels — the
    /// plain-blit sibling of `draw_nine_slice_px` above, which the editor
    /// had no way to ask for until it needed to draw sprite tiles and
    /// sprite thumbnails. `Renderer` already had the method itself
    /// (`Renderer::draw_texture_px`, what 9-slicing is built on); this
    /// just puts it on the trait the editor draws through.
    fn draw_texture_px(&mut self, dest: Rect, texture: &Texture, src: Option<Rect>, tint: Color);
    /// Draw one text run (7D-3, docs/ember2d-master-plan.md §5.4 — replaces
    /// the old `draw_text_px` as the REQUIRED method; `draw_text_px` below
    /// is now a default built on this). Rasterizes/looks up each glyph at
    /// `run.raster_px`, places it at `run.origin` plus that glyph's own
    /// offset scaled by `run.texel_scale`, and advances the pen by either
    /// `run.pitch` (if set) or the glyph's own advance times
    /// `run.texel_scale`. Returns the total horizontal advance, in LOGICAL
    /// pixels (matching `run.origin`'s own unit) — a caller in points
    /// (`UiPainter::text`) converts that back down itself.
    fn draw_text_run(&mut self, font: &mut dyn Font, run: &TextRun) -> f32;
    /// The theme-chrome twin of `draw_str` — draws `text` through an
    /// arbitrary caller-owned `Font` at a real pixel position/size,
    /// baseline-positioned (see `Renderer::draw_text_run`'s own doc
    /// comment). A default method (7D-3) built on `draw_text_run` at
    /// `texel_scale: 1.0, pitch: None` — reproduces every pre-7D-3 call
    /// site's exact behavior without each implementor repeating the same
    /// wrapper.
    fn draw_text_px(
        &mut self,
        font: &mut dyn Font,
        text: &str,
        pos: Vec2,
        px: f32,
        color: Color,
    ) -> f32 {
        self.draw_text_run(
            font,
            &TextRun { text, origin: pos, raster_px: px, texel_scale: 1.0, pitch: None, color },
        )
    }
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
    /// `draw_char_scaled_pixels` with a scale per axis (Step 9-5,
    /// docs/ember2d-master-plan.md §5.8) — the editor canvas draws a glyph
    /// stretched to the project's world cell with it.
    #[allow(clippy::too_many_arguments)]
    fn draw_char_sized_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        size: [f32; 2],
    );
    /// The `f32`-position twin of `draw_char_scaled_pixels` (7D-3,
    /// docs/ember2d-master-plan.md §5.4) — see `Renderer::draw_char_px`'s
    /// own doc comment for why the two coexist.
    fn draw_char_px(&mut self, pos: Vec2, ch: char, fg: Color, bg: Color, scale: f32);
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
    /// `f32` logical pixels (7D-3, docs/ember2d-master-plan.md §5.4, was
    /// `Option<(u32, u32, u32, u32)>`) — see `Renderer::set_scissor`'s own
    /// doc comment for why.
    fn set_scissor(&mut self, rect: Option<Rect>);
    /// This surface's render/OS display scale (7D-3, docs/ember2d-master-plan.md
    /// §5.4) — what `UiSpace::from_surface` reads to build a points-space
    /// conversion. `Renderer`'s real DPI-derived value; `NullRenderer`
    /// defaults to `(1, 1.0)`, overridable via `with_display` for a test
    /// exercising a specific render/OS scale.
    fn display_scale(&self) -> DisplayScale;
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
    fn draw_nine_slice_px(
        &mut self,
        dest: Rect,
        texture: &Texture,
        src: Rect,
        border: (f32, f32, f32, f32),
        border_scale: f32,
        tint: Color,
    ) {
        Renderer::draw_nine_slice(self, dest, texture, src, border, border_scale, tint);
    }
    fn draw_texture_px(&mut self, dest: Rect, texture: &Texture, src: Option<Rect>, tint: Color) {
        Renderer::draw_texture_px(self, dest, texture, src, tint);
    }
    fn draw_text_run(&mut self, font: &mut dyn Font, run: &TextRun) -> f32 {
        Renderer::draw_text_run(self, font, run)
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
    #[allow(clippy::too_many_arguments)]
    fn draw_char_sized_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        size: [f32; 2],
    ) {
        Renderer::draw_char_sized_pixels(self, px, py, ch, fg, bg, size);
    }
    fn draw_char_px(&mut self, pos: Vec2, ch: char, fg: Color, bg: Color, scale: f32) {
        Renderer::draw_char_px(self, pos, ch, fg, bg, scale);
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
    fn set_scissor(&mut self, rect: Option<Rect>) {
        Renderer::set_scissor(self, rect);
    }
    fn display_scale(&self) -> DisplayScale {
        Renderer::display_scale(self)
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
///
/// 7D-3 (docs/ember2d-master-plan.md §5.4): gained an opt-in draw-op log
/// (`start_recording`/`ops`/`clear_ops`) and a settable `DisplayScale` —
/// neither is used by any pre-7D-3 test, both default to their old
/// behavior (`recording: false`, `display: (1, 1.0)`) so no existing
/// caller's assertions change.
pub struct NullRenderer {
    width: usize,
    height: usize,
    pixel_width: usize,
    pixel_height: usize,
    display: DisplayScale,
    recording: bool,
    ops: Vec<DrawOp>,
}

impl NullRenderer {
    pub fn new(pixel_width: usize, pixel_height: usize) -> Self {
        NullRenderer {
            width: pixel_width / CELL_W,
            height: pixel_height / CELL_H,
            pixel_width,
            pixel_height,
            display: DisplayScale { render_scale: 1, os_scale_factor: 1.0 },
            recording: false,
            ops: Vec::new(),
        }
    }

    /// As `new`, but with an explicit `DisplayScale` — for a test exercising
    /// a specific render scale (R) or OS scale factor (for
    /// `UiScaleChoice::Auto` resolution) rather than the `(1, 1.0)` default.
    pub fn with_display(pixel_width: usize, pixel_height: usize, display: DisplayScale) -> Self {
        NullRenderer { display, ..NullRenderer::new(pixel_width, pixel_height) }
    }

    /// Start (or resume) recording every draw call as a `DrawOp` — see
    /// `draw_log::DrawOp`'s own header comment for why. Ops from before
    /// this call are not retroactively captured.
    pub fn start_recording(&mut self) {
        self.recording = true;
    }

    pub fn ops(&self) -> &[DrawOp] {
        &self.ops
    }

    pub fn clear_ops(&mut self) {
        self.ops.clear();
    }
}

impl DrawSurface for NullRenderer {
    fn draw_nine_slice_px(
        &mut self,
        dest: Rect,
        _texture: &Texture,
        src: Rect,
        border: (f32, f32, f32, f32),
        border_scale: f32,
        _tint: Color,
    ) {
        if self.recording {
            self.ops.push(DrawOp::NineSlice { dest, src, border, border_scale });
        }
    }
    fn draw_texture_px(&mut self, dest: Rect, texture: &Texture, src: Option<Rect>, _tint: Color) {
        if self.recording {
            self.ops.push(DrawOp::Texture { dest, src, texture: texture.id });
        }
    }
    fn draw_text_run(&mut self, font: &mut dyn Font, run: &TextRun) -> f32 {
        // No-op drawing, but a REAL measured advance — headless tests that
        // center title text against this return value (7D-2) need the same
        // number a real render would produce, not a dummy `0.0`.
        let (measured_w, _) = font.measure(run.text, run.raster_px);
        let advance = match run.pitch {
            Some(pitch) => pitch * run.text.chars().count() as f32,
            None => measured_w * run.texel_scale,
        };
        if self.recording {
            self.ops.push(DrawOp::Text {
                text: run.text.to_string(),
                origin: run.origin,
                raster_px: run.raster_px,
                texel_scale: run.texel_scale,
                pitch: run.pitch,
            });
        }
        advance
    }
    fn fill_rect_px(&mut self, rect: Rect, _color: Color) {
        if self.recording {
            self.ops.push(DrawOp::Fill(rect));
        }
    }
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
    #[allow(clippy::too_many_arguments)]
    fn draw_char_sized_pixels(
        &mut self,
        _px: i32,
        _py: i32,
        _ch: char,
        _fg: Color,
        _bg: Color,
        _size: [f32; 2],
    ) {
    }
    fn draw_char_px(&mut self, pos: Vec2, ch: char, _fg: Color, _bg: Color, scale: f32) {
        if self.recording {
            self.ops.push(DrawOp::Char { pos, ch, scale });
        }
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
    fn set_scissor(&mut self, rect: Option<Rect>) {
        if self.recording {
            self.ops.push(DrawOp::Scissor(rect));
        }
    }
    fn display_scale(&self) -> DisplayScale {
        self.display
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
