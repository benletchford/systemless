use systemless_future::{graphics::Rect, rounded::coverage};
fn bitmap(rectangles: &[Rect], bounds: Rect) -> Vec<u8> {
    let mut rows = vec![0; (bounds.bottom - bounds.top) as usize];
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            let count = rectangles
                .iter()
                .filter(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                .count();
            assert!(count <= 1, "overlapping coverage would corrupt XOR");
            if count == 1 {
                rows[(y - bounds.top) as usize] |= 0x80 >> (x - bounds.left);
            }
        }
    }
    rows
}
#[test]
fn rounded_pixels_match_legacy_and_frame_is_outer_minus_inset() {
    let bounds = Rect::new(0, 0, 8, 4);
    assert_eq!(
        bitmap(&coverage(bounds, 4, 4, None).unwrap(), bounds),
        [0x7e, 0xff, 0xff, 0x7e]
    );
    assert_eq!(
        bitmap(&coverage(bounds, 4, 4, Some((1, 1))).unwrap(), bounds),
        [0x7e, 0x81, 0x81, 0x7e]
    );
    assert_eq!(
        bitmap(&coverage(bounds, 4, 4, Some((2, 1))).unwrap(), bounds),
        [0x7e, 0xc3, 0xc3, 0x7e]
    );
    assert_eq!(
        bitmap(&coverage(bounds, 4, 4, Some((4, 2))).unwrap(), bounds),
        [0x7e, 0xff, 0xff, 0x7e]
    );
    assert!(coverage(bounds, 4, 4, Some((0, 0))).unwrap().is_empty());
    let moved = Rect::new(-7, -13, 1, -9);
    assert_eq!(
        bitmap(&coverage(moved, 4, 4, Some((1, 1))).unwrap(), moved),
        [0x7e, 0x81, 0x81, 0x7e]
    );
    assert_eq!(
        bitmap(&coverage(bounds, -1, 4, None).unwrap(), bounds),
        [0xff; 4]
    );
    assert!(coverage(bounds, 4, 4, Some((-1, 1))).is_err());
    assert!(coverage(Rect::new(0, 0, 32768, 1), 1, 1, None).is_err());
}
#[test]
fn geometry_is_bounded_at_guest_extremes() {
    for bounds in [
        Rect::new(-32768, -32768, 32767, 32767),
        Rect::new(0, -32768, 1, 32767),
        Rect::new(-32768, 0, 32767, 1),
    ] {
        for (w, h) in [(1, 32767), (32767, 1), (32767, 32767)] {
            let result = coverage(bounds, w, h, Some((32767, 32767))).unwrap();
            assert!(result.len() <= 131070);
            for r in result {
                assert!(
                    r.left >= bounds.left
                        && r.right <= bounds.right
                        && r.top >= bounds.top
                        && r.bottom <= bounds.bottom
                );
            }
        }
    }
    assert!(coverage(Rect::new(2, 2, 1, 1), 4, 4, None)
        .unwrap()
        .is_empty());
}

#[test]
#[ignore = "requires a real wgpu adapter"]
fn gpu_round_rect_clips_patterns_and_preserves_guest_storage() {
    use systemless_future::{
        graphics::Graphics,
        memory::{GuestPtr, Memory, Permissions},
        process::Process,
        quickdraw::Ports,
    };
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x1000), vec![0; 512], Permissions::RW)
        .unwrap();
    memory
        .map(GuestPtr(0x2000), vec![0x55; 8], Permissions::RW)
        .unwrap();
    let mut bm = [0; 14];
    bm[..4].copy_from_slice(&0x2000u32.to_be_bytes());
    bm[4..6].copy_from_slice(&2u16.to_be_bytes());
    bm[10..12].copy_from_slice(&4u16.to_be_bytes());
    bm[12..14].copy_from_slice(&8u16.to_be_bytes());
    memory.write(GuestPtr(0x1000), &bm).unwrap();
    let ports = Ports {
        current_slot: GuestPtr(0x1020),
        screen_bitmap: GuestPtr(0x1000),
    };
    let port = GuestPtr(0x1080);
    ports
        .open(&mut memory, &mut Process::default(), port)
        .unwrap();
    ports.clip_rect(&mut memory, Rect::new(1, 0, 7, 4)).unwrap();
    memory
        .write(GuestPtr(port.0 + 56), &10u16.to_be_bytes())
        .unwrap();
    memory.write(GuestPtr(port.0 + 58), &[0xaa; 8]).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let gpu = pollster::block_on(Graphics::new(&instance, None, 8, 4, GuestPtr(0x4000))).unwrap();
    ports
        .round_rect(&gpu, &mut memory, Rect::new(0, 0, 8, 4), 4, 4, true)
        .unwrap();
    assert_eq!(
        memory.read(GuestPtr(0x2000), 8).unwrap(),
        [0x7f, 0x55, 0x55, 0x55, 0x55, 0x55, 0x7f, 0x55]
    );
    memory.write(GuestPtr(0x2000), &[0x80]).unwrap();
    ports
        .round_rect(&gpu, &mut memory, Rect::new(0, 0, 8, 4), 4, 4, true)
        .unwrap();
    assert_eq!(
        memory.read(GuestPtr(0x2000), 8).unwrap(),
        [0xaa, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55]
    );
    let before = memory.read(GuestPtr(0x2000), 8).unwrap().to_vec();
    memory
        .write(GuestPtr(port.0 + 66), &(-1i16).to_be_bytes())
        .unwrap();
    ports
        .round_rect(&gpu, &mut memory, Rect::new(0, 0, 8, 4), 4, 4, true)
        .unwrap();
    assert_eq!(memory.read(GuestPtr(0x2000), 8).unwrap(), before);
    memory
        .write(GuestPtr(port.0 + 104), &1u32.to_be_bytes())
        .unwrap();
    assert!(ports
        .round_rect(&gpu, &mut memory, Rect::new(0, 0, 8, 4), 4, 4, true)
        .is_err());
    assert_eq!(memory.read(GuestPtr(0x2000), 8).unwrap(), before);
}
