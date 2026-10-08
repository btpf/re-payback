// The HUD's subtract plane: panels drawn through Alpha.256 table 1
// (`max(d - s, 0)`, `FadeDarken` stretched by `Draw2DEffect`). The texel
// colour is subtracted from the view under the HUD by the pipeline's blend
// (ReverseSubtract); texels with alpha 0 subtract nothing.

#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var plane: texture_2d<f32>;
@group(1) @binding(1) var plane_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(plane, plane_sampler, in.uv);
    return vec4<f32>(c.rgb * c.a, 0.0);
}
