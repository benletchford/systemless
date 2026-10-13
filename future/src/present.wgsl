@group(0) @binding(0) var image: texture_2d<f32>;
struct Vertex { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i:u32)->Vertex {
    let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return Vertex(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
@fragment fn fs(v:Vertex)->@location(0) vec4<f32> {
    let size=vec2<i32>(textureDimensions(image));
    let p=clamp(vec2<i32>(v.uv*vec2<f32>(size)),vec2<i32>(0),size-vec2<i32>(1));
    return textureLoad(image,p,0);
}
