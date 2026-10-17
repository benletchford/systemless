mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::{CreateSpecResource, Fork},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    process::resources::{AddResource, Op},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let fixture = support::pef_program(&[0x4e800020], &[b"AddResource"], 16);
    let mut a = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xab], Permissions::RX)
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
    assert!(
        a.runtime
            .process
            .resource_file(
                &mut a.runtime.memory,
                Op::OpenSpec {
                    spec,
                    permission: 3
                }
            )
            .unwrap()
            > 0
    );
    a
}
#[test]
fn both_cpus_attach_the_original_handle_set_changed_state_and_preserve_void_abi() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let h = a
            .runtime
            .process
            .new_handle(&mut a.runtime.memory, 3, false)
            .unwrap();
        let data = a.runtime.memory.u32(GuestPtr(h)).unwrap();
        a.runtime.memory.write(GuestPtr(data), &[1, 2, 3]).unwrap();
        a.runtime
            .process
            .set_handle_lock(&a.runtime.memory, h, true)
            .unwrap();
        let name = GuestPtr(STACK_BASE + 0x6100);
        a.runtime
            .memory
            .write(name, &[3, 0x80, b'x', b'y'])
            .unwrap();
        let stack = STACK_BASE + 0x7000;
        let mut frame = name.0.to_be_bytes().to_vec();
        frame.extend((-7i16).to_be_bytes());
        frame.extend(b"DATA");
        frame.extend(h.to_be_bytes());
        frame.extend([0x5a; 4]);
        a.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
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
                c.dar[0] = 0x11223344;
                c.dar[1] = 0xaabbccdd;
            }
            Cpu::PowerPc(c) => {
                c.gpr[3] = h;
                c.gpr[4] = u32::from_be_bytes(*b"DATA");
                c.gpr[5] = 0xabcdfff9;
                c.gpr[6] = name.0;
                c.gpr[7] = 0xaabbccdd;
            }
        }
        let free = a.runtime.process.free_memory();
        a.runtime.process.mem_error = -108;
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
        match &a.runtime.cpu {
            Cpu::M68k(c) => assert_eq!(
                (c.dar[0], c.dar[1], c.dar[15]),
                (0x11223344, 0xaabbccdd, stack + 14)
            ),
            Cpu::PowerPc(c) => assert_eq!(
                (c.gpr[3], c.gpr[4], c.gpr[5], c.gpr[6], c.gpr[7]),
                (
                    h,
                    u32::from_be_bytes(*b"DATA"),
                    0xabcdfff9,
                    name.0,
                    0xaabbccdd
                )
            ),
        }
        assert_eq!(a.runtime.memory.read(GuestPtr(stack), 18).unwrap(), frame);
        assert_eq!(a.runtime.process.res_error, 0);
        assert_eq!(a.runtime.memory.u16(GuestPtr(0xa60)).unwrap(), 0);
        assert_eq!(a.runtime.process.mem_error, -108);
        assert_eq!(a.runtime.process.free_memory(), free);
        assert_eq!(
            a.runtime
                .process
                .get_current_resource(&mut a.runtime.memory, *b"DATA", -7)
                .unwrap(),
            h
        );
        let entry = a.runtime.process.find_resource(*b"DATA", -7).unwrap();
        assert_eq!(
            (
                entry.attributes,
                entry.name.as_deref(),
                entry.data.as_slice()
            ),
            (2, Some([0x80, b'x', b'y'].as_slice()), [1, 2, 3].as_slice())
        );
        assert_eq!(
            a.runtime
                .process
                .handle_state(&a.runtime.memory, h)
                .unwrap()
                & 0xa0,
            0xa0
        );
        a.runtime
            .process
            .release_resource(&mut a.runtime.memory, h)
            .unwrap();
        assert_eq!(
            a.runtime
                .process
                .get_current_resource(&mut a.runtime.memory, *b"DATA", -7)
                .unwrap(),
            h
        );
        a.runtime
            .process
            .detach_resource(&a.runtime.memory, h)
            .unwrap();
        assert_eq!(a.runtime.process.res_error, -198);
        let reference = a.runtime.process.current_resource_file();
        assert_eq!(
            a.runtime
                .process
                .resource_file(&mut a.runtime.memory, Op::Close(reference))
                .unwrap(),
            0
        );
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
        assert_eq!(
            systemless_future::load::resource::ResourceFork::parse(
                a.runtime
                    .process
                    .files
                    .catalog_fork(node, Fork::Resource)
                    .unwrap()
            )
            .unwrap()
            .entries()
            .count(),
            1
        );
    }
}
#[test]
fn nil_attached_handles_duplicates_and_readonly_maps_never_overwrite_existing_entries() {
    let mut a = app();
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 0, false)
        .unwrap();
    let req = AddResource {
        handle: h,
        kind: *b"ZERO",
        id: 128,
        name: GuestPtr(0),
    };
    a.runtime
        .process
        .add_resource(&a.runtime.memory, AddResource { handle: 0, ..req })
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -194);
    a.runtime
        .process
        .add_resource(&a.runtime.memory, req)
        .unwrap();
    assert_eq!(a.runtime.process.res_error, 0);
    assert_eq!(
        a.runtime.process.find_resource(*b"ZERO", 128).unwrap().data,
        []
    );
    a.runtime
        .process
        .add_resource(&a.runtime.memory, AddResource { id: 129, ..req })
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -194);
    let other = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 0, false)
        .unwrap();
    let flags = a
        .runtime
        .process
        .handle_state(&a.runtime.memory, other)
        .unwrap();
    assert!(a
        .runtime
        .process
        .add_resource(
            &a.runtime.memory,
            AddResource {
                handle: other,
                ..req
            }
        )
        .is_err());
    assert_eq!(
        a.runtime
            .process
            .handle_state(&a.runtime.memory, other)
            .unwrap(),
        flags
    );
    assert_eq!(
        a.runtime
            .process
            .get_current_resource(&mut a.runtime.memory, *b"ZERO", 128)
            .unwrap(),
        h
    );
    let readonly = a
        .runtime
        .process
        .resource_file(
            &mut a.runtime.memory,
            Op::OpenSpec {
                spec: GuestPtr(STACK_BASE + 0x6000),
                permission: 1,
            },
        )
        .unwrap();
    a.runtime
        .process
        .resource_file(&mut a.runtime.memory, Op::Use(readonly))
        .unwrap();
    a.runtime
        .process
        .add_resource(
            &a.runtime.memory,
            AddResource {
                handle: other,
                id: 129,
                ..req
            },
        )
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -54);
}
#[test]
fn disk_exhaustion_does_not_attach_the_handle_or_mark_a_new_resource_changed() {
    use systemless_future::files::Op as FileOp;
    let mut a = app();
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
    let fork_size = a
        .runtime
        .process
        .files
        .catalog_fork(node, Fork::Resource)
        .unwrap()
        .len();
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
            &((64 * 1024 * 1024 - fork_size) as u32).to_be_bytes(),
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
    let h = a
        .runtime
        .process
        .new_handle(&mut a.runtime.memory, 1, true)
        .unwrap();
    let flags = a
        .runtime
        .process
        .handle_state(&a.runtime.memory, h)
        .unwrap();
    let free = a.runtime.process.free_memory();
    a.runtime
        .process
        .add_resource(
            &a.runtime.memory,
            AddResource {
                handle: h,
                kind: *b"DATA",
                id: 1,
                name: GuestPtr(0),
            },
        )
        .unwrap();
    assert_eq!(a.runtime.process.res_error, -34);
    assert_eq!(
        a.runtime
            .process
            .handle_state(&a.runtime.memory, h)
            .unwrap(),
        flags
    );
    assert!(a.runtime.process.find_resource(*b"DATA", 1).is_none());
    assert_eq!(a.runtime.process.free_memory(), free);
}
#[test]
fn native_frame_name_and_error_global_faults_preserve_stack_and_shared_bytes() {
    use systemless_future::{abi::Abi, memory::Memory};
    for which in 0..3 {
        let mut m = Memory::default();
        m.map(
            GuestPtr(0x1000),
            vec![0; if which == 0 { 13 } else { 18 }],
            Permissions::RW,
        )
        .unwrap();
        m.map(GuestPtr(0x2000), vec![2, b'x', b'y'], Permissions::RX)
            .unwrap();
        m.map(
            GuestPtr(0xa60),
            vec![0x5a; 2],
            if which == 2 {
                Permissions::RX
            } else {
                Permissions::RW
            },
        )
        .unwrap();
        let mut frame = (if which == 1 { 0x2002u32 } else { 0x2000u32 })
            .to_be_bytes()
            .to_vec();
        frame.extend(128i16.to_be_bytes());
        frame.extend(b"DATA");
        frame.extend(0u32.to_be_bytes());
        let length = if which == 0 { 13 } else { 14 };
        m.write(GuestPtr(0x1000), &frame[..length]).unwrap();
        let Cpu::M68k(mut c) = Cpu::new(Architecture::M68k, 0, 0x1000) else {
            unreachable!()
        };
        assert!(Abi::trap(0xa9ab, &mut c, &m).is_err());
        assert_eq!(c.dar[15], 0x1000);
        assert_eq!(m.read(GuestPtr(0x1000), length).unwrap(), &frame[..length]);
        assert_eq!(m.read(GuestPtr(0xa60), 2).unwrap(), [0x5a; 2]);
    }
}
