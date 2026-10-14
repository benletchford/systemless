use systemless_future::{
    bitmap::Bitmap,
    graphics::{Draw, Graphics, Rect, Transfer},
    memory::{GuestPtr, Memory, Permissions},
};
fn fixture() -> (Memory, Bitmap) {
    let mut memory = Memory::default();
    // 10x2 pixels, padded to 4 bytes/row, origin (-3,5).
    let mut descriptor = vec![0; 14];
    descriptor[..4].copy_from_slice(&0x2000u32.to_be_bytes());
    for (offset, value) in [(4, 4i16), (6, 5), (8, -3), (10, 7), (12, 7)] {
        descriptor[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }
    memory
        .map(GuestPtr(0x1000), descriptor, Permissions::RW)
        .unwrap();
    memory
        .map(
            GuestPtr(0x2000),
            vec![0x80, 0x3f, 0xaa, 0xbb, 0, 0x3f, 0xcc, 0xdd],
            Permissions::RW,
        )
        .unwrap();
    let bitmap = Bitmap::read(&memory, GuestPtr(0x1000)).unwrap();
    (memory, bitmap)
}
#[test]
fn bitmap_layout_rejects_bad_stride_flags_bounds_and_storage() {
    let (mut memory, bitmap) = fixture();
    assert_eq!(bitmap.extent().unwrap(), (10, 2));
    assert_eq!(bitmap.bounds, Rect::new(-3, 5, 7, 7));
    for stride in [0u16, 1, 0x8004, 0x4004] {
        memory
            .write(GuestPtr(0x1004), &stride.to_be_bytes())
            .unwrap();
        assert!(Bitmap::read(&memory, GuestPtr(0x1000)).is_err());
    }
    memory.write(GuestPtr(0x1004), &4u16.to_be_bytes()).unwrap();
    memory.write(GuestPtr(0x100a), &5u16.to_be_bytes()).unwrap();
    assert!(Bitmap::read(&memory, GuestPtr(0x1000)).is_err());
    memory.write(GuestPtr(0x100a), &8u16.to_be_bytes()).unwrap();
    assert!(Bitmap::read(&memory, GuestPtr(0x1000)).is_err());
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_monochrome_drawing_preserves_padding_origin_pattern_and_guest_writes() {
    let (mut memory, bitmap) = fixture();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 10, 2, GuestPtr(0x4000))).unwrap();
    gpu.palette[0] = [255, 255, 255, 255];
    gpu.palette[1] = [0, 0, 0, 255];
    let draw = Draw::Fill {
        rect: bitmap.bounds,
        clip: bitmap.bounds,
        pattern: [0x80; 8],
        foreground: 1,
        background: 0,
        mode: Transfer::Xor,
    };
    bitmap.execute(&gpu, &mut memory, draw).unwrap();
    // Local x=0 (staging x=3) is patterned; existing local x=-3 stays set.
    assert_eq!(
        memory.read(bitmap.base, 8).unwrap(),
        &[0x90, 0x3f, 0xaa, 0xbb, 0x10, 0x3f, 0xcc, 0xdd]
    );
    memory.write(bitmap.base, &[0x10]).unwrap(); // guest clears first pixel directly
    bitmap.execute(&gpu, &mut memory, draw).unwrap();
    assert_eq!(
        memory.read(bitmap.base, 8).unwrap(),
        &[0, 0x3f, 0xaa, 0xbb, 0, 0x3f, 0xcc, 0xdd]
    );
    assert_eq!(gpu.read_rgba().unwrap(), vec![255; 80]);
    memory.write(bitmap.base, &[0x80]).unwrap();
    bitmap
        .execute(
            &gpu,
            &mut memory,
            Draw::Copy {
                rect: Rect::new(-2, 5, 7, 6),
                clip: bitmap.bounds,
                source_x: -3,
                source_y: 5,
                mode: Transfer::Copy,
            },
        )
        .unwrap();
    assert_eq!(
        memory.read(bitmap.base, 8).unwrap(),
        &[0xc0, 0x3f, 0xaa, 0xbb, 0, 0x3f, 0xcc, 0xdd]
    );
    let rgba = gpu.read_rgba().unwrap();
    assert_eq!(&rgba[..8], &[0, 0, 0, 255, 0, 0, 0, 255]);
    assert!(rgba[8..].iter().all(|v| *v == 255));
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn offscreen_bitmap_can_differ_from_host_target_size() {
    let (mut memory, bitmap) = fixture();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu =
        pollster::block_on(Graphics::new(&instance, None, 512, 342, GuestPtr(0x4000))).unwrap();
    bitmap
        .execute(
            &gpu,
            &mut memory,
            Draw::Fill {
                rect: bitmap.bounds,
                clip: bitmap.bounds,
                pattern: [255; 8],
                foreground: 1,
                background: 0,
                mode: Transfer::Copy,
            },
        )
        .unwrap();
    assert_eq!(
        memory.read(bitmap.base, 8).unwrap(),
        &[255, 255, 0xaa, 0xbb, 255, 255, 0xcc, 0xdd]
    );
    assert_eq!((gpu.width, gpu.height), (512, 342));
    assert!(gpu.target(0, 1).is_err());
    assert!(gpu.target(4097, 1).is_err());
}
