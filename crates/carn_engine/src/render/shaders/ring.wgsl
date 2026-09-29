// Rings spreading on the water: the ring model, added onto
// what is behind it and fading as it grows.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip

@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var tex_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // how strongly it adds, 0..1
    @location(2) alpha: f32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
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
    out.clip = position_world_to_clip(mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz);
    out.uv = v.uv;
    out.alpha = v.alpha;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, tex_sampler, in.uv);
    // drawn at a light of 250
    return vec4<f32>(to_linear(t.rgb * (250.0 / 255.0)), in.alpha);
}
