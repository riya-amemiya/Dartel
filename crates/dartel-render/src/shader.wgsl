struct Globals {
    size: vec2<f32>,
    padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;

struct Instance {
    @location(0) rect: vec4<i32>,
    @location(1) source: vec4<u32>,
    @location(2) color: vec4<f32>,
}

struct Varyings {
    @builtin(position) position: vec4<f32>,
    @location(0) texel: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) textured: u32,
}

@vertex
fn vertex(@builtin(vertex_index) index: u32, instance: Instance) -> Varyings {
    let offset = vec2<f32>(f32(index & 1u), f32(index >> 1u)) * vec2<f32>(instance.rect.zw);
    let pixel = vec2<f32>(instance.rect.xy) + offset;
    var out: Varyings;
    out.position = vec4<f32>(
        pixel.x / globals.size.x * 2.0 - 1.0,
        1.0 - pixel.y / globals.size.y * 2.0,
        0.0,
        1.0,
    );
    out.texel = vec2<f32>(instance.source.xy) + offset;
    out.color = vec4<f32>(instance.color.rgb * instance.color.a, instance.color.a);
    out.textured = instance.source.z;
    return out;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    if in.textured == 0u {
        return in.color;
    }
    return in.color * textureLoad(atlas, vec2<i32>(in.texel), 0);
}
