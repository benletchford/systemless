mod support;
use support::*;
use systemless_future::{
    cpu::Cpu,
    load::{LoadedApplication, A5_BASE, SECTION_BASE, STACK_BASE, STACK_SIZE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    traps,
};

#[test]
fn installed_toolbox_patch_chains_to_saved_builtin_and_can_be_restored() {
    let mut code = vec![
        0x303c, 0xa975, 0xa746, 0x2b48, 0xfffc, // save TickCount veneer
        0x207c, 0, 0, 0x303c, 0xa975, 0xa647, // install patch
        0xa746, 0x2b48, 0xfff8, // lookup observes replacement
        0x598f, 0xa975, 0x201f, 0x2b40, 0xfff4, // call patched TickCount
        0x206d, 0xfffc, 0x303c, 0xa975, 0xa647, // restore original
        0x598f, 0xa975, 0x201f, 0x2b40, 0xfff0, // call original
        0xa9f4,
    ];
    let patch = SECTION_BASE + code.len() as u32 * 2;
    code[6] = (patch >> 16) as u16;
    code[7] = patch as u16;
    code.extend([0x52ad, 0xffec, 0x206d, 0xfffc, 0x4ed0]); // count, JMP saved
    let mut app = LoadedApplication::code(&code_program(&code)).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &42u32.to_be_bytes())
        .unwrap();
    // A patch is normal budgeted guest code, not a synchronous host callback.
    for _ in 0..100 {
        if app.runtime.advance(1).unwrap() == Stop::Exit(0) {
            break;
        }
    }
    assert_eq!(app.runtime.advance(1).unwrap(), Stop::Exit(0));
    let memory = &app.runtime.memory;
    for (offset, value) in [(8, patch), (12, 42), (16, 42), (20, 1)] {
        assert_eq!(memory.u32(GuestPtr(A5_BASE - offset)).unwrap(), value);
    }
    assert_eq!(
        traps::lookup(memory, 0xad75, true).unwrap(),
        traps::address(0xa975, true)
    );
    let Cpu::M68k(cpu) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(cpu.dar[15], STACK_BASE + STACK_SIZE - 0x100);
}

#[test]
fn auto_pop_glue_returns_to_its_caller_for_builtin_and_patched_traps() {
    // Reserve TickCount result, JSR glue, consume result, exit; glue must not
    // execute the illegal instruction after its auto-pop trap.
    let code = [
        0x598f, 0x4eb9, 0x0100, 0x000e, 0x201f, 0xa9f4, 0x4afc, 0xad75, 0x4afc,
    ];
    for patched in [false, true] {
        let mut app = LoadedApplication::code(&code_program(&code)).unwrap();
        app.runtime
            .memory
            .write(GuestPtr(0x16a), &27u32.to_be_bytes())
            .unwrap();
        if patched {
            // A saved native veneer is itself a valid chain destination.
            let patch = SECTION_BASE + 0x1000;
            let old = traps::lookup(&app.runtime.memory, 0xa975, true).unwrap();
            let bytes = words(&[0x4ef9, (old >> 16) as u16, old as u16]);
            app.runtime
                .memory
                .map(GuestPtr(patch), bytes, Permissions::RX)
                .unwrap();
            traps::set_toolbox(&mut app.runtime.memory, 0x175, patch).unwrap();
        }
        assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
        let Cpu::M68k(cpu) = &app.runtime.cpu else {
            panic!()
        };
        assert_eq!(cpu.dar[0], 27);
        assert_eq!(cpu.dar[15], STACK_BASE + STACK_SIZE - 0x100);
    }
}

#[test]
fn ppc_setter_and_getter_share_the_table_used_by_68k_dispatch() {
    let f = pef_program(
        &[
            0x7fc802a6, 0x7c5f1378, // save LR and TOC
            0x3c600123, 0x60634566, 0x3880a975, // pointer and trap number
            0x819f0008, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421,
            0x38600175, // getter's number
            0x819f000c, 0x800c0000, 0x804c0004, 0x7c0903a6, 0x4e800421, 0x7fc803a6, 0x4e800020,
        ],
        &[b"SetToolboxTrapAddress", b"GetToolboxTrapAddress"],
        32,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    assert_eq!(
        app.runtime.advance(100).unwrap(),
        Stop::Returned(0x01234566)
    );
    assert_eq!(
        traps::lookup(&app.runtime.memory, 0xad75, true).unwrap(),
        0x01234566
    );
    // A pointer installed by PPC dispatches actual 68K code in the same memory.
    app.runtime
        .memory
        .map(
            GuestPtr(0x01234566),
            words(&[0x2f7c, 0, 55, 4, 0x4e75]),
            Permissions::RX,
        )
        .unwrap();
    let start = 0x02000000;
    app.runtime
        .memory
        .map(GuestPtr(start), words(&[0xa975, 0x4e71]), Permissions::RX)
        .unwrap();
    let sp = STACK_BASE + 0x100;
    let mut cpu = Cpu::new(systemless_future::cpu::Architecture::M68k, start, sp);
    for _ in 0..3 {
        cpu.step_with_abi(&mut app.runtime.memory, &app.runtime.abi)
            .unwrap();
    }
    assert_eq!(cpu.pc(), start + 2);
    assert_eq!(app.runtime.memory.u32(GuestPtr(sp)).unwrap(), 55);
    // An unmapped pointer can be installed but faults before altering the stack.
    traps::set_toolbox(&mut app.runtime.memory, 0xa975, 0x03000000).unwrap();
    let mut cpu = Cpu::new(systemless_future::cpu::Architecture::M68k, start, sp);
    assert!(cpu
        .step_with_abi(&mut app.runtime.memory, &app.runtime.abi)
        .is_err());
    let Cpu::M68k(cpu) = cpu else { panic!() };
    assert_eq!(cpu.dar[15], sp);
}

#[test]
fn patch_can_call_another_pascal_service_before_chaining_to_original() {
    let mut app =
        LoadedApplication::code(&code_program(&[0x598f, 0xa975, 0x201f, 0xa9f4])).unwrap();
    let old = traps::lookup(&app.runtime.memory, 0xa975, true).unwrap();
    let patch = SECTION_BASE + 0x1000;
    app.runtime
        .memory
        .map(
            GuestPtr(patch),
            words(&[
                0x598f,
                0x2f3c,
                0x4441,
                0x5441,
                0x3f3c,
                0xfff9,
                0xa9a0,
                0x2b5f,
                0xffe0, // store nested GetResource result, leaving outer return PC
                0x4ef9,
                (old >> 16) as u16,
                old as u16,
            ]),
            Permissions::RX,
        )
        .unwrap();
    traps::set_toolbox(&mut app.runtime.memory, 0xa975, patch).unwrap();
    app.runtime
        .memory
        .write(GuestPtr(0x16a), &42u32.to_be_bytes())
        .unwrap();
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    let handle = app.runtime.memory.u32(GuestPtr(A5_BASE - 32)).unwrap();
    let data = app.runtime.memory.u32(GuestPtr(handle)).unwrap();
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data), 3).unwrap(),
        [1, 2, 3]
    );
    let Cpu::M68k(cpu) = &app.runtime.cpu else {
        panic!()
    };
    assert_eq!(cpu.dar[0], 42);
    assert_eq!(cpu.dar[15], STACK_BASE + STACK_SIZE - 0x100);
}
