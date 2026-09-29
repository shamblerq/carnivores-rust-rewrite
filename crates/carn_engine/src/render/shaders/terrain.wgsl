// The ground.
//
// The mesh is the height map, one vertex per map vertex. Which texture a
// cell shows, how it is turned and which way its diagonal runs are read per
// fragment from the cell table, so neighbouring cells can share vertices.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import carn::common::{ground_fog, finish, time_ms}

@group(2) @binding(0) var textures: texture_2d_array<f32>;
@group(2) @binding(1) var tex_sampler: sampler;
// r: first triangle's texture, g: second's, b: the cell's flags,
// a: the water body's texture (Carnivores 2), 0xFFFF for none.
@group(2) @binding(4) var cells: texture_2d<u32>;
@group(2) @binding(5) var skymap: texture_2d<f32>;
@group(2) @binding(6) var sky_sampler: sampler;
// x: texture count, y: reverse-diagonal bit, z: map size in cells,
// w: 1 for Carnivores
@group(2) @binding(7) var<uniform> params: vec4<f32>;
// x: cloud shadows on, y: under water (the fog uniform is not visible to
// vertex shaders, so what they need of the frame comes this way)
@group(2) @binding(8) var<uniform> frame: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // x: light map value, y: random 0..1023, z: 1 on Carnivores water,
    // w: Carnivores' lake bottom (units), where the ground is drawn while
    // the hunter is under water
    @location(1) attr: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // Where the vertex is before any water motion: textures follow it.
    @location(1) tpos: vec2<f32>,
    @location(2) light: f32,
};

const PI: f32 = 3.14159265;

@vertex
fn vertex(v: Vertex) -> VOut {
    var out: VOut;
    let m = get_world_from_local(v.instance_index);
    var wp = mesh_position_local_to_world(m, vec4<f32>(v.position, 1.0)).xyz;
    out.tpos = wp.xz;
    let t = time_ms();
    let c1 = params.w > 0.5;
    let under = frame.y > 0.5;
    let cell = floor(wp.xz / 256.0 + 0.5);
    if (c1 && under) {
        wp.y = v.attr.w;
    }

    // Moving cloud shadows: the sky's shadow map scrolls one cell every
    // half second.
    var db = 0.0;
    if (frame.x > 0.5) {
        let s = textureSampleLevel(skymap, sky_sampler, (cell + vec2(t / 512.0) + 0.5) / 128.0, 0.0).r * 255.0;
        if (c1) {
            db = clamp(s / 8.0 - 10.0, 0.0, 12.0);
        } else {
            db = clamp(s / 2.0 - 40.0, 0.0, 48.0);
        }
    }

    if (c1) {
        // Light map is darkness, 0..63.
        var clt = min(62.0, v.attr.x + db);
        let r = v.attr.y;
        if (under) {
            // Under water all the ground ripples, a little lighter.
            let wd = sin(-PI / 2.0 + r / 512.0 + t / 400.0);
            clt = clamp(clt + trunc(6.0 + wd * 8.0), 6.0, 44.0);
        } else if (v.attr.z > 0.5) {
            // Water: the vertices sway and the light ripples.
            let skyd = t / 2.0;
            wp.x += sin(cell.x + cell.y + skyd / 124.0) * 16.0;
            wp.z += sin(PI / 2.0 + cell.x + cell.y + skyd / 124.0) * 16.0;
            let wd = sin(-PI / 2.0 + r / 512.0 + t / (400.0 + r / 512.0)) * 6.0;
            clt = clamp(clt + trunc(wd), 6.0, 44.0);
        }
        out.light = (255.0 - clt * 4.0) / 255.0;
    } else {
        out.light = max(64.0, v.attr.x - db) / 255.0;
    }

    out.world = wp;
    out.clip = position_world_to_clip(wp);
    return out;
}

@fragment
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let g = in.tpos / 256.0;
    let gx = dpdx(g);
    let gy = dpdy(g);

    let size = i32(params.z);
    let ci = clamp(vec2<i32>(floor(g)), vec2<i32>(0), vec2<i32>(size - 1));
    let c = textureLoad(cells, ci, 0);
    let f = fract(g);
    let rev = (c.b & u32(params.y)) != 0u;

    // Which of the cell's two triangles this is.
    var second = f.y > f.x;
    if (rev) {
        second = f.x + f.y > 1.0;
    }
    var layer = c.r;
    if (second) {
        layer = c.g;
    }
    // Carnivores' lake beds take texture 1 where the surface shows water.
    if (params.w > 0.5 && frame.y > 0.5 && layer == 0u) {
        layer = 1u;
    }
    layer = min(layer, u32(params.x) - 1u);

    // Quarter-turn of the texture: the cell's four cases.
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

    let tex = textureSampleGrad(textures, tex_sampler, uv, i32(layer), dx, dy);
    let col = tex.rgb * in.light;
    return vec4<f32>(finish(col, ground_fog(in.world), in.world), 1.0);
}
