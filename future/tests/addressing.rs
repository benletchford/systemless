mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    addressing::{self, MMU32_BIT},
    cpu::{Architecture, Call, Cpu},
    environment,
    load::{LoadedApplication, STACK_BASE},
    memory::{Access, GuestPtr, Memory, Permissions},
    runtime::Stop,
    Error,
};
fn setup() -> LoadedApplication {
    let fixture = support::pef_program(
        &[0x4e800020],
        &[b"GetMMUMode", b"SwapMMUMode", b"StripAddress"],
        128,
    );
    let mut app = LoadedApplication::pef(&fixture.bytes, None).unwrap();
    app.runtime
        .memory
        .map(
            GuestPtr(0x50000000),
            vec![0xa0, 0x5d, 0x10, 0x38, 0x0c, 0xb2],
            Permissions::RX,
        )
        .unwrap();
    app
}
fn invoke_ppc(
    app: &mut LoadedApplication,
    index: u32,
    arg: u32,
) -> systemless_future::Result<Stop> {
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(app.info.toc_or_a5 + 8 + index * 4))?;
    let gate = app.runtime.memory.u32(GuestPtr(vector))?;
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, gate, STACK_BASE + 0x8000);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.gpr[3] = arg;
        c.gpr[4] = 0xa55a5aa5;
    }
    app.runtime.advance(1)
}
#[test]
fn boot_mode_matches_real_high_address_access_and_callable_os_trap_veneer() {
    // GetOSTrapAddress(A05D), JSR its guest veneer, then read above 24-bit space.
    let code = support::code_program(&[
        0x303c, 0xa05d, 0xa346, 0x203c, 0xaabb, 0xcc01, 0x4e90, 0x13c0, 0x5300, 0x0000, 0x207c,
        0xab12, 0x0000, 0x2010, 0xa9f4,
    ]);
    let mut app = LoadedApplication::code(&code).unwrap();
    assert_eq!(app.runtime.memory.read(MMU32_BIT, 1).unwrap(), [1]);
    app.runtime
        .memory
        .map(
            GuestPtr(0xab120000),
            0xdeadbeefu32.to_be_bytes().to_vec(),
            Permissions::RX,
        )
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x53000000), vec![0xa5], Permissions::RW)
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert!(app.runtime.services.calls.contains(&Call::SwapMmuMode(1)));
    assert_eq!(
        app.runtime.memory.read(GuestPtr(0x53000000), 1).unwrap(),
        [1]
    );
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0xdeadbeef);
    assert_eq!(c.dar[8], 0xab120000);
    assert_eq!(addressing::current(&app.runtime.memory).unwrap(), 1);
    // Started in 32-bit mode and supports it; no system-zone claim yet.
    assert_eq!(
        environment::gestalt(u32::from_be_bytes(*b"addr")).unwrap(),
        5
    );
}
#[test]
fn ppc_var_byte_and_68k_register_inline_queries_share_the_same_mode() {
    let mut app = setup();
    let pointer = app.info.toc_or_a5 + 65;
    app.runtime
        .memory
        .write(GuestPtr(pointer - 1), &[0xa5, 1, 0x5a])
        .unwrap();
    assert_eq!(invoke_ppc(&mut app, 0, 0).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 1);
    assert_eq!(invoke_ppc(&mut app, 1, pointer).unwrap(), Stop::Budget);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(pointer - 1), 3).unwrap(),
        [0xa5, 1, 0x5a]
    );
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], pointer);
    assert_eq!(c.gpr[4], 0xa55a5aa5);
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK_BASE + 0x8000);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[0] = 0xaabbcc01;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0xaabbcc01);
    assert_eq!(c.dar[15], STACK_BASE + 0x8000);
    // GetMMUMode is an inline low-memory byte load on 68K, not a made-up trap.
    app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000002, STACK_BASE + 0x8000);
    if let Cpu::M68k(c) = &mut app.runtime.cpu {
        c.dar[0] = 0x112233ff;
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
    let Cpu::M68k(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[0], 0x11223301);
    assert_eq!(invoke_ppc(&mut app, 2, 0xfedcba98).unwrap(), Stop::Budget);
    let Cpu::PowerPc(c) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.gpr[3], 0xfedcba98);
}
#[test]
fn unsupported_modes_never_change_cpu_results_outparameters_or_mode_flag() {
    for requested in [0u8, 2, 255] {
        for architecture in [Architecture::PowerPc, Architecture::M68k] {
            // Runtime faults are terminal; each failure needs a fresh machine.
            let mut app = setup();
            let pointer = app.info.toc_or_a5 + 64;
            app.runtime
                .memory
                .write(GuestPtr(pointer), &[requested, 0xa5])
                .unwrap();
            let result = if architecture == Architecture::PowerPc {
                invoke_ppc(&mut app, 1, pointer)
            } else {
                app.runtime.cpu = Cpu::new(Architecture::M68k, 0x50000000, STACK_BASE + 0x8000);
                if let Cpu::M68k(c) = &mut app.runtime.cpu {
                    c.dar[0] = 0xaabbcc00 | u32::from(requested);
                }
                app.runtime.advance(1)
            };
            assert!(
                matches!(result,Err(Error::AddressMode(value)) if value==requested),
                "{result:?}"
            );
            assert_eq!(
                app.runtime.memory.read(GuestPtr(pointer), 2).unwrap(),
                [requested, 0xa5]
            );
            assert_eq!(app.runtime.memory.read(MMU32_BIT, 1).unwrap(), [1]);
            if let Cpu::M68k(c) = &app.runtime.cpu {
                assert_eq!(c.dar[0], 0xaabbcc00 | u32::from(requested));
                assert_eq!(c.dar[15], STACK_BASE + 0x8000);
            }
        }
    }
}
#[test]
fn var_byte_permissions_and_global_inconsistency_fail_without_writes() {
    for (permissions, access) in [
        (Permissions::RX, Access::Write),
        (
            Permissions {
                read: false,
                write: true,
                execute: false,
            },
            Access::Read,
        ),
    ] {
        let mut app = setup();
        app.runtime
            .memory
            .map(GuestPtr(0x53000000), vec![1], permissions)
            .unwrap();
        let result = invoke_ppc(&mut app, 1, 0x53000000);
        assert!(
            matches!(result,Err(Error::Memory {access:actual,..}) if actual==access),
            "{result:?}"
        );
        assert_eq!(app.runtime.memory.read(MMU32_BIT, 1).unwrap(), [1]);
        if permissions.read {
            assert_eq!(
                app.runtime.memory.read(GuestPtr(0x53000000), 1).unwrap(),
                [1]
            );
        }
    }
    for index in [0, 1] {
        let mut app = setup();
        app.runtime
            .memory
            .map(GuestPtr(0x53000000), vec![1], Permissions::RW)
            .unwrap();
        assert_eq!(invoke_ppc(&mut app, 1, 0x53000000).unwrap(), Stop::Budget);
        app.runtime.memory.write(MMU32_BIT, &[0]).unwrap();
        assert!(matches!(
            invoke_ppc(&mut app, index, 0x53000000),
            Err(Error::AddressMode(0))
        ));
        assert_eq!(
            app.runtime.memory.read(GuestPtr(0x53000000), 1).unwrap(),
            [1]
        );
        assert_eq!(app.runtime.memory.read(MMU32_BIT, 1).unwrap(), [0]);
    }
}
#[test]
fn installed_swap_and_oval_entries_are_real_readable_veneers() {
    use systemless_future::traps;
    let app = setup();
    for opcode in [0xa05du16, 0xa8b7, 0xa8b8, 0xa8b9, 0xa8ba, 0xa8bb] {
        let entry = traps::lookup(&app.runtime.memory, opcode, opcode & 0x800 != 0).unwrap();
        assert_ne!(entry, traps::UNIMPLEMENTED);
        assert_eq!(traps::opcode(entry), Some(opcode));
        assert_eq!(app.runtime.memory.u16(GuestPtr(entry)).unwrap(), opcode);
        assert_eq!(app.runtime.memory.u16(GuestPtr(entry + 2)).unwrap(), 0x4e75);
    }
}
#[test]
fn swap_abis_use_low_register_byte_or_one_writable_byte_reference() {
    let mut memory = Memory::default();
    memory
        .map(GuestPtr(0x1000), vec![1, 0xa5], Permissions::RW)
        .unwrap();
    let mut ppc = Cpu::new(Architecture::PowerPc, 0, 0x9000);
    let Cpu::PowerPc(c) = &mut ppc else { panic!() };
    c.gpr[3] = 0x1000;
    assert_eq!(
        Abi::import(Service::SwapMmuMode, c, &memory).unwrap(),
        (Call::SwapMmuMode(1), Reply::StackByte(GuestPtr(0x1000)))
    );
    let mut m68 = Cpu::new(Architecture::M68k, 0, 0x9000);
    let Cpu::M68k(c) = &mut m68 else { panic!() };
    c.dar[0] = 0xfedcba01;
    assert_eq!(
        Abi::trap(0xa05d, c, &memory).unwrap(),
        (Call::SwapMmuMode(1), Reply::M68kByte)
    );
    assert_eq!(c.dar[15], 0x9000);
}
