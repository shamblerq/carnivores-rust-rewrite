// Models: the objects standing on the map, and (later) creatures.
//
// A vertex's brightness is the light of the spot the model stands on,
// carried per instance in the mesh tag, plus the vertex's own sun light
// baked into the mesh.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world, get_tag}
#import bevy_pbr::view_transformations::position_world_to_clip
#import carn::common::{ground_fog, finish, to_linear}
#import bevy_pbr::mesh_view_bindings::view

@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
// x: past this distance a map object with a sprite leaves the drawing to it
@group(2) @binding(4) var<uniform> lod: vec4<f32>;

#ifdef WEAPON_FX
// A weapon's shine (render::WeaponMaterial): the highlights and the
// environment map, holding their values as the games stored them.
@group(2) @binding(5) var specular_tex: texture_2d<f32>;
@group(2) @binding(6) var specular_sampler: sampler;
@group(2) @binding(7) var envmap_tex: texture_2d<f32>;
@group(2) @binding(8) var envmap_sampler: sampler;
// rgb: the highlights' colour, as stored
@group(2) @binding(9) var<uniform> phong_color: vec4<f32>;

fn to_gamma(c: vec3<f32>) -> vec3<f32> {
    let cc = clamp(c, vec3(0.0), vec3(1.0));
    let lo = cc * 12.92;
    let hi = 1.055 * pow(cc, vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, cc <= vec3(0.0031308));
}
#endif

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // x: the vertex's own light; y: 1 on colour-keyed faces, plus 2 on faces
    // seen from the front only (render::face_kind), and on a weapon 4 for
    // the highlights and 8 for the environment map
    @location(2) attr: vec2<f32>,
#ifdef WEAPON_FX
    // the highlights' texture coordinates, and the environment map's
    @location(3) fx_uv: vec4<f32>,
#endif
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
    @location(3) kind: f32,
#ifdef WEAPON_FX
    @location(4) fx_uv: vec4<f32>,
#endif
};

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    let tag = get_tag(v.instance_index);
    // Past the object distance the sprite stands in (see sprite.wgsl).
    if (lod.x > 0.0 && (tag & 0x100u) != 0u) {
        let foot = mesh_position_local_to_world(m, vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
        if (distance(foot, view.world_position) > lod.x) {
            out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
            return out;
        }
    }
    let wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    // The tag's low byte is the light of the object's spot, 0..255.
    let base = f32(tag & 0xFFu);
    out.light = clamp(base + v.attr.x, 0.0, 255.0) / 255.0;
    out.kind = v.attr.y;
#ifdef WEAPON_FX
    out.fx_uv = v.fx_uv;
#endif
    out.uv = v.uv;
    out.world = wp;
    out.clip = position_world_to_clip(wp);
    return out;
}

@fragment
fn fragment(in: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let kind = u32(in.kind + 0.5);
    // A one-sided face turned away is not drawn. The games kept the faces
    // that wind clockwise on the screen, which is the back
    // of this pipeline's counter-clockwise front.
    if ((kind & 2u) != 0u && front) {
        discard;
    }
    let t = textureSample(tex, tex_sampler, in.uv);
    if ((kind & 1u) != 0u && t.a < 0.5) {
        discard;
    }
    var col = finish(t.rgb * in.light, ground_fog(in.world), in.world);
#ifdef WEAPON_FX
    // The games added these over the drawn weapon, on the values they
    // stored: the highlights in their colour where a face takes them, then the environment map where it takes that.
    if ((kind & 12u) != 0u) {
        var g = to_gamma(col);
        if ((kind & 4u) != 0u) {
            g += textureSampleLevel(specular_tex, specular_sampler, in.fx_uv.xy, 0.0).rgb
                * phong_color.rgb;
        }
        if ((kind & 8u) != 0u) {
            g += textureSampleLevel(envmap_tex, envmap_sampler, in.fx_uv.zw, 0.0).rgb;
        }
        col = to_linear(min(g, vec3(1.0)));
    }
#endif
    return vec4<f32>(col, 1.0);
}
