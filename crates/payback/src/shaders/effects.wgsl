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
// EFFECT_HARDWARE: the Warp3D look (`effects.rs`), blend set per material.

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var t = textureSample(atlas, atlas_sampler, in.uv);
#ifdef VERTEX_COLORS
    t = vec4(t.rgb * in.color.rgb, t.a);
#endif
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
