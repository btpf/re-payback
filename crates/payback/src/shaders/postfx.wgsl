// Final pass of a game view (`postfx.rs`, docs/re/rendering.md §3):
// snap to the palette, motion blur through Alpha.256 table 4, palette flash.
// Values in the frame are linear (sRGB target); the arithmetic is on the
// gamma-encoded bytes, as on the palette.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// Copy of the frame (same pixel coordinates as the target).
@group(0) @binding(0) var frame: texture_2d<f32>;
// Palette index of every pixel of the previous frame (R8, k / 255).
@group(0) @binding(1) var prev_index: texture_2d<f32>;
// Nearest palette index of every RGB (x = R, y = G, z = B).
@group(0) @binding(2) var lut: texture_3d<f32>;
// Palette, 256x1, encoded RGB.
@group(0) @binding(3) var palette: texture_2d<f32>;
// Alpha.256 table 4: x = current index, y = previous index.
@group(0) @binding(4) var average: texture_2d<f32>;
// x = flash (0..1), y = motion blur, z = snap.
@group(0) @binding(5) var<uniform> params: vec4<f32>;

struct Out {
    @location(0) colour: vec4<f32>,
    @location(1) index: vec4<f32>,
};

fn encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3(0.0)) + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

fn byte(v: f32) -> i32 {
    return i32(round(clamp(v, 0.0, 1.0) * 255.0));
}

fn pal(i: i32) -> vec3<f32> {
    return textureLoad(palette, vec2<i32>(i, 0), 0).rgb;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> Out {
    let p = vec2<i32>(floor(in.position.xy));
    let src = textureLoad(frame, p, 0);
    let enc = encode(src.rgb);
    let cur = byte(textureLoad(lut, vec3<i32>(byte(enc.r), byte(enc.g), byte(enc.b)), 0).r);
    let blur = params.y > 0.5;
    var rgb = enc;
    if params.z > 0.5 {
        var shown = cur;
        if blur {
            let prev = byte(textureLoad(prev_index, p, 0).r);
            shown = byte(textureLoad(average, vec2<i32>(cur, prev), 0).r);
        }
        rgb = pal(shown);
    } else if blur {
        let prev = byte(textureLoad(prev_index, p, 0).r);
        rgb = (enc + pal(prev)) * 0.5;
    }
    // The flash raises every palette entry (after the snap).
    rgb = min(rgb + vec3(params.x), vec3(1.0));
    var out: Out;
    out.colour = vec4(decode(rgb), src.a);
    out.index = vec4(f32(cur) / 255.0, 0.0, 0.0, 1.0);
    return out;
}
