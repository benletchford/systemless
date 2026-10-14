mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};
const SOURCE: u32 = 0x5800_1000;
const CHILD: u32 = 0x5800_2000;
const SHARED: u32 = 0x5800_0000;
const RETURN: u32 = 0x5000_0000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = pef_program(
        &[0x60000000],
        &[
            b"GetMemFragment",
            b"CloseConnection",
            b"CountSymbols",
            b"FindSymbol",
            b"ExitToShell",
        ],
        512,
    );
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(SHARED), vec![0; 16], Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(RETURN), vec![0x60, 0, 0, 0], Permissions::RX)
        .unwrap();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .write(GuestPtr(data + 96), b"\x04plug")
        .unwrap();
    app
}
fn library(error: i16) -> Pef {
    let code = [
        0x3860004d,
        0x4e800020, // main returns 77
        0x80830008,
        0x90820020,
        0x90620024,
        0x3ca05800,
        0x38c0000b,
        0x90c50000,
        0x38600000 | error as u16 as u32,
        0x4e800020, // init
        0x3ca05800,
        0x80c50000,
        0x38c60001,
        0x90c50000,
        0x4e800020,
    ];
    let mut f = pef_program(&code, &[b"TickCount"], 128);
    w16(&mut f.bytes, f.reloc, 0x4602);
    w32(&mut f.bytes, f.data + 8, 8);
    w32(&mut f.bytes, f.data + 16, 40);
    w32(&mut f.bytes, f.loader + 8, 1);
    w32(&mut f.bytes, f.loader + 12, 8);
    w32(&mut f.bytes, f.loader + 16, 1);
    w32(&mut f.bytes, f.loader + 20, 16);
    export_main(&mut f);
    f
}
fn start(app: &mut LoadedApplication, service: u32, args: &[u32]) {
    let data = app.info.toc_or_a5;
    let vector = app
        .runtime
        .memory
        .u32(GuestPtr(data + 8 + service * 4))
        .unwrap();
    let entry = app.runtime.memory.u32(GuestPtr(vector)).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::PowerPc, entry, STACK);
    if let Cpu::PowerPc(c) = &mut app.runtime.cpu {
        c.lr = RETURN;
        c.gpr[2] = data;
        c.gpr[30] = 0x12345678;
        c.fpr[14] = 0x7ff8123456789abc;
        c.cr = 0x13572468;
        for (i, arg) in args.iter().enumerate() {
            c.gpr[3 + i] = *arg;
        }
    }
}
fn finish_call(app: &mut LoadedApplication) -> i16 {
    for _ in 0..500 {
        assert_eq!(app.runtime.advance(1).unwrap(), Stop::Budget);
        if app.runtime.cpu.pc() == RETURN {
            return app.runtime.cpu.result() as i16;
        }
    }
    panic!("CFM call did not finish");
}
fn call(app: &mut LoadedApplication, service: u32, args: &[u32]) -> i16 {
    start(app, service, args);
    finish_call(app)
}
fn load(app: &mut LoadedApplication, f: &Pef, flags: u32) -> i16 {
    let data = app.info.toc_or_a5;
    call(
        app,
        0,
        &[
            SOURCE,
            f.bytes.len() as u32,
            data + 96,
            flags,
            data + 64,
            data + 68,
            data + 128,
        ],
    )
}
fn word(app: &LoadedApplication, address: u32) -> u32 {
    app.runtime.memory.u32(GuestPtr(address)).unwrap()
}
#[test]
fn dynamic_initializer_runs_in_shared_world_and_preserves_caller_before_close() {
    let mut app = app();
    let f = library(0);
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(STACK), &[0x55; 64])
        .unwrap();
    assert_eq!(load(&mut app, &f, 5), 0);
    let id = word(&app, data + 64);
    let main = word(&app, data + 68);
    let code = word(&app, main);
    let toc = word(&app, main + 4);
    assert_ne!(main, code); // GetMemFragment returns the symbol, not its code entry.
    assert_eq!(word(&app, toc + 32), id);
    let block = word(&app, toc + 36);
    assert_eq!(word(&app, block + 16), SOURCE);
    assert_eq!(word(&app, block + 20), f.bytes.len() as u32);
    let name = word(&app, block + 28);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(name), 5).unwrap(),
        b"\x04plug"
    );
    assert_eq!(word(&app, SHARED), 11);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(STACK), 64).unwrap(),
        &[0x55; 64]
    );
    if let Cpu::PowerPc(c) = &app.runtime.cpu {
        assert_eq!(
            (c.gpr[1], c.gpr[2], c.gpr[30], c.fpr[14], c.cr),
            (STACK, data, 0x12345678, 0x7ff8123456789abc, 0x13572468)
        );
    }
    assert_eq!(call(&mut app, 2, &[id, data + 72]), 0);
    assert_eq!(word(&app, data + 72), 1);
    app.runtime
        .memory
        .write(GuestPtr(data + 100), b"\x04main")
        .unwrap();
    assert_eq!(
        call(&mut app, 3, &[id, data + 100, data + 76, data + 80]),
        0
    );
    assert_eq!(word(&app, data + 76), main);
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    assert_eq!(word(&app, data + 64), 0);
    assert_eq!(word(&app, SHARED), 12);
    assert!(app.runtime.memory.read(GuestPtr(code), 1).is_err());
    assert!(!app.runtime.abi.imports.contains_key(&0x75000000));
    assert_eq!(call(&mut app, 2, &[id, data + 72]), -2801);
}
#[test]
fn initializer_failure_cleans_mappings_and_identity_without_publishing_outputs() {
    let mut app = app();
    let f = library(-50);
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .write(GuestPtr(data + 64), &[0x55; 8])
        .unwrap();
    assert_eq!(load(&mut app, &f, 5), -2821);
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data + 64), 8).unwrap(),
        &[0x55; 8]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data + 128), 5).unwrap(),
        b"\x04plug"
    );
    assert!(app.runtime.memory.read(GuestPtr(0x80000000), 1).is_err());
    assert!(!app.runtime.abi.imports.contains_key(&0x75000000));
    assert_eq!(call(&mut app, 2, &[2, data + 72]), -2801);
    assert_eq!(word(&app, SHARED), 11); // No terminator runs after failed initialization.
    let good = library(0);
    app.runtime
        .memory
        .write(GuestPtr(SOURCE), &good.bytes)
        .unwrap();
    assert_eq!(load(&mut app, &good, 5), 0);
    assert!(word(&app, data + 64) > 2); // Stale IDs cannot identify a later load.
    assert_eq!(word(&app, word(&app, data + 68)), 0x80000000); // Reservation reused.
}
#[test]
fn reference_find_and_private_copy_have_distinct_lifetimes() {
    let mut app = app();
    let f = library(0);
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    assert_eq!(load(&mut app, &f, 2), -2804);
    assert_eq!(load(&mut app, &f, 1), 0);
    let first = word(&app, data + 64);
    let main = word(&app, data + 68);
    let toc = word(&app, main + 4);
    app.runtime
        .memory
        .write(GuestPtr(toc + 40), &99u32.to_be_bytes())
        .unwrap();
    assert_eq!(load(&mut app, &f, 1), 0);
    assert_eq!(word(&app, data + 64), first);
    assert_eq!(load(&mut app, &f, 2), 0);
    assert_eq!(word(&app, data + 64), first);
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    assert_eq!(word(&app, toc + 40), 99); // One reference remains; find added none.
    assert_eq!(load(&mut app, &f, 5), 0);
    let second = word(&app, data + 64);
    let other_main = word(&app, data + 68);
    assert_ne!(first, second);
    assert_ne!(main, other_main);
    assert_eq!(word(&app, word(&app, other_main + 4) + 40), 0);
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    app.runtime
        .memory
        .write(GuestPtr(data + 64), &first.to_be_bytes())
        .unwrap();
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    assert!(app.runtime.memory.read(GuestPtr(main), 1).is_err());
}
#[test]
fn corrupt_or_unresolved_fragment_never_changes_the_running_image() {
    for unresolved in [false, true] {
        let mut app = app();
        let mut f = library(0);
        let data = app.info.toc_or_a5;
        if unresolved {
            f.bytes[f.strings + 13] = b'X';
        } else {
            f.bytes[0] = 0;
        }
        app.runtime
            .memory
            .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
            .unwrap();
        let imports = app.runtime.abi.imports.len();
        assert_eq!(
            load(&mut app, &f, 5),
            if unresolved { -2807 } else { -2820 }
        );
        assert_eq!(app.runtime.abi.imports.len(), imports);
        assert_eq!(word(&app, data + 64), 0);
        assert!(app.runtime.memory.read(GuestPtr(0x80000000), 1).is_err());
        let expected = if unresolved {
            b"\x0cInterfaceLib".as_slice()
        } else {
            b"\x04plug"
        };
        assert_eq!(
            app.runtime
                .memory
                .read(GuestPtr(data + 128), expected.len())
                .unwrap(),
            expected
        );
    }
}
#[test]
fn looping_initializer_is_budgeted_and_does_not_publish_success() {
    let mut app = app();
    let mut f = library(0);
    let data = app.info.toc_or_a5;
    let code_offset = u32::from_be_bytes(f.bytes[60..64].try_into().unwrap()) as usize;
    w32(&mut f.bytes, code_offset + 8, 0x48000000);
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    start(
        &mut app,
        0,
        &[
            SOURCE,
            f.bytes.len() as u32,
            data + 96,
            5,
            data + 64,
            data + 68,
            0,
        ],
    );
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Budget);
    assert_eq!(word(&app, data + 64), 0);
    assert_eq!(app.runtime.cpu.pc(), 0x80000008);
}
#[test]
fn nested_loads_resume_parent_and_exit_runs_all_terminators() {
    let child = library(0);
    let mut code = vec![
        0x38600007,
        0x4e800020,
        0x7fc802a6,
        0x7c5f1378,
        0x3c605800,
        0x60632000,
        0x38800000 | child.bytes.len() as u32,
        0x38a00000,
        0x38c00005,
        0x38ff0040,
        0x391f0044,
        0x39200000,
        0x819f0018,
        0x800c0000,
        0x804c0004,
        0x7c0903a6,
        0x4e800421,
        0x7fe2fb78,
        0x7fc803a6,
        0x4e800020,
    ];
    let term = code.len() as u32 * 4;
    code.extend([
        0x3ca05800, 0x80c50000, 0x1cc6000a, 0x38c60002, 0x90c50000, 0x4e800020,
    ]);
    let mut parent = pef_program(&code, &[b"GetMemFragment"], 128);
    w16(&mut parent.bytes, parent.reloc, 0x4602);
    w32(&mut parent.bytes, parent.data + 8, 8);
    w32(&mut parent.bytes, parent.data + 16, term);
    w32(&mut parent.bytes, parent.loader + 8, 1);
    w32(&mut parent.bytes, parent.loader + 12, 8);
    w32(&mut parent.bytes, parent.loader + 16, 1);
    w32(&mut parent.bytes, parent.loader + 20, 16);
    let mut app = app();
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), parent.bytes.clone(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(CHILD), child.bytes.clone(), Permissions::RW)
        .unwrap();
    assert_eq!(load(&mut app, &parent, 5), 0);
    let parent_main = word(&app, data + 68);
    let parent_toc = word(&app, parent_main + 4);
    let child_id = word(&app, parent_toc + 64);
    assert_eq!(call(&mut app, 2, &[child_id, data + 72]), 0);
    assert_eq!(word(&app, data + 72), 1);
    start(&mut app, 4, &[]);
    let mut stop = Stop::Budget;
    for _ in 0..100 {
        stop = app.runtime.advance(1).unwrap();
        if stop != Stop::Budget {
            break;
        }
    }
    assert_eq!(stop, Stop::Exit(0));
    assert_eq!(word(&app, SHARED), 122); // Child termination before parent.
    for base in [0x80000000, 0x88000000] {
        assert!(app.runtime.memory.read(GuestPtr(base), 1).is_err());
    }
    let before = app.runtime.instructions;
    assert_eq!(app.runtime.advance(100).unwrap(), Stop::Exit(0));
    assert_eq!(app.runtime.instructions, before);
}

#[test]
fn data_main_and_absent_main_are_valid_for_dynamic_fragments() {
    for absent in [false, true] {
        let mut app = app();
        let data = app.info.toc_or_a5;
        let mut f = pef_program(&[0x4e800020], &[], 24);
        if absent {
            w32(&mut f.bytes, f.loader, u32::MAX);
        } else {
            w32(&mut f.bytes, f.loader + 4, 16);
            w32(&mut f.bytes, f.data + 16, 0x12345678);
        }
        app.runtime
            .memory
            .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
            .unwrap();
        assert_eq!(load(&mut app, &f, 5), 0);
        let main = word(&app, data + 68);
        if absent {
            assert_eq!(main, 0);
        } else {
            assert_eq!(word(&app, main), 0x12345678);
        }
        assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    }
}
#[test]
fn bad_load_outputs_fault_before_adopting_any_fragment() {
    let mut app = app();
    let f = library(0);
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    let readonly = app.info.entry;
    start(
        &mut app,
        0,
        &[SOURCE, f.bytes.len() as u32, 0, 5, data + 64, readonly, 0],
    );
    assert!(app.runtime.advance(1).is_err());
    assert_eq!(word(&app, data + 64), 0);
    assert!(app.runtime.memory.read(GuestPtr(0x80000000), 1).is_err());
    assert_eq!(word(&app, SHARED), 0);
}
#[test]
fn fragment_address_exhaustion_is_bounded_and_close_reuses_space_with_a_new_id() {
    let mut app = app();
    let data = app.info.toc_or_a5;
    let f = pef_program(&[0x4e800020], &[], 24);
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    let mut first = 0;
    for i in 0..14 {
        assert_eq!(load(&mut app, &f, 5), 0);
        if i == 0 {
            first = word(&app, data + 64);
        }
    }
    assert_eq!(load(&mut app, &f, 5), -2810);
    app.runtime
        .memory
        .write(GuestPtr(data + 64), &first.to_be_bytes())
        .unwrap();
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    assert_eq!(load(&mut app, &f, 5), 0);
    assert!(word(&app, data + 64) > first);
    assert_eq!(word(&app, word(&app, data + 68)), 0x80000000);
}

#[test]
fn first_dynamic_stdclib_import_installs_process_storage_once_and_unload_preserves_it() {
    let mut app = app();
    let data = app.info.toc_or_a5;
    let mut f = pef_program(&[0x4e800020], &[b"__C_phase"], 24);
    f.bytes[f.strings..f.strings + 8].copy_from_slice(b"StdCLib\0");
    f.bytes[f.loader + 80] = 1; // data import, not a transition vector
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    assert!(app.runtime.memory.read(GuestPtr(0x72000040), 1).is_err());
    assert_eq!(load(&mut app, &f, 5), 0);
    let first = word(&app, data + 64);
    let main = word(&app, data + 68);
    let toc = word(&app, main + 4);
    assert_eq!(word(&app, toc + 8), 0x72000040);
    app.runtime
        .memory
        .write(GuestPtr(0x72000040), &99u32.to_be_bytes())
        .unwrap();
    assert_eq!(load(&mut app, &f, 5), 0);
    assert_eq!(word(&app, 0x72000040), 99);
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    app.runtime
        .memory
        .write(GuestPtr(data + 64), &first.to_be_bytes())
        .unwrap();
    assert_eq!(call(&mut app, 1, &[data + 64]), 0);
    assert_eq!(word(&app, 0x72000040), 99);
}
#[test]
fn loading_ppc_fragments_from_68k_is_explicitly_not_mixed_mode_support() {
    let mut app = app();
    let f = library(0);
    let data = app.info.toc_or_a5;
    app.runtime
        .memory
        .map(GuestPtr(SOURCE), f.bytes.clone(), Permissions::RW)
        .unwrap();
    app.runtime
        .memory
        .map(GuestPtr(RETURN + 0x1000), vec![0xaa, 0x5a], Permissions::RX)
        .unwrap();
    let args = [SOURCE, f.bytes.len() as u32, 0, 5, data + 64, data + 68, 0];
    let mut frame = 3u16.to_be_bytes().to_vec();
    for value in args.iter().rev() {
        frame.extend(value.to_be_bytes());
    }
    frame.extend([0; 2]);
    app.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
    app.runtime.cpu = Cpu::new(Architecture::M68k, RETURN + 0x1000, STACK);
    assert!(app
        .runtime
        .advance(1)
        .unwrap_err()
        .to_string()
        .contains("Mixed Mode"));
    assert_eq!(word(&app, data + 64), 0);
    assert!(app.runtime.memory.read(GuestPtr(0x80000000), 1).is_err());
}
