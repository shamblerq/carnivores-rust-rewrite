// Particles: little discs facing the view, one colour at the
// centre and another at the rim, as the games stored them. No fog: the
// games drew them without.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
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
    out.color = v.color;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(to_linear(in.color.rgb), in.color.a);
}
