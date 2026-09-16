// Integer area coverage over a compact retained surface. No bilinear or sRGB
// conversion: quantize once, just like resize_argb_coverage on the CPU.
Buffer<uint> cells : register(t0);
Buffer<uint> detail : register(t1);
cbuffer Dimensions : register(b0) {
    uint width, height, scale, dw;
    uint dh, ox, oy, unused;
};

float4 vertex(uint id : SV_VertexID) : SV_Position {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
}

uint sample_rgb(uint x, uint y) {
    uint cell = cells[(y / scale) * width + x / scale];
    return (cell & 0x80000000) ? detail[(cell & 0x7fffffff) + (y % scale) * scale + x % scale] : cell;
}

float4 pixel(float4 position : SV_Position) : SV_Target {
    uint x = (uint)position.x - ox, y = (uint)position.y - oy;
    uint sw = width * scale, sh = height * scale;
    bool shrink_x = sw > dw, shrink_y = sh > dh;
    uint left = x * sw, right = (x + 1) * sw;
    uint top = y * sh, bottom = (y + 1) * sh;
    uint x0 = shrink_x ? left / dw : min((2 * x + 1) * sw / (2 * dw), sw - 1);
    uint x1 = shrink_x ? (right + dw - 1) / dw : x0 + 1;
    uint y0 = shrink_y ? top / dh : min((2 * y + 1) * sh / (2 * dh), sh - 1);
    uint y1 = shrink_y ? (bottom + dh - 1) / dh : y0 + 1;
    uint3 sum = 0;
    for (uint sy = y0; sy < y1; sy++) {
        uint wy = shrink_y ? min(bottom, (sy + 1) * dh) - max(top, sy * dh) : 1;
        for (uint sx = x0; sx < x1; sx++) {
            uint wx = shrink_x ? min(right, (sx + 1) * dw) - max(left, sx * dw) : 1;
            uint rgb = sample_rgb(sx, sy);
            sum += ((rgb >> uint3(16, 8, 0)) & 255) * (wx * wy);
        }
    }
    uint total = (shrink_x ? sw : 1) * (shrink_y ? sh : 1);
    uint3 result = (sum + total / 2) / total;
    return float4(float3(result) / 255.0, 1);
}
