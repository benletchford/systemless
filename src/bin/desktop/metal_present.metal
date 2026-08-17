#include <metal_stdlib>

using namespace metal;

struct RasterVertex {
    float4 position [[position]];
    float2 tex_coord;
};

vertex RasterVertex raster_vertex(uint vertex_id [[vertex_id]]) {
    constexpr float2 positions[] = {
        {-1.0,  1.0},
        {-1.0, -1.0},
        { 1.0,  1.0},
        { 1.0, -1.0},
    };
    constexpr float2 tex_coords[] = {
        {0.0, 0.0},
        {0.0, 1.0},
        {1.0, 0.0},
        {1.0, 1.0},
    };

    RasterVertex out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    out.tex_coord = tex_coords[vertex_id];
    return out;
}

fragment float4 raster_fragment(
    RasterVertex in [[stage_in]],
    texture2d<float> framebuffer [[texture(0)]])
{
    constexpr sampler nearest_sampler(
        coord::normalized,
        address::clamp_to_edge,
        filter::nearest);
    return framebuffer.sample(nearest_sampler, in.tex_coord);
}

struct GuestFrameUniforms {
    uint row_bytes;
    uint width;
    uint height;
    uint pixel_size;
    uint content_left;
    uint content_top;
    uint cursor_kind;
    uint cursor_width;
    uint cursor_height;
    int cursor_left;
    int cursor_top;
};

struct GuestCursorData {
    uint data_rows[16];
    uint mask_rows[16];
    uint color_pixels[256];
};

static float4 unpack_argb(uint argb) {
    return float4(
        float((argb >> 16) & 0xFF) / 255.0,
        float((argb >> 8) & 0xFF) / 255.0,
        float(argb & 0xFF) / 255.0,
        1.0);
}

fragment float4 guest_raster_fragment(
    RasterVertex in [[stage_in]],
    device const uchar* framebuffer [[buffer(0)]],
    constant uint* palette [[buffer(1)]],
    constant GuestFrameUniforms& frame [[buffer(2)]],
    constant GuestCursorData& cursor [[buffer(3)]])
{
    uint x = frame.content_left
        + min(uint(in.tex_coord.x * float(frame.width)), frame.width - 1);
    uint y = frame.content_top
        + min(uint(in.tex_coord.y * float(frame.height)), frame.height - 1);
    uint argb;

    if (frame.pixel_size == 8) {
        uint index = framebuffer[y * frame.row_bytes + x];
        argb = palette[index];
    } else if (frame.pixel_size == 4) {
        uchar packed = framebuffer[y * frame.row_bytes + x / 2];
        uint index = (x & 1) == 0 ? packed >> 4 : packed & 0x0F;
        argb = palette[index];
    } else {
        uchar packed = framebuffer[y * frame.row_bytes + x / 8];
        bool black = (packed & (0x80 >> (x & 7))) != 0;
        argb = black ? 0xFF000000 : 0xFFFFFFFF;
    }

    int cursor_x = int(x) - frame.cursor_left;
    int cursor_y = int(y) - frame.cursor_top;
    if (frame.cursor_kind != 0
        && cursor_x >= 0 && cursor_y >= 0
        && cursor_x < int(frame.cursor_width)
        && cursor_y < int(frame.cursor_height)) {
        uint row = uint(cursor_y);
        uint column = uint(cursor_x);
        uint bit = column < 16 ? (0x8000 >> column) : 0;
        bool mask_set = row < 16 && column < 16 && (cursor.mask_rows[row] & bit) != 0;

        if (frame.cursor_kind == 1) {
            if (mask_set) {
                argb = (cursor.data_rows[row] & bit) != 0 ? 0xFF000000 : 0xFFFFFFFF;
            }
        } else {
            uint cursor_argb = cursor.color_pixels[row * frame.cursor_width + column];
            if (mask_set) {
                argb = cursor_argb | 0xFF000000;
            } else if (cursor_argb == 0xFF000000) {
                argb ^= 0x00FFFFFF;
            }
        }
    }

    return unpack_argb(argb);
}
