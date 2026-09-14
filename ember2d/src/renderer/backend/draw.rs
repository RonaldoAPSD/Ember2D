// renderer/backend/draw.rs — WgpuBackend's per-frame drawing/frame-lifecycle
// API (split out of backend.rs at R76, docs/ember2d-master-plan.md §3.2 —
// backend.rs was 810 real lines, over CLAUDE.md's 750-line limit; this is
// purely a file split, no behavior change). backend.rs itself keeps
// `WgpuBackend`'s struct definition and the setup/texture-upload half of its
// impl (`new`/`update_globals`/`ensure_batch`/`has_texture`/
// `upload_texture`) — this file is everything a caller reaches for after
// that: clearing and filling one frame's draw list, then submitting it to
// the GPU. Splitting along this exact seam isn't new here — backend.rs
// already carried its impl as two separate `impl WgpuBackend` blocks (this
// file is the second one, moved as-is) precisely because "build the
// pipeline once" and "draw with it every frame" were already two distinct
// halves of the type's own API.

use super::vertex::SpriteInstance;
use super::{default_bg_clear_color, WgpuBackend};
use crate::renderer::color::{Color, DEFAULT_BG, DEFAULT_FG};
use crate::renderer::texture::Texture;
use crate::renderer::{CELL_H, CELL_W};
use ember2d_sim::math::Rect;

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

    /// `px`/`py` are `f32` (7D-3, docs/ember2d-master-plan.md §5.4, was
    /// `i32`) — `Renderer::draw_char_scaled_pixels` (its one pre-7D-3
    /// caller, always a whole logical pixel already) casts when calling
    /// this; the new `Renderer::draw_char_px` (`UiPainter::tile_glyph`'s
    /// own backing method) needs the real fractional position a UI-points
    /// preview glyph can land at.
    pub fn draw_char_scaled_pixels(
        &mut self,
        px: f32,
        py: f32,
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
        let cell_x = px / CELL_W as f32;
        let cell_y = py / CELL_H as f32;

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
    /// `px`/`py` are `f32` (7D-3, docs/ember2d-master-plan.md §5.4, was
    /// `i32`) — `Renderer::draw_texture_px` snaps its own `dest` to the
    /// physical pixel grid and needs the exact, possibly-fractional
    /// logical-pixel result of that snap to reach the GPU unrounded;
    /// `Renderer::draw_texture`/`draw_texture_world` (world-space, cell-
    /// quantized already) cast their whole-logical-pixel positions here.
    pub fn draw_texture(
        &mut self,
        px: f32,
        py: f32,
        texture: &Texture,
        size: [f32; 2],
        rotation: f32,
        tint: Color,
        uv_rect: Option<[f32; 4]>,
    ) {
        self.ensure_batch(texture.id);

        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): was a hardcoded
        // `/ 8.0` / `/ 16.0` literal pair duplicating CELL_W/CELL_H.
        let cell_x = px / CELL_W as f32;
        let cell_y = py / CELL_H as f32;
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

    /// `f32` logical pixels (7D-3, docs/ember2d-master-plan.md §5.4, was
    /// `Option<(u32, u32, u32, u32)>`) — see `Renderer::set_scissor`'s own
    /// doc comment.
    pub fn set_scissor(&mut self, rect: Option<Rect>) {
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
                    let (raw_x, raw_y, raw_w, raw_h) = if let Some(r) = batch.scissor {
                        // Scissor rects are always relative to the render
                        // target itself, independent of whatever viewport
                        // is set — panels specify these in logical pixels
                        // (`f32`, 7D-3, docs/ember2d-master-plan.md §5.4 —
                        // was `u32`, which threw away sub-logical-pixel
                        // precision before this ever reached the ONE
                        // rounding step that actually matters, to a whole
                        // PHYSICAL pixel, right here), so both the per-axis
                        // scale AND the letterbox origin
                        // (`render_scale`/`render_origin`, set once a frame
                        // from `Renderer::screen_mapping()`) are needed to
                        // land on the same physical pixels the viewport
                        // above just placed the actual content at.
                        (
                            (self.render_origin.0 + r.x * self.render_scale.0).round() as u32,
                            (self.render_origin.1 + r.y * self.render_scale.1).round() as u32,
                            (r.w * self.render_scale.0).round() as u32,
                            (r.h * self.render_scale.1).round() as u32,
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
