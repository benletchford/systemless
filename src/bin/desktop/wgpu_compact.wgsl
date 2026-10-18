@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array(vec2(-1., -1.), vec2(3., -1.), vec2(-1., 3.));
    return vec4(p[i], 0., 1.);
}
override sample_scale: u32 = 4u;
override uniform_fast_path: bool = true;
struct Geometry {
    source: vec4<u32>, // width, height, sample scale, padding
    crop: vec4<u32>,
    viewport: vec4<u32>,
    ratio: vec4<u32>, // source/destination dimensions reduced independently by gcd
}
@group(0) @binding(0) var<storage, read> cells: array<u32>;
@group(0) @binding(1) var<storage, read> detail: array<u32>;
@group(0) @binding(2) var<uniform> geometry: Geometry;
fn sample_rgb(x: u32, y: u32) -> vec3<u32> {
    let scale = sample_scale;
    let point = vec2(x, y) + geometry.crop.xy * scale;
    let cell = cells[(point.y / scale) * geometry.source.x + point.x / scale];
    var rgb = cell;
    if (cell & 0x80000000u) != 0u {
        rgb = detail[(cell & 0x7fffffffu) + (point.y % scale) * scale + point.x % scale];
    }
    return (vec3(rgb) >> vec3(16u, 8u, 0u)) & vec3(255u);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<u32>(p.xy) - geometry.viewport.xy;
    let size = geometry.ratio.xy;
    let destination = geometry.ratio.zw;
    let shrink = size > destination;
    let lower = point * size;
    let upper = (point + vec2(1u)) * size;
    let nearest = min((2u * point + vec2(1u)) * size / (2u * destination), geometry.crop.zw * sample_scale - vec2(1u));
    let first = select(nearest, lower / destination, shrink);
    let last = select(nearest + vec2(1u), (upper + destination - vec2(1u)) / destination, shrink);
    // Flat UI areas need one color, even when their footprint crosses cells.
    // Only inspect corners when they cover all (at most 2x2) guest cells.
    let cell_first = first / sample_scale;
    let cell_last = (last - vec2(1u)) / sample_scale;
    if uniform_fast_path && all(cell_last - cell_first <= vec2(1u)) {
        let row0 = (cell_first.y + geometry.crop.y) * geometry.source.x;
        let row1 = (cell_last.y + geometry.crop.y) * geometry.source.x;
        let x0 = cell_first.x + geometry.crop.x;
        let x1 = cell_last.x + geometry.crop.x;
        let color = cells[row0 + x0];
        if (color & 0x80000000u) == 0u && color == cells[row0 + x1] && color == cells[row1 + x0] && color == cells[row1 + x1] {
            return vec4(vec3<f32>((vec3(color) >> vec3(16u,8u,0u)) & vec3(255u)) / 255., 1.);
        }
    }
    var sum = vec3(0u);
    for (var sy = first.y; sy < last.y; sy++) {
        let wy = select(1u, min(upper.y, (sy + 1u) * destination.y) - max(lower.y, sy * destination.y), shrink.y);
        for (var sx = first.x; sx < last.x; sx++) {
            let wx = select(1u, min(upper.x, (sx + 1u) * destination.x) - max(lower.x, sx * destination.x), shrink.x);
            sum += sample_rgb(sx, sy) * (wx * wy);
        }
    }
    let weights = select(vec2(1u), size, shrink);
    let total = weights.x * weights.y;
    let rounded = sum + vec3(total / 2u);
    if (total & (total - 1u)) == 0u {
        return vec4(vec3<f32>(rounded >> vec3(firstTrailingBit(total))) / 255., 1.);
    }
    return vec4(vec3<f32>(rounded / total) / 255., 1.);
}
