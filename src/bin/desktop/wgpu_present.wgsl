@group(0) @binding(0) var image: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(p[i], 0., 1.);
}
struct Geometry { crop: vec4<u32>, viewport: vec4<u32> }
@group(0) @binding(1) var<uniform> geometry: Geometry;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let relative = (p.xy - vec2<f32>(geometry.viewport.xy)) / vec2<f32>(geometry.viewport.zw);
    let source = geometry.crop.xy + min(vec2<u32>(relative * vec2<f32>(geometry.crop.zw)), geometry.crop.zw - vec2(1u));
    return vec4(textureLoad(image, vec2<i32>(source), 0).rgb, 1.);
}
