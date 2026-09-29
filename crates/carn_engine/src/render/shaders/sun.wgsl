// The sun or the moon: its model, added onto the sky
// behind everything else - the games drew it right after the sky, at the
// farthest depth, so all the world covers it.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip

@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
// x: how strongly it adds, 0..1
@group(2) @binding(2) var<uniform> params: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3(0.055)) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    var clip = position_world_to_clip(mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz);
    // As far as depth goes (reversed: 0 is infinitely far).
    clip.z = clip.w * 1e-7;
    out.clip = clip;
    out.uv = v.uv;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, tex_sampler, in.uv);
    return vec4<f32>(to_linear(t.rgb), params.x);
}
