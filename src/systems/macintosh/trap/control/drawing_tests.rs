//! Asset-free Control Manager regressions using actual 68K callbacks.
use crate::cpu::{M68kCpu, Register, StepResult};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::TrapDispatcher;

const SP: u32 = 0x100000;
const RETURN_PC: u32 = 0x1F0000;
const OWNER: u32 = 0x181000;
const OTHER_PORT: u32 = 0x183000;
const SCREEN: u32 = 0x300000;
const RECT: u32 = 0x190000;
const TITLE: u32 = 0x190020;
const LOG_CURSOR: u32 = 0x190040;
const LOG: u32 = 0x190100;

struct Fixture {
    dispatcher: TrapDispatcher,
    cpu: M68kCpu,
    bus: MacMemoryBus,
}

impl Fixture {
    fn new() -> Self {
        let mut f = Self {
            dispatcher: TrapDispatcher::new(),
            cpu: M68kCpu::new(),
            bus: MacMemoryBus::new(4 * 1024 * 1024),
        };
        f.dispatcher
            .set_screen_mode_for_test(SCREEN, 64, 512, 342, 1);
        f.bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
        f.bus.write_long(0x824, SCREEN);
        f.bus.write_word(0x828, 64);
        f.cpu.write_reg(Register::A5, 0x180000);
        f.bus.write_long(0x180000, 0x180100);
        f.bus.write_long(SP, 0x180100);
        f.call(0xA86E); // InitGraf
        f.bus.write_long(SP, OTHER_PORT);
        f.call(0xA86F); // OpenPort
        f.bus.write_long(SP, OWNER);
        f.call(0xA86F);
        f
    }

    fn dispatch(&mut self, trap: u16) {
        self.cpu.write_reg(Register::A7, SP);
        self.cpu.write_reg(Register::PC, RETURN_PC);
        self.dispatcher
            .dispatch(trap, &mut self.cpu, &mut self.bus)
            .unwrap();
    }

    fn finish_callback(&mut self) {
        for _ in 0..1000 {
            if self.cpu.read_reg(Register::PC) == RETURN_PC {
                return;
            }
            match self.cpu.step(&mut self.bus) {
                StepResult::Ok => (),
                StepResult::Aline(trap) => self
                    .dispatcher
                    .dispatch(trap, &mut self.cpu, &mut self.bus)
                    .unwrap(),
                _ => panic!("synthetic CDEF stopped before returning"),
            }
        }
        panic!("synthetic CDEF exceeded its instruction budget");
    }

    fn call(&mut self, trap: u16) {
        self.dispatch(trap);
        self.finish_callback();
    }

    fn create(&mut self, rect: (i16, i16, i16, i16), visible: bool, custom: bool) -> u32 {
        for (i, value) in [rect.0, rect.1, rect.2, rect.3].into_iter().enumerate() {
            self.bus.write_word(RECT + i as u32 * 2, value as u16);
        }
        self.bus.write_byte(TITLE, 0);
        self.bus.write_long(SP, 0);
        self.bus.write_word(SP + 4, if custom { 6400 } else { 0 });
        self.bus.write_word(SP + 6, 1);
        self.bus.write_word(SP + 8, 0);
        self.bus.write_word(SP + 10, 0);
        self.bus
            .write_word(SP + 12, if visible { 0xFF00 } else { 0 });
        self.bus.write_long(SP + 14, TITLE);
        self.bus.write_long(SP + 18, RECT);
        self.bus.write_long(SP + 22, OWNER);
        self.call(0xA954); // NewControl
        assert_eq!(self.cpu.read_reg(Register::A7), SP + 26);
        self.bus.read_long(SP + 26)
    }

    fn install_cdef(&mut self) {
        self.install_cdef_with_drawing(false);
    }

    fn install_cdef_with_drawing(&mut self, draw: bool) {
        // Independently authored Pascal CDEF. Append (message, control) to the
        // fixture log, return zero and remove its 12-byte argument block.
        let words: [u16; 18] = [
            0x4E56, 0, // LINK A6,#0
            0x207C, 0x0019, 0x0040, // MOVEA.L #LOG_CURSOR,A0
            0x2250, // MOVEA.L (A0),A1
            0x32EE, 0x000C, // MOVE.W 12(A6),(A1)+
            0x22EE, 0x000E, // MOVE.L 14(A6),(A1)+
            0x2089, // MOVE.L A1,(A0)
            0x42AE, 0x0014, // CLR.L 20(A6)
            0x4E5E, 0x205F, // UNLK A6; MOVEA.L (SP)+,A0
            0x4FEF, 0x000C, 0x4ED0, // LEA 12(SP),SP; JMP (A0)
        ];
        let mut words = words.to_vec();
        if draw {
            words.splice(
                2..2,
                [
                    0x206E, 0x000E, // MOVEA.L 14(A6),A0 (control handle)
                    0x2050, // MOVEA.L (A0),A0 (ControlRecord)
                    0x4868, 0x0008, // PEA contrlRect(A0)
                    0xA8A2, // PaintRect
                ],
            );
        }
        let code: Vec<u8> = words.into_iter().flat_map(u16::to_be_bytes).collect();
        self.dispatcher
            .install_test_resource(&mut self.bus, *b"CDEF", 400, &code);
        self.reset_log();
    }

    fn reset_log(&mut self) {
        self.bus.write_long(LOG_CURSOR, LOG);
    }

    fn callbacks(&self) -> Vec<(u16, u32)> {
        (LOG..self.bus.read_long(LOG_CURSOR))
            .step_by(6)
            .map(|p| (self.bus.read_word(p), self.bus.read_long(p + 2)))
            .collect()
    }

    fn clear(&mut self) {
        self.bus.write_bytes(SCREEN, &vec![0; 64 * 342]);
    }

    fn pixels(&self) -> Vec<u8> {
        self.bus.read_bytes(SCREEN, 64 * 342)
    }

    fn draw_one(&mut self, handle: u32) {
        self.bus.write_long(SP, handle);
        self.call(0xA96D);
    }

    fn prepare_move(&mut self, handle: u32) {
        self.bus.write_word(SP, 140);
        self.bus.write_word(SP + 2, 160);
        self.bus.write_long(SP + 4, handle);
    }
}

#[test]
fn drawcontrols_overlapping_standard_controls_leave_first_created_frontmost() {
    let mut f = Fixture::new();
    let first = f.create((40, 40, 80, 120), true, false);
    let second = f.create((55, 60, 95, 140), true, false);
    f.clear();
    f.bus.write_long(SP, OWNER);
    f.call(0xA969);
    let actual = f.pixels();
    f.clear();
    f.draw_one(second);
    f.draw_one(first);
    let expected = f.pixels();
    f.clear();
    f.draw_one(first);
    f.draw_one(second);
    assert_ne!(
        expected,
        f.pixels(),
        "the overlap must distinguish drawing order"
    );
    assert!(
        actual == expected,
        "DrawControls must draw newest first, oldest last"
    );
    assert_eq!(f.bus.read_long(OWNER + 140), second);
    assert_eq!(f.bus.read_long(f.bus.read_long(second)), first);
}

#[test]
fn drawcontrols_executes_newest_first_cdefs_and_skips_hidden_controls() {
    let mut f = Fixture::new();
    f.install_cdef();
    let first = f.create((40, 40, 80, 120), true, true);
    let second = f.create((55, 60, 95, 140), true, true);
    f.create((60, 65, 100, 145), false, true);
    f.reset_log();
    f.bus.write_long(SP, OWNER);
    f.call(0xA969);
    assert_eq!(f.callbacks(), [(0, second), (0, first)]);
    assert_eq!(f.cpu.read_reg(Register::A7), SP + 4);
}

#[test]
fn movecontrol_redraws_visible_standard_control_in_owner_clip() {
    let mut f = Fixture::new();
    let control = f.create((40, 40, 80, 120), true, false);
    // Clip the new control's right half, then make another port current.
    let clip = f.bus.read_long(f.bus.read_long(OWNER + 28));
    f.bus.write_word(clip + 8, 200);
    f.bus.write_long(SP, OTHER_PORT);
    f.call(0xA873);
    f.prepare_move(control);
    f.call(0xA959);
    let actual = f.pixels();
    assert_eq!(f.cpu.read_reg(Register::A7), SP + 8);
    assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
    let ptr = f.bus.read_long(control);
    assert_eq!(
        (0..4)
            .map(|i| f.bus.read_word(ptr + 8 + i * 2))
            .collect::<Vec<_>>(),
        [140, 160, 180, 240]
    );
    f.clear();
    f.draw_one(control);
    let expected = f.pixels();
    assert!(
        expected.iter().any(|&p| p != 0),
        "the moved control must be visible"
    );
    assert!(
        actual == expected,
        "MoveControl must erase the old position and draw at the new one"
    );
    assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
}

#[test]
fn movecontrol_hidden_standard_control_changes_geometry_without_pixels() {
    let mut f = Fixture::new();
    let control = f.create((40, 40, 80, 120), false, false);
    f.bus.write_bytes(SCREEN, &vec![0xAA; 64 * 342]);
    let before = f.pixels();
    f.prepare_move(control);
    f.call(0xA959);
    assert!(
        before == f.pixels(),
        "moving a hidden control must not erase or draw"
    );
    let ptr = f.bus.read_long(control);
    assert_eq!(f.bus.read_byte(ptr + 16), 0);
    assert_eq!(f.bus.read_word(ptr + 8), 140);
    assert_eq!(f.cpu.read_reg(Register::A7), SP + 8);
}

#[test]
fn movecontrol_executes_cdef_and_restores_caller_registers_port_and_device() {
    let mut f = Fixture::new();
    f.install_cdef();
    let control = f.create((40, 40, 80, 120), true, true);
    f.bus.write_long(SP, OTHER_PORT);
    f.call(0xA873);
    let saved_device = *f.dispatcher.current_gdevice;
    let registers = [
        Register::D0,
        Register::D1,
        Register::D2,
        Register::D3,
        Register::A0,
        Register::A1,
        Register::A2,
        Register::A3,
        Register::A6,
    ];
    for (index, reg) in registers.into_iter().enumerate() {
        f.cpu.write_reg(reg, 0x12340000 + index as u32);
    }
    f.reset_log();
    f.prepare_move(control);
    f.dispatch(0xA959);
    assert_eq!(
        *f.dispatcher.current_port, OWNER,
        "CDEF must run in its owner port"
    );
    f.finish_callback();
    assert_eq!(f.callbacks(), [(0, control)]);
    assert_eq!(f.cpu.read_reg(Register::A7), SP + 8);
    assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
    assert_eq!(*f.dispatcher.current_gdevice, saved_device);
    for (index, reg) in registers.into_iter().enumerate() {
        assert_eq!(f.cpu.read_reg(reg), 0x12340000 + index as u32, "{reg:?}");
    }
    assert_eq!(f.bus.read_word(f.bus.read_long(control) + 8), 140);
    f.bus.write_byte(f.bus.read_long(control) + 16, 0);
    f.reset_log();
    f.prepare_move(control);
    f.call(0xA959);
    assert!(
        f.callbacks().is_empty(),
        "hidden controls must not invoke their CDEF"
    );
}

#[test]
fn drawcontrols_interleaves_standard_controls_and_executed_cdefs_in_list_order() {
    for custom_first in [false, true] {
        let mut f = Fixture::new();
        f.install_cdef_with_drawing(true);
        let first = f.create((40, 40, 80, 120), true, custom_first);
        let second = f.create((55, 60, 95, 140), true, !custom_first);
        f.clear();
        f.bus.write_long(SP, OWNER);
        f.call(0xA969);
        let actual = f.pixels();
        f.clear();
        f.draw_one(second);
        f.draw_one(first);
        let expected = f.pixels();
        f.clear();
        f.draw_one(first);
        f.draw_one(second);
        assert_ne!(
            expected,
            f.pixels(),
            "mixed controls must distinguish ordering"
        );
        assert!(
            actual == expected,
            "mixed standard/custom order, custom_first={custom_first}"
        );
    }
}

#[test]
fn movecontrol_erasure_respects_owner_visibility_and_clip() {
    let mut f = Fixture::new();
    f.install_cdef();
    let control = f.create((40, 40, 80, 120), true, true);
    let clip = f.bus.read_long(f.bus.read_long(OWNER + 28));
    f.bus.write_word(clip + 8, 80);
    let visible = f.bus.read_long(f.bus.read_long(OWNER + 24));
    f.bus.write_word(visible + 6, 60);
    f.bus.write_bytes(SCREEN, &vec![0xFF; 64 * 342]);
    f.bus.write_long(SP, OTHER_PORT);
    f.call(0xA873);
    f.reset_log();
    f.prepare_move(control);
    f.call(0xA959);
    let actual = f.pixels();
    for y in 0..342 {
        for x in 0..512 {
            let erased = (40..60).contains(&y) && (40..80).contains(&x);
            let pixel = (actual[y * 64 + x / 8] >> (7 - x % 8)) & 1;
            assert_eq!(pixel, u8::from(!erased), "unexpected erase at ({x}, {y})");
        }
    }
    assert_eq!(f.callbacks(), [(0, control)]);
    assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
}

#[test]
fn nested_movecontrol_cdef_preserves_outer_callback_and_remaining_draw_chain() {
    const TARGET: u32 = 0x190060;
    for draw_all in [false, true] {
        let mut f = Fixture::new();
        // The outer CDEF moves another custom control once. Log each completed
        // callback after its nested MoveControl returns, then return Pascal.
        let words: [u16; 31] = [
            0x4E56, 0, // LINK A6,#0
            0x2039, 0x0019, 0x0060, // MOVE.L TARGET,D0
            0x6712, // BEQ.S past MoveControl
            0x42B9, 0x0019, 0x0060, // CLR.L TARGET (one-shot guard)
            0x2F00, // MOVE.L D0,-(SP)
            0x3F3C, 160, 0x3F3C, 140,    // push h, v
            0xA959, // MoveControl
            0x207C, 0x0019, 0x0040, // MOVEA.L #LOG_CURSOR,A0
            0x2250, // MOVEA.L (A0),A1
            0x32EE, 0x000C, 0x22EE, 0x000E, // log message, control
            0x2089, // MOVE.L A1,(A0)
            0x42AE, 0x0014, // CLR.L result(A6)
            0x4E5E, 0x205F, 0x4FEF, 0x000C, 0x4ED0, // Pascal return
        ];
        let code: Vec<u8> = words.into_iter().flat_map(u16::to_be_bytes).collect();
        f.reset_log();
        f.dispatcher
            .install_test_resource(&mut f.bus, *b"CDEF", 400, &code);
        let first = f.create((40, 40, 80, 120), true, true);
        let second = f.create((55, 60, 95, 140), true, true);
        f.bus.write_long(SP, OTHER_PORT);
        f.call(0xA873);
        let saved_device = *f.dispatcher.current_gdevice;
        for _ in 0..3 {
            f.reset_log();
            f.bus.write_long(TARGET, second);
            f.cpu.write_reg(Register::D0, 0x12345678);
            f.cpu.write_reg(Register::A0, 0x23456789);
            f.cpu.write_reg(Register::A6, 0x34567890);
            if draw_all {
                f.bus.write_long(SP, OWNER);
                f.call(0xA969);
                assert_eq!(f.callbacks(), [(0, second), (0, second), (0, first)]);
            } else {
                f.draw_one(first);
                assert_eq!(f.callbacks(), [(0, second), (0, first)]);
            }
            assert_eq!(f.cpu.read_reg(Register::A7), SP + 4);
            assert_eq!(f.cpu.read_reg(Register::D0), 0x12345678);
            assert_eq!(f.cpu.read_reg(Register::A0), 0x23456789);
            assert_eq!(f.cpu.read_reg(Register::A6), 0x34567890);
            assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
            assert_eq!(*f.dispatcher.current_gdevice, saved_device);
            assert!(f.dispatcher.control_callback_stack.is_empty());
            assert_eq!(f.dispatcher.control_def_trampoline_chain.len(), 2);
        }
    }
}

#[test]
fn nested_movecontrol_cdef_preserves_control_action_callback() {
    let mut f = Fixture::new();
    f.install_cdef();
    let control = f.create((40, 40, 80, 120), true, true);
    let action = f.bus.alloc(32);
    let words = [
        0x4E56,
        0, // LINK A6,#0
        0x2F3C,
        (control >> 16) as u16,
        control as u16, // push control
        0x3F3C,
        160,
        0x3F3C,
        140,    // push h, v
        0xA959, // MoveControl
        0x4E5E,
        0x205F,
        0x4FEF,
        6,
        0x4ED0, // Pascal procedure return
    ];
    for (index, word) in words.into_iter().enumerate() {
        f.bus.write_word(action + index as u32 * 2, word);
    }
    f.bus.write_long(SP, OTHER_PORT);
    f.call(0xA873);
    for _ in 0..3 {
        f.reset_log();
        f.cpu.write_reg(Register::D0, 0x12345678);
        f.cpu.write_reg(Register::A0, 0x23456789);
        assert!(f.dispatcher.arm_control_action_proc(
            &mut f.cpu,
            &mut f.bus,
            control,
            20,
            action,
            SP + 4,
            RETURN_PC,
        ));
        f.finish_callback();
        assert_eq!(f.callbacks(), [(0, control)]);
        assert_eq!(f.cpu.read_reg(Register::A7), SP + 4);
        assert_eq!(f.cpu.read_reg(Register::D0), 0x12345678);
        assert_eq!(f.cpu.read_reg(Register::A0), 0x23456789);
        assert_eq!(*f.dispatcher.current_port, OTHER_PORT);
        assert!(f.dispatcher.control_callback_stack.is_empty());
        assert_eq!(f.dispatcher.control_def_trampoline_chain.len(), 2);
    }
}

#[test]
fn control_callback_return_requires_registered_pc_and_stack_slot() {
    let mut f = Fixture::new();
    f.install_cdef();
    let control = f.create((40, 40, 80, 120), true, true);
    f.bus.write_long(SP, control);
    f.dispatch(0xA96D);
    let frame = f.dispatcher.control_callback_stack.last().unwrap();
    let (return_pc, return_slot) = (frame.return_trap_pc, frame.return_slot);
    for (pc, sp) in [(return_pc + 2, return_slot), (return_pc, return_slot + 4)] {
        f.cpu.write_reg(Register::PC, pc);
        f.cpu.write_reg(Register::A7, sp);
        f.cpu.write_reg(Register::D0, 0xFFFF); // not an ordinary selector
        assert!(!f
            .dispatcher
            .complete_control_callback_return(0xAA73, &mut f.cpu, &mut f.bus,));
        assert_eq!(f.dispatcher.control_callback_stack.len(), 1);
        assert_eq!(f.cpu.read_reg(Register::PC), pc);
        assert_eq!(f.cpu.read_reg(Register::A7), sp);
    }
}

#[test]
fn control_callback_return_bypasses_patched_control_dispatch_only_for_private_return() {
    let mut f = Fixture::new();
    f.install_cdef();
    let control = f.create((40, 40, 80, 120), true, true);
    let patch = f.bus.alloc(8);
    let entered = f.bus.alloc(4);
    f.bus.write_word(patch, 0x52B9); // ADDQ.L #1,entered
    f.bus.write_long(patch + 2, entered);
    f.bus.write_word(patch + 6, 0x4E75); // RTS
    f.dispatcher
        .install_trap_address(&mut f.bus, 0xAA73, patch)
        .unwrap();
    f.reset_log();
    f.draw_one(control);
    assert_eq!(f.callbacks(), [(0, control)]);
    assert_eq!(f.bus.read_long(entered), 0);
    assert!(f.dispatcher.control_callback_stack.is_empty());
    assert_eq!(f.cpu.read_reg(Register::A7), SP + 4);
    f.call(0xAA73);
    assert_eq!(
        f.bus.read_long(entered),
        1,
        "public calls must still enter the patch"
    );
    assert_eq!(f.cpu.read_reg(Register::A7), SP);
}
