[[block]]
struct Uniforms {
    // Optimization: multiply on cpu
    camera_view: mat4x4<f32>;
    camera_proj: mat4x4<f32>;
    px_range_factor: f32;
};

[[group(1), binding(0)]]
var<uniform> uniforms: Uniforms;

// Vertex shader

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] tex_coords: vec2<f32>;

    [[location(5)]] model_c0: vec4<f32>;
    [[location(6)]] model_c1: vec4<f32>;
    [[location(7)]] model_c2: vec4<f32>;
    // Optimization: not needed as this is projection?
    [[location(8)]] model_c3: vec4<f32>;

    [[location(9)]] scale: vec3<f32>;
    [[location(10)]] tint: vec3<f32>;
    [[location(11)]] texture_layer: i32;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
    [[location(1)]] tint: vec3<f32>;
    [[location(3)]] texture_layer: i32;
};

[[stage(vertex)]]
fn main(
    input: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;

    out.tex_coords = input.tex_coords;
    out.tint = input.tint;
    out.texture_layer = input.texture_layer;

    var model: mat4x4<f32> = mat4x4<f32>(input.model_c0, input.model_c1, input.model_c2, input.model_c3);
    var scaled: vec3<f32> = input.position * input.scale;
    // Optimization: merge view and model
    out.position = uniforms.camera_proj * (uniforms.camera_view * (model * vec4<f32>(scaled, 1.0)));

    return out;
}

// Fragment shader

[[group(0), binding(0)]]
var t_diffuse: texture_2d_array<f32>;
[[group(0), binding(1)]]
var s_diffuse: sampler;

struct FragmentOutput {
    [[location(0)]] color: vec4<f32>;
};

[[stage(fragment)]]
fn main(input: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    out.color = textureSample(t_diffuse, s_diffuse, input.tex_coords, input.texture_layer) * vec4<f32>(input.tint, 1.0);
    if (out.color.a < 0.0001) {
        discard;
    }

    return out;
}
