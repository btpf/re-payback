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

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var t = textureSample(atlas, atlas_sampler, in.uv);
#ifdef VERTEX_COLORS
    t = vec4(t.rgb * in.color.rgb, t.a);
#endif
#ifdef EFFECT_AVERAGE
    return vec4(t.rgb, 0.5);
#else
    // Texel 0 is transparent: it contributes nothing.
    let keep = select(0.0, 1.0, t.a > 0.0);
    return vec4(t.rgb * keep, t.a);
#endif
}
