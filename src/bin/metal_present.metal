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
