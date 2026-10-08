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

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

// x = scroll (bytes, 0..63), y = 1 in the rotating view
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var index_atlas: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sky: texture_2d<f32>;

const ATLAS_SIZE: f32 = 2048.0;
const SCROLL_FLAG: f32 = 100000.0;
const BG_W: i32 = 320;
const BG_H: i32 = 266;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var tile = in.uv_b;
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
    return textureLoad(sky, p, 0);
}
