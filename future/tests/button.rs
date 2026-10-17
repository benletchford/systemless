mod support;
use systemless_future::{
    abi::{Abi, Reply, Service},
    cpu::{Architecture, Call, Cpu},
    events::{self, Input},
    host_input::Pointer,
    load::{LoadedApplication, STACK_BASE},
    memory::{GuestPtr, Memory, Permissions},
    runtime::Stop,
};
fn app() -> LoadedApplication {
    let f = support::pef_program(&[0x4e800020], &[b"Button"], 12);
    let mut a = LoadedApplication::pef(&f.bytes, None).unwrap();
    a.runtime
        .memory
        .map(GuestPtr(0x50000000), vec![0xa9, 0x74], Permissions::RX)
        .unwrap();
    a
}
fn configure(a: &mut LoadedApplication, arch: Architecture) {
    let sp = STACK_BASE + 0x7000;
    a.runtime.memory.write(GuestPtr(sp), &[0xa5; 4]).unwrap();
    let pc = if arch == Architecture::M68k {
        0x50000000
    } else {
        let tv = a
            .runtime
            .memory
            .u32(GuestPtr(a.info.toc_or_a5 + 8))
            .unwrap();
        a.runtime.memory.u32(GuestPtr(tv)).unwrap()
    };
    a.runtime.cpu = Cpu::new(arch, pc, sp);
    match &mut a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                c.dar[i] = 0x12340000 + i as u32;
            }
        }
        Cpu::PowerPc(c) => {
            for i in 2..32 {
                c.gpr[i] = 0x12340000 + i as u32;
            }
        }
    }
}
fn invoke(a: &mut LoadedApplication, arch: Architecture) -> bool {
    configure(a, arch);
    let sp = STACK_BASE + 0x7000;
    let queue = a
        .runtime
        .memory
        .read(events::QUEUE_HEADER, 10)
        .unwrap()
        .to_vec();
    let free = a.runtime.process.free_memory();
    a.runtime.process.mem_error = -108;
    a.runtime.process.res_error = -192;
    assert_eq!(a.runtime.advance(1).unwrap(), Stop::Budget);
    assert_eq!(a.runtime.services.calls.last(), Some(&Call::Button));
    assert_eq!(
        a.runtime.memory.read(events::QUEUE_HEADER, 10).unwrap(),
        queue
    );
    assert_eq!(a.runtime.process.free_memory(), free);
    assert_eq!(a.runtime.process.mem_error, -108);
    assert_eq!(a.runtime.process.res_error, -192);
    match &a.runtime.cpu {
        Cpu::M68k(c) => {
            for i in 0..15 {
                assert_eq!(c.dar[i], 0x12340000 + i as u32);
            }
            assert_eq!(c.dar[15], sp);
            assert_eq!(
                a.runtime.memory.read(GuestPtr(sp + 1), 3).unwrap(),
                [0xa5; 3]
            );
            let b = a.runtime.memory.read(GuestPtr(sp), 1).unwrap()[0];
            assert!(b <= 1);
            b != 0
        }
        Cpu::PowerPc(c) => {
            assert_eq!(c.gpr[1], sp);
            for i in 2..32 {
                if i != 3 {
                    assert_eq!(c.gpr[i], 0x12340000 + i as u32);
                }
            }
            assert!(c.gpr[3] <= 1);
            c.gpr[3] != 0
        }
    }
}
fn post(a: &mut LoadedApplication, what: u16) {
    assert_eq!(
        events::post(&mut a.runtime.memory, what, 0, 1, Input::default()).unwrap(),
        0
    );
}
#[test]
fn native_boolean_result_preflights_storage_and_readable_trap_veneer_exists() {
    let mut m = Memory::default();
    systemless_future::traps::install(&mut m).unwrap();
    for opcode in [0xa974, 0xa973] {
        let entry = systemless_future::traps::lookup(&m, opcode, true).unwrap();
        assert_ne!(entry, systemless_future::traps::UNIMPLEMENTED);
        assert_eq!(m.u16(GuestPtr(entry)).unwrap(), opcode);
    }
    m.map(GuestPtr(0x1000), vec![0xa5; 2], Permissions::RW)
        .unwrap();
    m.map(GuestPtr(0x2000), vec![0xa5; 2], Permissions::RX)
        .unwrap();
    let mut cpu = Cpu::new(Architecture::M68k, 0, 0x1000);
    let Cpu::M68k(c) = &mut cpu else { panic!() };
    assert_eq!(
        Abi::trap(0xa974, c, &m).unwrap(),
        (Call::Button, Reply::StackByte(GuestPtr(0x1000)))
    );
    assert_eq!(c.dar[15], 0x1000);
    for sp in [0x2000, 0x3000] {
        c.dar[15] = sp;
        assert!(Abi::trap(0xa974, c, &m).is_err());
        assert_eq!(c.dar[15], sp);
    }
    let mut cpu = Cpu::new(Architecture::PowerPc, 0, 0);
    let Cpu::PowerPc(c) = &mut cpu else { panic!() };
    assert_eq!(Abi::import(Service::Button, c, &m).unwrap().0, Call::Button);
}
#[test]
fn both_cpus_read_current_high_bit_and_leave_queue_and_other_state_unchanged() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        assert_eq!(a.runtime.memory.read(events::MB_STATE, 1).unwrap(), [0x80]);
        assert!(!invoke(&mut a, arch));
        post(&mut a, 1);
        post(&mut a, 2);
        post(&mut a, 3);
        assert!(!invoke(&mut a, arch));
        let head = a
            .runtime
            .memory
            .u32(GuestPtr(events::QUEUE_HEADER.0 + 2))
            .unwrap();
        let bytes = a.runtime.memory.read(GuestPtr(head), 24).unwrap().to_vec();
        for state in [0u8, 1, 0x7f, 0x80, 0x81, 0xff] {
            a.runtime.memory.write(events::MB_STATE, &[state]).unwrap();
            a.runtime.process.input.modifiers = if state & 0x80 == 0 { 0x80 } else { 0 };
            assert_eq!(invoke(&mut a, arch), state & 0x80 == 0);
            assert_eq!(a.runtime.memory.read(events::MB_STATE, 1).unwrap(), [state]);
            assert_eq!(a.runtime.memory.read(GuestPtr(head), 24).unwrap(), bytes);
        }
        events::flush(&mut a.runtime.memory, 0xffff, 0).unwrap();
        a.runtime.memory.write(events::MB_STATE, &[0x7f]).unwrap();
        assert!(invoke(&mut a, arch));
    }
}
#[test]
fn host_press_release_and_dequeue_keep_current_state_independent_from_queued_clicks() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        let mut p = Pointer::default();
        p.motion(
            &mut a.runtime.process.input,
            &mut a.runtime.memory,
            (100.0, 100.0),
            (512, 342),
            (512, 342),
        )
        .unwrap();
        a.runtime.memory.write(events::MB_STATE, &[0x93]).unwrap();
        p.button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
            .unwrap();
        assert_eq!(a.runtime.memory.read(events::MB_STATE, 1).unwrap(), [0x13]);
        assert!(invoke(&mut a, arch));
        let input = a.runtime.process.input;
        assert!(
            events::receive(&mut a.runtime.memory, 2, GuestPtr(0x500), true, 10, input).unwrap()
        );
        assert!(invoke(&mut a, arch));
        p.button(&mut a.runtime.memory, &mut a.runtime.process.input, false)
            .unwrap();
        assert_eq!(a.runtime.memory.read(events::MB_STATE, 1).unwrap(), [0x93]);
        assert!(!invoke(&mut a, arch));
        assert!(events::available(&a.runtime.memory, 4).unwrap());
        post(&mut a, 1);
        assert!(!invoke(&mut a, arch));
    }
}
#[test]
fn masked_host_press_changes_polled_state_and_focus_loss_releases_it() {
    let mut a = app();
    a.runtime
        .memory
        .write(events::SYSTEM_MASK, &0u16.to_be_bytes())
        .unwrap();
    let mut p = Pointer::default();
    p.motion(
        &mut a.runtime.process.input,
        &mut a.runtime.memory,
        (10.0, 10.0),
        (512, 342),
        (512, 342),
    )
    .unwrap();
    p.button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
        .unwrap();
    assert_eq!(a.runtime.process.input.modifiers & 0x80, 0);
    assert!(!events::available(&a.runtime.memory, 0xffff).unwrap());
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert!(invoke(&mut a, arch));
    }
    p.unfocus(&mut a.runtime.memory, &mut a.runtime.process.input)
        .unwrap();
    assert_eq!(a.runtime.process.input.modifiers & 0x80, 0x80);
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        assert!(!invoke(&mut a, arch));
    }
}
#[test]
fn polling_does_not_traverse_or_mutate_corrupt_event_queue_links() {
    for arch in [Architecture::M68k, Architecture::PowerPc] {
        let mut a = app();
        a.runtime
            .memory
            .write(
                GuestPtr(events::QUEUE_HEADER.0 + 2),
                &0x1234u32.to_be_bytes(),
            )
            .unwrap();
        a.runtime.memory.write(events::MB_STATE, &[0]).unwrap();
        assert!(invoke(&mut a, arch));
        a.runtime.memory.write(events::MB_STATE, &[0x80]).unwrap();
        assert!(!invoke(&mut a, arch));
    }
}
#[test]
fn protected_button_state_preflights_before_posting_or_changing_host_state() {
    let mut memory = Memory::default();
    memory
        .map(events::MOUSE, vec![0; 4], Permissions::RW)
        .unwrap();
    memory
        .map(GuestPtr(0x16a), vec![0; 4], Permissions::RW)
        .unwrap();
    memory
        .map(events::MB_STATE, vec![0x80], Permissions::RX)
        .unwrap();
    let mut input = Input::default();
    let mut pointer = Pointer::default();
    pointer
        .motion(&mut input, &mut memory, (1.0, 1.0), (10, 10), (10, 10))
        .unwrap();
    assert!(matches!(
        pointer.button(&mut memory, &mut input, true),
        Err(systemless_future::Error::Memory {
            address: 0x172,
            access: systemless_future::memory::Access::Write
        })
    ));
    assert_eq!(input.modifiers, 0x80);
    assert_eq!(memory.read(events::MB_STATE, 1).unwrap(), [0x80]);
}

#[test]
fn still_down_distinguishes_original_press_release_and_repress_without_consuming_events() {
    let mut a = app();
    let mut pointer = Pointer::default();
    pointer
        .motion(
            &mut a.runtime.process.input,
            &mut a.runtime.memory,
            (1.0, 1.0),
            (512, 342),
            (512, 342),
        )
        .unwrap();
    assert!(!events::still_down(&a.runtime.memory).unwrap());
    pointer
        .button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
        .unwrap();
    assert!(events::still_down(&a.runtime.memory).unwrap());
    pointer
        .button(&mut a.runtime.memory, &mut a.runtime.process.input, false)
        .unwrap();
    assert!(!events::still_down(&a.runtime.memory).unwrap());
    pointer
        .button(&mut a.runtime.memory, &mut a.runtime.process.input, true)
        .unwrap();
    assert!(events::button(&a.runtime.memory).unwrap());
    assert!(!events::still_down(&a.runtime.memory).unwrap());
    let queue = a
        .runtime
        .memory
        .read(events::QUEUE_HEADER, 10)
        .unwrap()
        .to_vec();
    assert!(!events::still_down(&a.runtime.memory).unwrap());
    assert_eq!(
        a.runtime.memory.read(events::QUEUE_HEADER, 10).unwrap(),
        queue
    );
    events::flush(&mut a.runtime.memory, 4, 0).unwrap();
    assert!(events::still_down(&a.runtime.memory).unwrap());
    a.runtime
        .memory
        .write(events::SYSTEM_MASK, &0u16.to_be_bytes())
        .unwrap();
    pointer
        .unfocus(&mut a.runtime.memory, &mut a.runtime.process.input)
        .unwrap();
    assert!(!events::still_down(&a.runtime.memory).unwrap());
}
