// City cube faces with the Gouraud light (docs/re/rendering.md §4,
// `crates/payback/src/lighting.rs`). Every vertex carries its quad's four
// corners and their static light (128 = unchanged); the dynamic lights add
// min(255, I / d²) per channel at each corner, d in object units (2048 per
// square, 16 per h-unit), for corners inside the light's square box. Each
// corner's light is clamped to 255 and interpolated across the quad,
// quantised to 8 (`& 0xF8`) and added to the texel: pal[texel] + light −
// 128 (`Gouraud.256`), in gamma-encoded RGB. The palette snap of
// `Gouraud.256` is done by the view's resolve pass (`resolve.wgsl`).
//
// Interpolation (CONFIRMED, Mac `drawGroundFlat2GouraudColour` @ 100b5180,
// `drawGroundVertGouraudColour` @ 100b6260 — also the top-down walls —
// and the Horiz variants; OS4 `FUN_0107a5d8`): the top-down rasteriser
// walks rows with the perspective-correct row parameter, interpolates the
// light down the two edges by it and linearly across the span; a span is
// a row of constant depth, so the result is the bilinear blend of the four
// corners at the texel's (s, t), as here. The rotating view
// (`lights.params.z = 1`) splits every quad into two triangles along
// corner 1 - corner 3 (tops (NE, NW, SW) + (SE, NE, SW),
// `DrawAffineTriangleGouraud` @ 100c36a0; walls the same diagonal,
// `DrawPerspectiveTriangleGouraud` @ 100bdbf0 from `drawCubeSide_North`
// @ 100d9a50 etc.) and interpolates the light linearly in screen space per
// triangle (affine fillers; INFERRED for the walls' perspective filler).
//
// GAMMA_TARGET (the game views, `postfx.rs` `GammaWorld`): the output is
// the gamma-encoded palette RGB itself, so the Alpha faces' 50 % blend
// averages palette bytes (`Alpha.256` table 4) as the original does.
//
// Hardware (Warp3D) look, `lights.params.y = 1` (docs/re/rendering.md,
// "Hardware (Warp3D) renderer"; `FUN_010920d8`, `FUN_010a4004`): the
// texture is A1R5G5B5 (5 bits per channel, colour 0 transparent),
// bilinear filtered (W3D_SetFilter LINEAR/LINEAR) and repeated in its tile;
// the light is a W3D vertex colour `min(L, 255) / 256`, interpolated per
// triangle (each quad is drawn as two triangles split along its corner 1 -
// corner 3 diagonal: NE-SW on tops), not quantised, and multiplies the
// texel (the default MODULATE texture environment).

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

const MAX_LIGHTS: u32 = #{MAX_LIGHTS}u;

struct Lights {
    // x = number of lights, y = 1 hardware look, z = 1 rotating view
    params: vec4<f32>,
    // per light: (render position, square radius), (R, G, B, 0)
    data: array<vec4<f32>, #{LIGHT_SLOTS}>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> lights: Lights;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) st: vec2<f32>,
    @location(3) p0: vec3<f32>,
    @location(4) p1: vec3<f32>,
    @location(5) p2: vec3<f32>,
    @location(6) p3: vec3<f32>,
    @location(7) l0: vec3<f32>,
    @location(8) l1: vec3<f32>,
    @location(9) l2: vec3<f32>,
    @location(10) l3: vec3<f32>,
    @location(11) tile: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) st: vec2<f32>,
    @location(2) @interpolate(flat) l0: vec3<f32>,
    @location(3) @interpolate(flat) l1: vec3<f32>,
    @location(4) @interpolate(flat) l2: vec3<f32>,
    @location(5) @interpolate(flat) l3: vec3<f32>,
    @location(6) @interpolate(flat) tile: vec2<f32>,
    // The vertex's own corner light, linear in screen space (the rotating
    // view's per-triangle Gouraud; the mesh splits along corner 1 - 3).
#ifdef WEBGL2
    // WebGL2 (GLSL ES 3.0) has no noperspective: own * w and w, divided per
    // fragment, interpolate linearly in screen space all the same.
    @location(7) own: vec4<f32>,
#else
    @location(7) @interpolate(linear) own: vec3<f32>,
#endif
};

// Static plus dynamic light at a corner (world position), clamped to 255.
fn corner_light(wp: vec3<f32>, base: vec3<f32>) -> vec3<f32> {
    var light = base;
    let n = min(u32(lights.params.x), MAX_LIGHTS);
    // Corner grid position (squares).
    let g = round(wp.xz);
    for (var i = 0u; i < n; i = i + 1u) {
        let p = lights.data[2u * i];
        let s = floor(p.xz);
        let lo = max(s - vec2<f32>(p.w), vec2<f32>(1.0)) + vec2<f32>(1.0);
        let hi = min(s + vec2<f32>(p.w), vec2<f32>(126.0));
        if any(g < lo) || any(g > hi) {
            continue;
        }
        let c = lights.data[2u * i + 1u];
        let d = (wp - p.xyz) * vec3<f32>(2048.0, 4096.0, 2048.0);
        let d2 = max(dot(d, d), 1.0);
        light += floor(min(c.rgb / d2, vec3<f32>(255.0)));
    }
    return clamp(light, vec3<f32>(0.0), vec3<f32>(255.0));
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.clip = position_world_to_clip(wp.xyz);
    out.uv = v.uv;
    out.st = v.st;
    out.tile = v.tile;
    let w = array<vec3<f32>, 4>(
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p0, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p1, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p2, 1.0)).xyz,
        mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.p3, 1.0)).xyz,
    );
    out.l0 = corner_light(w[0], v.l0);
    out.l1 = corner_light(w[1], v.l1);
    out.l2 = corner_light(w[2], v.l2);
    out.l3 = corner_light(w[3], v.l3);
    // Corner of this vertex: st (0,0), (1,0), (1,1), (0,1).
    let corners = array<vec3<f32>, 4>(out.l0, out.l1, out.l2, out.l3);
    let k = select(select(3u, 2u, v.st.x > 0.5), select(0u, 1u, v.st.x > 0.5), v.st.y < 0.5);
#ifdef WEBGL2
    out.own = vec4<f32>(corners[k] * out.clip.w, out.clip.w);
#else
    out.own = corners[k];
#endif
    return out;
}

// Exact sRGB transfer functions (the textures are decoded exactly).
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

// The colour written to the target: the gamma-encoded value itself in a
// game view (GAMMA_TARGET), linear light otherwise.
fn out_colour(c: vec3<f32>) -> vec3<f32> {
#ifdef GAMMA_TARGET
    return c;
#else
    return to_linear(c);
#endif
}

// One texel of the 64x64 tile (wrapping), as the card holds it:
// gamma-encoded A1R5G5B5 expanded to 8 bits.
fn texel_hw(tile: vec2<f32>, p: vec2<f32>) -> vec4<f32> {
    let q = vec2<i32>(tile) + (vec2<i32>(p) % vec2<i32>(64) + vec2<i32>(64)) % vec2<i32>(64);
    let t = textureLoad(atlas, q, 0);
    let c5 = floor(round(to_srgb(t.rgb) * 255.0) / 8.0);
    let c8 = (c5 * 8.0 + floor(c5 / 4.0)) / 255.0;
    return vec4<f32>(c8, select(0.0, 1.0, t.a > 0.5));
}

fn fragment_hw(in: VertexOutput) -> vec4<f32> {
    // Bilinear: texel centres at +0.5.
    let p = in.uv * 2048.0 - in.tile - vec2<f32>(0.5);
    let i = floor(p);
    let f = p - i;
    let t = mix(
        mix(texel_hw(in.tile, i), texel_hw(in.tile, i + vec2(1.0, 0.0)), f.x),
        mix(texel_hw(in.tile, i + vec2(0.0, 1.0)), texel_hw(in.tile, i + vec2(1.0, 1.0)), f.x),
        f.y,
    );
    // Gouraud per triangle (corners 0-1-3 and 1-2-3).
    let st = clamp(in.st, vec2(0.0), vec2(1.0));
    var light: vec3<f32>;
    if st.x + st.y <= 1.0 {
        light = in.l0 + st.x * (in.l1 - in.l0) + st.y * (in.l3 - in.l0);
    } else {
        light = in.l2 + (1.0 - st.x) * (in.l3 - in.l2) + (1.0 - st.y) * (in.l1 - in.l2);
    }
    let colour = min(light, vec3<f32>(255.0)) / 256.0;
    let rgb = t.rgb * colour;
#ifdef CITY_MASKED
    if t.a <= 0.0 {
        discard;
    }
    return vec4<f32>(out_colour(rgb), t.a);
#else
#ifdef CITY_ALPHA
    // Ice2 / RoofWindow are loaded as ARGB with alpha 0x7f (`FUN_0106b7a0`).
    return vec4<f32>(out_colour(rgb), 127.0 / 255.0);
#else
    return vec4<f32>(out_colour(rgb), 1.0);
#endif
#endif
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if lights.params.y > 0.5 {
        return fragment_hw(in);
    }
    let t = textureSample(atlas, atlas_sampler, in.uv);
    let fw = fwidth(in.st);
#ifdef CITY_MASKED
    if t.a < 0.5 {
        discard;
    }
#endif
    let st = clamp(in.st, vec2(0.0), vec2(1.0));
    let rotating = lights.params.z > 0.5;
    var light = mix(mix(in.l0, in.l1, st.x), mix(in.l3, in.l2, st.x), st.y);
    if rotating {
#ifdef WEBGL2
        light = in.own.xyz / in.own.w;
#else
        light = in.own;
#endif
    }
#ifdef CITY_ALPHA
    // Top-down Alpha tops are screen rectangles whose right and bottom
    // edges use cx − 1 / cy − 1 (`Draw2DEffect` @ 100a8210 → OS4
    // `FUN_01036044`; a bug of both builds): the last column and row of
    // the square are not blended. Here: the last pixel at s = 1 / t = 1.
    if !rotating && (1.0 - in.st.x < fw.x || 1.0 - in.st.y < fw.y) {
        discard;
    }
    // That path draws the texel through table 4 unlit (no Gouraud); the
    // rotating view's `DrawAffineTriangleAlphaGouraud` @ 100c64f0 lights it.
    if !rotating {
        light = vec3<f32>(128.0);
    }
#endif
    // (+0.01: interpolation of equal integer values may land just below.)
    let l = floor((light + 0.01) / 8.0) * 8.0;
    let texel = round(to_srgb(t.rgb) * 255.0);
    let rgb = clamp((texel + l - vec3<f32>(128.0)) / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
#ifdef CITY_ALPHA
    return vec4<f32>(out_colour(rgb), 0.5);
#else
    return vec4<f32>(out_colour(rgb), 1.0);
#endif
}
