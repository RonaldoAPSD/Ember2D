// renderer/backend.rs — Abstracted rendering backends.

use std::collections::HashMap;
use crate::renderer::color::{Color, DEFAULT_BG, DEFAULT_FG};
use crate::renderer::texture::Texture;
use bytemuck::{Pod, Zeroable};

pub trait RenderBackend {
    fn name(&self) -> &str;
    fn clear(&mut self);
    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color);
    fn draw_char_scaled_pixels(&mut self, px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32);
    /// `size` is the instance's on-screen size in cell units (post-zoom —
    /// same convention as `draw_char_scaled_pixels`'s `scale`, but per-axis
    /// so non-square sprites and world-space sizing (Step 2c) are possible).
    /// `rotation` is radians, about the instance's own center (Step 2b).
    /// `uv_rect` is a normalized `[x, y, w, h]` (0..1) sub-rect of the
    /// texture to sample — `None` samples the whole thing. This is what
    /// `SpriteSource::Texture::src` (Step 3b) backs, e.g. for sprite sheets.
    fn draw_texture(&mut self, px: i32, py: i32, texture: &Texture, size: [f32; 2], rotation: f32, tint: Color, uv_rect: Option<[f32; 4]>);
    /// `surface_width`/`surface_height` are the *actual* physical pixel size
    /// of the render target (`Renderer`'s `wgpu::SurfaceConfiguration`) —
    /// the backend needs these to clamp scissor rects; see the comment at
    /// their one use site for why a recomputed value isn't safe to trust.
    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, surface_width: u32, surface_height: u32);
    fn resize(&mut self, width: usize, height: usize);
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn set_sprite_mode(&mut self, enabled: bool);
    fn upload_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture: &Texture);
    fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>);
    fn set_render_scale(&mut self, scale: f32);
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    pub position: [f32; 2],
    pub size:     [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_size:   [f32; 2],
    pub color_fg:  [f32; 4],
    pub color_bg:  [f32; 4],
    pub mode:      u32, // 0 = ASCII, 1 = Sprite
    /// Radians, applied in the vertex shader around the instance's own
    /// center (Phase 2 — replaces what used to be unused padding here).
    pub rotation:  f32,
}

impl SpriteInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        2 => Float32x2, 3 => Float32x2, 4 => Float32x2, 5 => Float32x2, 6 => Float32x4, 7 => Float32x4, 8 => Uint32, 9 => Float32
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub projection: [[f32; 4]; 4],
}

#[derive(Clone, PartialEq, Eq)]
struct Batch {
    texture_id: u64,
    instance_range: std::ops::Range<u32>,
    scissor: Option<(u32, u32, u32, u32)>,
}

// ────────────────────────── WgpuBackend ──────────────────────────────────────

pub struct WgpuBackend {
    width: usize,
    height: usize,
    pub is_sprite_mode: bool,
    pub render_scale:   f32,
    
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer:  wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    
    instances: Vec<SpriteInstance>,
    batches:   Vec<Batch>,
    current_scissor: Option<(u32, u32, u32, u32)>,
    
    font_texture_id: u64,
    texture_cache:   HashMap<u64, wgpu::BindGroup>,
    sampler:         wgpu::Sampler,
    texture_bind_group_layout: wgpu::BindGroupLayout,

    globals_buffer:  wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
}

impl WgpuBackend {
    pub fn new(width: usize, height: usize, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        // ── Sampler ──────────────────────────────────────────────────────
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // ── Texture Bind Group Layout ────────────────────────────────────
        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true } },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Create Font Atlas ─────────────────────────────────────────────
        use font8x8::legacy::BASIC_LEGACY;
        let mut font_data = vec![0u8; 128 * 8 * 8 * 4];
        for (ch, bitmap) in BASIC_LEGACY.iter().enumerate() {
            for y in 0..8 {
                let row = bitmap[y];
                for x in 0..8 {
                    let pixel_on = (row >> x) & 1 != 0;
                    let idx = (ch * 64 + y * 8 + x) * 4;
                    let val = if pixel_on { 255 } else { 0 };
                    font_data[idx] = val;
                    font_data[idx + 1] = val;
                    font_data[idx + 2] = val;
                    font_data[idx + 3] = val;
                }
            }
        }

        let font_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Texture"),
            size: wgpu::Extent3d { width: 8, height: 1024, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // Defect D14: this was Rgba8UnormSrgb while loaded textures are
            // Rgba8Unorm and the surface is deliberately non-sRGB (see
            // renderer/mod.rs). textureSample() auto-decodes an *Srgb
            // texture from gamma-encoded storage to linear values on read,
            // which loaded textures never get — two different color spaces
            // feeding the same non-sRGB, no-further-conversion output.
            // Matching the surface and loaded textures here (both Unorm)
            // means every texture in the pipeline is interpreted the same
            // way: raw byte value in, same float value out, no hidden
            // conversion. (fs_main's glyph path only ever thresholds this
            // texture's red channel as a boolean mask, so this had no
            // visible symptom yet — but it's the trap Phase 2/3 would
            // inherit the moment glyphs are sampled/blended like sprites.)
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &font_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &font_data,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(8 * 4), rows_per_image: Some(1024) },
            wgpu::Extent3d { width: 8, height: 1024, depth_or_array_layers: 1 },
        );
        let font_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let font_texture_id = 0u64; // Reserve 0 for font
        let font_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&font_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let mut texture_cache = HashMap::new();
        texture_cache.insert(font_texture_id, font_bind_group);

        // ── Globals Uniform ───────────────────────────────────────────────
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Globals Buffer"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Globals Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buffer.as_entire_binding() }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&texture_bind_group_layout, &globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), SpriteInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertices = [
            Vertex { position: [0.0, 0.0], uv: [0.0, 0.0] },
            Vertex { position: [1.0, 0.0], uv: [1.0, 0.0] },
            Vertex { position: [1.0, 1.0], uv: [1.0, 1.0] },
            Vertex { position: [0.0, 1.0], uv: [0.0, 1.0] },
        ];
        let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];

        use wgpu::util::DeviceExt;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        
        let instance_buffer_capacity = 16384; // Start with 16k capacity
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (std::mem::size_of::<SpriteInstance>() * instance_buffer_capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        WgpuBackend {
            width, height, is_sprite_mode: false,
            render_scale: 1.0,
            pipeline, vertex_buffer, index_buffer, instance_buffer,
            instance_buffer_capacity,
            instances: Vec::with_capacity(instance_buffer_capacity),
            batches:   Vec::new(),
            current_scissor: None,
            font_texture_id,
            texture_cache,
            sampler,
            texture_bind_group_layout,
            globals_buffer,
            globals_bind_group,
        }
    }

    fn update_globals(&self, queue: &wgpu::Queue) {
        let projection = glam::Mat4::orthographic_lh(0.0, self.width as f32, self.height as f32, 0.0, -1.0, 1.0);
        let globals = Globals { projection: projection.to_cols_array_2d() };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));
    }

    fn ensure_batch(&mut self, texture_id: u64) {
        if let Some(last) = self.batches.last_mut() {
            if last.texture_id == texture_id && last.scissor == self.current_scissor {
                return;
            }
            last.instance_range.end = self.instances.len() as u32;
        }
        
        self.batches.push(Batch {
            texture_id,
            instance_range: (self.instances.len() as u32)..(self.instances.len() as u32),
            scissor: self.current_scissor,
        });
    }

    fn upload_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture: &Texture) {
        if self.texture_cache.contains_key(&texture.id) { return; }

        let size = wgpu::Extent3d { width: texture.width, height: texture.height, depth_or_array_layers: 1 };
        let wgpu_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, // Texture::load gives 0xRRGGBBAA
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture { texture: &wgpu_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&texture.pixels),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * texture.width), rows_per_image: Some(texture.height) },
            size,
        );

        let view = wgpu_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        self.texture_cache.insert(texture.id, bind_group);
    }
}

impl RenderBackend for WgpuBackend {
    fn name(&self) -> &str { if self.is_sprite_mode { "WGPU Sprites" } else { "WGPU ASCII" } }
    
    fn clear(&mut self) {
        self.instances.clear();
        self.batches.clear();
        self.current_scissor = None;
    }

    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.ensure_batch(self.font_texture_id);
        
        let fg_rgba = fg.to_rgba(DEFAULT_FG);
        let bg_rgba = bg.to_rgba(DEFAULT_BG);
        let ch_idx = ch as usize % 128;
        let uv_y = ch_idx as f32 / 128.0;
        
        self.instances.push(SpriteInstance {
            position: [x as f32, y as f32],
            size: [1.0, 1.0],
            uv_offset: [0.0, uv_y],
            uv_size: [1.0, 1.0 / 128.0],
            color_fg: fg_rgba,
            color_bg: bg_rgba,
            mode: 0,
            rotation: 0.0,
        });
    }

    fn draw_char_scaled_pixels(&mut self, px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        self.ensure_batch(self.font_texture_id);
        
        let fg_rgba = fg.to_rgba(DEFAULT_FG);
        let bg_rgba = bg.to_rgba(DEFAULT_BG);
        let ch_idx = ch as usize % 128;
        let uv_y = ch_idx as f32 / 128.0;
        let cell_x = px as f32 / 8.0;
        let cell_y = py as f32 / 16.0;
        
        self.instances.push(SpriteInstance {
            position: [cell_x, cell_y],
            size: [scale, scale],
            uv_offset: [0.0, uv_y],
            uv_size: [1.0, 1.0 / 128.0],
            color_fg: fg_rgba,
            color_bg: bg_rgba,
            mode: 0,
            rotation: 0.0,
        });
    }

    fn draw_texture(&mut self, px: i32, py: i32, texture: &Texture, size: [f32; 2], rotation: f32, tint: Color, uv_rect: Option<[f32; 4]>) {
        self.ensure_batch(texture.id);

        let cell_x = px as f32 / 8.0;
        let cell_y = py as f32 / 16.0;
        let (uv_offset, uv_size) = match uv_rect {
            Some([x, y, w, h]) => ([x, y], [w, h]),
            None => ([0.0, 0.0], [1.0, 1.0]),
        };

        self.instances.push(SpriteInstance {
            position: [cell_x, cell_y],
            size,
            uv_offset,
            uv_size,
            // Reset means "no tint" here, i.e. white — DEFAULT_FG (the
            // ASCII text default) would incorrectly darken every sprite.
            color_fg: tint.to_rgba(0xFFFFFF),
            color_bg: [0.0, 0.0, 0.0, 0.0], // Background not used for sprites
            mode: 1,
            rotation,
        });
    }

    fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>) {
        self.current_scissor = rect;
    }

    fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, surface_width: u32, surface_height: u32) {
        self.update_globals(queue);

        if self.instances.is_empty() { return; }
        
        // Finalize last batch range
        if let Some(last) = self.batches.last_mut() {
            last.instance_range.end = self.instances.len() as u32;
        }

        // Dynamically resize instance buffer if needed
        if self.instances.len() > self.instance_buffer_capacity {
            self.instance_buffer_capacity = self.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer (Resized)"),
                size: (std::mem::size_of::<SpriteInstance>() * self.instance_buffer_capacity) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("WgpuBackend Encoder") });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("WgpuBackend Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(1, &self.globals_bind_group, &[]);
            rp.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rp.set_vertex_buffer(1, self.instance_buffer.slice(..));
            rp.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for batch in &self.batches {
                if let Some(bind_group) = self.texture_cache.get(&batch.texture_id) {
                    let (raw_x, raw_y, raw_w, raw_h) = if let Some((x, y, w, h)) = batch.scissor {
                        (
                            (x as f32 * self.render_scale).round() as u32,
                            (y as f32 * self.render_scale).round() as u32,
                            (w as f32 * self.render_scale).round() as u32,
                            (h as f32 * self.render_scale).round() as u32,
                        )
                    } else {
                        // "No explicit scissor" means "reset to the full
                        // surface" — NOT "recompute the full surface size
                        // from self.width/height". Those are cell counts
                        // produced by ceiling-rounding an arbitrary physical
                        // resize (Renderer::try_handle_resize), so
                        // `cells * CELL_W * scale` can land a few pixels
                        // *past* the real surface whenever the window's
                        // physical size doesn't divide evenly into whole
                        // cells (e.g. maximizing to a height like 829px).
                        // wgpu's set_scissor_rect rejects — and panics on —
                        // any rect not fully contained in the render target,
                        // so this used to crash the whole game on maximize.
                        (0, 0, surface_width, surface_height)
                    };
                    // Defensive clamp for both branches: an editor panel's
                    // scissor could in principle also land outside the
                    // surface after some other resize edge case, and the
                    // failure mode (a hard panic, not a validation warning)
                    // is bad enough to guard unconditionally rather than
                    // trust either source to always stay in bounds.
                    let sx = raw_x.min(surface_width.saturating_sub(1));
                    let sy = raw_y.min(surface_height.saturating_sub(1));
                    let sw = raw_w.min(surface_width.saturating_sub(sx)).max(1);
                    let sh = raw_h.min(surface_height.saturating_sub(sy)).max(1);
                    rp.set_scissor_rect(sx, sy, sw, sh);
                    rp.set_bind_group(0, bind_group, &[]);
                    rp.draw_indexed(0..6, 0, batch.instance_range.clone());
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }
    fn width(&self) -> usize { self.width }
    fn height(&self) -> usize { self.height }
    fn set_sprite_mode(&mut self, enabled: bool) { self.is_sprite_mode = enabled; }

    fn upload_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture: &Texture) {
        self.upload_texture(device, queue, texture);
    }

    fn set_render_scale(&mut self, scale: f32) {
        self.render_scale = scale;
    }
}
