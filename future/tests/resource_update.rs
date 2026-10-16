mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{CreateSpecResource, Fork},
    load::{resource::ResourceFork, LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::resources::{AddResource, Op},
    runtime::Stop,
};
fn app() -> (LoadedApplication, u32, i16) {
    let fixture = support::pef_program(&[0x4e800020], &[b"UpdateResFile"], 16);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x99], Permissions::RX)
        .unwrap();
    let spec = GuestPtr(STACK_BASE + 0x6000);
    let mut bytes = [0; 70];
    bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
    bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
    bytes[6] = 5;
    bytes[7..12].copy_from_slice(b"Prefs");
    a.runtime.memory.write(spec, &bytes).unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .create_spec_resource(
                &a.runtime.memory,
                CreateSpecResource {
                    spec,
                    creator: *b"TEST",
                    kind: *b"pref",
                    script: 0
                }
            )
            .unwrap(),
        0
    );
    let reference = a
        .runtime
        .process
        .resource_file(
            &mut a.runtime.memory,
            Op::OpenSpec {
                spec,
                permission: 3,
            },
        )
        .unwrap();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 3, true)
        .unwrap();
    a.runtime
        .process
        .add_resource(
            &a.runtime.memory,
            AddResource {
                handle: h,
                kind: *b"DATA",
                id: 128,
                name: GuestPtr(0),
            },
        )
        .unwrap();
    (a, h, reference)
}
fn disk(a: &LoadedApplication) -> ResourceFork {
    let node = a
        .runtime
        .process
        .files
        .directory_view(2)
        .unwrap()
        .entries
        .iter()
        .find(|e| e.name == b"Prefs")
        .unwrap()
        .id;
    ResourceFork::parse(
        a.runtime
            .process
            .files
            .catalog_fork(node, Fork::Resource)
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn both_cpus_write_live_resized_data_clear_changed_flags_and_reopen_saved_volumes() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let (mut a, h, reference) = app();
        a.runtime
            .process
            .set_handle_size(&mut a.runtime.memory, h, 5)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(data), &[9, 8, 7, 6, 5])
            .unwrap();
        assert!(disk(&a).get(*b"DATA", 128).is_none());
        let stack = STACK_BASE + 0x7000;
        a.runtime.memory.write(GuestPtr(stack), &[0x5a; 8]).unwrap();
        let vector = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        let gate = a.runtime.memory.u32(GuestPtr(vector)).unwrap();
        a.runtime.cpu = Cpu::new(
            arch,
            if arch == Architecture::M68k {
                0x50000000
            } else {
                gate
            },
            stack,
        );
        match &mut a.runtime.cpu {
            Cpu::M68k(c) => {
                c.dar[0] = 0xaabbccdd;
                c.dar[1] = 0x11223344;
                a.runtime
                    .memory
                    .write(GuestPtr(stack), &reference.to_be_bytes())
                    .unwrap();
            }
            Cpu::PowerPc(c) => {
                c.gpr[3] = 0xabcd0000 | u32::from(reference as u16);
                c.gpr[4] = 0x11223344;
            }
        }
        // Updating a specified file must not depend on the current search selection.
        a.runtime
            .process
            .resource_file(&mut a.runtime.memory, Op::Use(0))
            .unwrap();
        let current = a.runtime.process.current_resource_file();
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                assert_eq!(
                    (c.dar[0], c.dar[1], c.dar[15]),
                    (0xaabbccdd, 0x11223344, stack + 2)
                );
                assert_eq!(
                    a.runtime.memory.read(GuestPtr(stack + 2), 6).unwrap(),
                    [0x5a; 6]
                );
            }
            Cpu::PowerPc(c) => assert_eq!(
                (c.gpr[3], c.gpr[4]),
                (0xabcd0000 | u32::from(reference as u16), 0x11223344)
            ),
        }
        assert_eq!(a.runtime.process.res_error, 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
        assert_eq!(a.runtime.process.current_resource_file(), current);
        let saved = disk(&a);
        assert_eq!(saved.get(*b"DATA", 128).unwrap().data, [9, 8, 7, 6, 5]);
        assert_eq!(saved.get(*b"DATA", 128).unwrap().attributes & 2, 0);
        assert_eq!(saved.attributes & 0x60, 0);
        a.runtime
            .process
            .resource_file(&mut a.runtime.memory, Op::Close(reference))
            .unwrap();
        assert_eq!(a.runtime.process.res_error, 0);
        let image = a.runtime.process.files.export_volume().unwrap();
        let fixture = support::pef_program(&[0x4e800020], &[], 16);
        let mut fresh = LoadedApplication::pef(&fixture.bytes, None).unwrap();
        fresh.runtime.process.files.restore_volume(&image).unwrap();
        let spec = GuestPtr(STACK_BASE + 0x6000);
        let bytes = a.runtime.memory.read(spec, 70).unwrap().to_vec();
        fresh.runtime.memory.write(spec, &bytes).unwrap();
        assert!(
            fresh
                .runtime
                .process
                .resource_file(
                    &mut fresh.runtime.memory,
                    Op::OpenSpec {
                        spec,
                        permission: 1
                    }
                )
                .unwrap()
                > 0
        );
        let loaded = fresh
            .runtime
            .process
            .get_current_resource(&mut fresh.runtime.memory, *b"DATA", 128)
            .unwrap();
        let pointer = fresh.runtime.memory.u32(GuestPtr(loaded)).unwrap();
        assert_eq!(
            fresh.runtime.memory.read(GuestPtr(pointer), 5).unwrap(),
            [9, 8, 7, 6, 5]
        );
    }
}
#[test]
fn corrupt_master_failure_retains_disk_and_dirty_state_and_allows_retry() {
    let (mut a, h, reference) = app();
    let old = disk(&a).encode().unwrap();
    let pointer = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime
        .memory
        .write(GuestPtr(h), &0u32.to_be_bytes())
        .unwrap();
    assert!(a
        .runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Update(reference))
        .is_err());
    assert_eq!(disk(&a).encode().unwrap(), old);
    assert_ne!(
        a.runtime
            .process
            .find_resource(*b"DATA", 128)
            .unwrap()
            .attributes
            & 2,
        0
    );
    a.runtime
        .memory
        .write(GuestPtr(h), &pointer.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(pointer), &[4, 5, 6])
        .unwrap();
    a.runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Update(reference))
        .unwrap();
    assert_eq!(disk(&a).get(*b"DATA", 128).unwrap().data, [4, 5, 6]);
    a.runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Update(0x7fff))
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -192);
}
#[test]
fn disk_full_update_preserves_fork_dirty_flags_and_reservation_until_retry() {
    use systemless_future::files::Op as FileOp;
    let (mut a, h, reference) = app();
    let old = disk(&a).encode().unwrap();
    // Fill all space left after AddResource's initial pending-growth reservation.
    let projected = a
        .runtime
        .process
        .find_resource(*b"DATA", 128)
        .unwrap()
        .data
        .len();
    assert_eq!(projected, 3);
    // Empty fork: 286 bytes. Added DATA reference: 8 type + 12 reference + 8 data bytes.
    let reserved_length = old.len() + 28;
    let name = GuestPtr(STACK_BASE + 0x6100);
    let pb = GuestPtr(STACK_BASE + 0x6200);
    a.runtime.memory.write(name, b"\x04Fill").unwrap();
    let mut block = [0; 80];
    block[18..22].copy_from_slice(&name.0.to_be_bytes());
    block[22..24].copy_from_slice(&(-1i16).to_be_bytes());
    block[27] = 3;
    a.runtime.memory.write(pb, &block).unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .io(
                &mut a.runtime.memory,
                pb,
                FileOp::Create {
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    assert_eq!(
        a.runtime
            .process
            .files
            .io(
                &mut a.runtime.memory,
                pb,
                FileOp::Open {
                    kind: Fork::Data,
                    hierarchical: false
                }
            )
            .unwrap(),
        0
    );
    a.runtime
        .memory
        .write(
            GuestPtr(pb.0 + 28),
            &((64 * 1024 * 1024 - reserved_length) as u32).to_be_bytes(),
        )
        .unwrap();
    assert_eq!(
        a.runtime
            .process
            .files
            .io(&mut a.runtime.memory, pb, FileOp::SetEof)
            .unwrap(),
        0
    );
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 4096)
        .unwrap();
    a.runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Update(reference))
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -34);
    assert_eq!(disk(&a).encode().unwrap(), old);
    assert_ne!(
        a.runtime
            .process
            .find_resource(*b"DATA", 128)
            .unwrap()
            .attributes
            & 2,
        0
    );
    a.runtime
        .process
        .set_handle_size(&mut a.runtime.memory, h, 3)
        .unwrap();
    let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
    a.runtime.memory.write(GuestPtr(data), &[7, 8, 9]).unwrap();
    a.runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Update(reference))
        .unwrap();
    assert_eq!(a.runtime.process.res_error, 0);
    assert_eq!(disk(&a).get(*b"DATA", 128).unwrap().data, [7, 8, 9]);
}
