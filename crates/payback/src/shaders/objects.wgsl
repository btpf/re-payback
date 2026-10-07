// Object shader: the default mesh path plus the "Shine" faces of
// `FUN_010c101c` (mode & 0x10). `uv_b` = (atlas origin u + 2·kind, origin
// v) with the reflection centre already added to the origin; negative x =
// an ordinary face. Kinds:
//   1  mode 0x32: the slot texture at u = 127 + 127·nx, v = 63 + 63·ny
//      (per-vertex normal, world space; y = south = render +z)
//   2  mode 0x12/0x14: the slot texture at screen-space coordinates
//      u = 88 + 80·sx + 127·nx, v = 52 + 32·sy + 63·ny (face normal; sx, sy
//      = vertex position across the screen, 0..1)
//   3  mode 0x31: file-UV texture, then CloudsReflect at the kind-1
//      coordinates blended over it with Alpha.256 table 9 (d + s − 32)
//   4  mode 0x11: file-UV texture, then CloudsReflect at the kind-2
//      coordinates, table 9
// Coordinates are clamped to the 256x128 texture like the original.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
}

// Atlas size in texels (objects.rs ATLAS_W x ATLAS_FULL_H).
const ATLAS: vec2<f32> = vec2<f32>(2048.0, 4096.0);
// Reflection centres (kept in step with objects.rs ENV_CENTRE_*).
const CENTRE_SMOOTH: vec2<f32> = vec2<f32>(127.0, 63.0);
const CENTRE_SCREEN: vec2<f32> = vec2<f32>(88.0, 52.0);

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vec2<f32>(-1.0, -1.0);
#ifdef VERTEX_NORMALS
    if vertex.uv_b.x >= 0.0 {
        let kind = floor(vertex.uv_b.x / 2.0);
        // Texel origin of the texture and the reflection centre.
        let base = vec2<f32>(vertex.uv_b.x - 2.0 * kind, vertex.uv_b.y) * ATLAS;
        let n = normalize(out.world_normal);
        var offset = vec2<f32>(127.0 * n.x, 63.0 * n.z);
        var centre = CENTRE_SMOOTH;
        if kind == 2.0 || kind == 4.0 {
            let ndc = out.position.xy / out.position.w;
            let screen = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            offset += vec2<f32>(80.0, 32.0) * screen;
            centre = CENTRE_SCREEN;
        }
        let texel = clamp(centre + offset, vec2<f32>(0.0), vec2<f32>(255.0, 127.0));
        let uv = (base - centre + texel) / ATLAS;
        if kind <= 2.0 {
            out.uv = uv;
        } else {
            out.uv_b = uv;
        }
    }
#endif
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif
    return out;
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    var colour = pbr_input.material.base_color;
#ifdef VERTEX_UVS_B
    if in.uv_b.x >= 0.0 {
        // Table 9 over the palette: d + s - 32 per channel.
        var overlay_in = in;
        overlay_in.uv = in.uv_b;
        let s = pbr_input_from_standard_material(overlay_in, is_front).material.base_color.rgb;
        let mixed = clamp(to_srgb(colour.rgb) + to_srgb(s) - vec3<f32>(32.0 / 255.0), vec3<f32>(0.0), vec3<f32>(1.0));
        colour = vec4<f32>(to_linear(mixed), colour.a);
    }
#endif
    out.color = main_pass_post_lighting_processing(pbr_input, colour);
    return out;
}
