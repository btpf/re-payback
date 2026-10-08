// City cube faces with the Gouraud light (docs/re/rendering.md §4,
// `crates/payback/src/lighting.rs`). The vertex colour holds the static
// light / 255 (128 = unchanged); the dynamic lights add min(255, I / d²)
// per channel at the vertex, d in object units (2048 per square, 16 per
// h-unit), for corners inside the light's square box. The light is clamped
// to 255, interpolated, quantised to 8 (`& 0xF8`) and added to the texel:
// pal[texel] + light − 128 (`Gouraud.256`), in sRGB.

#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip,
}

const MAX_LIGHTS: u32 = 64u;

struct Lights {
    // x = number of lights
    params: vec4<f32>,
    // per light: (render position, square radius), (R, G, B, 0)
    data: array<vec4<f32>, 128>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> lights: Lights;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.clip = position_world_to_clip(wp.xyz);
    out.uv = v.uv;
    var light = v.light.rgb * 255.0;
    let n = min(u32(lights.params.x), MAX_LIGHTS);
    // Corner grid position of the vertex (squares).
    let g = round(wp.xz);
    for (var i = 0u; i < n; i = i + 1u) {
        let p = lights.data[2u * i];
        let c = lights.data[2u * i + 1u];
        let s = floor(p.xz);
        let lo = max(s - vec2<f32>(p.w), vec2<f32>(1.0)) + vec2<f32>(1.0);
        let hi = min(s + vec2<f32>(p.w), vec2<f32>(126.0));
        if any(g < lo) || any(g > hi) {
            continue;
        }
        let d = (wp.xyz - p.xyz) * vec3<f32>(2048.0, 4096.0, 2048.0);
        let d2 = max(dot(d, d), 1.0);
        light += floor(min(c.rgb / d2, vec3<f32>(255.0)));
    }
    out.light = light;
    return out;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, atlas_sampler, in.uv);
#ifdef CITY_MASKED
    if t.a < 0.5 {
        discard;
    }
#endif
    // (+0.01: interpolation of equal integer values may land just below.)
    let l = floor((clamp(in.light, vec3<f32>(0.0), vec3<f32>(255.0)) + 0.01) / 8.0) * 8.0;
    let rgb = clamp(to_srgb(t.rgb) + (l - vec3<f32>(128.0)) / 255.0, vec3<f32>(0.0), vec3<f32>(1.0));
#ifdef CITY_ALPHA
    return vec4<f32>(to_linear(rgb), 0.5);
#else
    return vec4<f32>(to_linear(rgb), 1.0);
#endif
}
