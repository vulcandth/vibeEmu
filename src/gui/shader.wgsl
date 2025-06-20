struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VSOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 2.0),
    );
    var out: VSOut;
    out.pos = vec4<f32>(positions[i], 0.0, 1.0);
    out.uv = uvs[i];
    return out;
}

@group(0) @binding(0) var my_tex: texture_2d<f32>;
@group(0) @binding(1) var my_sampler: sampler;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    return textureSample(my_tex, my_sampler, uv);
}
