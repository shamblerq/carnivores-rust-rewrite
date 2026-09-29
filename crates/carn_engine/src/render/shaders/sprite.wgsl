// Far objects: each drawn as its sprite, a flat picture
// standing upright at the object's foot and turned to face the eye, once
// it is past the object distance. The mesh holds a chunk's sprites of one
// kind; every corner carries the foot, and the shader stands it up.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::mesh_view_bindings::view
#import carn::common::{ground_fog, finish}

@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
// x: sprites only past this distance
@group(2) @binding(4) var<uniform> lod: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // the object's foot
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // x across, y up from the foot, z the light 0..1
    @location(2) attr: vec3<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    let foot = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    if (lod.x <= 0.0 || distance(foot, view.world_position) <= lod.x) {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    // Across the view, level; up as the world's up.
    let r = view.world_from_view[0].xyz;
    let right = normalize(vec3<f32>(r.x, 0.0, r.z));
    let wp = foot + right * v.attr.x + vec3<f32>(0.0, v.attr.y, 0.0);
    out.world = wp;
    out.clip = position_world_to_clip(wp);
    out.uv = v.uv;
    out.light = v.attr.z;
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, tex_sampler, in.uv);
    if (t.a < 0.5) {
        discard;
    }
    return vec4<f32>(finish(t.rgb * in.light, ground_fog(in.world), in.world), 1.0);
}
