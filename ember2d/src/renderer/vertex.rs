// renderer/vertex.rs — GPU vertex/instance/uniform layout types for
// WgpuBackend's render pipeline. Split out of backend.rs (7B-3,
// docs/ember2d-master-plan.md §5.2) once that file crossed the project's
// 750-line hard limit (CLAUDE.md) adding `TextureBudget` (R26) — pure data
// layout, no logic depending on anything else in `backend`, so this was
// the next-cleanest unit to pull out after `TextureBudget` itself still
// wasn't enough on its own.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    pub(super) fn desc() -> wgpu::VertexBufferLayout<'static> {
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
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_size: [f32; 2],
    pub color_fg: [f32; 4],
    pub color_bg: [f32; 4],
    pub mode: u32, // 0 = ASCII, 1 = Sprite
    /// Radians, applied in the vertex shader around the instance's own
    /// center (Phase 2 — replaces what used to be unused padding here).
    pub rotation: f32,
}

impl SpriteInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        2 => Float32x2, 3 => Float32x2, 4 => Float32x2, 5 => Float32x2, 6 => Float32x4, 7 => Float32x4, 8 => Uint32, 9 => Float32
    ];

    pub(super) fn desc() -> wgpu::VertexBufferLayout<'static> {
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
pub(super) struct Batch {
    pub(super) texture_id: u64,
    pub(super) instance_range: std::ops::Range<u32>,
    pub(super) scissor: Option<(u32, u32, u32, u32)>,
}
