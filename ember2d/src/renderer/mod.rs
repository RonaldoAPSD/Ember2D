// renderer/mod.rs — The Wgpu-backed renderer.

pub mod buffer;
pub mod color;
pub mod backend;
pub mod texture;
pub mod assets;
pub mod font;

use std::io;
use std::sync::Arc;
use winit::window::{Window, WindowBuilder};
use winit::event_loop::EventLoop;

pub use color::{Color, DEFAULT_FG, DEFAULT_BG};
pub use texture::{Texture, TextureId};
pub use backend::{RenderBackend, WgpuBackend};
pub use assets::AssetManager;
pub use font::{Font, GlyphInfo, BitmapFont, TtfFont};

// `pub` since Phase 7 Part 1e (docs/ember2d-phase7-plan.md, E2) — the
// editor previously re-derived this exact pixel size as duplicated local
// magic-number literals (`8.0`/`16.0`) in three different files instead of
// importing it from here. This is the one allowed additive change to this
// crate for that step.
pub const CELL_W: usize = 8;
pub const CELL_H: usize = 16;
pub const SCALE: usize = 2;

pub struct Renderer {
    window: Arc<Window>,
    pub width: usize,
    pub height: usize,
    pub pixel_width: usize,
    pub pixel_height: usize,
    
    // WGPU core objects
    surface:  wgpu::Surface<'static>,
    device:   wgpu::Device,
    queue:    wgpu::Queue,
    config:   wgpu::SurfaceConfiguration,

    backend: Box<dyn RenderBackend>,

    /// A 1×1 opaque white texture, built once here rather than fetched
    /// through `AssetManager` (Phase 7 Part 1a, docs/ember2d-phase7-plan.md)
    /// — `fill_rect_px` needs one unconditionally and `Renderer` has no
    /// `AssetManager` reference to ask (that lives on `Engine`/`RenderContext`
    /// separately), so owning a tiny built-in copy here avoids threading one
    /// through every pixel-primitive call site. `Texture` is cheap to clone
    /// (a 1-element `Vec<u32>`), so callers get an owned copy without a
    /// second `NEXT_ID` allocation.
    white_texture: Texture,
}

impl Renderer {
    pub fn new(width: usize, height: usize, title: &str, event_loop: &EventLoop<()>) -> io::Result<Self> {
        let pixel_width  = width  * CELL_W;
        let pixel_height = height * CELL_H;

        let window = Arc::new(WindowBuilder::new()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(pixel_width as f32 * SCALE as f32, pixel_height as f32 * SCALE as f32))
            .build(event_loop)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?);

        // ── WGPU Initialization ───────────────────────────────────────────
        
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // wgpu 0.19+ accepts Arc<Window> as SurfaceTarget
        let surface = instance.create_surface(Arc::clone(&window))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        )).expect("Failed to find an appropriate adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        )).expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| !f.is_srgb()) // Prefer non-SRGB for linear behavior
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let backend = Box::new(WgpuBackend::new(width, height, &device, &queue, surface_format));

        Ok(Renderer {
            window,
            width,
            height,
            pixel_width,
            pixel_height,
            surface,
            device,
            queue,
            config,
            backend,
            white_texture: Texture::solid(0xFFFFFFFF),
        })
    }

    pub fn backend_name(&self) -> &str { self.backend.name() }
    pub fn set_backend(&mut self, backend: Box<dyn RenderBackend>) {
        self.backend = backend;
        self.width = self.backend.width();
        self.height = self.backend.height();
    }

    /// Toggles between ASCII and 2D Sprite rendering modes (if supported by backend).
    pub fn set_sprite_mode(&mut self, enabled: bool) {
        self.backend.set_sprite_mode(enabled);
    }

    /// Returns the ratio between the window's inner physical width and our internal pixel width.
    pub fn scale_factor(&self) -> f32 {
        self.window.inner_size().width as f32 / self.pixel_width as f32
    }

    #[cfg(target_os = "windows")]
    pub fn maximize(&self) {
        self.window.set_maximized(true);
    }

    pub fn clear(&mut self) {
        self.backend.clear();
    }

    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.backend.draw_char(x, y, ch, fg, bg);
    }

    pub fn draw_char_scaled_pixels(&mut self, px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        self.backend.draw_char_scaled_pixels(px, py, ch, fg, bg, scale);
    }

    pub fn upload_texture(&mut self, texture: &Texture) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
    }

    pub fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>) {
        self.backend.set_scissor(rect);
    }

    pub fn draw_texture(&mut self, px: i32, py: i32, texture: &Texture, scale: f32) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        // Preserves the exact size/rotation/tint this always had before the
        // backend gained real per-axis size, rotation, and tint (Step 2c).
        let size = [texture.width as f32 * scale / CELL_W as f32, texture.height as f32 * scale / CELL_H as f32];
        self.backend.draw_texture(px, py, texture, size, 0.0, Color::White, None);
    }

    /// Draw a glyph at a world-space position, through `camera`. A thin
    /// wrapper over `draw_char_scaled_pixels` — `world_pos` is converted to
    /// screen cells via `camera.world_to_screen`, then to the same
    /// pixel-snapped convention `draw_char_scaled_pixels` already uses (the
    /// editor's zoomed viewport does the identical conversion by hand in
    /// `editor/ui/canvas.rs::grid_to_pixel`).
    pub fn draw_char_world(&mut self, camera: &crate::camera::Camera, world_pos: ember2d_sim::math::Vec2, ch: char, fg: Color, bg: Color) {
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
    pub fn draw_texture_world(&mut self, camera: &crate::camera::Camera, world_pos: ember2d_sim::math::Vec2, texture: &Texture, size: ember2d_sim::math::Vec2, rotation: f32, tint: Color, src: Option<ember2d_sim::math::Rect>) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        let (px, py) = screen_cell_to_pixel(camera.world_to_screen(world_pos));
        let cell_size = [size.x * camera.zoom, size.y * camera.zoom];
        let uv_rect = src.map(|r| [
            r.x / texture.width as f32,
            r.y / texture.height as f32,
            r.w / texture.width as f32,
            r.h / texture.height as f32,
        ]);
        self.backend.draw_texture(px, py, texture, cell_size, rotation, tint, uv_rect);
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
    pub fn draw_texture_px(&mut self, dest: ember2d_sim::math::Rect, texture: &Texture, src: Option<ember2d_sim::math::Rect>, tint: Color) {
        self.backend.upload_texture(&self.device, &self.queue, texture);
        let size = pixel_size_to_cells(dest.w, dest.h);
        let uv_rect = src.map(|r| uv_rect_for(texture.width, texture.height, r));
        self.backend.draw_texture(dest.x.round() as i32, dest.y.round() as i32, texture, size, 0.0, tint, uv_rect);
    }

    /// Draw `text` through `font` at `px`, baseline-positioned (Phase 7
    /// Part 2d, docs/ember2d-phase7-plan.md): `pos` is where the FIRST
    /// glyph's baseline-left sits, not its top-left corner — "text draws
    /// from a baseline, not a top-left corner... mixed sizes on one line
    /// only align correctly on a shared baseline." Built entirely out of
    /// `draw_texture_px` plus one glyph lookup per character, so it works
    /// for any `Font` impl without knowing which one it got. Returns the
    /// total horizontal advance (matches `Font::measure`'s width for the
    /// same `text`/`px`), so a caller can position what comes next on the
    /// same baseline.
    ///
    /// Two different atlas lifetimes to handle, via `Font::atlas_texture`/
    /// `take_dirty`: `BitmapFont`'s is baked once into the GPU at startup
    /// and never changes (`atlas_texture` returns `None` — nothing to
    /// upload, ever, so a `pixels`-less placeholder `Texture` is fine,
    /// `upload_texture`'s cache check short-circuits before reading it).
    /// `TtfFont`'s `GlyphAtlas` texture is real and grows in place as new
    /// glyphs get rasterized — resolving every glyph in `text` FIRST
    /// (rather than drawing as each is resolved) means any newly-packed
    /// glyphs are already in `texture.pixels` before this decides whether
    /// to invalidate the stale GPU copy and force a fresh upload.
    ///
    /// Not yet called from any live editor UI — that's Part 4's restyle.
    /// Cloning `atlas_texture()`'s real `Texture` (when there is one) on
    /// every call is wasteful for a large atlas redrawn every frame —
    /// fine for now since nothing does that yet; worth a second look
    /// whenever this does get wired into a live per-frame draw path.
    pub fn draw_text_px(&mut self, font: &mut dyn Font, text: &str, pos: ember2d_sim::math::Vec2, px: f32, color: Color) -> f32 {
        let glyphs: Vec<GlyphInfo> = text.chars().filter_map(|ch| font.glyph(ch, px)).collect();

        let dirty = font.take_dirty();
        let tex_id = font.texture_id();
        let (tex_w, tex_h) = font.texture_size();
        let real_texture = font.atlas_texture().cloned();

        if dirty {
            self.backend.invalidate_texture(tex_id.0);
        }
        let atlas = real_texture.unwrap_or_else(|| Texture { id: tex_id.0, width: tex_w, height: tex_h, pixels: Vec::new() });

        let mut pen_x = pos.x;
        for g in glyphs {
            if g.atlas_rect.w > 0.0 && g.atlas_rect.h > 0.0 {
                let dest = ember2d_sim::math::Rect::new(
                    pen_x + g.offset.x, pos.y + g.offset.y, g.atlas_rect.w, g.atlas_rect.h,
                );
                self.draw_texture_px(dest, &atlas, Some(g.atlas_rect), color);
            }
            pen_x += g.advance;
        }
        pen_x - pos.x
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
    pub fn draw_nine_slice(&mut self, dest: ember2d_sim::math::Rect, texture: &Texture, border: (f32, f32, f32, f32), tint: Color) {
        for (d, s) in nine_slice_quads(dest, texture.width as f32, texture.height as f32, border) {
            self.draw_texture_px(d, texture, Some(s), tint);
        }
    }

    pub fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        for (i, ch) in s.chars().enumerate() {
            self.draw_char(x.saturating_add(i), y, ch, fg, bg);
        }
    }

    pub fn draw_lines(&mut self, x: usize, y: usize, lines: &[&str], fg: Color, bg: Color) {
        for (i, line) in lines.iter().enumerate() {
            self.draw_str(x, y.saturating_add(i), line, fg, bg);
        }
    }

    pub fn draw_rect_outline(&mut self, x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color) {
        if w < 2 || h < 2 { return; }
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

    pub fn draw_rect_filled(&mut self, x: usize, y: usize, w: usize, h: usize, ch: char, fg: Color, bg: Color) {
        for row in y..(y + h) {
            for col in x..(x + w) {
                self.draw_char(col, row, ch, fg, bg);
            }
        }
    }

    pub fn present(&mut self) -> io::Result<()> {
        let output = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        };

        // Sync scale factor for scissor clipping
        let scale = self.scale_factor();
        self.backend.set_render_scale(scale);

        // The actual swapchain texture's own size — the authoritative
        // render-target dimensions wgpu will validate scissor rects against,
        // not `self.config`'s (which could in principle be one resize event
        // stale) or a value re-derived from cell counts (see the backend's
        // render() for why that rounds past the real surface and panics).
        let surface_size = output.texture.size();
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.backend.render(&self.device, &self.queue, &view, surface_size.width, surface_size.height);

        output.present();

        Ok(())
    }

    pub fn try_handle_resize(&mut self) -> bool {
        let size = self.window.inner_size();
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
            
            let new_w = ((size.width as usize + (SCALE * CELL_W - 1)) / SCALE / CELL_W).max(20);
            let new_h = ((size.height as usize + (SCALE * CELL_H - 1)) / SCALE / CELL_H).max(6);
            
            if new_w == self.width && new_h == self.height { return false; }
            
            self.width = new_w;
            self.height = new_h;
            self.pixel_width = new_w * CELL_W;
            self.pixel_height = new_h * CELL_H;
            
            self.backend.resize(new_w, new_h);
            return true;
        }
        false
    }
}

/// Convert a screen-space cell position (as `Camera::world_to_screen`
/// returns it) into the pixel-snapped convention `draw_char_scaled_pixels`
/// and the backend's `draw_texture` expect — multiply by the cell size in
/// pixels, then round to a whole pixel. Pulled out as a free function so the
/// coordinate math (Step 2c) is testable without a live GPU-backed `Renderer`.
fn screen_cell_to_pixel(screen: ember2d_sim::math::Vec2) -> (i32, i32) {
    ((screen.x * CELL_W as f32).round() as i32, (screen.y * CELL_H as f32).round() as i32)
}

/// A pixel size converted to the "cell units" convention
/// `SpriteInstance::size`/`RenderBackend::draw_texture`'s own `size`
/// parameter already use (see `Renderer::draw_texture`'s `scale`
/// computation for the same divide) — pulled out so `fill_rect_px`/
/// `draw_texture_px`'s coordinate math is testable without a live
/// GPU-backed `Renderer` (Phase 7 Part 1a).
fn pixel_size_to_cells(w: f32, h: f32) -> [f32; 2] {
    [w / CELL_W as f32, h / CELL_H as f32]
}

/// A pixel-space sub-rect of a texture, normalized to the `[x, y, w, h]`
/// (0..1) convention `RenderBackend::draw_texture`'s `uv_rect` expects — the
/// same computation `draw_texture_world` does inline, pulled out here
/// (Phase 7 Part 1a) so it's independently testable.
fn uv_rect_for(texture_w: u32, texture_h: u32, src: ember2d_sim::math::Rect) -> [f32; 4] {
    [
        src.x / texture_w as f32,
        src.y / texture_h as f32,
        src.w / texture_w as f32,
        src.h / texture_h as f32,
    ]
}

/// The nine (dest, src) rect pairs `draw_nine_slice` draws, in row-major
/// order — index 4 is always the stretched center. `border` is `(left,
/// top, right, bottom)` in SOURCE pixels; corners keep that exact size on
/// both sides (source and dest), which is what makes them 1:1 rather than
/// stretched, as long as `dest` is at least as large as the combined
/// left+right / top+bottom borders — a smaller `dest` clamps the
/// middle column/row to zero width/height rather than going negative.
fn nine_slice_quads(dest: ember2d_sim::math::Rect, tex_w: f32, tex_h: f32, border: (f32, f32, f32, f32)) -> Vec<(ember2d_sim::math::Rect, ember2d_sim::math::Rect)> {
    use ember2d_sim::math::Rect;

    let (bl, bt, br, bb) = border;
    let src_x = [0.0, bl, tex_w - br];
    let src_w = [bl, (tex_w - bl - br).max(0.0), br];
    let src_y = [0.0, bt, tex_h - bb];
    let src_h = [bt, (tex_h - bt - bb).max(0.0), bb];

    let dst_x = [dest.x, dest.x + bl, dest.x + dest.w - br];
    let dst_w = [bl, (dest.w - bl - br).max(0.0), br];
    let dst_y = [dest.y, dest.y + bt, dest.y + dest.h - bb];
    let dst_h = [bt, (dest.h - bt - bb).max(0.0), bb];

    let mut quads = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            let d = Rect::new(dst_x[col], dst_y[row], dst_w[col], dst_h[row]);
            let s = Rect::new(src_x[col], src_y[row], src_w[col], src_h[row]);
            quads.push((d, s));
        }
    }
    quads
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use ember2d_sim::math::{Rect, Vec2};

    #[test]
    fn screen_cell_to_pixel_scales_by_cell_size_and_rounds() {
        assert_eq!(screen_cell_to_pixel(Vec2::new(0.0, 0.0)), (0, 0));
        assert_eq!(screen_cell_to_pixel(Vec2::new(1.0, 1.0)), (CELL_W as i32, CELL_H as i32));
        assert_eq!(screen_cell_to_pixel(Vec2::new(2.5, 3.0)), (20, 48)); // 2.5*8=20, 3.0*16=48
    }

    #[test]
    fn camera_position_lands_at_the_viewport_center_in_pixels() {
        let mut cam = Camera::new(80.0, 24.0);
        cam.position = Vec2::new(10.0, 5.0);
        cam.zoom = 1.0;

        let (px, py) = screen_cell_to_pixel(cam.world_to_screen(cam.position));
        assert_eq!((px, py), ((40 * CELL_W) as i32, (12 * CELL_H) as i32));
    }

    // ── Tests: Phase 7 Part 1a pixel-space primitives
    // (docs/ember2d-phase7-plan.md) ─────────────────────────────────────────

    #[test]
    fn pixel_size_to_cells_divides_by_cell_dimensions() {
        assert_eq!(pixel_size_to_cells(8.0, 16.0), [1.0, 1.0]);
        assert_eq!(pixel_size_to_cells(16.0, 32.0), [2.0, 2.0]);
        assert_eq!(pixel_size_to_cells(4.0, 8.0), [0.5, 0.5]);
    }

    #[test]
    fn uv_rect_for_normalizes_a_pixel_sub_rect_to_0_1() {
        assert_eq!(uv_rect_for(64, 64, Rect::new(0.0, 0.0, 64.0, 64.0)), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(uv_rect_for(64, 64, Rect::new(0.0, 0.0, 32.0, 32.0)), [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(uv_rect_for(100, 50, Rect::new(50.0, 25.0, 50.0, 25.0)), [0.5, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn nine_slice_quads_produces_nine_quads_with_the_center_at_index_4() {
        let dest = Rect::new(10.0, 20.0, 100.0, 60.0);
        let quads = nine_slice_quads(dest, 30.0, 30.0, (5.0, 5.0, 5.0, 5.0));
        assert_eq!(quads.len(), 9);

        // Corners keep the exact border size on both source and dest sides
        // — that's what "1:1, not stretched" means.
        let (top_left_d, top_left_s) = quads[0];
        assert_eq!(top_left_d, Rect::new(10.0, 20.0, 5.0, 5.0));
        assert_eq!(top_left_s, Rect::new(0.0, 0.0, 5.0, 5.0));

        let (bottom_right_d, bottom_right_s) = quads[8];
        assert_eq!(bottom_right_d, Rect::new(105.0, 75.0, 5.0, 5.0));
        assert_eq!(bottom_right_s, Rect::new(25.0, 25.0, 5.0, 5.0));

        // The center (index 4) stretches: dest grows to fill the remaining
        // area, but its source rect stays the texture's own unstretched
        // middle — that's the actual point of a nine-slice.
        let (center_d, center_s) = quads[4];
        assert_eq!(center_d, Rect::new(15.0, 25.0, 90.0, 50.0));
        assert_eq!(center_s, Rect::new(5.0, 5.0, 20.0, 20.0));
    }

    #[test]
    fn nine_slice_quads_with_zero_border_degenerates_to_one_stretched_center() {
        let dest = Rect::new(0.0, 0.0, 40.0, 20.0);
        let quads = nine_slice_quads(dest, 8.0, 8.0, (0.0, 0.0, 0.0, 0.0));
        assert_eq!(quads.len(), 9);
        // The four true corners (0, 2, 6, 8) collapse to zero in BOTH
        // dimensions — there's no border pixel to draw. The four edges
        // (1, 3, 5, 7) collapse only along the axis their border would
        // have occupied; the other axis still spans the whole dest, since
        // that's the axis the (now-zero) corners would otherwise have
        // shared width/height with.
        for &i in &[0, 2, 6, 8] {
            let (d, s) = quads[i];
            assert_eq!((d.w, d.h), (0.0, 0.0), "corner quad {i} should be degenerate in both axes with a zero border");
            assert_eq!((s.w, s.h), (0.0, 0.0), "corner quad {i} should be degenerate in both axes with a zero border");
        }
        for &i in &[1, 7] { // top edge, bottom edge: zero height, full width
            let (d, _) = quads[i];
            assert_eq!(d.h, 0.0, "edge quad {i} should be degenerate along its border axis");
            assert_eq!(d.w, dest.w);
        }
        for &i in &[3, 5] { // left edge, right edge: zero width, full height
            let (d, _) = quads[i];
            assert_eq!(d.w, 0.0, "edge quad {i} should be degenerate along its border axis");
            assert_eq!(d.h, dest.h);
        }
        let (center_d, center_s) = quads[4];
        assert_eq!(center_d, dest, "with a zero border the center dest must cover the whole rect");
        assert_eq!(center_s, Rect::new(0.0, 0.0, 8.0, 8.0), "with a zero border the center src must cover the whole texture");
    }

    #[test]
    fn nine_slice_quads_clamps_a_dest_smaller_than_the_combined_borders() {
        // dest (30px) is smaller than the combined left+right border (40px)
        // — the middle column must clamp to zero width, not go negative.
        let dest = Rect::new(0.0, 0.0, 30.0, 30.0);
        let quads = nine_slice_quads(dest, 100.0, 100.0, (20.0, 20.0, 20.0, 20.0));
        let (center_d, _) = quads[4];
        assert_eq!(center_d.w, 0.0);
        assert_eq!(center_d.h, 0.0);
    }
}
