mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn pattern_lookup_uses_one_resource_identity_across_both_cpu_adapters() {
    let fork = resource(vec![(
        *b"PAT ",
        -123,
        None,
        vec![0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
    )]);
    let f = pef_program(&[0x4e800020], &[b"GetPattern"], 32);
    let mut app = LoadedApplication::pef(&f.bytes, Some(&fork)).unwrap();
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8))
        .unwrap();
    let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0xb8], Permissions::RX)
        .unwrap();
    let stack = STACK_BASE + 0x8000;
    let mut handle = 0;
    for (i, id) in [-123i16, -123, -124, -124, -123].into_iter().enumerate() {
        let arch = if i % 2 == 0 {
            Architecture::PowerPc
        } else {
            Architecture::M68k
        };
        let pc = if i % 2 == 0 { gate } else { 0x50000000 };
        app.runtime.cpu = Cpu::new(arch, pc, stack);
        let mut frame = id.to_be_bytes().to_vec();
        frame.extend([0x5a; 6]);
        app.runtime.memory.write(GuestPtr(stack), &frame).unwrap();
        if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
            c.gpr[3] = id as i32 as u32;
        }
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        let value = match &app.runtime.cpu {
            Cpu::PowerPc(c) => c.gpr[3],
            Cpu::M68k(c) => {
                assert_eq!(c.dar[15], stack + 2);
                assert_eq!(app.runtime.memory.u16(GuestPtr(stack + 6)).unwrap(), 0x5a5a);
                app.runtime.memory.u32(GuestPtr(stack + 2)).unwrap()
            }
        };
        if i == 0 {
            assert_ne!(value, 0);
            handle = value;
            let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
            app.runtime.memory.write(GuestPtr(data), &[0x42]).unwrap();
        }
        if id == -123 {
            assert_eq!(value, handle);
            assert_eq!(app.runtime.process.res_error, 0);
            let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
            assert_eq!(app.runtime.memory.read(GuestPtr(data), 1).unwrap(), [0x42]);
            assert_eq!(
                app.runtime
                    .process
                    .get_resource(&mut app.runtime.memory, *b"PAT ", -123)
                    .unwrap(),
                handle
            );
        } else {
            assert_eq!(value, 0);
            assert_eq!(app.runtime.process.res_error, -192);
        }
    }
}
