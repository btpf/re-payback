// Reflect (water) faces, after the software renderer (docs/re/rendering.md §3):
// the face texture is a displacement map. For each screen pixel (sx, sy) on
// the base 320x256 screen, with water texel value t, the output is
//     BG[sy * 320 + sx + t]
// where BG is the background picture rescaled to 320x266. Scrolling ('#')
// textures read their texels `scroll` bytes further along.
// In the rotating view (`FUN_01081ba4` / `FUN_0108a2a0`, software) the row
// read is sy / 2 + tilt rows instead of sy, where
// tilt rows = (pitch - 0x157c) * 127 / 259 (127 = the view's centre row):
// rows 127..255 of the background looking straight down, 0..127 at the
// full tilt. The reflection does not turn with the view.
//
// Hardware (Warp3D) renderer, params.z = 1 (`FUN_010920d8`, Gouraud path,
// docs/re/rendering.md "Hardware (Warp3D) renderer"): the quad is textured
// with the 256x256 background (bilinear, repeating) at, per vertex,
//     u = 128 + 8 * dx / fx + 255 * k / 16384
//     v = 128 + 8 * dy / fy + 255 * k / 16384
// (dx, dy = vertex screen position minus the screen centre, fx = fy = 16 on
// the 320x256 base screen), k = sin[i] (5760-entry table, amplitude 256)
// with i = ((g * 16 + scroll) * 90) mod 5759 for the scrolling #Water
// texture and (g * 1440) mod 5759 otherwise, g = the corner's grid x + y;
// the vertex colour is clamp(L + (k >> 3), 0, 255) / 256 with L the corner's
// Gouraud light, modulating the texture. Each quad is two triangles split
// along corner 1 - corner 3 (NE-SW).
// Rotating view (hardware branch of the reflect triangle filler, OS4
// `FUN_01081ba4` / `FUN_0108a2a0`, Mac `DrawAffineTriangleReflect` @
// 100ba910; CONFIRMED in both): with a = 0xb3f - yaw, b = 0x167f - yaw
// (yaw = the view's `DAT_0114c7d0`, 5760 per turn) and the 256-amplitude
// cos/sin tables C, S,
//     u = 128 + 8 * ((dx C[a] + dy S[a]) >> 8) / fx - k (C[b] + S[b]) / 16384
//     v = 128 + 8 * ((dy C[a] - dx S[a]) >> 8) / fy - k (C[b] - S[b]) / 16384
// i.e. the offsets turn with the yaw and, at yaw 0 (north up), point the
// opposite way to the top-down formula above.
//
// GAMMA_TARGET (the game views): the output is the gamma-encoded RGB.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}
#import bevy_pbr::mesh_view_bindings::view

// x = scroll (bytes, 0..63), y = 1 in the rotating view, z = 1 hardware
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var index_atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sky: texture_2d<f32>;

const ATLAS_SIZE: f32 = 2048.0;
const SCROLL_FLAG: f32 = 100000.0;
const BG_W: i32 = 320;
const BG_H: i32 = 266;
const TAU: f32 = 6.2831853;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) uv_b: vec2<f32>,
    @location(3) st: vec2<f32>,
    @location(4) p0: vec3<f32>,
    @location(5) p1: vec3<f32>,
    @location(6) p2: vec3<f32>,
    @location(7) p3: vec3<f32>,
    @location(8) l0: vec3<f32>,
    @location(9) l1: vec3<f32>,
    @location(10) l2: vec3<f32>,
    @location(11) l3: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // The tile origin is the same on all four corners: flat, so it is
    // never interpolated (a perspective-interpolated 101984 lands on
    // 101983.99 near the quad edges, which picked the neighbouring atlas
    // tile's texel there and drew a seam along every square).
    @location(1) @interpolate(flat) uv_b: vec2<f32>,
    @location(2) st: vec2<f32>,
    // Hardware: per corner the vertex colour (rgb) and the wobble k (w).
    @location(3) @interpolate(flat) c0: vec4<f32>,
    @location(4) @interpolate(flat) c1: vec4<f32>,
    @location(5) @interpolate(flat) c2: vec4<f32>,
    @location(6) @interpolate(flat) c3: vec4<f32>,
};

// Wobble of the corner at world position p (render x = map x, z = map y).
fn wobble(p: vec3<f32>, scrolls: bool) -> f32 {
    let g = round(p.x) + round(p.z);
    var i: f32;
    if scrolls {
        i = ((g * 16.0 + params.x) * 90.0) % 5759.0;
    } else {
        i = (g * 1440.0) % 5759.0;
    }
    return trunc(sin(i * TAU / 5760.0) * 256.0);
}

fn corner(p: vec3<f32>, l: vec3<f32>, scrolls: bool) -> vec4<f32> {
    let k = wobble(p, scrolls);
    let c = clamp(l + vec3<f32>(floor(k / 8.0)), vec3<f32>(0.0), vec3<f32>(255.0)) / 256.0;
    return vec4<f32>(c, k);
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.position = position_world_to_clip(wp.xyz);
    out.uv = v.uv;
    out.uv_b = v.uv_b;
    out.st = v.st;
    let scrolls = v.uv_b.x >= SCROLL_FLAG;
    let w = array<vec3<f32>, 4>(
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p0, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p1, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p2, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p3, 1.0)).xyz,
    );
    out.c0 = corner(w[0], v.l0, scrolls);
    out.c1 = corner(w[1], v.l1, scrolls);
    out.c2 = corner(w[2], v.l2, scrolls);
    out.c3 = corner(w[3], v.l3, scrolls);
    return out;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3(0.0)) + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

// The colour written to the target from gamma-encoded RGB.
fn out_colour(c: vec3<f32>) -> vec3<f32> {
#ifdef GAMMA_TARGET
    return c;
#else
    return to_linear(c);
#endif
}

// cos / sin table entry i (5760 per turn, amplitude 256).
fn cs_table(i: f32) -> vec2<f32> {
    let a = i * TAU / 5760.0;
    return round(vec2<f32>(cos(a), sin(a)) * 256.0);
}

// One background texel, gamma-encoded as the card holds it.
fn sky_texel(p: vec2<f32>) -> vec4<f32> {
    let q = (vec2<i32>(p) % vec2<i32>(256) + vec2<i32>(256)) % vec2<i32>(256);
    return vec4<f32>(to_srgb(textureLoad(sky, q, 0).rgb), 1.0);
}

fn fragment_hw(in: VertexOutput) -> vec4<f32> {
    // Per-triangle interpolation of the corner values (0-1-3, 1-2-3).
    let st = clamp(in.st, vec2(0.0), vec2(1.0));
    var c: vec4<f32>;
    if st.x + st.y <= 1.0 {
        c = in.c0 + st.x * (in.c1 - in.c0) + st.y * (in.c3 - in.c0);
    } else {
        c = in.c2 + (1.0 - st.x) * (in.c3 - in.c2) + (1.0 - st.y) * (in.c1 - in.c2);
    }
    // Screen offset from the centre on the 320x256 base screen.
    let vp = view.viewport;
    var d = vec2<f32>(
        (in.position.x - vp.x) / vp.z * 320.0 - 160.0,
        (in.position.y - vp.y) / vp.w * 256.0 - 128.0,
    );
    var uv = vec2<f32>(128.0) + 8.0 * d / 16.0 + vec2<f32>(255.0 * c.w / 16384.0);
    if (params.y > 0.5) {
        // The view's yaw: the camera's right vector is (cos, sin) of
        // yaw · 2π / 5760 on the map (`camera.rs` `view_transform`).
        let right = view.world_from_view[0].xyz;
        let yaw = atan2(right.z, right.x) * 5760.0 / TAU;
        let ca = cs_table(2879.0 - yaw);
        let cb = cs_table(5759.0 - yaw);
        let off = vec2<f32>(d.x * ca.x + d.y * ca.y, d.y * ca.x - d.x * ca.y) / 256.0;
        let k = c.w;
        uv = vec2<f32>(128.0) + 8.0 * off / 16.0
            - k * vec2<f32>(cb.x + cb.y, cb.x - cb.y) / 16384.0;
    }
    // Bilinear, texel centres at +0.5 (sky holds sRGB; filter encoded).
    let p = uv - vec2<f32>(0.5);
    let i = floor(p);
    let f = p - i;
    let t = mix(
        mix(sky_texel(i), sky_texel(i + vec2(1.0, 0.0)), f.x),
        mix(sky_texel(i + vec2(0.0, 1.0)), sky_texel(i + vec2(1.0, 1.0)), f.x),
        f.y,
    );
    return vec4<f32>(out_colour(t.rgb * c.rgb), 1.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if (params.z > 0.5) {
        return fragment_hw(in);
    }
    var tile = round(in.uv_b);
    var scroll = 0;
    if (tile.x >= SCROLL_FLAG) {
        tile.x -= SCROLL_FLAG;
        scroll = i32(params.x);
    }
    let local = clamp(floor(in.uv * ATLAS_SIZE - tile), vec2(0.0), vec2(63.0));
    let lin = (i32(local.y) * 64 + i32(local.x) + scroll) % 4096;
    let texel = vec2<i32>(i32(tile.x) + lin % 64, i32(tile.y) + lin / 64);
    let t = i32(round(textureLoad(index_atlas, texel, 0).r * 255.0));

    let vp = view.viewport;
    let sx = i32(floor((in.position.x - vp.x) / vp.z * 320.0));
    var sy = i32(floor((in.position.y - vp.y) / vp.w * 256.0));
    if (params.y > 0.5) {
        // Pitch from the view's tilt (0x1680 per turn, 0x167f straight
        // down); camera forward = -Z of world_from_view.
        let forward = -view.world_from_view[2].xyz;
        let tilt = acos(clamp(-forward.y, -1.0, 1.0));
        let pitch = 5760.0 - tilt * 5760.0 / 6.2831853;
        let rows = i32(floor(clamp(pitch - 5500.0, 0.0, 259.0) * 127.0 / 259.0));
        sy = sy / 2 + rows;
    }
    let idx = sy * BG_W + sx + t;
    let p = vec2<i32>(idx % BG_W, clamp(idx / BG_W, 0, BG_H - 1));
#ifdef GAMMA_TARGET
    return vec4<f32>(to_srgb(textureLoad(sky, p, 0).rgb), 1.0);
#else
    return textureLoad(sky, p, 0);
#endif
}
