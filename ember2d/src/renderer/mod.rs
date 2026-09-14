// renderer/mod.rs — The Wgpu-backed renderer.

pub mod assets;
pub mod backend;
pub mod color;
mod draw_surface;
pub mod font;
pub mod texture;

// geometry.rs (7D-3, docs/ember2d-master-plan.md §5.4): the coordinate-space
// free functions (`ScreenMapping`, `compute_layout`, `nine_slice_quads`,
// etc.) that used to live at the bottom of this file — see that file's own
// header comment for why they moved. `ScreenMapping` is re-exported below
// (`mouse.rs` and the editor's test harness construct one directly); the
// rest stay crate-internal, reached through the plain `use` just below.
mod geometry;
pub use geometry::ScreenMapping;
use geometry::{
    compute_layout, nine_slice_quads, pixel_size_to_cells, screen_cell_to_pixel,
    snap_rect_to_scale, uv_rect_for,
};

// ui_space.rs/ui_painter.rs (7D-3, docs/ember2d-master-plan.md §5.4): the
// points<->logical<->physical coordinate-space conversion and the
// theme-agnostic drawing choke point built on top of `DrawSurface` — see
// each file's own header comment. Public: the editor (a different crate)
// builds a `UiSpace` every frame and draws every chrome pixel through a
// `UiPainter`, and Phase 9-3's planned script-facing pixel HUD is meant to
// reuse both unchanged.
pub mod ui_painter;
pub mod ui_space;

// draw_log.rs (7D-3, docs/ember2d-master-plan.md §5.4): `NullRenderer`'s
// opt-in draw-op recording — see that file's own header comment for why a
// headless test needs it to verify physical-pixel snapping without a GPU.
pub mod draw_log;

#[path = "text.rs"]
mod text;

use std::io;
use std::sync::Arc;
use winit::window::Window;

pub use assets::AssetManager;
pub use backend::WgpuBackend;
pub use color::{Color, DEFAULT_BG, DEFAULT_FG};
pub use draw_surface::{DisplayScale, DrawSurface, NullRenderer, TextRun};
pub use font::{
    glyph_atlas_side_for, ui_font_from_env, BitmapFont, Font, GlyphInfo, TtfFont, UiFontKind,
};
pub use texture::{Texture, TextureId};
pub use ui_painter::UiPainter;
pub use ui_space::UiSpace;

// `pub` since Phase 7 Part 1e (docs/ember2d-phase7-plan.md, E2) — the
// editor previously re-derived this exact pixel size as duplicated local
// magic-number literals (`8.0`/`16.0`) in three different files instead of
// importing it from here. This is the one allowed additive change to this
// crate for that step.
pub const CELL_W: usize = 8;
pub const CELL_H: usize = 16;

/// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): the fallback initial guess
/// for `WindowInit`'s requested window size (`engine.rs`), used only before
/// a window — and therefore a real DPI reading — exists. Every other use of
/// "how many physical pixels per cell" goes through `Renderer::scale`
/// (DPI-derived, integer, re-read on `new`/resize/`ScaleFactorChanged`) —
/// see that field's own doc comment for why a fixed constant was the R21
/// bug, not the fix.
pub(crate) const INITIAL_SCALE_GUESS: f32 = 2.0;

/// 7B-2 follow-up (found live testing this phase, docs/ember2d-master-plan.md
/// §5.2): the floor `Renderer::scale` is clamped to. On a standard
/// 100%-scale display, `window.scale_factor()` is `1.0`, which used to mean
/// cells rendered at their literal native `CELL_W`×`CELL_H` (8×16) physical
/// pixels — correct per R21's "land on a whole physical pixel" fix, but
/// tiny and hard to read on any modern display, since the pre-7B-2 code
/// had *always* rendered at a fixed 2x (`16×32`) regardless of real DPI.
/// Flooring at `2.0` restores that comfortable default for the common
/// (non-HiDPI) case while still scaling *up* correctly above it on a
/// genuinely HiDPI display (150%/200%+ rounds to 2 or higher already, so
/// this floor only changes anything at 100%/125%, which both used to round
/// to 1). A real user-facing UI scale *setting* — choosing a value other
/// than "whatever this floor computes" — is 7D-3 (Phase 7D), not this.
pub(crate) const MIN_UI_SCALE: f32 = 2.0;

/// R29 (7B-1, docs/ember2d-master-plan.md §5.2): shows a native error
/// dialog then exits — the two `Renderer::new` call sites that used to be
/// bare `.expect()` panics on unsupported hardware. A panic's message goes
/// to stderr, which a user launching the built exe directly (no attached
/// console — the common case) never sees; this makes the failure visible
/// before the process disappears.
fn fatal_gpu_error(message: &str) -> ! {
    rfd::MessageDialog::new()
        .set_title("Ember2D — Graphics Error")
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
    std::process::exit(1);
}

pub struct Renderer {
    window: Arc<Window>,
    pub width: usize,
    pub height: usize,
    pub pixel_width: usize,
    pub pixel_height: usize,

    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): our own integer
    /// render scale — physical pixels per un-scaled `CELL_W`/`CELL_H`
    /// pixel. DPI-derived (`window.scale_factor().round().max(MIN_UI_SCALE)`),
    /// not a fixed constant, so cells always land on a whole physical pixel
    /// regardless of the display's actual scale factor; re-read on
    /// `new`/resize/`ScaleFactorChanged` via `recompute_layout`. See
    /// `MIN_UI_SCALE`'s own doc comment for why the floor isn't `1.0`.
    scale: f32,
    /// The window's raw, un-rounded `scale_factor()` (7D-3,
    /// docs/ember2d-master-plan.md §5.4) — `scale` above is `this.round()`
    /// floored at `MIN_UI_SCALE`, deliberately lossy so the render/cell
    /// grid always lands on a whole physical pixel; `os_scale_factor` keeps
    /// the real value around too, since `UiScaleChoice::Auto` (the
    /// editor's UI-scale preference, `ember2d-editor/src/editor/prefs.rs`)
    /// needs the display's actual DPI reading, not `scale`'s already-
    /// rounded-and-floored one, to derive a sensible default. Read via
    /// `display_scale()` (`DrawSurface`); kept in sync with `scale` by
    /// `recompute_layout`.
    os_scale_factor: f32,
    /// The current physical<->cell-space mapping — see `ScreenMapping`'s
    /// own doc comment. Kept in sync with `width`/`height`/`scale` by
    /// `recompute_layout`; never computed anywhere else.
    mapping: ScreenMapping,

    // WGPU core objects
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    backend: WgpuBackend,

    /// A 1×1 opaque white texture, built once here rather than fetched
    /// through `AssetManager` (Phase 7 Part 1a, docs/ember2d-phase7-plan.md)
    /// — `fill_rect_px` needs one unconditionally and `Renderer` has no
    /// `AssetManager` reference to ask (that lives on `Engine`/`RenderContext`
    /// separately), so owning a tiny built-in copy here avoids threading one
    /// through every pixel-primitive call site. `Texture` is cheap to clone
    /// (a 1-element `Vec<u32>`), so callers get an owned copy without a
    /// second `NEXT_ID` allocation.
    white_texture: Texture,

    /// This process's active UI font, the pixel size `draw_str` renders it
    /// at, and which of the two it is — see `ui_font_from_env`'s own doc
    /// comment (7B-5, docs/ember2d-master-plan.md §5.2). `Box<dyn Font>`
    /// rather than a concrete type for the same reason `EditorState::font`
    /// already is (mod.rs, Phase 7 Part 2c): a caller never branches on
    /// which implementation it got — except `draw_str` itself, which reads
    /// `ui_font_kind` (not by downcasting `ui_font`) to pick its draw path;
    /// see that method's own doc comment for why.
    ui_font: Box<dyn Font>,
    ui_font_px: f32,
    ui_font_kind: UiFontKind,

    /// Chrome textures (theme atlases) loaded via `theme::Theme::load`
    /// (7D-1, master plan §5.4) — deliberately separate from the game's own
    /// `AssetManager` (which lives on `Engine`/`RenderContext`, cleared on
    /// every project switch): a theme's chrome atlas is an editor-wide
    /// resource, not a per-project asset, and must survive exactly the
    /// clears that evict the game's own textures.
    pub ui_assets: AssetManager,
}

impl Renderer {
    /// 7B-1 (docs/ember2d-master-plan.md §5.2): takes an already-created
    /// `Window` instead of a `title`/`&EventLoop` pair and building one
    /// itself — winit 0.30's `ApplicationHandler` split removed
    /// `WindowBuilder`/direct-from-`EventLoop` window creation entirely; a
    /// window can only be created via `ActiveEventLoop::create_window`
    /// inside a `resumed()` callback. `Engine::new` (`engine.rs`) does that
    /// creation (title, size, `INITIAL_SCALE_GUESS` all applied there now)
    /// and hands the result in here, so this constructor's own job —
    /// everything from wgpu initialization down — is unchanged.
    ///
    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): no longer takes
    /// `width`/`height` — the grid size a caller *requested* (what
    /// `WindowInit` sized the window to ask for) isn't necessarily what
    /// the window manager or the display's real DPI actually produced.
    /// Cell width/height/`scale`/`ScreenMapping` are derived from the
    /// window's own real `inner_size()`/`scale_factor()` instead, via
    /// `recompute_layout` — the same source of truth `try_handle_resize`
    /// already used for every resize after the first; this just applies
    /// it at construction too instead of trusting a caller's guess.
    /// `Engine::new` reads `renderer.width`/`height` back afterward for
    /// its own copies rather than the `width`/`height` it originally
    /// passed to `WindowInit`.
    pub fn new(window: Arc<Window>) -> io::Result<Self> {
        // ── WGPU Initialization ───────────────────────────────────────────

        // 7B-1: InstanceDescriptor no longer implements Default (wgpu 30) —
        // new_without_display_handle() is the documented equivalent (we
        // don't need a platform display handle; that's only for GLES on
        // Wayland, and this project doesn't ship a GLES backend).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        // wgpu 0.19+ accepts Arc<Window> as SurfaceTarget
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // R29 (7B-1): both `.expect()`s below used to panic with no
        // user-facing message on unsupported hardware — nothing a user
        // launching the exe directly (no attached console) would ever see.
        // `fatal_gpu_error` shows a native dialog first.
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                // Limit-bucketing mitigates GPU fingerprinting for untrusted
                // content (e.g. a browser embedding wgpu) — not applicable to
                // a native desktop app that owns its own process.
                apply_limit_buckets: false,
            })) {
                Ok(a) => a,
                Err(e) => {
                    fatal_gpu_error(&format!("No compatible graphics adapter was found.\n\n{e}"))
                }
            };

        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })) {
                Ok(dq) => dq,
                Err(e) => fatal_gpu_error(&format!("Failed to create a graphics device.\n\n{e}")),
            };

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb()) // Prefer non-SRGB for linear behavior
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            // New in wgpu 30 — `Auto` matches this project's pre-30
            // behavior (whatever color space the surface's own format
            // implies) exactly, so this is not a visible change.
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): scale/width/height/
        // mapping are all derived from the real window here, not trusted
        // from a caller — see this function's own doc comment. Floors at
        // `MIN_UI_SCALE`, not `1.0` — see that constant's own doc comment.
        let os_scale_factor = window.scale_factor() as f32;
        let scale = os_scale_factor.round().max(MIN_UI_SCALE);
        let (width, height, mapping) = compute_layout(size.width, size.height, scale);
        let pixel_width = width * CELL_W;
        let pixel_height = height * CELL_H;

        let backend = WgpuBackend::new(width, height, &device, &queue, surface_format);
        let (ui_font, ui_font_px, ui_font_kind) = ui_font_from_env();

        Ok(Renderer {
            window,
            width,
            height,
            pixel_width,
            pixel_height,
            scale,
            os_scale_factor,
            mapping,
            surface,
            device,
            queue,
            config,
            backend,
            white_texture: Texture::solid(0xFFFFFFFF),
            ui_font,
            ui_font_px,
            ui_font_kind,
            ui_assets: AssetManager::new(),
        })
    }

    pub fn backend_name(&self) -> &str {
        self.backend.name()
    }

    /// Toggles between ASCII and 2D Sprite rendering modes (if supported by backend).
    pub fn set_sprite_mode(&mut self, enabled: bool) {
        self.backend.set_sprite_mode(enabled);
    }

    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): replaces the old
    /// single-axis, DPI-blind `scale_factor()` (width-only ratio between
    /// physical size and `pixel_width`) — see `ScreenMapping`'s own doc
    /// comment for what this carries instead and why.
    pub fn screen_mapping(&self) -> ScreenMapping {
        self.mapping
    }

    /// `DrawSurface::display_scale` — see that trait method's own doc
    /// comment (7D-3, docs/ember2d-master-plan.md §5.4).
    pub fn display_scale(&self) -> DisplayScale {
        // `self.scale` is already a whole number (`.round().max(MIN_UI_SCALE)`
        // at construction/`recompute_layout`) — no further rounding needed.
        DisplayScale { render_scale: self.scale as u32, os_scale_factor: self.os_scale_factor }
    }

    pub fn clear(&mut self) {
        self.backend.clear();
    }

    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.backend.draw_char(x, y, ch, fg, bg);
    }

    pub fn draw_char_scaled_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        scale: f32,
    ) {
        self.backend.draw_char_scaled_pixels(px as f32, py as f32, ch, fg, bg, scale);
    }

    /// The `f32`-position twin of `draw_char_scaled_pixels` (7D-3,
    /// docs/ember2d-master-plan.md §5.4) — `draw_char_scaled_pixels` keeps
    /// its `i32` signature for its one existing caller (`draw_char_world`,
    /// which already rounds to a whole logical pixel via
    /// `screen_cell_to_pixel`), but the UI-points painter's tile-glyph
    /// preview (`ui_painter::UiPainter::tile_glyph`) needs a real, possibly
    /// fractional position — a chrome glyph preview drawn at a
    /// physical-pixel-snapped points position, not a cell-quantized one.
    pub fn draw_char_px(
        &mut self,
        pos: ember2d_sim::math::Vec2,
        ch: char,
        fg: Color,
        bg: Color,
        scale: f32,
    ) {
        self.backend.draw_char_scaled_pixels(pos.x, pos.y, ch, fg, bg, scale);
    }

    pub fn upload_texture(&mut self, texture: &Texture) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
    }

    /// R27 (7B-3, docs/ember2d-master-plan.md §5.2): whether `id` is
    /// already GPU-resident — see `draw_text_px`'s own doc comment for why
    /// this matters.
    pub fn has_texture(&self, id: u64) -> bool {
        self.backend.has_texture(id)
    }

    /// `rect` is in logical pixels, `f32` (7D-3, docs/ember2d-master-plan.md
    /// §5.4 — was `Option<(u32, u32, u32, u32)>`): a panel's own pixel rect
    /// (`UiRect`) is `f32` and not necessarily a whole logical pixel after a
    /// drag/resize, so rounding to `u32` here — before the backend's own
    /// physical-scale multiply — used to throw away up to a logical pixel of
    /// precision on top of whatever the caller passed in (part of R66's
    /// viewport-seam mismatch, §3 in the master plan). The backend still
    /// rounds to a whole PHYSICAL pixel once (`WgpuBackend::render`), which
    /// is the only rounding a scissor rect can ever need.
    pub fn set_scissor(&mut self, rect: Option<ember2d_sim::math::Rect>) {
        self.backend.set_scissor(rect);
    }

    pub fn draw_texture(&mut self, px: i32, py: i32, texture: &Texture, scale: f32) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        // Preserves the exact size/rotation/tint this always had before the
        // backend gained real per-axis size, rotation, and tint (Step 2c).
        let size = [
            texture.width as f32 * scale / CELL_W as f32,
            texture.height as f32 * scale / CELL_H as f32,
        ];
        self.backend.draw_texture(px as f32, py as f32, texture, size, 0.0, Color::White, None);
    }

    /// Draw a glyph at a world-space position, through `camera`. A thin
    /// wrapper over `draw_char_scaled_pixels` — `world_pos` is converted to
    /// screen cells via `camera.world_to_screen`, then to the same
    /// pixel-snapped convention `draw_char_scaled_pixels` already uses (the
    /// editor's zoomed viewport does the identical conversion by hand in
    /// `editor/ui/canvas.rs::grid_to_pixel`).
    pub fn draw_char_world(
        &mut self,
        camera: &crate::camera::Camera,
        world_pos: ember2d_sim::math::Vec2,
        ch: char,
        fg: Color,
        bg: Color,
    ) {
        let (px, py) = screen_cell_to_pixel(camera.world_to_screen(world_pos));
        self.draw_char_scaled_pixels(px, py, ch, fg, bg, camera.zoom);
    }

    /// Draw a texture at a world-space position, through `camera`. `size` is
    /// in world units — e.g. a 1.0×1.0 sprite occupies exactly one grid cell
    /// at zoom 1.0, the same footprint a glyph would. `world_pos` is the
    /// sprite's top-left corner before rotation (matching `Transform`'s
    /// position convention), and `rotation` is applied about its center
    /// (Step 2b's shader change). `src` is an optional pixel-space sub-rect
    /// of `texture` to sample (`SpriteSource::Texture::src`, Step 3b) —
    /// `None` samples the whole texture.
    pub fn draw_texture_world(
        &mut self,
        camera: &crate::camera::Camera,
        world_pos: ember2d_sim::math::Vec2,
        texture: &Texture,
        size: ember2d_sim::math::Vec2,
        rotation: f32,
        tint: Color,
        src: Option<ember2d_sim::math::Rect>,
    ) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        let (px, py) = screen_cell_to_pixel(camera.world_to_screen(world_pos));
        let cell_size = [size.x * camera.zoom, size.y * camera.zoom];
        let uv_rect = src.map(|r| {
            [
                r.x / texture.width as f32,
                r.y / texture.height as f32,
                r.w / texture.width as f32,
                r.h / texture.height as f32,
            ]
        });
        self.backend
            .draw_texture(px as f32, py as f32, texture, cell_size, rotation, tint, uv_rect);
    }

    /// Solid filled rectangle in pixels (Phase 7 Part 1a,
    /// docs/ember2d-phase7-plan.md). Backed by the 1×1 white texture, scaled
    /// to size and tinted — the same instanced path glyphs and sprites use,
    /// so it batches with them rather than forcing its own draw call.
    /// `rect` is in the same pixel-snapped convention as
    /// `draw_char_scaled_pixels`/`draw_texture` (pre-`SCALE` — physical
    /// pixels before the window's own display scaling).
    pub fn fill_rect_px(&mut self, rect: ember2d_sim::math::Rect, color: Color) {
        let white = self.white_texture.clone();
        self.draw_texture_px(rect, &white, None, color);
    }

    /// Texture sub-rect blit in pixels (Phase 7 Part 1a) — the primitive
    /// `draw_nine_slice` is built from. `dest` and `src` are both in
    /// pixels; `src` is an optional pixel-space sub-rect of `texture`
    /// (`None` samples the whole thing, matching `draw_texture_world`'s own
    /// `src` convention).
    ///
    /// `dest` is snapped to the PHYSICAL pixel grid (7D-3,
    /// docs/ember2d-master-plan.md §5.4), not just rounded to a whole
    /// logical pixel the way this used to (`dest.x.round()`) — a UI-points
    /// chrome fill or 9-slice piece can land at a fractional logical
    /// position whenever `ui_scale`/`render_scale` aren't in a whole-number
    /// ratio (e.g. 3 points per unscaled pixel on a 2x-DPI display is 1.5
    /// logical pixels), and rounding to the nearest LOGICAL pixel there
    /// would still leave the actual drawn quad off the real, physical pixel
    /// grid the GPU rasterizes to. Snapping both edges independently
    /// (`snap_rect_to_scale`) rather than origin-then-size is what keeps
    /// two quads sharing a logical edge (e.g. adjacent 9-slice pieces)
    /// landing on the same physical pixel instead of drifting apart by a
    /// pixel of rounding error and leaving a seam.
    pub fn draw_texture_px(
        &mut self,
        dest: ember2d_sim::math::Rect,
        texture: &Texture,
        src: Option<ember2d_sim::math::Rect>,
        tint: Color,
    ) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        let dest = snap_rect_to_scale(dest, self.scale);
        let size = pixel_size_to_cells(dest.w, dest.h);
        let uv_rect = src.map(|r| uv_rect_for(texture.width, texture.height, r));
        self.backend.draw_texture(dest.x, dest.y, texture, size, 0.0, tint, uv_rect);
    }

    /// Nine-slice: corners drawn 1:1, edges stretched along one axis,
    /// center stretched both ways (Phase 7 Part 1a). `border` is `(left,
    /// top, right, bottom)` — the inset in SOURCE pixels defining each
    /// corner's size; the same four values double as each corner's
    /// DESTINATION size too, which is what keeps a corner crisp (1:1)
    /// rather than stretched, as long as `dest` is at least as big as the
    /// combined borders. See `nine_slice_quads` for the actual per-quad
    /// math, pulled out as a free function so it's testable without a live
    /// GPU-backed `Renderer` — same reasoning as `screen_cell_to_pixel`.
    /// `src` is the 9-slice's own region within `texture`'s pixels — NOT
    /// necessarily the whole texture (7D-2, master plan §5.4: a theme's
    /// chrome atlas packs many named `SliceRole`s into one shared
    /// texture, each a `NineSlice { src, border }` sub-rect of it, the
    /// same way a sprite atlas already works via `draw_texture_px`'s own
    /// `src: Option<Rect>`). Pass `Rect::new(0.0, 0.0, texture.width as
    /// f32, texture.height as f32)` for a texture that's dedicated to
    /// exactly one 9-slice (this call's only shape before 7D-2).
    ///
    /// `border_scale` (7D-3, docs/ember2d-master-plan.md §5.4) — see
    /// `nine_slice_quads`'s own doc comment for the full contract. `1.0`
    /// reproduces every pre-7D-3 caller's behavior exactly.
    pub fn draw_nine_slice(
        &mut self,
        dest: ember2d_sim::math::Rect,
        texture: &Texture,
        src: ember2d_sim::math::Rect,
        border: (f32, f32, f32, f32),
        border_scale: f32,
        tint: Color,
    ) {
        for (d, s) in nine_slice_quads(dest, src, border, border_scale) {
            self.draw_texture_px(d, texture, Some(s), tint);
        }
    }

    pub fn draw_rect_outline(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        fg: Color,
        bg: Color,
    ) {
        if w < 2 || h < 2 {
            return;
        }
        for col in (x + 1)..(x + w - 1) {
            self.draw_char(col, y, '-', fg, bg);
            self.draw_char(col, y + h - 1, '-', fg, bg);
        }
        for row in (y + 1)..(y + h - 1) {
            self.draw_char(x, row, '|', fg, bg);
            self.draw_char(x + w - 1, row, '|', fg, bg);
        }
        self.draw_char(x, y, '+', fg, bg);
        self.draw_char(x + w - 1, y, '+', fg, bg);
        self.draw_char(x, y + h - 1, '+', fg, bg);
        self.draw_char(x + w - 1, y + h - 1, '+', fg, bg);
    }

    pub fn draw_rect_filled(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        ch: char,
        fg: Color,
        bg: Color,
    ) {
        for row in y..(y + h) {
            for col in x..(x + w) {
                self.draw_char(col, row, ch, fg, bg);
            }
        }
    }

    pub fn present(&mut self) -> io::Result<()> {
        // 7B-1 (docs/ember2d-master-plan.md §5.2): get_current_texture()
        // returns CurrentSurfaceTexture directly now, not
        // Result<SurfaceTexture, SurfaceError> — same branches as before
        // (reconfigure-and-retry-next-frame for Outdated/Lost, skip the
        // frame for a transient condition, surface a real error otherwise),
        // plus two new variants wgpu 30 added: Timeout and Occluded (the
        // window minimized or fully behind another) — both are exactly the
        // "skip this frame, try again later" case Outdated/Lost's
        // reconfigure branch already isn't (those need `configure()`
        // first; these don't, the surface itself is still fine).
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(io::Error::new(io::ErrorKind::Other, "wgpu surface validation error"));
            }
        };

        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): per-axis now (was a
        // single scalar from the old scale_factor()) — scissor rects still
        // arrive from panels in logical pixels and need both the scale
        // *and* the letterbox origin to land on the right physical pixels.
        let scale_x = self.mapping.cell_px.0 / CELL_W as f32;
        let scale_y = self.mapping.cell_px.1 / CELL_H as f32;
        self.backend.set_render_scale(scale_x, scale_y, self.mapping.origin_px);

        // The actual swapchain texture's own size — the authoritative
        // render-target dimensions wgpu will validate scissor rects against,
        // not `self.config`'s (which could in principle be one resize event
        // stale) or a value re-derived from cell counts (see the backend's
        // render() for why that rounds past the real surface and panics).
        let surface_size = output.texture.size();
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 7B-2: the drawable viewport — `cells * CELL * scale` physical
        // pixels, offset by the letterbox origin — is what actually
        // constrains drawing now, not "whatever the full surface happens
        // to be" (R21's stretching bug). `backend.render` clamps this to
        // the real surface size defensively, same reasoning as its
        // existing scissor clamp.
        let viewport_size = (
            self.width as f32 * self.mapping.cell_px.0,
            self.height as f32 * self.mapping.cell_px.1,
        );

        self.backend.render(
            &self.device,
            &self.queue,
            &view,
            surface_size.width,
            surface_size.height,
            self.mapping.origin_px,
            viewport_size,
        );

        // 7B-1: SurfaceTexture::present() replaced by Queue::present() in
        // wgpu 30.
        self.queue.present(output);

        Ok(())
    }

    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): shared by
    /// `try_handle_resize` and `handle_scale_factor_changed` — both need
    /// the exact same "re-derive everything from the window's current
    /// physical size and scale" logic, just triggered by different events.
    /// Returns whether the cell grid (`width`/`height`) actually changed —
    /// `mapping`/`pixel_width`/`pixel_height` can change (a DPI change at
    /// the same cell count still moves the letterbox origin) without the
    /// grid itself changing, which callers that only care about the grid
    /// (e.g. `Engine::width`/`height`) don't need to react to.
    fn recompute_layout(&mut self) -> bool {
        let size = self.window.inner_size();
        self.os_scale_factor = self.window.scale_factor() as f32;
        self.scale = self.os_scale_factor.round().max(MIN_UI_SCALE);
        let (new_w, new_h, mapping) = compute_layout(size.width, size.height, self.scale);
        self.mapping = mapping;
        self.pixel_width = new_w * CELL_W;
        self.pixel_height = new_h * CELL_H;

        if new_w == self.width && new_h == self.height {
            return false;
        }
        self.width = new_w;
        self.height = new_h;
        self.backend.resize(new_w, new_h);
        true
    }

    pub fn try_handle_resize(&mut self) -> bool {
        let size = self.window.inner_size();
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            return self.recompute_layout();
        }
        false
    }

    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): called from
    /// `Engine`'s `WindowEvent::ScaleFactorChanged` handler — e.g. the
    /// window moved to a monitor with a different DPI scale factor. Unlike
    /// `try_handle_resize`, the surface itself doesn't necessarily need
    /// reconfiguring (physical size may be unchanged), just the cell
    /// grid/mapping derived from it.
    pub fn handle_scale_factor_changed(&mut self) -> bool {
        self.recompute_layout()
    }
}

/// R26 (7B-3, docs/ember2d-master-plan.md §5.2): `Renderer` is the real
/// (only) implementor — see `TextureEvictor`'s own doc comment
/// (renderer/assets.rs) for why `AssetManager::clear` depends on the trait
/// rather than this concrete type directly.
impl assets::TextureEvictor for Renderer {
    fn evict_texture(&mut self, id: u64) {
        self.backend.evict_texture(id);
    }
}

// Tests split into tests.rs (7B-2, docs/ember2d-master-plan.md §5.2) — see
// that file's own header comment — once this file crossed the project's
// 750-line hard limit (CLAUDE.md). The pure coordinate-space free functions
// this file used to define below this point (`compute_layout`,
// `screen_cell_to_pixel`, `pixel_size_to_cells`, `uv_rect_for`,
// `nine_slice_quads`) moved to `geometry.rs` at 7D-3
// (docs/ember2d-master-plan.md §5.4), along with their own tests — this
// module still reaches them via the plain `use geometry::{...}` near the
// top of this file, and `tests` below (a child module of this one) sees
// them the same way through `use super::*`.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
