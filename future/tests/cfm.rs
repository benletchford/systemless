mod support;
use support::*;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
};

fn application() -> LoadedApplication {
    let mut f = pef_program(
        &[0x4e800020],
        &[b"CountSymbols", b"FindSymbol", b"GetIndSymbol"],
        256,
    );
    export_main(&mut f);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    app.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xaa, 0x5a], Permissions::RX)
        .unwrap();
    app
}
fn invoke(
    app: &mut LoadedApplication,
    arch: Architecture,
    service: usize,
    args: &[u32],
) -> Result<i16, systemless_future::Error> {
    let stack = STACK_BASE + 0x8000;
    app.runtime.cpu = match arch {
        Architecture::PowerPc => {
            let data = app.info.toc_or_a5;
            let vector = app
                .runtime
                .memory
                .u32(GuestPtr(data + 8 + service as u32 * 4))?;
            let gate = app.runtime.memory.u32(GuestPtr(vector))?;
            let mut cpu = Cpu::new(arch, gate, stack);
            if let Cpu::PowerPc(c) = &mut cpu {
                for (i, value) in args.iter().enumerate() {
                    c.gpr[3 + i] = *value;
                }
            }
            cpu
        }
        Architecture::M68k => {
            let selector = [6u16, 5, 7][service];
            let mut frame = selector.to_be_bytes().to_vec();
            for value in args.iter().rev() {
                frame.extend(value.to_be_bytes());
            }
            frame.extend([0x55; 4]);
            app.runtime.memory.write(GuestPtr(stack), &frame)?;
            Cpu::new(arch, 0x50000000, stack)
        }
    };
    assert_eq!(app.runtime.advance(1)?, Stop::Budget);
    match &app.runtime.cpu {
        Cpu::PowerPc(c) => Ok(c.gpr[3] as i16),
        Cpu::M68k(c) => {
            let result = stack + 2 + args.len() as u32 * 4;
            assert_eq!(c.dar[15], result);
            assert_eq!(app.runtime.memory.u16(GuestPtr(result + 2))?, 0x5555);
            Ok(app.runtime.memory.u16(GuestPtr(result))? as i16)
        }
    }
}
#[test]
fn symbol_queries_share_connection_and_exports_across_real_cpu_adapters() {
    let mut app = application();
    let data = app.info.toc_or_a5;
    let out = data + 64;
    let name = data + 96;
    app.runtime
        .memory
        .write(GuestPtr(name), b"\x04main")
        .unwrap();
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        assert_eq!(invoke(&mut app, arch, 0, &[1, out]).unwrap(), 0);
        assert_eq!(app.runtime.memory.u32(GuestPtr(out)).unwrap(), 1);
        assert_eq!(
            invoke(&mut app, arch, 1, &[1, name, out + 4, out + 8]).unwrap(),
            0
        );
        assert_eq!(app.runtime.memory.u32(GuestPtr(out + 4)).unwrap(), data);
        assert_eq!(app.runtime.memory.read(GuestPtr(out + 8), 1).unwrap(), &[2]);
        assert_eq!(
            invoke(&mut app, arch, 2, &[1, 0, out + 12, out + 4, out + 8]).unwrap(),
            0
        );
        assert_eq!(
            app.runtime.memory.read(GuestPtr(out + 12), 5).unwrap(),
            b"\x04main"
        );
        assert_eq!(invoke(&mut app, arch, 1, &[1, name, 0, 0]).unwrap(), 0);
        assert_eq!(invoke(&mut app, arch, 2, &[1, 0, 0, 0, 0]).unwrap(), 0);
    }
    assert_eq!(app.runtime.services.calls.len(), 10);
}
#[test]
fn symbol_errors_preserve_outputs_and_names_are_exact() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        let mut app = application();
        let data = app.info.toc_or_a5;
        let out = data + 64;
        let name = data + 96;
        app.runtime
            .memory
            .write(GuestPtr(out), &[0x55; 24])
            .unwrap();
        app.runtime
            .memory
            .write(GuestPtr(name), b"\x04Main")
            .unwrap();
        assert_eq!(invoke(&mut app, arch, 0, &[0, out]).unwrap(), -2801);
        assert_eq!(
            invoke(&mut app, arch, 1, &[1, name, out, out + 4]).unwrap(),
            -2802
        );
        for index in [1, u32::MAX] {
            assert_eq!(
                invoke(&mut app, arch, 2, &[1, index, out + 8, out, out + 4]).unwrap(),
                -2802
            );
        }
        assert_eq!(
            app.runtime.memory.read(GuestPtr(out), 24).unwrap(),
            &[0x55; 24]
        );
    }
}
#[test]
fn symbol_query_outputs_are_validated_before_mutation_and_optional_pointers_are_respected() {
    for arch in [Architecture::PowerPc, Architecture::M68k] {
        for indexed in [false, true] {
            let mut app = application();
            let data = app.info.toc_or_a5;
            let out = data + 64;
            let name = data + 96;
            app.runtime
                .memory
                .write(GuestPtr(out), &[0x55; 24])
                .unwrap();
            app.runtime
                .memory
                .write(GuestPtr(name), b"\x04main")
                .unwrap();
            let read_only = app.info.entry;
            let result = if indexed {
                invoke(&mut app, arch, 2, &[1, 0, out + 8, out, read_only])
            } else {
                invoke(&mut app, arch, 1, &[1, name, out, read_only])
            };
            assert!(result.is_err());
            assert_eq!(
                app.runtime.memory.read(GuestPtr(out), 24).unwrap(),
                &[0x55; 24]
            );
        }
    }
}
#[test]
fn initializer_receives_live_connection_and_main_queries_it_across_instruction_budgets() {
    let mut code = vec![0x7c5f1378]; // preserve TOC in r31
    fn call(code: &mut Vec<u32>, index: u32) {
        code.extend([
            0x819f0000 | (24 + index * 4),
            0x800c0000,
            0x804c0004,
            0x7c0903a6,
            0x4e800421,
            0x7fe2fb78,
        ]);
    }
    code.extend([0x807f0064, 0x389f0068]);
    call(&mut code, 0);
    code.extend([0x907f0080, 0x807f0064, 0x389f00c0, 0x38bf006c, 0x38df0070]);
    call(&mut code, 1);
    code.extend([
        0x907f0084, 0x807f0064, 0x38800000, 0x38bf00a0, 0x38df0074, 0x38ff0078,
    ]);
    call(&mut code, 2);
    code.push(0x907f0088);
    call(&mut code, 3);
    let init_offset = code.len() as u32 * 4;
    code.extend([
        0x7fc802a6, 0x7c5f1378, 0x80830008, 0x90820064, 0x7c832378, 0x389f008c,
    ]);
    call(&mut code, 0); // Query while initialization is still active.
    code.extend([0x907f0090, 0x7fc803a6, 0x38600000, 0x4e800020]);
    let mut f = pef_program(
        &code,
        &[
            b"CountSymbols",
            b"FindSymbol",
            b"GetIndSymbol",
            b"ExitToShell",
        ],
        256,
    );
    w16(&mut f.bytes, f.reloc, 0x4602); // three transition vectors precede imports
    w32(&mut f.bytes, f.data + 8, init_offset);
    w32(&mut f.bytes, f.loader + 8, 1);
    w32(&mut f.bytes, f.loader + 12, 8);
    f.bytes[f.data + 192..f.data + 197].copy_from_slice(b"\x04main");
    export_main(&mut f);
    let mut app = LoadedApplication::pef(&f.bytes, None).unwrap();
    let data = app.info.toc_or_a5;
    let mut stop = Stop::Budget;
    for _ in 0..100 {
        stop = app.runtime.advance(1).unwrap();
        if stop != Stop::Budget {
            break;
        }
    }
    assert_eq!(stop, Stop::Exit(0));
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 140)).unwrap(), 1);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 144)).unwrap(), 0);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 100)).unwrap(), 1);
    assert_eq!(app.runtime.memory.u32(GuestPtr(data + 104)).unwrap(), 1);
    for offset in [108, 116] {
        assert_eq!(
            app.runtime.memory.u32(GuestPtr(data + offset)).unwrap(),
            data
        );
    }
    for offset in [112, 120] {
        assert_eq!(
            app.runtime.memory.read(GuestPtr(data + offset), 1).unwrap(),
            &[2]
        );
    }
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data + 128), 12).unwrap(),
        &[0; 12]
    );
    assert_eq!(
        app.runtime.memory.read(GuestPtr(data + 160), 5).unwrap(),
        b"\x04main"
    );
}
