// Shared by the world shaders: the per-frame values, the ground fogs and
// the colour conversion.
//
// Per-frame values ride in the view's fog uniform (the camera's DistanceFog
// component, which nothing else reads): see render/mod.rs, update_globals.
//
//   fog.base_color.rgb              horizon (fade) colour, 0..1 as authored
//   fog.base_color.a                distance where the world has faded out
//   fog.be.x                        distance where the fade starts
//   fog.be.y                        underwater: the water's level (units)
//   fog.be.z                        view radius in cells
//   fog.bi.x                        cloud shadows on (1) or off (0)
//   fog.bi.y                        1 for Carnivores, 2 for Carnivores 2
//   fog.bi.z                        units per height-map step
//   fog.directional_light_color.rgb underwater fog colour
//   fog.directional_light_color.a   1 under water
//   fog.directional_light_exponent  1 when fogs are drawn (the Fog option)
//
// The games did all their colour arithmetic on the stored values, as if
// they were light levels, and sent the result to the screen as is. The
// shaders do the same and convert to linear only at the very end, so the
// sRGB framebuffer shows exactly those values.

#define_import_path carn::common

#import bevy_pbr::mesh_view_bindings::{view, fog, globals}

@group(2) @binding(2) var fog_map: texture_2d<u32>;
@group(2) @binding(3) var fog_table: texture_2d<f32>;
@group(2) @binding(10) var sky_tex: texture_2d<f32>;
@group(2) @binding(11) var sky_sampler: sampler;

fn fade_color() -> vec3<f32> { return fog.base_color.rgb; }
fn fade_start() -> f32 { return fog.be.x; }
fn fade_end() -> f32 { return fog.base_color.a; }
fn view_radius_cells() -> f32 { return fog.be.z; }
fn clouds_on() -> bool { return fog.bi.x > 0.5; }
fn is_c1() -> bool { return fog.bi.y < 1.5; }
fn height_scale() -> f32 { return fog.bi.z; }
fn underwater() -> bool { return fog.directional_light_color.a > 0.5; }
fn fog_on() -> bool { return fog.directional_light_exponent > 0.5; }
fn camera_pos() -> vec3<f32> { return view.world_position; }

// Milliseconds, like the games' clock. Bevy's clock wraps every hour.
fn time_ms() -> f32 { return globals.time * 1000.0; }

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let cc = clamp(c, vec3(0.0), vec3(1.0));
    let lo = cc / 12.92;
    let hi = pow((cc + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, cc <= vec3(0.04045));
}

// How far into the distance fade a point is, 0 (clear) to 1 (gone).
fn fade_amount(p: vec3<f32>) -> f32 {
    let d = length(p - camera_pos());
    return clamp((d - fade_start()) / max(fade_end() - fade_start(), 1.0), 0.0, 1.0);
}

fn fog_zone_at(p: vec3<f32>) -> u32 {
    let dim = vec2<i32>(textureDimensions(fog_map));
    let c = clamp(vec2<i32>(floor(p.xz / 512.0)), vec2<i32>(0), dim - vec2<i32>(1));
    return textureLoad(fog_map, c, 0).r;
}

struct FogEntity {
    rgb: vec3<f32>,
    // Top of the fog, in units.
    top: f32,
    transp: f32,
    limit: f32,
}

fn fog_entity(i: u32) -> FogEntity {
    var e: FogEntity;
    if (i == 127u && underwater()) {
        e.rgb = fog.directional_light_color.rgb;
        e.top = fog.be.y;
        e.transp = select(460.0, 450.0, is_c1());
        e.limit = select(200.0, 160.0, is_c1());
        return e;
    }
    let a = textureLoad(fog_table, vec2<i32>(i32(i), 0), 0);
    let b = textureLoad(fog_table, vec2<i32>(i32(i), 1), 0);
    e.rgb = a.rgb;
    e.top = a.a;
    e.transp = max(b.x, 1.0);
    e.limit = b.y;
    return e;
}

// The ground fogs: a fog is a layer of air below a height
// over the zones marked with it, and a point is fogged by the length of its
// sight line that runs through the layer. `cf` is the zone to use for the
// point. Returns the fog's colour and the amount, 0..1.
fn fog_for_zone(cf_in: u32, p: vec3<f32>) -> vec4<f32> {
    let cam = camera_pos();
    var cf = cf_in;
    var cam_cf = fog_zone_at(cam);
    if (underwater()) {
        cf = 127u;
        // Carnivores 2 counts the camera as inside the water's fog;
        // Carnivores goes by the fog zone the camera is over, so the water
        // there fogs by the depth of what is seen alone.
        if (!is_c1()) {
            cam_cf = 127u;
        }
    }
    var cam_in = false;
    if (cam_cf > 0u) {
        cam_in = fog_entity(cam_cf).top > cam.y;
    }
    var vinfog = true;
    if (cf == 0u && cam_in) {
        cf = cam_cf;
        vinfog = false;
    }
    if (!cam_in && cf == 0u) {
        return vec4<f32>(0.0);
    }
    let e = fog_entity(cf);
    let hs = height_scale();
    var d = length(p - cam);
    var fla = -(p.y - e.top) / hs;
    if (!vinfog && fla > 0.0) { fla = 0.0; }
    var flb = -(cam.y - e.top) / hs;
    if (!cam_in && flb > 0.0) { flb = 0.0; }
    if (fla < 0.0 && flb < 0.0) {
        return vec4<f32>(e.rgb, 0.0);
    }
    if (fla < 0.0) { d = d * flb / (flb - fla); fla = 0.0; }
    if (flb < 0.0) { d = d * fla / (fla - flb); flb = 0.0; }
    let fl = (fla + flb) * (d + e.transp / 2.0) / e.transp;
    return vec4<f32>(e.rgb, clamp(min(fl, e.limit) / 255.0, 0.0, 1.0));
}

fn zone_at_vertex(c: vec2<f32>) -> u32 {
    let dim = vec2<i32>(textureDimensions(fog_map));
    let i = clamp(vec2<i32>(c) / 2, vec2<i32>(0), dim - vec2<i32>(1));
    return textureLoad(fog_map, i, 0).r;
}

// The originals worked the fog out at each map vertex, from the zone the
// vertex is in, and blended across the triangles. Here it is worked out
// for the point itself once per surrounding vertex's zone and blended the
// same way, so the edge of a fog zone stays soft.
fn ground_fog(p: vec3<f32>) -> vec4<f32> {
    if (!fog_on()) {
        return vec4<f32>(0.0);
    }
    if (underwater()) {
        return fog_for_zone(127u, p);
    }
    let g = p.xz / 256.0;
    let c0 = floor(g);
    let f = g - c0;
    let z00 = zone_at_vertex(c0);
    let z10 = zone_at_vertex(c0 + vec2(1.0, 0.0));
    let z01 = zone_at_vertex(c0 + vec2(0.0, 1.0));
    let z11 = zone_at_vertex(c0 + vec2(1.0, 1.0));
    if (z00 == z10 && z00 == z01 && z00 == z11) {
        return fog_for_zone(z00, p);
    }
    let a = fog_for_zone(z00, p);
    let b = fog_for_zone(z10, p);
    let c = fog_for_zone(z01, p);
    let d = fog_for_zone(z11, p);
    let w = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    let amount = w.x * a.a + w.y * b.a + w.z * c.a + w.w * d.a;
    let rgb = (w.x * a.a * a.rgb + w.y * b.a * b.rgb + w.z * c.a * c.rgb + w.w * d.a * d.rgb) / max(amount, 1e-5);
    return vec4<f32>(rgb, amount);
}

// The sky plane's colour where it meets a sight line: a
// drifting picture 32768 units over the eye that melts into the horizon
// colour with distance. Below the horizon, the horizon colour.
fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let cam = camera_pos();
    if (dir.y <= 0.0005) {
        return fade_color();
    }
    let t = 32768.0 / dir.y;
    let hit = cam + dir * t;
    let dtt = (time_ms() % 131072.0) / 512.0;
    let uv = vec2(hit.x * 0.002 + dtt, hit.z * 0.002 - dtt) / 256.0;
    let tex = textureSampleLevel(sky_tex, sky_sampler, uv, 0.0).rgb;
    let d = t * length(dir.xz);
    let a = 40240.0 / (40240.0 + max(0.0, d - 100200.0));
    return mix(fade_color(), tex, a);
}

// Final colour of a world fragment: fogged, faded into the sky behind it
// towards the edge of the view (the originals faded the far ground out
// with alpha), and converted for the framebuffer. Past the edge nothing is
// drawn.
fn finish(c: vec3<f32>, gfog: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    let fa = fade_amount(p);
    if (fa >= 1.0) {
        discard;
    }
    var col = mix(c, gfog.rgb, gfog.a);
    if (fa > 0.0) {
        col = mix(col, sky_color(normalize(p - camera_pos())), fa);
    }
    return to_linear(col);
}
