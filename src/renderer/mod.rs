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
pub use texture::Texture;
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
        self.backend.draw_texture(px, py, texture, scale);
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

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.backend.render(&self.device, &self.queue, &view);

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
