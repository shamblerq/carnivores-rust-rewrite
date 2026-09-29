// Creature shadows: the posed model flattened onto the
// ground away from the sun.
//
// The flattened faces overlap one another, and the originals used the
// stencil so each pixel was darkened once. The main pass has no stencil, so
// a shadow writes only how much light it leaves into the frame's alpha,
// keeping the darkest; shadow_resolve.wgsl then darkens the frame by it.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::mesh_view_bindings::view

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // how much light the shadow leaves, 0..1
    @location(1) light: f32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) light: f32,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    let wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    // A hair in front of the ground so it wins the depth test there, as
    // the originals did by scaling the depth.
    out.clip = position_world_to_clip(mix(wp, view.world_position, 0.02));
    out.light = v.light;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, in.light);
}
