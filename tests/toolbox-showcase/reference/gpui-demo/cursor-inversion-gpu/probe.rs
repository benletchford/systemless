use metal::*;
fn read(texture: &TextureRef) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    texture.get_bytes(
        bytes.as_mut_ptr().cast(),
        8,
        MTLRegion::new_2d(0, 0, 2, 1),
        0,
    );
    bytes
}
fn main() {
    let device = Device::system_default().expect("Metal device");
    let queue = device.new_command_queue();
    let shaders = r#"
#include <metal_stdlib>
using namespace metal;
vertex float4 invert_vertex(uint id [[vertex_id]]) {
    const float2 vertices[3] = {float2(-1,-1),float2(3,-1),float2(-1,3)};
    return float4(vertices[id],0,1);
}
fragment float4 invert_fragment() { return float4(1); }
"#;
    let library = device
        .new_library_with_source(shaders, &CompileOptions::new())
        .expect("shader compilation");
    let descriptor = RenderPipelineDescriptor::new();
    descriptor.set_vertex_function(Some(&library.get_function("invert_vertex", None).unwrap()));
    descriptor.set_fragment_function(Some(
        &library.get_function("invert_fragment", None).unwrap(),
    ));
    let color = descriptor.color_attachments().object_at(0).unwrap();
    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    color.set_blending_enabled(true);
    color.set_rgb_blend_operation(MTLBlendOperation::Add);
    color.set_source_rgb_blend_factor(MTLBlendFactor::OneMinusDestinationColor);
    color.set_destination_rgb_blend_factor(MTLBlendFactor::Zero);
    color.set_write_mask(
        MTLColorWriteMask::Red | MTLColorWriteMask::Green | MTLColorWriteMask::Blue,
    );
    let pipeline = device
        .new_render_pipeline_state(&descriptor)
        .expect("pipeline");
    let td = TextureDescriptor::new();
    td.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    td.set_width(2);
    td.set_height(1);
    td.set_storage_mode(MTLStorageMode::Shared);
    td.set_usage(MTLTextureUsage::RenderTarget);
    let texture = device.new_texture(&td);
    for pixel in [
        [153u8, 102, 51, 128],
        [0, 0, 0, 255],
        [255, 255, 255, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [0, 0, 0, 0],
        [127, 128, 129, 64],
    ] {
        let original = [
            pixel[0], pixel[1], pixel[2], pixel[3], pixel[0], pixel[1], pixel[2], pixel[3],
        ];
        texture.replace_region(
            MTLRegion::new_2d(0, 0, 2, 1),
            0,
            original.as_ptr().cast(),
            8,
        );
        for phase in 1..=2 {
            let pass = RenderPassDescriptor::new();
            let attachment = pass.color_attachments().object_at(0).unwrap();
            attachment.set_texture(Some(&texture));
            attachment.set_load_action(MTLLoadAction::Load);
            attachment.set_store_action(MTLStoreAction::Store);
            let command = queue.new_command_buffer();
            let encoder = command.new_render_command_encoder(pass);
            encoder.set_render_pipeline_state(&pipeline);
            encoder.set_scissor_rect(MTLScissorRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            });
            encoder.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
            encoder.end_encoding();
            command.commit();
            command.wait_until_completed();
            let result = read(&texture);
            let expected = if phase == 1 {
                [
                    255 - pixel[0],
                    255 - pixel[1],
                    255 - pixel[2],
                    pixel[3],
                    pixel[0],
                    pixel[1],
                    pixel[2],
                    pixel[3],
                ]
            } else {
                original
            };
            assert_eq!(
                result, expected,
                "RGB inversion, alpha preservation and scissor clip"
            );
            println!("PASS source={pixel:?} phase={phase} BGRA={result:?}");
        }
    }
}
