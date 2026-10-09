// World resolve of a game view (`postfx.rs` `resolve_pass`, docs/re/rendering.md
// §3 "Palette output"): runs after the 3D passes, before the HUD.
//
// The world shaders write gamma-encoded palette RGB (the bytes the original
// adds and subtracts through `Alpha.256`) into a linear float target, so every
// blend of the 3D passes happens on those bytes as in the original. Here each
// pixel is
//   mode 0: decoded to linear light (no snap: menus, `--no-palette-snap`);
//   mode 1: snapped to the nearest palette colour (software renderer), then
//           decoded;
//   mode 2: cut to the 5 bits per channel of the 16-bit ("thousands",
//           5-5-5) screen the hardware renderer draws into, with the
//           card's ordered dither (`W3D_SetState(0x80000, 1)` /
//           `GL_DITHER`), then decoded.
// The HUD is then drawn on the linear frame as before.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var lut: texture_3d<f32>;
@group(0) @binding(2) var palette: texture_2d<f32>;
// x = mode
@group(0) @binding(3) var<uniform> params: vec4<f32>;

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3(0.0)) + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

fn byte(v: f32) -> i32 {
    return i32(round(clamp(v, 0.0, 1.0) * 255.0));
}

// 4x4 ordered (Bayer) dither thresholds, 0..15.
const BAYER = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    let src = textureLoad(frame, p, 0);
    let g = clamp(src.rgb, vec3(0.0), vec3(1.0));
    var rgb = g;
    if params.x > 1.5 {
        // 8 → 5 bits with the dither, then the DAC's bit replication.
        let d = BAYER[(p.y & 3) * 4 + (p.x & 3)] * 0.5;
        let b = round(g * 255.0);
        let c5 = min(floor((b + vec3(d)) / 8.0), vec3(31.0));
        rgb = (c5 * 8.0 + floor(c5 / 4.0)) / 255.0;
    } else if params.x > 0.5 {
        let i = textureLoad(lut, vec3<i32>(byte(g.r), byte(g.g), byte(g.b)), 0).r;
        rgb = textureLoad(palette, vec2<i32>(byte(i), 0), 0).rgb;
    }
    return vec4(decode(rgb), src.a);
}
