// Darkens the frame where shadows were drawn (see shadow.wgsl): the frame's
// alpha holds how much light is left, and the shadow's colour fills the
// rest. Drawn with the shadows' own shapes, each in its colour, putting
// the alpha back to opaque as it goes - so where the shapes overlap, the
// first one there does it and the rest find nothing left to do.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::mesh_view_bindings::view

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // the shadow's colour, linear
    @location(1) color: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    let wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    out.clip = position_world_to_clip(mix(wp, view.world_position, 0.02));
    out.color = v.color.rgb;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
