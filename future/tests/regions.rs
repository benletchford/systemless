use systemless_future::{
    memory::{GuestPtr, Memory, Permissions},
    process::Process,
    regions::{self, Op},
};
#[test]
fn new_rect_empty_and_dispose_use_shared_handles() {
    let mut m = Memory::default();
    let mut p = Process::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 1, 0, 2, 0, 5, 0, 7],
        Permissions::RW,
    )
    .unwrap();
    let h = regions::call(&mut m, &mut p, Op::New, 0, 0).unwrap();
    let data = GuestPtr(m.u32(GuestPtr(h)).unwrap());
    assert_eq!(m.read(data, 10).unwrap(), &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    regions::call(&mut m, &mut p, Op::Rect, h, 0x1000).unwrap();
    assert_eq!(m.read(data, 10).unwrap(), &[0, 10, 0, 1, 0, 2, 0, 5, 0, 7]);
    m.write(GuestPtr(0x1004), &0u16.to_be_bytes()).unwrap();
    regions::call(&mut m, &mut p, Op::Rect, h, 0x1000).unwrap();
    assert_eq!(m.read(data, 10).unwrap(), &[0, 10, 0, 0, 0, 0, 0, 0, 0, 0]);
    regions::call(&mut m, &mut p, Op::Dispose, h, 0).unwrap();
    assert_eq!(p.handle_size(&m, h).unwrap(), 0);
    assert_eq!(p.mem_error, -111);
}
#[test]
fn copy_preserves_variable_region_bytes_and_supports_self_copy() {
    let mut m = Memory::default();
    let mut p = Process::default();
    let source = regions::call(&mut m, &mut p, Op::New, 0, 0).unwrap();
    let destination = regions::call(&mut m, &mut p, Op::New, 0, 0).unwrap();
    let old = m.u32(GuestPtr(destination)).unwrap();
    // An L-shaped region in inversion-point storage. Copying is byte-preserving;
    // raster interpretation of this payload is deliberately a separate contract.
    let words: [u16; 18] = [
        36, 0, 0, 4, 4, 0, 0, 4, 0x7fff, 2, 2, 4, 0x7fff, 4, 0, 2, 0x7fff, 0x7fff,
    ];
    let bytes: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    p.set_handle_size(&mut m, source, 36).unwrap();
    m.write(GuestPtr(m.u32(GuestPtr(source)).unwrap()), &bytes)
        .unwrap();
    regions::call(&mut m, &mut p, Op::Copy, source, destination).unwrap();
    let data = GuestPtr(m.u32(GuestPtr(destination)).unwrap());
    assert_ne!(data.0, old);
    assert_eq!(p.handle_size(&m, destination).unwrap(), 36);
    assert_eq!(m.read(data, 36).unwrap(), bytes);
    regions::call(&mut m, &mut p, Op::Copy, destination, destination).unwrap();
    assert_eq!(m.read(data, 36).unwrap(), bytes);
    // Malformed source length cannot resize or corrupt the destination.
    m.write(
        GuestPtr(m.u32(GuestPtr(source)).unwrap()),
        &38u16.to_be_bytes(),
    )
    .unwrap();
    assert!(regions::call(&mut m, &mut p, Op::Copy, source, destination).is_err());
    assert_eq!(m.read(data, 36).unwrap(), bytes);
}

#[test]
fn boolean_destination_can_alias_sources_and_invalid_source_preserves_it() {
    use systemless_future::region_geometry::{decode, Boolean};
    let mut m = Memory::default();
    let mut p = Process::default();
    m.map(
        GuestPtr(0x1000),
        vec![0, 0, 0, 0, 0, 8, 0, 8, 0, 2, 0, 2, 0, 6, 0, 6],
        Permissions::RW,
    )
    .unwrap();
    let a = regions::call(&mut m, &mut p, Op::New, 0, 0).unwrap();
    let b = regions::call(&mut m, &mut p, Op::New, 0, 0).unwrap();
    regions::call(&mut m, &mut p, Op::Rect, a, 0x1000).unwrap();
    regions::call(&mut m, &mut p, Op::Rect, b, 0x1008).unwrap();
    regions::combine(&mut m, &mut p, Boolean::Difference, a, b, b).unwrap();
    let data = GuestPtr(m.u32(GuestPtr(b)).unwrap());
    let size = p.handle_size(&m, b).unwrap();
    let before = m.read(data, size as usize).unwrap().to_vec();
    assert!(decode(&before).unwrap().len() > 1);
    let source = GuestPtr(m.u32(GuestPtr(a)).unwrap());
    m.write(source, &11u16.to_be_bytes()).unwrap();
    assert!(regions::combine(&mut m, &mut p, Boolean::Union, a, b, b).is_err());
    assert_eq!(m.read(data, size as usize).unwrap(), before);
    regions::combine(&mut m, &mut p, Boolean::Xor, b, b, b).unwrap();
    assert_eq!(p.handle_size(&m, b).unwrap(), 10);
    assert_eq!(regions::call(&mut m, &mut p, Op::Empty, b, 0).unwrap(), 1);
}
