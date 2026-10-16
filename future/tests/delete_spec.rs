mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    files::CreateSpecResource,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
#[test]
fn both_adapters_delete_the_same_catalog_object_and_preserve_inputs() {
    for architecture in [Architecture::M68k, Architecture::PowerPc] {
        let fixture = support::pef_program(&[0x4e800020], &[b"FSpDelete"], 16);
        let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
        app.runtime
            .memory
            .map(GuestPtr(0x50000000), vec![0xaa, 0x52], Permissions::RX)
            .unwrap();
        let vector = app
            .runtime
            .memory
            .u32(GuestPtr(app.info.toc_or_a5 + 8))
            .unwrap();
        let gate = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
        let spec = GuestPtr(STACK_BASE + 0x6000);
        let stack = STACK_BASE + 0x7000;
        let mut bytes = [0; 70];
        bytes[..2].copy_from_slice(&(-1i16).to_be_bytes());
        bytes[2..6].copy_from_slice(&2u32.to_be_bytes());
        bytes[6] = 4;
        bytes[7..11].copy_from_slice(b"Test");
        app.runtime.memory.write(spec, &bytes).unwrap();
        assert_eq!(
            app.runtime
                .process
                .files
                .create_spec_resource(
                    &app.runtime.memory,
                    CreateSpecResource {
                        spec,
                        creator: *b"TEST",
                        kind: *b"pref",
                        script: 0,
                    }
                )
                .unwrap(),
            0
        );
        for error in [0i16, -43] {
            app.runtime
                .memory
                .write(
                    GuestPtr(stack),
                    &[spec.0.to_be_bytes().as_slice(), &[0x5a; 6]].concat(),
                )
                .unwrap();
            app.runtime.cpu = Cpu::new(
                architecture,
                if architecture == Architecture::M68k {
                    0x50000000
                } else {
                    gate
                },
                stack,
            );
            match &mut app.runtime.cpu {
                Cpu::M68k(c) => {
                    c.dar[0] = 0x12340006;
                    c.dar[1] = 0xaabbccdd;
                }
                Cpu::PowerPc(c) => {
                    c.gpr[3] = spec.0;
                    c.gpr[4] = 0xaabbccdd;
                }
            }
            assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
            match &app.runtime.cpu {
                Cpu::M68k(c) => {
                    assert_eq!(
                        (c.dar[0], c.dar[1], c.dar[15]),
                        (0x12340006, 0xaabbccdd, stack + 4)
                    );
                    assert_eq!(
                        app.runtime.memory.u16(GuestPtr(stack + 4)).unwrap() as i16,
                        error
                    );
                    assert_eq!(
                        app.runtime.memory.read(GuestPtr(stack + 6), 4).unwrap(),
                        [0x5a; 4]
                    );
                }
                Cpu::PowerPc(c) => {
                    assert_eq!((c.gpr[3] as i32, c.gpr[4]), (i32::from(error), 0xaabbccdd))
                }
            }
            assert_eq!(app.runtime.memory.read(spec, 70).unwrap(), bytes);
        }
    }
}
