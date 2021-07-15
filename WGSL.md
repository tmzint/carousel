# WGSL cheatsheet

## Structure

### Vertex input

```wgsl
// arbitrary named struct that describes the vertex shader input
struct VertexInput {
    
    // analogous to layout(location=0) in vec3 position;
    [[location(0)]] position: vec3<f32>;
    
    // analogous to layout(location=1) in vec3 position;
    [[location(1)]] color: vec3<f32>;
};
```

### Vertex Output / Fragment Input

```wgsl
// arbitrary named struct that describes the vertex shader output and fragment shader input
struct VertexOutput {
    // represents gl_Position variable
    [[builtin(position)]] clip_position: vec4<f32>;
    // analogous to layout(location=0) out vec3 color;
    [[location(0)]] color: vec3<f32>;
};
```

### Vertex entrypoint

```wgsl
// vertex stage entrypoint with name main
[[stage(vertex)]]
fn main(
    // vertex shader input
    model: VertexInput,
   // vertex shader output type
) -> VertexOutput {
    // creation of the vertex shader output instance
    var out: VertexOutput;
    
    // set the output values
    out.color = model.color;
    out.clip_position = vec4<f32>(model.position, 1.0);
    
    // return vertex shader output instance
    return out;
}
```

### Fragment Output

```wgsl
// arbitrary named struct that describes the fragment shader output
struct FragmentOutput {
    // analogous to layout(location=0) out vec4 color;
    [[location(0)]] color: vec4<f32>;
};
```

### Fragment entrypoint

```wgsl
// fragment stage entrypoint with name main
[[stage(fragment)]]
fn main(
    // vertex shader output / fragment shader input
    in: VertexOutput
    // fragment shader output type
)-> FragmentOutput {
    // creation of the fragment shader output instance
    var out: FragmentOutput;
    
    // set the output values
    out.color = vec4<f32>(in.color, 1.0);
    
    // return fragment shader output instance
    return out;
}
```

### Uniforms

```wgsl
// Block decorator denotes a type that corresponds to a buffer resource
[[block]]
// arbitrary named struct that describes a buffer
struct Uniforms {
    // analogous to mat4 view_proj;
    view_proj: mat4x4<f32>;
};
// analogous to layout(set=1, binding=0)
[[group(1), binding(0)]]
var<uniform> uniforms: Uniforms;

// analogous to layout(set = 0, binding = 0) uniform texture2D t_diffuse;
[[group(0), binding(0)]]
var t_diffuse: texture_2d<f32>;

// analogous to layout(set = 0, binding = 1) uniform sampler s_diffuse;
[[group(0), binding(1)]]
var s_diffuse: sampler;
```

### Discard

The `discard` statement will terminate the fragment shader without returning a value.
This statement is only available in the fragment stage.

```wgsl
if (color.a < 0.0001) {
    discard;
}
```