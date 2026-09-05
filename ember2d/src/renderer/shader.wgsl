// shader.wgsl — Simple 2D sprite/character shader.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

struct InstanceInput {
    @location(2) pos: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) uv_offset: vec2<f32>,
    @location(5) uv_size: vec2<f32>,
    @location(6) color_fg: vec4<f32>,
    @location(7) color_bg: vec4<f32>,
    @location(8) mode: u32, // 0 = ASCII, 1 = Sprite
    @location(9) rotation: f32, // radians, about the instance's own center
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color_fg: vec4<f32>,
    @location(2) color_bg: vec4<f32>,
    @location(3) @interpolate(flat) mode: u32,
}

struct Globals {
    projection: mat4x4<f32>,
}

@group(1) @binding(0)
var<uniform> globals: Globals;

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    // Rotate the quad's corners about the instance's own center, then place
    // that center at `instance.pos + half_size`. At rotation == 0.0 this
    // reduces exactly to the old `instance.pos + model.position * instance.size`.
    let half_size = instance.size * 0.5;
    let center = instance.pos + half_size;
    let local = (model.position - vec2<f32>(0.5, 0.5)) * instance.size;
    let c = cos(instance.rotation);
    let s = sin(instance.rotation);
    let rotated = vec2<f32>(local.x * c - local.y * s, local.x * s + local.y * c);
    let world_pos = center + rotated;
    out.clip_position = globals.projection * vec4<f32>(world_pos, 0.0, 1.0);
    
    out.uv = instance.uv_offset + model.uv * instance.uv_size;
    out.color_fg = instance.color_fg;
    out.color_bg = instance.color_bg;
    out.mode = instance.mode;
    
    return out;
}

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.mode == 0u) {
        // ASCII Mode: Use red channel as font mask
        let font_sample = textureSample(t_diffuse, s_diffuse, in.uv).r;
        
        if (font_sample > 0.5) {
            return in.color_fg;
        } else {
            if (in.color_bg.a < 0.01) {
                discard;
            }
            return in.color_bg;
        }
    } else {
        // Sprite Mode: Standard texture sampling
        let tex_sample = textureSample(t_diffuse, s_diffuse, in.uv);
        
        // Multiply by fg color for tinting (standard 2D practice)
        let final_color = tex_sample * in.color_fg;
        
        if (final_color.a < 0.01) {
            discard;
        }
        return final_color;
    }
}
