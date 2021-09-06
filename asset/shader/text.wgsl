// see:
//  https://github.com/VALIS-software/GPUText
//  https://github.com/Chlumsky/msdfgen
//  https://github.com/Chlumsky/msdfgen/issues/36#issuecomment-429240110
//  https://github.com/Chlumsky/msdfgen/issues/115
//  https://stackoverflow.com/questions/34563475/sdf-text-rendering-in-perspective-projection
//  https://metalbyexample.com/rendering-text-in-metal-with-signed-distance-fields/

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

    // [scale, point / size, distance_range]
    [[location(9)]] scale: vec3<f32>;
    [[location(10)]] tint: vec4<f32>;
    [[location(11)]] texture_layer: i32;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
    [[location(1)]] tint: vec4<f32>;
    [[location(2)]] distance_factor: f32;
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
    var scaled: vec3<f32> = input.position * vec3<f32>(input.scale.x, input.scale.x, input.scale.x);
    // Optimization: merge view and model
    out.position = uniforms.camera_proj * (uniforms.camera_view * (model * vec4<f32>(scaled, 1.0)));

    out.distance_factor = max(input.scale.y * input.scale.x * uniforms.px_range_factor * input.scale.z, 1.0);

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

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

[[stage(fragment)]]
fn main(input: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    var msd: vec3<f32> = textureSample(t_diffuse, s_diffuse, input.tex_coords, input.texture_layer).rgb;
    var sd: f32 = input.distance_factor * (median(msd.r, msd.g, msd.b) - 0.5);
    var fill_alpha: f32 = clamp(sd + 0.5, 0.0, 1.0);
    if (fill_alpha < 0.0001) {
        discard;
    }
    out.color = vec4<f32>(input.tint.rgb, input.tint.a * fill_alpha);

    return out;
}