use systemless_future::{
    graphics::{Graphics, Rect},
    memory::{GuestPtr, Memory, Permissions},
    process::Process,
    quickdraw::Ports,
};
fn setup() -> (Memory, Process, Ports) {
    let mut m = Memory::default();
    m.map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0; 8], Permissions::RW)
        .unwrap();
    let mut bm = [0; 14];
    bm[..4].copy_from_slice(&0x2000u32.to_be_bytes());
    bm[4..6].copy_from_slice(&2u16.to_be_bytes());
    bm[10..12].copy_from_slice(&4u16.to_be_bytes());
    bm[12..14].copy_from_slice(&8u16.to_be_bytes());
    m.write(GuestPtr(0x1000), &bm).unwrap();
    (
        m,
        Process::default(),
        Ports {
            current_slot: GuestPtr(0x1020),
            screen_bitmap: GuestPtr(0x1000),
        },
    )
}
#[test]
fn open_port_initializes_guest_fields_and_shared_heap_regions() {
    let (mut m, mut process, ports) = setup();
    ports.open(&mut m, &mut process, GuestPtr(0x1080)).unwrap();
    assert_eq!(ports.current(&m).unwrap(), GuestPtr(0x1080));
    assert_eq!(m.u16(GuestPtr(0x10b8)).unwrap(), 8);
    assert_eq!(m.read(GuestPtr(0x10ba), 8).unwrap(), &[255; 8]);
    let visible = m.u32(GuestPtr(0x1098)).unwrap();
    let clip = m.u32(GuestPtr(0x109c)).unwrap();
    assert_ne!(visible, clip);
    assert_eq!(process.handle_size(&m, visible).unwrap(), 10);
    ports.clip_rect(&mut m, Rect::new(1, 1, 7, 3)).unwrap();
    let data = m.u32(GuestPtr(clip)).unwrap();
    assert_eq!(
        m.read(GuestPtr(data + 2), 8).unwrap(),
        &[0, 1, 0, 1, 0, 3, 0, 7]
    );
    // The guest current-port slot is authoritative, not a cached host pointer.
    m.write(ports.current_slot, &0u32.to_be_bytes()).unwrap();
    assert!(ports.current(&m).is_err());
}
#[test]
#[ignore = "requires a real wgpu adapter"]
fn guest_port_clip_and_pen_changes_drive_gpu_pixels() {
    let (mut m, mut process, ports) = setup();
    ports.open(&mut m, &mut process, GuestPtr(0x1080)).unwrap();
    ports.clip_rect(&mut m, Rect::new(1, 1, 7, 3)).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[0, 0, 0x7e, 0, 0x7e, 0, 0, 0]
    );
    m.write(GuestPtr(0x10b8), &10u16.to_be_bytes()).unwrap(); // patXor
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), &[0; 8]);
    m.write(GuestPtr(0x1090), &[0, 0, 0, 2, 0, 4, 0, 8])
        .unwrap(); // portRect origin
    m.write(GuestPtr(0x10b8), &8u16.to_be_bytes()).unwrap();
    m.write(GuestPtr(0x10ba), &[0x80; 8]).unwrap();
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[0, 0, 0x20, 0, 0x20, 0, 0, 0]
    );
    let mut offscreen = m.read(GuestPtr(0x1000), 14).unwrap().to_vec();
    offscreen[..4].copy_from_slice(&0x2100u32.to_be_bytes());
    m.map(GuestPtr(0x1200), offscreen, Permissions::RW).unwrap();
    m.map(GuestPtr(0x2100), vec![0; 8], Permissions::RW)
        .unwrap();
    ports.set_bits(&mut m, GuestPtr(0x1200)).unwrap();
    let visible = m.u32(GuestPtr(0x1098)).unwrap();
    let visible_data = m.u32(GuestPtr(visible)).unwrap();
    m.write(GuestPtr(visible_data), &12u16.to_be_bytes())
        .unwrap();
    // Screen visibility is irrelevant to an offscreen destination.
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2100), 8).unwrap(),
        &[0, 0, 0x20, 0, 0x20, 0, 0, 0]
    );
    m.write(GuestPtr(0x10e8), &0x1234u32.to_be_bytes()).unwrap(); // grafProcs
    assert!(ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .is_err());
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[0, 0, 0x20, 0, 0x20, 0, 0, 0]
    );
}

#[test]
fn close_port_reclaims_regions_but_preserves_caller_storage_and_current_port() {
    let (mut memory, mut process, ports) = setup();
    let port = GuestPtr(0x1080);
    ports.open(&mut memory, &mut process, port).unwrap();
    let visible = memory.u32(GuestPtr(port.0 + 24)).unwrap();
    let clip = memory.u32(GuestPtr(port.0 + 28)).unwrap();
    let original = memory.read(port, 108).unwrap().to_vec();
    ports.close(&mut memory, &mut process, port).unwrap();
    assert_eq!(memory.read(port, 108).unwrap(), original);
    assert_eq!(ports.current(&memory).unwrap(), port);
    assert_eq!(memory.read(GuestPtr(0x2000), 8).unwrap(), &[0; 8]);
    assert_eq!(process.handle_size(&memory, visible).unwrap(), 0);
    assert_ne!(process.mem_error, 0);
    assert_eq!(process.handle_size(&memory, clip).unwrap(), 0);
    assert_ne!(process.mem_error, 0);
    // Reopening uses the freed storage instead of leaking two regions per cycle.
    ports.open(&mut memory, &mut process, port).unwrap();
    assert_eq!(memory.u32(GuestPtr(port.0 + 24)).unwrap(), visible);
    assert_eq!(memory.u32(GuestPtr(port.0 + 28)).unwrap(), clip);
}
#[test]
fn close_port_rejects_bad_second_handle_before_freeing_first() {
    let (mut memory, mut process, ports) = setup();
    let port = GuestPtr(0x1080);
    ports.open(&mut memory, &mut process, port).unwrap();
    let visible = memory.u32(GuestPtr(port.0 + 24)).unwrap();
    memory
        .write(GuestPtr(port.0 + 28), &0u32.to_be_bytes())
        .unwrap();
    assert!(ports.close(&mut memory, &mut process, port).is_err());
    assert_eq!(process.handle_size(&memory, visible).unwrap(), 10);
    memory
        .write(GuestPtr(port.0 + 28), &visible.to_be_bytes())
        .unwrap();
    assert!(ports.close(&mut memory, &mut process, port).is_err());
    assert_eq!(process.handle_size(&memory, visible).unwrap(), 10);
}

#[test]
fn init_port_restores_defaults_in_place_and_rejects_invalid_regions_atomically() {
    let (mut memory, mut process, ports) = setup();
    let port = GuestPtr(0x1080);
    ports.open(&mut memory, &mut process, port).unwrap();
    let original = memory.read(port, 108).unwrap().to_vec();
    let visible = memory.u32(GuestPtr(port.0 + 24)).unwrap();
    let clip = memory.u32(GuestPtr(port.0 + 28)).unwrap();
    let visible_data = GuestPtr(memory.u32(GuestPtr(visible)).unwrap());
    let clip_data = GuestPtr(memory.u32(GuestPtr(clip)).unwrap());
    let original_visible = memory.read(visible_data, 10).unwrap().to_vec();
    let original_clip = memory.read(clip_data, 10).unwrap().to_vec();
    ports.clip_rect(&mut memory, Rect::new(1, 1, 2, 2)).unwrap();
    memory
        .write(GuestPtr(port.0 + 56), &10u16.to_be_bytes())
        .unwrap();
    memory
        .write(ports.current_slot, &0u32.to_be_bytes())
        .unwrap();
    ports.init(&mut memory, &mut process, port).unwrap();
    assert_eq!(memory.read(port, 108).unwrap(), original);
    assert_eq!(memory.read(visible_data, 10).unwrap(), original_visible);
    assert_eq!(memory.read(clip_data, 10).unwrap(), original_clip);
    assert_eq!(ports.current(&memory).unwrap(), port);
    memory
        .write(GuestPtr(port.0 + 28), &0u32.to_be_bytes())
        .unwrap();
    let invalid = memory.read(port, 108).unwrap().to_vec();
    assert!(ports.init(&mut memory, &mut process, port).is_err());
    assert_eq!(memory.read(port, 108).unwrap(), invalid);
    assert_eq!(memory.read(visible_data, 10).unwrap(), original_visible);
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn complex_clip_keeps_holes_and_xor_single_pass_with_guest_writes() {
    let (mut m, mut process, ports) = setup();
    let port = GuestPtr(0x1080);
    ports.open(&mut m, &mut process, port).unwrap();
    let clip = m.u32(GuestPtr(port.0 + 28)).unwrap();
    let words: [u16; 22] = [
        44, 0, 0, 4, 8, 0, 0, 8, 32767, 1, 2, 6, 32767, 3, 2, 6, 32767, 4, 0, 8, 32767, 32767,
    ];
    process.set_handle_size(&mut m, clip, 44).unwrap();
    let bytes: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let data = GuestPtr(m.u32(GuestPtr(clip)).unwrap());
    m.write(data, &bytes).unwrap();
    m.write(GuestPtr(0x2000), &[0, 33, 0, 44, 0, 55, 0, 66])
        .unwrap();
    m.write(GuestPtr(port.0 + 56), &10u16.to_be_bytes())
        .unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let mut gpu =
        pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x75000000))).unwrap();
    gpu.palette[0] = [255; 4];
    gpu.palette[1] = [0, 0, 0, 255];
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[255, 33, 0xc3, 44, 0xc3, 55, 255, 66]
    );
    let rgba = gpu.read_rgba().unwrap();
    for y in 0..4 {
        for x in 0..8 {
            let black = y == 0 || y == 3 || !(2..6).contains(&x);
            assert_eq!(
                &rgba[(y * 8 + x) * 4..(y * 8 + x + 1) * 4],
                if black { &[0, 0, 0, 255] } else { &[255; 4] }
            );
        }
    }
    m.write(GuestPtr(0x2000), &[0x7f]).unwrap();
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[0x80, 33, 0, 44, 0, 55, 0, 66]
    );
    // A complex visible region intersects the complex clip for screen drawing.
    let visible = m.u32(GuestPtr(port.0 + 24)).unwrap();
    process.set_handle_size(&mut m, visible, 36).unwrap();
    let visible_words: [u16; 18] = [
        36, 0, 0, 4, 8, 0, 0, 4, 32767, 2, 4, 8, 32767, 4, 0, 8, 32767, 32767,
    ];
    let visible_bytes: Vec<_> = visible_words
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect();
    m.write(GuestPtr(m.u32(GuestPtr(visible)).unwrap()), &visible_bytes)
        .unwrap();
    ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .unwrap();
    assert_eq!(
        m.read(GuestPtr(0x2000), 8).unwrap(),
        &[0x70, 33, 0xc0, 44, 0xc3, 55, 255, 66]
    );
    // Geometry fails before drawing; no partial guest pixels are published.
    m.write(data, &43u16.to_be_bytes()).unwrap();
    let before = m.read(GuestPtr(0x2000), 8).unwrap().to_vec();
    assert!(ports
        .paint_rect(&gpu, &mut m, Rect::new(0, 0, 8, 4))
        .is_err());
    assert_eq!(m.read(GuestPtr(0x2000), 8).unwrap(), before);
}

#[test]
fn point_conversion_uses_current_bitmap_origin_and_signed_word_coordinates() {
    let (mut m, mut process, ports) = setup();
    ports.open(&mut m, &mut process, GuestPtr(0x1080)).unwrap();
    let point = GuestPtr(0x1040);
    m.write(GuestPtr(0x1088), &[0xff, 0xba, 0xff, 0xc4])
        .unwrap(); // top=-70,left=-60
                   // portRect is deliberately unrelated; it does not define this transform.
    m.write(GuestPtr(0x1090), &[0, 9, 0, 11]).unwrap();
    m.write(point, &[0, 3, 0, 4]).unwrap();
    ports.convert_point(&mut m, point, true).unwrap();
    assert_eq!(m.read(point, 4).unwrap(), [0, 73, 0, 64]);
    ports.convert_point(&mut m, point, false).unwrap();
    assert_eq!(m.read(point, 4).unwrap(), [0, 3, 0, 4]);
    // Read the current guest record on every call, including changed origins.
    m.write(GuestPtr(0x1088), &[0, 1, 0xff, 0xff]).unwrap();
    m.write(point, &[0x80, 0, 0x7f, 0xff]).unwrap();
    ports.convert_point(&mut m, point, true).unwrap();
    assert_eq!(m.read(point, 4).unwrap(), [0x7f, 0xff, 0x80, 0]);
    ports.convert_point(&mut m, point, false).unwrap();
    assert_eq!(m.read(point, 4).unwrap(), [0x80, 0, 0x7f, 0xff]);
    ports.open(&mut m, &mut process, GuestPtr(0x1100)).unwrap();
    m.write(point, &[0, 3, 0, 4]).unwrap();
    ports.convert_point(&mut m, point, true).unwrap();
    assert_eq!(m.read(point, 4).unwrap(), [0, 3, 0, 4]);
}

#[test]
fn point_conversion_rejects_invalid_ports_and_outputs_without_partial_writes() {
    let (mut m, mut process, ports) = setup();
    ports.open(&mut m, &mut process, GuestPtr(0x1080)).unwrap();
    m.map(GuestPtr(0x3000), vec![0, 3, 0, 4], Permissions::RX)
        .unwrap();
    assert!(ports.convert_point(&mut m, GuestPtr(0x3000), true).is_err());
    assert_eq!(m.read(GuestPtr(0x3000), 4).unwrap(), [0, 3, 0, 4]);
    m.write(GuestPtr(0x11fe), &[0x12, 0x34]).unwrap();
    assert!(ports
        .convert_point(&mut m, GuestPtr(0x11fe), false)
        .is_err());
    assert_eq!(m.read(GuestPtr(0x11fe), 2).unwrap(), [0x12, 0x34]);
    let point = GuestPtr(0x1040);
    m.write(point, &[0, 3, 0, 4]).unwrap();
    m.write(GuestPtr(0x1086), &0xc000u16.to_be_bytes()).unwrap();
    assert!(ports.convert_point(&mut m, point, true).is_err());
    assert_eq!(m.read(point, 4).unwrap(), [0, 3, 0, 4]);
    m.write(ports.current_slot, &0u32.to_be_bytes()).unwrap();
    assert!(ports.convert_point(&mut m, point, true).is_err());
    assert_eq!(m.read(point, 4).unwrap(), [0, 3, 0, 4]);
}
