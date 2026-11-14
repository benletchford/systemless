struct Params {
    // x: mode (0 = rgba, 1 = indexed8, 2 = compact)
    // y: compact source_width
    // z: compact scale (1..4)
    // w: compact output_scale (1..4)
    mode_info: vec4<u32>,
    // x, y, width, height (width == 0 when no cursor patch is active)
    cursor_rect: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var rgba_pixels: texture_2d<f32>;
@group(0) @binding(2) var indexed_pixels: texture_2d<u32>;
@group(0) @binding(3) var palette: texture_2d<f32>;
@group(0) @binding(4) var cursor_pixels: texture_2d<f32>;
@group(0) @binding(5) var<storage, read> cells: array<u32>;
@group(0) @binding(6) var<storage, read> detail: array<u32>;

@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(points[index], 0.0, 1.0);
}

fn unpack_rgb(word: u32) -> vec3<u32> {
    return (vec3<u32>(word) >> vec3<u32>(16u, 8u, 0u)) & vec3<u32>(255u);
}

fn compact_color(pos: vec2<u32>) -> vec3<f32> {
    let source_width = params.mode_info.y;
    let scale = params.mode_info.z;
    let output_scale = params.mode_info.w;
    let cell_coord = pos / output_scale;
    let cell = cells[cell_coord.y * source_width + cell_coord.x];
    if (cell & 0x80000000u) == 0u {
        return vec3<f32>(unpack_rgb(cell)) / 255.0;
    }
    let offset = cell & 0x7fffffffu;
    let within = pos - cell_coord * output_scale;
    if scale <= output_scale {
        let sample = min(
            (within * 2u + vec2<u32>(1u)) * scale / (output_scale * 2u),
            vec2<u32>(scale - 1u),
        );
        let word = detail[offset + sample.y * scale + sample.x];
        return vec3<f32>(unpack_rgb(word)) / 255.0;
    }
    var sum = vec3<u32>(0u);
    for (var y = 0u; y < 4u; y++) {
        if y < scale {
            let low_y = max(within.y * scale, y * output_scale);
            let high_y = min((within.y + 1u) * scale, (y + 1u) * output_scale);
            if high_y > low_y {
                let wy = high_y - low_y;
                for (var x = 0u; x < 4u; x++) {
                    if x < scale {
                        let low_x = max(within.x * scale, x * output_scale);
                        let high_x = min((within.x + 1u) * scale, (x + 1u) * output_scale);
                        if high_x > low_x {
                            let wx = high_x - low_x;
                            let word = detail[offset + y * scale + x];
                            sum += unpack_rgb(word) * (wx * wy);
                        }
                    }
                }
            }
        }
    }
    let total = scale * scale;
    let rgb = (sum + vec3<u32>(total / 2u)) / total;
    return vec3<f32>(rgb) / 255.0;
}

@fragment fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pos = vec2<u32>(position.xy);
    if params.mode_info.x == 2u {
        return vec4<f32>(compact_color(pos), 1.0);
    }
    let rect = params.cursor_rect;
    if pos.x >= rect.x && pos.y >= rect.y && pos.x < rect.x + rect.z && pos.y < rect.y + rect.w {
        let local = vec2<i32>(pos - rect.xy);
        return vec4<f32>(textureLoad(cursor_pixels, local, 0).rgb, 1.0);
    }
    if params.mode_info.x == 1u {
        let index = textureLoad(indexed_pixels, vec2<i32>(pos), 0).r;
        return vec4<f32>(textureLoad(palette, vec2<i32>(i32(index), 0), 0).rgb, 1.0);
    }
    return vec4<f32>(textureLoad(rgba_pixels, vec2<i32>(pos), 0).rgb, 1.0);
}
