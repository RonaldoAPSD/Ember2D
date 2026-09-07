// renderer/backend.rs — Abstracted rendering backends.

use crate::renderer::color::{Color, DEFAULT_BG, DEFAULT_FG};
use crate::renderer::texture::Texture;
use crate::renderer::{CELL_H, CELL_W};
use std::collections::HashMap;

/// R26 (7B-3, docs/ember2d-master-plan.md §5.2): `texture_cache`'s default
/// eviction budget — see `TextureBudget`'s own doc comment.
const DEFAULT_TEXTURE_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// 7B-3 (docs/ember2d-master-plan.md §5.2): `DEFAULT_BG` converted to the
/// `f64`, 0..1-per-channel shape `wgpu::Color` needs — same
/// component/255.0 formula `Color::to_rgba` (ember2d-sim/src/color.rs)
/// uses, just computed once here since `render()`'s clear color is fixed,
/// not per-draw-call like every other color this renderer touches.
fn default_bg_clear_color() -> wgpu::Color {
    let r = ((DEFAULT_BG >> 16) & 0xFF) as f64 / 255.0;
    let g = ((DEFAULT_BG >> 8) & 0xFF) as f64 / 255.0;
    let b = (DEFAULT_BG & 0xFF) as f64 / 255.0;
    wgpu::Color { r, g, b, a: 1.0 }
}

// 7B-3 (docs/ember2d-master-plan.md §5.2, dead code): the `RenderBackend`
// trait used to abstract over `WgpuBackend`, but `WgpuBackend` was its only
// implementor and a second backend isn't on any phase — the indirection had
// no caller that needed it. `WgpuBackend`'s own methods (below, in a plain
// inherent `impl` instead of a trait `impl`) are unchanged; `Renderer` now
// holds a `WgpuBackend` directly instead of `Box<dyn RenderBackend>`.

// Vertex/SpriteInstance/Globals/Batch moved into their own file, vertex.rs
// (7B-3, docs/ember2d-master-plan.md §5.2) — pure GPU data layout, no logic
// depending on anything else here, the next-cleanest unit to pull out once
// TextureBudget (R26) alone wasn't enough to get this file back under the
// 750-line hard limit (CLAUDE.md). See that file's own header comment.
#[path = "vertex.rs"]
mod vertex;
use vertex::{Batch, Globals, SpriteInstance, Vertex};

// TextureBudget (R26) moved into its own file, texture_budget.rs, rather
// than just its own test file here — once this file crossed the project's
// 750-line hard limit (CLAUDE.md), the struct itself was the more natural
// unit to extract than only splitting its tests off. See that file's own
// header comment.
#[path = "texture_budget.rs"]
mod texture_budget;
use texture_budget::TextureBudget;

// ────────────────────────── WgpuBackend ──────────────────────────────────────

pub struct WgpuBackend {
    width: usize,
    height: usize,
    pub is_sprite_mode: bool,
    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): physical pixels per
    /// logical pixel, per axis (was one scalar) — set each frame from
    /// `Renderer::screen_mapping()` via `set_render_scale`.
    pub render_scale: (f32, f32),
    /// The letterbox origin, in physical pixels — added to a scissor
    /// rect's logical-pixel coordinates after scaling, same reasoning as
    /// `render_scale`.
    pub render_origin: (f32, f32),

    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,

    instances: Vec<SpriteInstance>,
    batches: Vec<Batch>,
    current_scissor: Option<(u32, u32, u32, u32)>,

    font_texture_id: u64,
    texture_cache: HashMap<u64, wgpu::BindGroup>,
    /// R26 (7B-3, docs/ember2d-master-plan.md §5.2): approximate byte
    /// accounting and LRU order for `texture_cache`, kept separate so the
    /// eviction *policy* is testable without a live GPU device — see
    /// `TextureBudget`'s own doc comment. Does not track `font_texture_id`
    /// (inserted directly into `texture_cache` in `new`, below, and never
    /// evicted — see that call site's own comment).
    texture_budget: TextureBudget,
    sampler: wgpu::Sampler,
    texture_bind_group_layout: wgpu::BindGroupLayout,

    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
}

impl WgpuBackend {
    pub fn new(
        width: usize,
        height: usize,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
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
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
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
        // 7B-1 (docs/ember2d-master-plan.md §5.2): ImageCopyTexture/
        // ImageDataLayout renamed to TexelCopyTextureInfo/TexelCopyBufferLayout
        // in wgpu 30 — same fields, name only.
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &font_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &font_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8 * 4),
                rows_per_image: Some(1024),
            },
            wgpu::Extent3d { width: 8, height: 1024, depth_or_array_layers: 1 },
        );
        let font_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let font_texture_id = 0u64; // Reserve 0 for font
        let font_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&font_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // R26 (7B-3, docs/ember2d-master-plan.md §5.2): inserted directly,
        // not through `upload_texture`/`texture_budget` — the font atlas
        // is foundational (every glyph draw needs it) and tiny (8x1024
        // RGBA8 = 32 KB), so it's simplest to just never make it an
        // eviction candidate at all rather than track-then-exempt it.
        let mut texture_cache = HashMap::new();
        texture_cache.insert(font_texture_id, font_bind_group);

        // ── Globals Uniform ───────────────────────────────────────────────
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Globals Buffer"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Globals Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        // 7B-1 (docs/ember2d-master-plan.md §5.2): wgpu 30 wraps each bind
        // group layout entry in `Option` (a `None` gap means "unbound" —
        // not used here, both slots are always filled) and renamed
        // `push_constant_ranges` to `immediate_size: u32` (wgpu's "push
        // constants" -> "immediates" rename) — this pipeline never used
        // either, so `0` is the same "no immediates" as the old `&[]`.
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                Some(&globals_bind_group_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::desc()), Some(SpriteInstance::desc())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            // `multiview` renamed `multiview_mask` (wgpu 30); this pipeline
            // never used multiview rendering either way. `cache` is new —
            // an optional `PipelineCache` for faster recompiles, not needed
            // here.
            multiview_mask: None,
            cache: None,
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
            width,
            height,
            is_sprite_mode: false,
            render_scale: (1.0, 1.0),
            render_origin: (0.0, 0.0),
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_buffer_capacity,
            instances: Vec::with_capacity(instance_buffer_capacity),
            batches: Vec::new(),
            current_scissor: None,
            font_texture_id,
            texture_cache,
            texture_budget: TextureBudget::new(DEFAULT_TEXTURE_BUDGET_BYTES),
            sampler,
            texture_bind_group_layout,
            globals_buffer,
            globals_bind_group,
        }
    }

    fn update_globals(&self, queue: &wgpu::Queue) {
        // 7B-1 (docs/ember2d-master-plan.md §5.2): glam 0.33 deprecated the
        // flat `Mat4::orthographic_lh` free function in favor of this
        // module path — same six args, same left-handed DirectX-style
        // convention (`top`/`bottom` swapped from a math-convention ortho,
        // which is what flips our screen-space Y-down into clip space).
        let projection = glam::camera::lh::proj::directx::orthographic(
            0.0,
            self.width as f32,
            self.height as f32,
            0.0,
            -1.0,
            1.0,
        );
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

    /// R27 (7B-3, docs/ember2d-master-plan.md §5.2): lets a caller (namely
    /// `Renderer::draw_text_px`) check whether `upload_texture` would
    /// actually need real pixel data before assembling it — see that
    /// function's own doc comment.
    pub fn has_texture(&self, id: u64) -> bool {
        self.texture_cache.contains_key(&id)
    }

    pub fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &Texture,
    ) {
        if self.texture_cache.contains_key(&texture.id) {
            // R26 (7B-3, docs/ember2d-master-plan.md §5.2): mark this id
            // most-recently-used on every access, not just first upload —
            // otherwise a texture drawn every frame would still look like
            // the oldest (and therefore next-evicted) entry the moment
            // anything else gets uploaded, since nothing else ever
            // touched it again. `font_texture_id` never reaches here (see
            // `new`'s own comment on why it's exempt), so this is safe to
            // call unconditionally for every real cache hit.
            self.texture_budget.touch(texture.id);
            return;
        }

        let size = wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        };
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
            wgpu::TexelCopyTextureInfo {
                texture: &wgpu_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&texture.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * texture.width),
                rows_per_image: Some(texture.height),
            },
            size,
        );

        let view = wgpu_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.texture_cache.insert(texture.id, bind_group);

        // R26 (7B-3, docs/ember2d-master-plan.md §5.2): texture_cache used
        // to grow unbounded — nothing ever freed a GPU texture once
        // uploaded, even across a level's worth of textures no longer in
        // use. `TextureBudget::insert` reports which ids (oldest first)
        // need evicting to stay under budget; this is the one place that
        // actually removes them from the real `texture_cache`, since
        // `TextureBudget` itself has no reference to it.
        let size_bytes = texture.width as usize * texture.height as usize * 4;
        for evicted_id in self.texture_budget.insert(texture.id, size_bytes) {
            self.texture_cache.remove(&evicted_id);
            eprintln!(
                "[renderer] texture cache exceeded its {} MB budget — evicted texture id {evicted_id}",
                DEFAULT_TEXTURE_BUDGET_BYTES / (1024 * 1024)
            );
        }
    }
}

impl WgpuBackend {
    pub fn name(&self) -> &str {
        if self.is_sprite_mode {
            "WGPU Sprites"
        } else {
            "WGPU ASCII"
        }
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.batches.clear();
        self.current_scissor = None;
    }

    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
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

    pub fn draw_char_scaled_pixels(
        &mut self,
        px: i32,
        py: i32,
        ch: char,
        fg: Color,
        bg: Color,
        scale: f32,
    ) {
        self.ensure_batch(self.font_texture_id);

        let fg_rgba = fg.to_rgba(DEFAULT_FG);
        let bg_rgba = bg.to_rgba(DEFAULT_BG);
        let ch_idx = ch as usize % 128;
        let uv_y = ch_idx as f32 / 128.0;
        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): was a hardcoded
        // `/ 8.0` / `/ 16.0` literal pair duplicating CELL_W/CELL_H.
        let cell_x = px as f32 / CELL_W as f32;
        let cell_y = py as f32 / CELL_H as f32;

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

    /// `size` is the instance's on-screen size in cell units (post-zoom —
    /// same convention as `draw_char_scaled_pixels`'s `scale`, but per-axis
    /// so non-square sprites and world-space sizing (Step 2c) are possible).
    /// `rotation` is radians, about the instance's own center (Step 2b).
    /// `uv_rect` is a normalized `[x, y, w, h]` (0..1) sub-rect of the
    /// texture to sample — `None` samples the whole thing. This is what
    /// `SpriteSource::Texture::src` (Step 3b) backs, e.g. for sprite sheets.
    pub fn draw_texture(
        &mut self,
        px: i32,
        py: i32,
        texture: &Texture,
        size: [f32; 2],
        rotation: f32,
        tint: Color,
        uv_rect: Option<[f32; 4]>,
    ) {
        self.ensure_batch(texture.id);

        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): was a hardcoded
        // `/ 8.0` / `/ 16.0` literal pair duplicating CELL_W/CELL_H.
        let cell_x = px as f32 / CELL_W as f32;
        let cell_y = py as f32 / CELL_H as f32;
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

    pub fn set_scissor(&mut self, rect: Option<(u32, u32, u32, u32)>) {
        self.current_scissor = rect;
    }

    /// `surface_width`/`surface_height` are the *actual* physical pixel size
    /// of the render target (`Renderer`'s `wgpu::SurfaceConfiguration`) —
    /// the backend needs these to clamp scissor rects; see the comment at
    /// their one use site for why a recomputed value isn't safe to trust.
    /// `viewport_origin`/`viewport_size` (7B-2, docs/ember2d-master-plan.md
    /// §5.2, R21) are the letterboxed drawable rect within that surface —
    /// `cells * CELL * scale` physical pixels, centered — everything
    /// outside it stays the clear color instead of every cell stretching
    /// to fill whatever's left over (R21's actual bug).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        surface_width: u32,
        surface_height: u32,
        viewport_origin: (f32, f32),
        viewport_size: (f32, f32),
    ) {
        self.update_globals(queue);

        if self.instances.is_empty() {
            return;
        }

        // Finalize last batch range
        if let Some(last) = self.batches.last_mut() {
            last.instance_range.end = self.instances.len() as u32;
        }

        // Dynamically resize instance buffer if needed
        if self.instances.len() > self.instance_buffer_capacity {
            self.instance_buffer_capacity = self.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer (Resized)"),
                size: (std::mem::size_of::<SpriteInstance>() * self.instance_buffer_capacity)
                    as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("WgpuBackend Encoder"),
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("WgpuBackend Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    // New in wgpu 30, for indexing a single slice of a 3D
                    // texture view — `view` here is always a plain 2D
                    // surface texture view, so `None` (not applicable).
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 7B-3 (docs/ember2d-master-plan.md §5.2): was
                        // wgpu::Color::BLACK — PlayState::render used to
                        // paint over this every frame with an explicit
                        // width*height blank-glyph fill just to get the
                        // real default background (DEFAULT_BG, 0x111111,
                        // not pure black) instead. Clearing to the real
                        // default here let that redundant fill be deleted
                        // outright, rather than papering over the color
                        // mismatch it was hiding.
                        load: wgpu::LoadOp::Clear(default_bg_clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                // Multiview layer mask (wgpu 30) — this pass never renders
                // to a multiview target.
                multiview_mask: None,
            });

            rp.set_pipeline(&self.pipeline);
            rp.set_bind_group(1, &self.globals_bind_group, &[]);
            rp.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rp.set_vertex_buffer(1, self.instance_buffer.slice(..));
            rp.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): restricts
            // drawing to the letterboxed drawable rect — everything
            // outside stays this pass's own Clear color instead of every
            // cell stretching to fill the leftover surface (R21's actual
            // bug). Clamped to the real surface bounds: `viewport_size`
            // comes from `cells * CELL * scale`, which floor-division
            // (`compute_layout`, renderer/mod.rs) guarantees never exceeds
            // the physical surface — this clamp only matters if the window
            // shrank below the `.max(20)`/`.max(6)` minimum grid floor,
            // where the requested viewport can be wider than the surface
            // itself; wgpu rejects — and panics on — a viewport not fully
            // contained in the render target.
            let vp_w = viewport_size.0.min(surface_width as f32);
            let vp_h = viewport_size.1.min(surface_height as f32);
            rp.set_viewport(viewport_origin.0, viewport_origin.1, vp_w, vp_h, 0.0, 1.0);

            for batch in &self.batches {
                if let Some(bind_group) = self.texture_cache.get(&batch.texture_id) {
                    let (raw_x, raw_y, raw_w, raw_h) = if let Some((x, y, w, h)) = batch.scissor {
                        // Scissor rects are always relative to the render
                        // target itself, independent of whatever viewport
                        // is set — panels specify these in logical pixels,
                        // so both the per-axis scale AND the letterbox
                        // origin (`render_scale`/`render_origin`, set once
                        // a frame from `Renderer::screen_mapping()`) are
                        // needed to land on the same physical pixels the
                        // viewport above just placed the actual content at.
                        (
                            (self.render_origin.0 + x as f32 * self.render_scale.0).round() as u32,
                            (self.render_origin.1 + y as f32 * self.render_scale.1).round() as u32,
                            (w as f32 * self.render_scale.0).round() as u32,
                            (h as f32 * self.render_scale.1).round() as u32,
                        )
                    } else {
                        // "No explicit scissor" means "no clipping" — the
                        // viewport above already confines actual drawing to
                        // the letterboxed rect, so resetting to the full
                        // surface here (not the smaller drawable rect) is
                        // still correct, not a stretch: scissor and
                        // viewport clip independently, and wgpu validates
                        // scissor rects against the render target, not the
                        // current viewport.
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

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn set_sprite_mode(&mut self, enabled: bool) {
        self.is_sprite_mode = enabled;
    }

    /// Drop `id` from the uploaded-texture cache, so the next
    /// `upload_texture` call for it re-uploads from scratch instead of
    /// silently skipping (Phase 7 Part 2d, docs/ember2d-phase7-plan.md) —
    /// `upload_texture`'s cache check is a permanent "already have it,"
    /// which is correct for textures loaded once from a file but wrong for
    /// `GlyphAtlas`'s texture, whose pixels change in place every time a
    /// new glyph gets rasterized and packed into it. `GlyphAtlas::dirty`
    /// (via `Font::take_dirty`) tracks when that's happened.
    pub fn invalidate_texture(&mut self, id: u64) {
        self.texture_cache.remove(&id);
        self.texture_budget.remove(id);
    }

    /// R26 (7B-3, docs/ember2d-master-plan.md §5.2): the GPU-side half of
    /// `AssetManager::clear()` — CPU-side asset bookkeeping clearing on its
    /// own left every previously-uploaded GPU texture resident forever.
    /// Just `invalidate_texture` under a name that reads right at its own
    /// call site (`AssetManager::clear` doesn't know or care that it's the
    /// same underlying operation as a dirty-atlas re-upload).
    pub fn evict_texture(&mut self, id: u64) {
        self.invalidate_texture(id);
    }

    /// 7B-2 (docs/ember2d-master-plan.md §5.2, R21): per-axis now (was one
    /// scalar) — panels still specify `set_scissor` rects in logical
    /// pixels; converting to physical needs both this scale *and*
    /// `origin_px` (the letterbox offset `scale_factor()` never accounted
    /// for) to land on the right pixels.
    pub fn set_render_scale(&mut self, scale_x: f32, scale_y: f32, origin_px: (f32, f32)) {
        self.render_scale = (scale_x, scale_y);
        self.render_origin = origin_px;
    }
}
