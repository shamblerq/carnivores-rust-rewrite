// The water surfaces. Carnivores 2's: a see-through sheet at each body's
// level, clearer where it is shallow and seen from above. Carnivores': its
// lakes seen from below, drawn only while the hunter is under water.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import carn::common::{time_ms, camera_pos, fade_amount, sky_color, to_linear, view_radius_cells}

@group(2) @binding(0) var textures: texture_2d_array<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
@group(2) @binding(4) var cells: texture_2d<u32>;
// x: texture count, y: reverse-diagonal bit, z: map size in cells,
// w: units per height step
@group(2) @binding(7) var<uniform> params: vec4<f32>;
// y: under water, z: 1 for Carnivores
@group(2) @binding(8) var<uniform> frame: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Carnivores 2: x: depth in steps, y: the body's transparency,
    // z: random 0..1023, w: 1 over open water (not the shore ring).
    // Carnivores: x: 1 on a lake vertex, y: 1 where it sways, z: random
    // 0..1023, w: light map value.
    @location(1) attr: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) tpos: vec2<f32>,
    @location(2) light: f32,
    @location(3) alpha: f32,
};

const PI: f32 = 3.14159265;

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    var wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    out.tpos = wp.xz;
    let t = time_ms();
    let cam = camera_pos();
    let cell = floor(wp.xz / 256.0 + 0.5);
    let ccell = floor(cam.xz / 256.0);
    let r = max(abs(cell.x - ccell.x), abs(cell.y - ccell.y));

    if (frame.z > 0.5) {
        return c1_vertex(v, wp, cell);
    }

    let wdelta = sin(-PI / 2.0 + floor(v.attr.z / 128.0) + t / 200.0);
    if (v.attr.w > 0.5 && r < 24.0) {
        wp.x += sin(cell.x + cell.y + t / 200.0) * 16.0;
        wp.z += sin(PI / 2.0 + cell.x + cell.y + t / 200.0) * 16.0;
    }
    out.light = (168.0 - wdelta * 24.0) / 255.0;

    var alpha = 255.0;
    let rel = wp - cam;
    if (frame.y > 0.5) {
        alpha = max(10.0, 160.0 - length(rel) * 160.0 / 220.0 / 60.0);
    } else if (r < 30.0) {
        alpha = max(0.0, (v.attr.x * 2.0 + 4.0) * v.attr.y + length(rel) / 256.0 + wdelta * 2.0);
        // Clearer the steeper the look down at the bottom under the vertex.
        let bottom = vec3(v.position.x, v.position.y - v.attr.x * params.w, v.position.z) - cam;
        let dy = max(0.0, -normalize(bottom).y);
        alpha = min(255.0, alpha * 6.0 / (dy + 0.1));
    }
    out.alpha = alpha / 255.0;
    out.world = wp;
    out.clip = position_world_to_clip(wp);
    return out;
}

// Carnivores' surface from below: lit full where it covers the lake, the
// shore's own ripple light where it meets the land, and swaying.
fn c1_vertex(v: Vertex, p: vec3<f32>, cell: vec2<f32>) -> VOut {
    var out: VOut;
    var wp = p;
    out.tpos = wp.xz;
    if (frame.y < 0.5) {
        // Not under water: nothing to draw.
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let t = time_ms();
    if (v.attr.y > 0.5) {
        wp.x += sin(cell.x + cell.y + t / 256.0) * 20.0;
        wp.z += sin(PI / 2.0 + cell.x + cell.y + t / 256.0) * 20.0;
    }
    var clt = 0.0;
    if (v.attr.x < 0.5) {
        let wd = sin(-PI / 2.0 + v.attr.z / 512.0 + t / 400.0);
        clt = clamp(min(62.0, v.attr.w) + trunc(6.0 + wd * 8.0), 6.0, 44.0);
    }
    out.light = (255.0 - clt * 4.0) / 255.0;
    out.alpha = 1.0;
    out.world = wp;
    out.clip = position_world_to_clip(wp);
    return out;
}

// Texture 0 laid on the cell as the ground lays it, see-through: 0x70,
// thinning to 0x10 over the far half of the view.
fn c1_fragment(in: VOut) -> vec4<f32> {
    let g = in.tpos / 256.0;
    let gx = dpdx(g);
    let gy = dpdy(g);
    let size = i32(params.z);
    let c = textureLoad(cells, clamp(vec2<i32>(floor(g)), vec2<i32>(0), vec2<i32>(size - 1)), 0);
    let f = fract(g);
    let dir = c.b & 3u;
    var uv = f;
    var dx = gx;
    var dy = gy;
    if (dir == 1u) {
        uv = vec2(f.y, 1.0 - f.x);
        dx = vec2(gx.y, -gx.x);
        dy = vec2(gy.y, -gy.x);
    } else if (dir == 2u) {
        uv = vec2(1.0 - f.x, 1.0 - f.y);
        dx = -gx;
        dy = -gy;
    } else if (dir == 3u) {
        uv = vec2(1.0 - f.y, f.x);
        dx = vec2(-gx.y, gx.x);
        dy = vec2(-gy.y, gy.x);
    }
    let tex = textureSampleGrad(textures, tex_sampler, uv, 0, dx, dy);
    let reach = view_radius_cells() * 256.0;
    let d = length(in.world - camera_pos());
    if (d > reach) {
        discard;
    }
    let half = reach / 2.0;
    let a = 112.0 - 96.0 * clamp((d - half) / half, 0.0, 1.0);
    return vec4<f32>(to_linear(tex.rgb * in.light), a / 255.0);
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    if (frame.z > 0.5) {
        return c1_fragment(in);
    }
    let g = in.tpos / 256.0;
    let size = i32(params.z);
    let ci = clamp(vec2<i32>(floor(g)), vec2<i32>(0), vec2<i32>(size - 1));
    let c = textureLoad(cells, ci, 0);
    let layer = min(c.a, u32(params.x) - 1u);
    let t = textureSample(textures, tex_sampler, fract(g), i32(layer));
    var col = t.rgb * in.light;
    let fa = fade_amount(in.world);
    if (fa >= 1.0) {
        discard;
    }
    col = mix(col, sky_color(normalize(in.world - camera_pos())), fa);
    return vec4<f32>(to_linear(col), in.alpha);
}
