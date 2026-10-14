mod support;
use support::*;
use systemless_future::{
    cpu::Cpu,
    load::{LoadedApplication, A5_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
    sound,
};
fn parameter(level: i16) -> [u8; 30] {
    let mut pb = [0x5a; 30];
    pb[24..26].copy_from_slice(&(-4i16).to_be_bytes());
    pb[26..28].copy_from_slice(&2i16.to_be_bytes());
    pb[28..30].copy_from_slice(&level.to_be_bytes());
    pb
}
#[test]
fn classic_control_updates_only_volume_bits_and_io_result() {
    for opcode in [0xa004, 0xa204] {
        let mut app =
            LoadedApplication::code(&code_program(&[0x207c, 0x003f, 0xffc0, opcode, 0xa9f4]))
                .unwrap();
        let pointer = GuestPtr(A5_BASE - 64);
        let mut expected = parameter(3);
        app.runtime.memory.write(pointer, &expected).unwrap();
        app.runtime.memory.write(GuestPtr(0x260), &[0xa8]).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x220), &[0xab, 0xcd])
            .unwrap();
        assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
        expected[16..18].fill(0);
        assert_eq!(app.runtime.memory.read(pointer, 30).unwrap(), expected);
        assert_eq!(app.runtime.memory.read(GuestPtr(0x260), 1).unwrap(), [0xab]);
        assert_eq!(app.runtime.memory.u16(GuestPtr(0x220)).unwrap(), 0xabcd);
        let Cpu::M68k(c) = app.runtime.cpu else {
            panic!()
        };
        assert_eq!(c.dar[0], 0);
        assert_eq!(c.dar[8], pointer.0);
    }
}
#[test]
fn ppc_volume_imports_and_control_share_canonical_guest_state() {
    let mut instructions = vec![0x7fc802a6, 0x7c5f1378, 0x38600003];
    fn call(code: &mut Vec<u32>, slot: u32) {
        code.extend([
            0x819f0000 | slot,
            0x800c0000,
            0x804c0004,
            0x7c0903a6,
            0x4e800421,
        ]);
    }
    call(&mut instructions, 8); // SetSoundVol(3)
    instructions.push(0x387f0020);
    call(&mut instructions, 12); // GetSoundVol(data+32)
    instructions.push(0x387f0030);
    call(&mut instructions, 16); // PBControlSync(data+48), level 6
    instructions.push(0x387f0022);
    call(&mut instructions, 12); // GetSoundVol(data+34)
    instructions.extend([0xa07f0022, 0x7fc803a6, 0x4e800020]);
    let f = pef_program(
        &instructions,
        &[b"SetSoundVol", b"GetSoundVol", b"PBControlSync"],
        128,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.sections[1].0;
    app.runtime
        .memory
        .write(GuestPtr(data + 48), &parameter(6))
        .unwrap();
    app.runtime.memory.write(GuestPtr(0x260), &[0xa8]).unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Returned(6));
    assert_eq!(app.runtime.memory.u16(GuestPtr(data + 32)).unwrap(), 3);
    assert_eq!(app.runtime.memory.u16(GuestPtr(data + 34)).unwrap(), 6);
    assert_eq!(app.runtime.memory.u16(GuestPtr(data + 64)).unwrap(), 0);
    assert_eq!(app.runtime.memory.read(GuestPtr(0x260), 1).unwrap(), [0xae]);
    // Direct writes remain authoritative for the API getter.
    app.runtime.memory.write(GuestPtr(0x260), &[0xd5]).unwrap();
    sound::get_volume(&mut app.runtime.memory, GuestPtr(data + 34)).unwrap();
    assert_eq!(app.runtime.memory.u16(GuestPtr(data + 34)).unwrap(), 5);
}
#[test]
fn unsupported_or_invalid_controls_do_not_mutate_volume_or_parameters() {
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0), vec![0; 4096], Permissions::RW)
        .unwrap();
    memory.write(GuestPtr(0x260), &[0xb5]).unwrap();
    for (offset, value) in [(24, -5i16), (26, 3), (28, 8), (28, -1)] {
        let mut pb = parameter(3);
        pb[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        memory.write(GuestPtr(0x800), &pb).unwrap();
        assert!(sound::control(&mut memory, GuestPtr(0x800)).is_err());
        assert_eq!(memory.read(GuestPtr(0x800), 30).unwrap(), pb);
        assert_eq!(memory.read(GuestPtr(0x260), 1).unwrap(), [0xb5]);
    }
    memory
        .map(GuestPtr(0x2000), parameter(3).to_vec(), Permissions::RX)
        .unwrap();
    assert!(sound::control(&mut memory, GuestPtr(0x2000)).is_err());
    assert_eq!(memory.read(GuestPtr(0x260), 1).unwrap(), [0xb5]);
    assert!(sound::control(&mut memory, GuestPtr(4090)).is_err());
    assert!(sound::get_volume(&mut memory, GuestPtr(4095)).is_err());
}
