mod support;
use systemless_future::{
    cpu::{Architecture, Cpu},
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Permissions},
    runtime::Stop,
    timers::{Action, Op},
};
const TASK: GuestPtr = GuestPtr(0x50000000);
const CODE: u32 = 0x51000000;
fn app() -> LoadedApplication {
    let f = support::pef_program(
        &[0x4e800020],
        &[b"InsTime", b"PrimeTime", b"RmvTime", b"WaitNextEvent"],
        32,
    );
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(TASK, vec![0; 4096], Permissions::RW)
        .unwrap();
    let mut code = vec![0; 512];
    code[..8].copy_from_slice(&[0xa0, 0x58, 0xa0, 0x5a, 0xa0, 0x59, 0x60, 0xfe]);
    for (i, w) in [0x52a9u16, 24, 0x2349, 28, 0x7007, 0x7809, 0x4e75]
        .into_iter()
        .enumerate()
    {
        code[64 + i * 2..66 + i * 2].copy_from_slice(&w.to_be_bytes());
    }
    for (i, w) in [
        0x80830018u32,
        0x38840001,
        0x90830018,
        0x9063001c,
        0x39c003e7,
        0x38600007,
        0x4e800020,
    ]
    .into_iter()
    .enumerate()
    {
        code[128 + i * 4..132 + i * 4].copy_from_slice(&w.to_be_bytes());
    }
    code[256..260].copy_from_slice(&0x48000000u32.to_be_bytes());
    a.runtime
        .memory
        .map(GuestPtr(CODE), code, Permissions::RX)
        .unwrap();
    a
}
fn call(a: &mut LoadedApplication, arch: Architecture, action: Action) {
    let index = match action {
        Action::Insert => 0,
        Action::Prime(_) => 1,
        Action::Remove => 2,
    };
    let stack = STACK_BASE + 0x8000;
    if arch == Architecture::M68k {
        a.runtime.cpu = Cpu::new(arch, CODE + index * 2, stack);
        let Cpu::M68k(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.dar[8] = TASK.0;
        c.dar[0] = if let Action::Prime(n) = action {
            n as u32
        } else {
            0xdeadbeef
        };
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8 + index * 4))
            .unwrap();
        let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
        a.runtime.cpu = Cpu::new(arch, pc, stack);
        let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
            panic!()
        };
        c.gpr[3] = TASK.0;
        if let Action::Prime(n) = action {
            c.gpr[4] = n as u32;
        }
    }
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            assert_eq!(c.dar[0], 0);
            assert_eq!(c.dar[8], TASK.0);
            assert_eq!(c.dar[15], stack)
        }
        Cpu::PowerPc(c) => assert_eq!(c.gpr[3], TASK.0),
    }
}
fn callback(a: &mut LoadedApplication, arch: Architecture) {
    let routine = if arch == Architecture::M68k {
        CODE + 64
    } else {
        let descriptor = TASK.0 + 128;
        let tv = TASK.0 + 160;
        let mut rd = [0; 32];
        rd[..4].copy_from_slice(&[0xaa, 0xfe, 7, 0]);
        rd[12..16].copy_from_slice(&0xb802u32.to_be_bytes());
        rd[17] = 1;
        rd[20..24].copy_from_slice(&tv.to_be_bytes());
        a.runtime.memory.write(GuestPtr(descriptor), &rd).unwrap();
        a.runtime
            .memory
            .write(GuestPtr(tv), &(CODE + 128).to_be_bytes())
            .unwrap();
        a.runtime
            .memory
            .write(GuestPtr(tv + 4), &0x12345678u32.to_be_bytes())
            .unwrap();
        descriptor
    };
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 6), &routine.to_be_bytes())
        .unwrap();
}
fn idle(a: &mut LoadedApplication, arch: Architecture) {
    a.runtime.cpu = Cpu::new(
        arch,
        if arch == Architecture::M68k {
            CODE + 6
        } else {
            CODE + 256
        },
        STACK_BASE + 0x8000,
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
fn both_cpus_run_one_shot_callbacks_and_restore_interrupted_registers_and_stack() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        callback(&mut a, arch);
        call(&mut a, arch, Action::Insert);
        assert_eq!(
            a.runtime.memory.u16(GuestPtr(TASK.0 + 4)).unwrap() & 0x8000,
            0
        );
        call(&mut a, arch, Action::Prime(1));
        idle(&mut a, arch);
        assert_eq!(a.runtime.advance(30).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 0);
        assert_eq!(a.runtime.advance(100).unwrap(), Stop::Budget);
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 1);
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 28)).unwrap(), TASK.0);
        assert_eq!(a.runtime.services.completed_timer_callbacks, 1);
        match &a.runtime.cpu {
            Cpu::M68k(c) => {
                for i in 0..15 {
                    assert_eq!(c.dar[i], 0xabc00000 + i as u32);
                }
                assert_eq!(c.get_sr(), 0x15);
                assert_eq!(c.dar[15], STACK_BASE + 0x8000);
                assert_eq!(c.pc, CODE + 6)
            }
            Cpu::PowerPc(c) => {
                for i in 2..32 {
                    assert_eq!(c.gpr[i], 0xabc00000 + i as u32);
                }
                assert_eq!(c.gpr[1], STACK_BASE + 0x8000);
                assert_eq!(c.lr, 0x12345678);
                assert_eq!(c.cr, 0xdeadbeef);
                assert_eq!(c.fpr[1], 12.5f64.to_bits());
                assert_eq!(c.pc, CODE + 256)
            }
        }
        assert_eq!(
            a.runtime.memory.u16(GuestPtr(TASK.0 + 4)).unwrap() & 0x8000,
            0
        );
        call(&mut a, arch, Action::Remove);
    }
}
#[test]
fn negative_delays_host_idle_time_and_removed_unused_time_share_one_clock() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        callback(&mut a, arch);
        call(&mut a, arch, Action::Insert);
        call(&mut a, arch, Action::Prime(-100));
        idle(&mut a, arch);
        a.runtime.advance(5).unwrap();
        assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 0);
        a.runtime.advance(30).unwrap();
        assert_eq!(a.runtime.services.completed_timer_callbacks, 1);
        call(&mut a, arch, Action::Prime(100));
        a.runtime.elapse_ticks(1).unwrap();
        call(&mut a, arch, Action::Remove);
        assert_eq!(
            a.runtime.memory.u32(GuestPtr(TASK.0 + 10)).unwrap() as i32,
            -83317
        );
        call(&mut a, arch, Action::Insert);
        call(&mut a, arch, Action::Prime(1));
        call(&mut a, arch, Action::Remove);
        idle(&mut a, arch);
        a.runtime.elapse_ticks(60).unwrap();
        a.runtime.advance(100).unwrap();
        assert_eq!(a.runtime.services.completed_timer_callbacks, 1);
    }
}
#[test]
fn instruction_ticks_do_not_double_count_timer_time_and_nil_tasks_expire_without_callbacks() {
    let mut a = app();
    call(&mut a, Architecture::M68k, Action::Insert);
    call(&mut a, Architecture::M68k, Action::Prime(10000));
    idle(&mut a, Architecture::M68k);
    a.runtime.advance(1000).unwrap();
    call(&mut a, Architecture::M68k, Action::Remove);
    assert_eq!(
        a.runtime.memory.u32(GuestPtr(TASK.0 + 10)).unwrap() as i32,
        -9983317
    );
    call(&mut a, Architecture::M68k, Action::Insert);
    call(&mut a, Architecture::M68k, Action::Prime(0));
    idle(&mut a, Architecture::M68k);
    a.runtime.advance(1).unwrap();
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(TASK.0 + 4)).unwrap() & 0x8000,
        0
    );
    assert_eq!(a.runtime.services.completed_timer_callbacks, 0);
}
#[test]
fn task_queue_mutations_preflight_memory_and_reject_unknown_or_duplicate_records() {
    let mut a = app();
    let t = &mut a.runtime.process.timers;
    let m = &mut a.runtime.memory;
    assert!(t
        .call(
            m,
            Op {
                task: TASK,
                action: Action::Prime(1)
            }
        )
        .is_err());
    t.call(
        m,
        Op {
            task: TASK,
            action: Action::Insert,
        },
    )
    .unwrap();
    let before = m.read(TASK, 14).unwrap().to_vec();
    assert!(t
        .call(
            m,
            Op {
                task: TASK,
                action: Action::Insert
            }
        )
        .is_err());
    assert_eq!(m.read(TASK, 14).unwrap(), before);
    let second = GuestPtr(TASK.0 + 32);
    t.call(
        m,
        Op {
            task: second,
            action: Action::Insert,
        },
    )
    .unwrap();
    assert_eq!(m.u32(TASK).unwrap(), second.0);
    t.call(
        m,
        Op {
            task: second,
            action: Action::Remove,
        },
    )
    .unwrap();
    assert_eq!(m.u32(TASK).unwrap(), 0);
    m.map(GuestPtr(0x60000000), vec![0; 14], Permissions::RX)
        .unwrap();
    assert!(t
        .call(
            m,
            Op {
                task: GuestPtr(0x60000000),
                action: Action::Insert
            }
        )
        .is_err());
    assert_eq!(m.read(TASK, 14).unwrap(), before);
}

#[test]
fn timer_callback_can_rearm_itself_without_reentry_and_68k_mask_defers_delivery() {
    let mut a = app();
    let address = CODE + 0x1000;
    let words = [
        0x52a9u16, 24, 0x0ca9, 0, 3, 24, 0x6c06, 0x2049, 0x7000, 0xa05a, 0x4e75,
    ];
    a.runtime
        .memory
        .map(
            GuestPtr(address),
            words.into_iter().flat_map(u16::to_be_bytes).collect(),
            Permissions::RX,
        )
        .unwrap();
    a.runtime
        .memory
        .write(GuestPtr(TASK.0 + 6), &address.to_be_bytes())
        .unwrap();
    call(&mut a, Architecture::M68k, Action::Insert);
    call(&mut a, Architecture::M68k, Action::Prime(0));
    idle(&mut a, Architecture::M68k);
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.set_sr(0x0700);
    a.runtime.advance(100).unwrap();
    assert_eq!(a.runtime.services.completed_timer_callbacks, 0);
    assert_ne!(
        a.runtime.memory.u16(GuestPtr(TASK.0 + 4)).unwrap() & 0x8000,
        0
    );
    let Cpu::M68k(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.set_sr(0);
    a.runtime.advance(100).unwrap();
    assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 3);
    assert_eq!(a.runtime.services.completed_timer_callbacks, 3);
    assert_eq!(
        a.runtime.memory.u16(GuestPtr(TASK.0 + 4)).unwrap() & 0x8000,
        0
    );
    call(&mut a, Architecture::M68k, Action::Remove);
}
#[test]
fn idle_event_wait_wakes_for_timer_then_resumes_the_original_wait() {
    let mut a = app();
    let arch = Architecture::PowerPc;
    callback(&mut a, arch);
    call(&mut a, arch, Action::Insert);
    call(&mut a, arch, Action::Prime(1));
    let tv = a
        .runtime
        .memory
        .u32(GuestPtr(a.info.toc_or_a5 + 8 + 3 * 4))
        .unwrap();
    let pc = a.runtime.memory.u32(GuestPtr(tv)).unwrap();
    a.runtime.cpu = Cpu::new(arch, pc, STACK_BASE + 0x8000);
    let Cpu::PowerPc(c) = &mut a.runtime.cpu else {
        panic!()
    };
    c.gpr[3] = 8;
    c.gpr[4] = TASK.0 + 512;
    c.gpr[5] = 10;
    c.gpr[6] = 0;
    assert_eq!(a.runtime.advance(100).unwrap(), Stop::Waiting { ticks: 1 });
    a.runtime.elapse_ticks(1).unwrap();
    assert_eq!(a.runtime.advance(100).unwrap(), Stop::Waiting { ticks: 9 });
    assert_eq!(a.runtime.services.completed_timer_callbacks, 1);
    assert_eq!(a.runtime.memory.u32(GuestPtr(TASK.0 + 24)).unwrap(), 1);
    a.runtime.elapse_ticks(9).unwrap();
    assert_eq!(a.runtime.advance(100).unwrap(), Stop::Returned(0));
}

#[test]
fn invalid_callback_entries_and_descriptors_fault_before_expiration_or_stack_writes() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        callback(&mut a, arch);
        if arch == Architecture::M68k {
            a.runtime
                .memory
                .write(GuestPtr(TASK.0 + 6), &0xdeadbeefu32.to_be_bytes())
                .unwrap();
        } else {
            a.runtime
                .memory
                .write(GuestPtr(TASK.0 + 128 + 15), &[3])
                .unwrap();
        }
        call(&mut a, arch, Action::Insert);
        call(&mut a, arch, Action::Prime(0));
        idle(&mut a, arch);
        let bytes = a.runtime.memory.read(TASK, 4096).unwrap().to_vec();
        let stack = GuestPtr(STACK_BASE + 0x8000 - 64);
        let frame = a.runtime.memory.read(stack, 64).unwrap().to_vec();
        let pc = a.runtime.cpu.pc();
        assert!(a.runtime.advance(1).is_err());
        assert_eq!(a.runtime.memory.read(TASK, 4096).unwrap(), bytes);
        assert_eq!(a.runtime.memory.read(stack, 64).unwrap(), frame);
        assert_eq!(a.runtime.cpu.pc(), pc);
        assert_eq!(a.runtime.services.completed_timer_callbacks, 0);
        assert!(a.runtime.advance(1).is_err());
    }
}
