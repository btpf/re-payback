// Object and scenery shadows (`objects/shadows.rs`; Mac `DrawShadow` @
// 1007f7d0, `DrawSceneryShadow` @ 1007ea20). Software: the result is
// subtracted from the screen (Alpha.256 table 1, the blend is set by
// `ShadowMaterial::specialize`). Hardware (params.x = 1): black at an alpha,
// blended SRC_ALPHA / ONE_MINUS_SRC_ALPHA (Warp3D `(7, 8)`); `params.x`.
// Software subtraction is done as a darkening factor (`darken`).
//
// kind = vertex colour red · 4:
//   0  a texel of the `shadow` page (table 1; hardware alpha = max(R, G, B))
//   1  Tree/Tree2/tanne face: table 2 (d - 16) where the texel is not 0;
//      hardware the clone texture: alpha 0x32 where the texel is not 0
//   2  another face: the `shadow` texel (10, 5); hardware flat black
//   3  the `Shad` sprite (table 1; hardware alpha = max(R, G, B))
//   4  another mode-3 face: as 1 in software, flat black in hardware

#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sprite: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var sprite_sampler: sampler;
// x = 1: the hardware look.
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> params: vec4<f32>;

// Software: d - s in palette (sRGB) values, applied as dst * (1 - k) on the
// linear framebuffer, with k exact for a ground of sRGB REF_GROUND.
const REF_GROUND: f32 = 0.55;
// Table 2's "- 16".
const TABLE2_STEP: f32 = 16.0 / 255.0;

fn darken(s: f32) -> f32 {
    return 1.0 - pow(max(1.0 - s / REF_GROUND, 0.0), 2.2);
}
// The hardware alpha of flat shadow faces and of the GenlockShadow clones.
const FLAT_ALPHA: f32 = 0.3;
const CLONE_ALPHA: f32 = 50.0 / 255.0;

fn max3(c: vec3<f32>) -> f32 {
    // Alpha = max(R, G, B) of the palette (sRGB) values.
    return pow(max(c.r, max(c.g, c.b)), 1.0 / 2.2);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var kind = 0.0;
#ifdef VERTEX_COLORS
    kind = round(in.color.r * 4.0);
#endif
    // Both sampled in uniform control flow, then picked.
    let ts = textureSample(sprite, sprite_sampler, in.uv);
    let ta = textureSample(atlas, atlas_sampler, in.uv);
    let t = select(ta, ts, kind > 2.5 && kind < 3.5);
    let keyed = (kind > 0.5 && kind < 1.5) || kind > 3.5;
    if keyed && t.a < 0.5 {
        discard;
    }
    if params.x > 0.5 {
        if kind > 3.5 || (kind > 1.5 && kind < 2.5) {
            return vec4<f32>(0.0, 0.0, 0.0, FLAT_ALPHA);
        }
        if keyed {
            return vec4<f32>(0.0, 0.0, 0.0, CLONE_ALPHA);
        }
        return vec4<f32>(0.0, 0.0, 0.0, max3(t.rgb));
    }
    if keyed {
        return vec4<f32>(vec3<f32>(darken(TABLE2_STEP)), 1.0);
    }
    let srgb = pow(t.rgb, vec3<f32>(1.0 / 2.2));
    return vec4<f32>(darken(srgb.r), darken(srgb.g), darken(srgb.b), 1.0);
}
