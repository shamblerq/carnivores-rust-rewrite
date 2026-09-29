// The sky: a textured plane high over the hunter that drifts with the
// wind and melts into the horizon colour with distance.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import carn::common::{camera_pos, sky_color, to_linear}


struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    let wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    out.world = wp;
    out.clip = position_world_to_clip(wp);
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(to_linear(sky_color(normalize(in.world - camera_pos()))), 1.0);
}
