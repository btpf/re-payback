// Effect sprites (docs/re/rendering.md §7), blended like the Alpha.256
// tables (§3). The atlas holds palette RGB with alpha = max(R, G, B) / 255,
// and alpha 0 for texel 0. The blend equation itself is set per material by
// `EffectMaterial::specialize`:
//   add       dst + src                 (table 0x00000; also flares, weather)
//   subtract  dst - src                 (table 0x10000)
//   tint      src + dst * (1 - max(src)) (table 0x30000, premultiplied alpha)
//   average   (dst + src) / 2           (table 0x40000; texel 0 not skipped)
// The vertex colour multiplies the texel: the rain colour, the flare
// channel masks of tables 5-8 and the screen flash.
// EFFECT_HARDWARE: the Warp3D look (`effects.rs`), blend set per material;
// every 64x64 sprite (and every quarter of a 128x128 one) is its own
// texture there, filtered bilinearly with GL_REPEAT wrapping (Mac
// `W3D_AllocTexObj` on OpenGL, analysis/mac/rendering.md §5), so the
// filter wraps inside the atlas tile instead of reading its neighbours.
// GAMMA_TARGET (the game views): the output is gamma-encoded palette RGB,
// so the blends add, subtract and average the palette bytes as the
// Alpha.256 tables do (clamped per channel by the target only at the end).

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

// Linear-light colour → what the target holds.
fn out_colour(c: vec3<f32>) -> vec3<f32> {
#ifdef GAMMA_TARGET
    return to_srgb(c);
#else
    return c;
#endif
}

// One texel of the 64x64 tile at `origin`, wrapping (hardware).
fn texel_tile(origin: vec2<i32>, p: vec2<f32>) -> vec4<f32> {
    let q = origin + (vec2<i32>(floor(p)) % vec2<i32>(64) + vec2<i32>(64)) % vec2<i32>(64);
    return textureLoad(atlas, q, 0);
}

// Bilinear filtering inside the sprite's own tile (hardware).
fn sample_tile(uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(atlas));
    let p = uv * size;
    let origin = vec2<i32>(floor(p / 64.0)) * 64;
    let q = p - vec2<f32>(origin) - vec2<f32>(0.5);
    let i = floor(q);
    let f = q - i;
    return mix(
        mix(texel_tile(origin, i), texel_tile(origin, i + vec2(1.0, 0.0)), f.x),
        mix(texel_tile(origin, i + vec2(0.0, 1.0)), texel_tile(origin, i + vec2(1.0, 1.0)), f.x),
        f.y,
    );
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef EFFECT_HARDWARE
    var t = sample_tile(in.uv);
#else
    var t = textureSample(atlas, atlas_sampler, in.uv);
#endif
#ifdef VERTEX_COLORS
    t = vec4(t.rgb * in.color.rgb, t.a);
#endif
    t = vec4(out_colour(t.rgb), t.a);
#ifdef EFFECT_HARDWARE
    // Warp3D: the converted texture under ONE/ONE (alpha ignored) or
    // SRC_ALPHA/ONE_MINUS_SRC_ALPHA (alpha = the texture's); bilinear.
#ifdef EFFECT_HW_OVERLAY
    // Flares and weather under ONE / ONE_MINUS_SRC_ALPHA: a vertex alpha
    // below 1 (rain, 0.4) blends at that alpha, premultiplied; the rest
    // (alpha 1, compared with margin: interpolation is not exact) adds.
#ifdef VERTEX_COLORS
    if (in.color.a < 0.9) {
        return vec4(t.rgb * in.color.a, in.color.a);
    }
#endif
    return vec4(t.rgb, 0.0);
#else
    return t;
#endif
#else
#ifdef EFFECT_AVERAGE
    return vec4(t.rgb, 0.5);
#else
    // Texel 0 is transparent: it contributes nothing.
    let keep = select(0.0, 1.0, t.a > 0.0);
    return vec4(t.rgb * keep, t.a);
#endif
#endif
}
