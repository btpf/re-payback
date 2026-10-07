// Object vertex shader: the default mesh vertex path plus environment-mapped
// ("Shine") faces (`FUN_010c101c`, mode & 0x10): their UV comes from the
// world-space normal, u = cu + 127·nx, v = cv + 63·ny (in texels/rows of the
// 256x128 texture; y = south = render +z). `uv_b` carries the texture's atlas
// origin plus the centre (cu, cv); negative = an ordinary face.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

// Atlas size in texels (objects.rs ATLAS_W x ATLAS_FULL_H).
const ATLAS: vec2<f32> = vec2<f32>(2048.0, 4096.0);

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
    out.uv_b = vertex.uv_b;
#ifdef VERTEX_NORMALS
    if vertex.uv_b.x >= 0.0 {
        let n = normalize(out.world_normal);
        out.uv = vertex.uv_b + vec2<f32>(127.0 * n.x, 63.0 * n.z) / ATLAS;
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
