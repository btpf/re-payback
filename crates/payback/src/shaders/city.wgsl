// City cube faces with the Gouraud light (docs/re/rendering.md §4,
// `crates/payback/src/lighting.rs`). Every vertex carries its quad's four
// corners and their static light (128 = unchanged); the dynamic lights add
// min(255, I / d²) per channel at each corner, d in object units (2048 per
// square, 16 per h-unit), for corners inside the light's square box. Each
// corner's light is clamped to 255 and interpolated bilinearly across the
// quad, as the software rasteriser interpolates along the edges and then
// the spans (no diagonal seam), quantised to 8 (`& 0xF8`) and added to the
// texel: pal[texel] + light − 128 (`Gouraud.256`), in gamma-encoded RGB.
// The palette snap of `Gouraud.256` is done by the view's final pass
// (`postfx.wgsl`).

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

const MAX_LIGHTS: u32 = 510u;

struct Lights {
    // x = number of lights
    params: vec4<f32>,
    // per light: (render position, square radius), (R, G, B, 0)
    data: array<vec4<f32>, 1020>,
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
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) st: vec2<f32>,
    @location(2) @interpolate(flat) l0: vec3<f32>,
    @location(3) @interpolate(flat) l1: vec3<f32>,
    @location(4) @interpolate(flat) l2: vec3<f32>,
    @location(5) @interpolate(flat) l3: vec3<f32>,
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, atlas_sampler, in.uv);
#ifdef CITY_MASKED
    if t.a < 0.5 {
        discard;
    }
#endif
    let st = clamp(in.st, vec2(0.0), vec2(1.0));
    let light = mix(mix(in.l0, in.l1, st.x), mix(in.l3, in.l2, st.x), st.y);
    // (+0.01: interpolation of equal integer values may land just below.)
    let l = floor((light + 0.01) / 8.0) * 8.0;
    let texel = round(to_srgb(t.rgb) * 255.0);
    let rgb = clamp((texel + l - vec3<f32>(128.0)) / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
#ifdef CITY_ALPHA
    return vec4<f32>(to_linear(rgb), 0.5);
#else
    return vec4<f32>(to_linear(rgb), 1.0);
#endif
}
