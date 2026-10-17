mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    retrace,
    runtime::Stop,
};
const TASK: GuestPtr = GuestPtr(0x50000000);
const CODE: u32 = 0x51000000;
const STACK: u32 = STACK_BASE + 0x8000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[
            b"VInstall",
            b"VRemove",
            b"SlotVInstall",
            b"SlotVRemove",
            b"WaitNextEvent",
            b"GetVBLQHdr",
        ],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(TASK, vec![0; 4096], Permissions::RW)
        .unwrap();
    let mut code = vec![0; 512];
    code[..12].copy_from_slice(&support::words(&[
        0xa033, 0xa034, 0xa06f, 0xa070, 0x60fe, 0xa860,
    ]));
    let native = [
        0x52a8, 24, 0x2148, 28, 0x0ca8, 0, 3, 24, 0x6c06, 0x317c, 1, 10, 0x7007, 0x7809, 0x4e75,
    ];
    code[64..64 + native.len() * 2].copy_from_slice(&support::words(&native));
    let ppc = [
        0x80830018u32,
        0x38840001,
        0x90830018,
        0x9063001c,
        0x2c040003,
        0x4080000c,
        0x38a00001,
        0xb0a3000a,
        0x39c003e7,
        0x38600007,
        0x4e800020,
    ];
    for (i, w) in ppc.iter().enumerate() {
        code[128 + i * 4..132 + i * 4].copy_from_slice(&w.to_be_bytes());
    }
    code[256..260].copy_from_slice(&0x48000000u32.to_be_bytes());
    code[384..390].copy_from_slice(&support::words(&[0x7004, 0xa11e, 0x4e75]));
    code[392..394].copy_from_slice(&0xa058u16.to_be_bytes());
    a.runtime
        .memory
        .map(GuestPtr(CODE), code, Permissions::RX)
        .unwrap();
    a
}
fn prepare(a: &mut LoadedApplication, arch: Architecture, count: u16) {
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 4), &1u16.to_be_bytes())
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 10), &count.to_be_bytes())
        .unwrap();
    let routine = if arch == Architecture::M68k {
        CODE + 64
    } else {
        let mut rd = [0; 32];
        rd[..4].copy_from_slice(&[0xaa, 0xfe, 7, 0]);
        rd[12..16].copy_from_slice(&0x9802u32.to_be_bytes());
        rd[17] = 1;
        rd[20..24].copy_from_slice(&(TASK.0 + 160).to_be_bytes());
        a.runtime.memory.write(GuestPtr(TASK.0 + 128), &rd).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(TASK.0 + 160), &(CODE + 128).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(TASK.0 + 164), &0x12345678u32.to_be_bytes())
            .unwrap();
        TASK.0 + 128
    };
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 6), &routine.to_be_bytes())
        .unwrap();
}
fn call(
    a: &mut LoadedApplication,
    arch: Architecture,
    index: u32,
    task: GuestPtr,
    slot: i16,
) -> u32 {
    if arch == Architecture::M68k {
        a.runtime.cpu = Cpu::new(arch, CODE + index * 2, STACK);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[8] = task.0;
        c.dar[0] = slot as i32 as u32;
        c.dar[1] = 0x12345678;
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, STACK);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = task.0;
        c.gpr[4] = slot as i32 as u32;
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[8], task.0);
            assert_eq!(c.dar[1], 0x12345678);
            assert_eq!(c.dar[15], STACK);
            c.dar[0]
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], STACK);
            c.gpr[3]
        }
    }
}
fn idle(a: &mut LoadedApplication, arch: Architecture) {
    a.runtime.cpu = Cpu::new(
        arch,
        if arch == Architecture::M68k {
            CODE + 8
        } else {
            CODE + 256
        },
        STACK,
    );
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                c.dar[i] = 0xabc00000 + i as u32;
            }
            c.set_sr(0x15);
        }
        Cpu::PowerPc(c) => {
            for i in 2..32 {
                c.gpr[i] = 0xabc00000 + i as u32;
            }
            c.lr = 0x12345678;
            c.cr = 0xdeadbeef;
            c.fpr[1] = 12.5f64.to_bits();
        }
    }
}
#[test]
fn both_cpu_adapters_own_distinct_system_and_display_queues_and_report_real_errors() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        prepare(&mut a, arch, 0);
        assert_eq!(call(&mut a, arch, 2, TASK, 1), -360i32 as u32);
        a.runtime
            .memory
            .write(GuestPtr(TASK.0 + 4), &2u16.to_be_bytes())
            .unwrap();
        assert_eq!(call(&mut a, arch, 0, TASK, 0), -2i32 as u32);
        prepare(&mut a, arch, 0);
        assert_eq!(call(&mut a, arch, 2, TASK, 0), 0);
        assert_eq!(
            a.runtime
                .memory
                .u32(GuestPtr(retrace::SCREEN_QUEUE.0 + 2))
                .unwrap(),
            TASK.0
        );
        assert_eq!(
            a.runtime
                .memory
                .u32(GuestPtr(retrace::SYSTEM.0 + 2))
                .unwrap(),
            0
        );
        assert_eq!(call(&mut a, arch, 1, TASK, 0), -1i32 as u32);
        assert_eq!(call(&mut a, arch, 3, TASK, 0), 0);
        assert_eq!(call(&mut a, arch, 3, TASK, 0), -1i32 as u32);
        assert_eq!(call(&mut a, arch, 0, TASK, 0), 0);
        assert_eq!(call(&mut a, arch, 1, TASK, 0), 0);
        if arch == Architecture::PowerPc {
            assert_eq!(call(&mut a, arch, 5, TASK, 0), retrace::SYSTEM.0);
        }
    }
}
#[test]
fn real_callbacks_rearm_three_times_catch_up_bounded_time_and_restore_the_whole_cpu() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        prepare(&mut a, arch, 1);
        assert_eq!(call(&mut a, arch, 2, TASK, 0), 0);
        idle(&mut a, arch);
        a.runtime.elapse_ticks(10).unwrap();
        assert_eq!(a.runtime.advance(100).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 3);
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 28)).unwrap(), TASK.0);
        assert_eq!(a.runtime.services.completed_vbl_callbacks, 3);
        assert_eq!(
            a.runtime.memory.read(retrace::SCREEN_QUEUE, 10).unwrap(),
            [0; 10]
        );
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                for i in 0..15 {
                    assert_eq!(c.dar[i], 0xabc00000 + i as u32);
                }
                assert_eq!(c.dar[15], STACK);
                assert_eq!(c.get_sr(), 0x15);
            }
            Cpu::PowerPc(c) => {
                for i in 2..32 {
                    assert_eq!(c.gpr[i], 0xabc00000 + i as u32);
                }
                assert_eq!(c.gpr[1], STACK);
                assert_eq!(c.lr, 0x12345678);
                assert_eq!(c.cr, 0xdeadbeef);
                assert_eq!(c.fpr[1], 12.5f64.to_bits());
            }
        }
    }
}
#[test]
fn masked_native_interrupts_defer_callbacks_and_restore_the_interrupted_mask() {
    let mut a = app();
    prepare(&mut a, Architecture::M68k, 1);
    call(&mut a, Architecture::M68k, 0, TASK, 0);
    idle(&mut a, Architecture::M68k);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.set_sr(0x215);
    }
    a.runtime.elapse_ticks(1).unwrap();
    a.runtime.advance(20).unwrap();
    assert_eq!(a.runtime.services.completed_vbl_callbacks, 0);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.set_sr(0x15);
    }
    a.runtime.advance(1).unwrap();
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.dar[8], TASK.0);
    assert_eq!(c.get_sr() & 0x700, 0x100);
    assert_eq!(a.runtime.memory.u16(retrace::SYSTEM).unwrap() & 0x40, 0x40);
    a.runtime.advance(30).unwrap();
    assert_eq!(a.runtime.services.completed_vbl_callbacks, 1);
    let Cpu::M68k(c) = &a.runtime.cpu else {
        panic!()
    };
    assert_eq!(c.get_sr(), 0x15);
}
#[test]
fn callbacks_interrupt_and_resume_event_waits_on_both_cpus() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        prepare(&mut a, arch, 1);
        call(&mut a, arch, 0, TASK, 0);
        if arch == Architecture::M68k {
            let mut frame = Vec::new();
            for v in [0u32, 20, TASK.0 + 256] {
                frame.extend(v.to_be_bytes());
            }
            frame.extend(0u16.to_be_bytes());
            frame.extend([0xa5; 2]);
            a.runtime.memory.write(GuestPtr(STACK), &frame).unwrap();
            a.runtime.cpu = Cpu::new(arch, CODE + 10, STACK);
        } else {
            let tv = a
                .runtime
                .memory
                .u32(GuestPtr(a.info.toc_or_a5 + 8 + 4 * 4))
                .unwrap();
            let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
            a.runtime.cpu = Cpu::new(arch, pc, STACK);
            let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
                panic!()
            };
            c.gpr[3] = 0;
            c.gpr[4] = TASK.0 + 256;
            c.gpr[5] = 20;
            c.gpr[6] = 0;
        }
        assert_eq!(a.runtime.advance(1).unwrap(), Stop::Waiting { ticks: 1 });
        for n in 1..=3 {
            a.runtime.elapse_ticks(1).unwrap();
            assert!(matches!(
                a.runtime.advance(100).unwrap(),
                Stop::Waiting { .. }
            ));
            assert_eq!(a.runtime.services.completed_vbl_callbacks, n);
        }
        assert_eq!(a.runtime.memory.u16(retrace::SYSTEM).unwrap() & 0x40, 0);
    }
}
#[test]
fn interrupt_time_allocation_faults_and_foreign_timer_record_ownership_are_guarded() {
    let mut a = app();
    prepare(&mut a, Architecture::M68k, 1);
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 6), &(CODE + 384).to_be_bytes())
        .unwrap();
    call(&mut a, Architecture::M68k, 0, TASK, 0);
    idle(&mut a, Architecture::M68k);
    a.runtime.elapse_ticks(1).unwrap();
    let error = a.runtime.advance(10).unwrap_err().to_string();
    assert!(error.contains("VBL interrupt time"));
    assert_eq!(a.runtime.services.completed_vbl_callbacks, 0);
    assert_eq!(a.runtime.memory.read(retrace::SYSTEM, 10).unwrap(), [0; 10]);
    let mut a = app();
    prepare(&mut a, Architecture::M68k, 0);
    a.runtime
        .process
        .timers
        .call(
            &mut a.runtime.memory,
            systemless_future::timers::Op {
                task: TASK,
                action: systemless_future::timers::Action::Insert,
            },
        )
        .unwrap();
    // Runtime rejects cross-manager installation before either manager can relink.
    a.runtime.cpu = Cpu::new(Architecture::M68k, CODE, STACK);
    if let Cpu::M68k(c) = &mut a.runtime.cpu {
        c.dar[8] = TASK.0;
    }
    assert!(a
        .runtime
        .advance(1)
        .unwrap_err()
        .to_string()
        .contains("overlaps"));
}
#[test]
fn explicit_removal_before_deadline_prevents_any_callback() {
    let mut a = app();
    prepare(&mut a, Architecture::M68k, 1);
    call(&mut a, Architecture::M68k, 2, TASK, 0);
    call(&mut a, Architecture::M68k, 3, TASK, 0);
    idle(&mut a, Architecture::M68k);
    a.runtime.elapse_ticks(100).unwrap();
    a.runtime.advance(100).unwrap();
    assert_eq!(a.runtime.services.completed_vbl_callbacks, 0);
}

#[test]
fn timer_records_cannot_overwrite_either_retrace_header_or_the_screen_queue_pointer() {
    for pointer in [
        retrace::SYSTEM,
        retrace::SCREEN_QUEUE,
        retrace::SCREEN_POINTER,
    ] {
        let mut a = app();
        a.runtime.cpu = Cpu::new(Architecture::M68k, CODE + 392, STACK);
        if let Cpu::M68k(c) = &mut a.runtime.cpu {
            c.dar[8] = pointer.0;
        }
        assert!(a
            .runtime
            .advance(1)
            .unwrap_err()
            .to_string()
            .contains("overlaps a managed VBL"));
        assert_eq!(
            a.runtime.memory.u32(retrace::SCREEN_POINTER).unwrap(),
            retrace::SCREEN_QUEUE.0
        );
        assert_eq!(a.runtime.memory.read(retrace::SYSTEM, 10).unwrap(), [0; 10]);
        assert_eq!(
            a.runtime.memory.read(retrace::SCREEN_QUEUE, 10).unwrap(),
            [0; 10]
        );
    }
}
