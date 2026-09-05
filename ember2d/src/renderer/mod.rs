// renderer/mod.rs — The Wgpu-backed renderer.

pub mod buffer;
pub mod color;
pub mod backend;
pub mod texture;
pub mod assets;

use std::io;
use std::sync::Arc;
use winit::window::{Window, WindowBuilder};
use winit::event_loop::EventLoop;

pub use color::{Color, DEFAULT_FG, DEFAULT_BG};
pub use texture::{Texture, TextureId};
pub use backend::{RenderBackend, WgpuBackend};
pub use assets::AssetManager;

const CELL_W: usize = 8;
const CELL_H: usize = 16;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use ember2d_sim::math::Vec2;

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
}
