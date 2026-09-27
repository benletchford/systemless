use super::super::test_helpers::{setup, setup_with_port};
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::trap::menu::{Menu, MenuItem};
use crate::trap::TrapDispatcher;
use crate::ui_theme::UiThemeId;

fn build_cntl_resource(
    rect: (i16, i16, i16, i16),
    value: i16,
    visible: bool,
    min: i16,
    max: i16,
    proc_id: i16,
    ref_con: u32,
    title: &[u8],
) -> Vec<u8> {
    let mut data = Vec::with_capacity(23 + title.len());
    data.extend_from_slice(&(rect.0 as u16).to_be_bytes());
    data.extend_from_slice(&(rect.1 as u16).to_be_bytes());
    data.extend_from_slice(&(rect.2 as u16).to_be_bytes());
    data.extend_from_slice(&(rect.3 as u16).to_be_bytes());
    data.extend_from_slice(&(value as u16).to_be_bytes());
    data.extend_from_slice(&(if visible { 0xFFFFu16 } else { 0 }).to_be_bytes());
    data.extend_from_slice(&(max as u16).to_be_bytes());
    data.extend_from_slice(&(min as u16).to_be_bytes());
    data.extend_from_slice(&(proc_id as u16).to_be_bytes());
    data.extend_from_slice(&ref_con.to_be_bytes());
    let len = title.len().min(255);
    data.push(len as u8);
    data.extend_from_slice(&title[..len]);
    data
}

fn fill_rect_pict_resource(
    frame: (i16, i16, i16, i16),
    fill_rect: (i16, i16, i16, i16),
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0u16.to_be_bytes()); // picSize placeholder
    for value in [frame.0, frame.1, frame.2, frame.3] {
        data.extend_from_slice(&(value as u16).to_be_bytes());
    }
    data.push(0x11); // versionOp
    data.push(0x01); // PICT v1
    data.push(0x0A); // FillPat
    data.extend_from_slice(&[0xFF; 8]);
    data.push(0x34); // fillRect
    for value in [fill_rect.0, fill_rect.1, fill_rect.2, fill_rect.3] {
        data.extend_from_slice(&(value as u16).to_be_bytes());
    }
    data.push(0xFF); // EndOfPicture
    let size = data.len() as u16;
    data[0..2].copy_from_slice(&size.to_be_bytes());
    data
}

fn alloc_control_handle(
    bus: &mut MacMemoryBus,
    rect: (i16, i16, i16, i16),
    vis: u8,
    hilite: u8,
) -> (u32, u32) {
    let ctrl_ptr = bus.alloc(40);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    bus.write_word(ctrl_ptr + 8, rect.0 as u16);
    bus.write_word(ctrl_ptr + 10, rect.1 as u16);
    bus.write_word(ctrl_ptr + 12, rect.2 as u16);
    bus.write_word(ctrl_ptr + 14, rect.3 as u16);
    bus.write_byte(ctrl_ptr + 16, vis);
    bus.write_byte(ctrl_ptr + 17, hilite);
    (ctrl_handle, ctrl_ptr)
}

fn alloc_scrollbar_control(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut MacMemoryBus,
    rect: (i16, i16, i16, i16),
    vis: u8,
    hilite: u8,
    value: i16,
    min: i16,
    max: i16,
) -> (u32, u32) {
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(bus, rect, vis, hilite);
    bus.write_word(ctrl_ptr + 18, value as u16);
    bus.write_word(ctrl_ptr + 20, min as u16);
    bus.write_word(ctrl_ptr + 22, max as u16);
    disp.control_manager.set_proc_id(ctrl_ptr, 16);
    (ctrl_handle, ctrl_ptr)
}

fn new_control_handle<C: CpuOps>(
    disp: &mut TrapDispatcher,
    cpu: &mut C,
    bus: &mut MacMemoryBus,
    window_ptr: u32,
    visible: bool,
    proc_id: i16,
) -> u32 {
    let sp = 0x300000;
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(16);

    bus.write_word(bounds_ptr, 10);
    bus.write_word(bounds_ptr + 2, 20);
    bus.write_word(bounds_ptr + 4, 30);
    bus.write_word(bounds_ptr + 6, 80);
    bus.write_byte(title_ptr, 4);
    bus.write_bytes(title_ptr + 1, b"Aux!");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x1234_5678);
    bus.write_word(sp + 4, proc_id as u16);
    bus.write_word(sp + 6, 99);
    bus.write_word(sp + 8, 1);
    bus.write_word(sp + 10, 7);
    bus.write_byte(sp + 12, if visible { 1 } else { 0 });
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, window_ptr);

    let result = disp.dispatch_control(true, 0x154, cpu, bus);
    assert!(result.unwrap().is_ok());
    bus.read_long(cpu.read_reg(Register::A7))
}

/// IM:I I-331 declares `NewControl`'s `visible` as a `BOOLEAN`, which is
/// one byte in a word-aligned stack slot. MPW C writes the flag in the
/// high byte; Dark Castle's build writes it in the low byte and zeroes
/// the high one, so a fixed-half read makes every one of its controls
/// invisible and none of them draw.
#[test]
fn newcontrol_accepts_a_visible_flag_in_either_byte() {
    for (label, word) in [
        ("high byte (MPW C)", 0xFF00u16),
        ("low byte (Dark Castle)", 0x00FFu16),
        ("both bytes", 0xFFFFu16),
    ] {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = 0x300000u32;
        let window_ptr = bus.alloc(200);
        let bounds_ptr = bus.alloc(8);
        for (i, v) in [276i16, 20, 292, 87].iter().enumerate() {
            bus.write_word(bounds_ptr + (i as u32) * 2, *v as u16);
        }
        let title_ptr = bus.alloc(16);
        bus.write_byte(title_ptr, 4);
        bus.write_bytes(title_ptr + 1, b"Play");

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, 0); // pushButProc
        bus.write_word(sp + 6, 1);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, word);
        bus.write_long(sp + 14, title_ptr);
        bus.write_long(sp + 18, bounds_ptr);
        bus.write_long(sp + 22, window_ptr);

        disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
            .expect("NewControl handled")
            .expect("NewControl returns");

        let handle = bus.read_long(sp + 26);
        let ctrl_ptr = bus.read_long(handle);
        assert_eq!(
            bus.read_byte(ctrl_ptr + 16),
            255,
            "contrlVis must be 255 for a TRUE visible flag in the {} slot",
            label
        );
    }

    // A genuinely FALSE flag stays invisible.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(4);
    bus.write_byte(title_ptr, 0);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 0);
    bus.write_word(sp + 6, 1);
    bus.write_word(sp + 8, 0);
    bus.write_word(sp + 10, 0);
    bus.write_word(sp + 12, 0x0000);
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, 0);
    disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
        .expect("NewControl handled")
        .expect("NewControl returns");
    let handle = bus.read_long(sp + 26);
    let ctrl_ptr = bus.read_long(handle);
    assert_eq!(bus.read_byte(ctrl_ptr + 16), 0, "FALSE stays invisible");
}

fn make_control_ctab_handle(bus: &mut MacMemoryBus) -> u32 {
    let ctab_ptr = bus.alloc(8 + 4 * 8);
    let ctab_handle = bus.alloc(4);
    bus.write_long(ctab_handle, ctab_ptr);
    bus.write_long(ctab_ptr, 0); // ccSeed
    bus.write_word(ctab_ptr + 4, 0); // ccRider
    bus.write_word(ctab_ptr + 6, 3); // ctSize (4 ColorSpec entries)
    for (index, part_id) in [0u16, 1, 2, 3].iter().enumerate() {
        let entry = ctab_ptr + 8 + index as u32 * 8;
        bus.write_word(entry, *part_id);
        bus.write_word(entry + 2, 0x1111u16.wrapping_mul(index as u16 + 1));
        bus.write_word(entry + 4, 0x0101u16.wrapping_mul(index as u16 + 2));
        bus.write_word(entry + 6, 0x0202u16.wrapping_mul(index as u16 + 3));
    }
    ctab_handle
}

#[test]
fn new_control_initializes_record_and_links_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000;
    let window_ptr = bus.alloc(200);
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(16);

    bus.write_word(bounds_ptr, 10);
    bus.write_word(bounds_ptr + 2, 20);
    bus.write_word(bounds_ptr + 4, 30);
    bus.write_word(bounds_ptr + 6, 80);
    bus.write_byte(title_ptr, 4);
    bus.write_bytes(title_ptr + 1, b"Test");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x1234_5678);
    bus.write_word(sp + 4, 0);
    bus.write_word(sp + 6, 99);
    bus.write_word(sp + 8, 1);
    bus.write_word(sp + 10, 7);
    // Pascal BOOLEAN at SP+12 — value goes in HIGH byte (MPW C convention).
    bus.write_byte(sp + 12, 1);
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, window_ptr);

    let result = disp.dispatch_control(true, 0x154, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let handle = bus.read_long(sp + 26);
    let ctrl_ptr = bus.read_long(handle);
    assert_ne!(handle, 0);
    assert_ne!(ctrl_ptr, 0);
    assert_eq!(bus.read_long(window_ptr + 140), handle);
    assert_eq!(bus.read_long(ctrl_ptr + 4), window_ptr);
    assert_eq!(bus.read_word(ctrl_ptr + 8), 10);
    assert_eq!(bus.read_word(ctrl_ptr + 10), 20);
    assert_eq!(bus.read_word(ctrl_ptr + 12), 30);
    assert_eq!(bus.read_word(ctrl_ptr + 14), 80);
    assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
    assert_eq!(bus.read_word(ctrl_ptr + 18), 7);
    assert_eq!(bus.read_word(ctrl_ptr + 20), 1);
    assert_eq!(bus.read_word(ctrl_ptr + 22), 99);
    assert_eq!(bus.read_long(ctrl_ptr + 36), 0x1234_5678);
    assert_eq!(bus.read_byte(ctrl_ptr + 40), 4);
    assert_eq!(bus.read_bytes(ctrl_ptr + 41, 4), b"Test");
    assert_eq!(cpu.read_reg(Register::A7), sp + 26);
}

#[test]
fn newcontrol_creates_aux_record_and_getauxctl_returns_true() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_ptr = bus.alloc(200);
    let ctrl_handle = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);

    let aux_state = disp
        .control_aux_state(ctrl_handle)
        .expect("fresh control should have an AuxCtlRec on BasiliskII System 7.5.3");
    let aux_ptr = bus.read_long(aux_state.handle);
    assert_ne!(aux_ptr, 0, "AuxCtlHandle must master a live AuxCtlRec");
    assert_eq!(
        bus.read_long(aux_ptr + 4),
        ctrl_handle,
        "acOwner must point back to the control handle"
    );
    assert_ne!(
        bus.read_long(aux_ptr + 8),
        0,
        "fresh controls should already carry a non-NIL acCTable"
    );
    assert_eq!(
        bus.read_long(0x0CD4),
        aux_state.handle,
        "AuxCtlHead low-memory global should point at the fresh control's aux record"
    );

    let aux_out = bus.alloc(4);
    let sp = 0x300100;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, aux_out);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xFFFF);

    let result = disp.dispatch_quickdraw(true, 0x244, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(
        bus.read_long(aux_out),
        aux_state.handle,
        "GetAuxCtl should write the tracked AuxCtlHandle for fresh controls"
    );
    assert_eq!(
        bus.read_word(sp + 8),
        0x0100,
        "GetAuxCtl should return TRUE for fresh tracked controls"
    );
}

#[test]
fn setctlcolor_reuses_aux_record_and_getauxctl_reports_custom_colors() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_ptr = bus.alloc(200);
    let ctrl_handle = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);
    let aux_before = disp
        .control_aux_state(ctrl_handle)
        .expect("fresh control should already have an AuxCtlRec");
    let custom_ctab = make_control_ctab_handle(&mut bus);

    let sp = 0x300140;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, custom_ctab);
    bus.write_long(sp + 4, ctrl_handle);

    let result = disp.dispatch_control(true, 0x243, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);

    let aux_after = disp
        .control_aux_state(ctrl_handle)
        .expect("SetCtlColor should keep the AuxCtlRec alive");
    assert_eq!(
        aux_after.handle, aux_before.handle,
        "SetCtlColor should rewrite the existing AuxCtlRec instead of allocating a new one"
    );
    let aux_ptr = bus.read_long(aux_after.handle);
    assert_eq!(
        bus.read_long(aux_ptr + 8),
        custom_ctab,
        "SetCtlColor should rewrite acCTable to the caller-supplied CCTabHandle"
    );

    let aux_out = bus.alloc(4);
    let get_sp = 0x300180;
    cpu.write_reg(Register::A7, get_sp);
    bus.write_long(get_sp, aux_out);
    bus.write_long(get_sp + 4, ctrl_handle);
    bus.write_word(get_sp + 8, 0);

    let get_result = disp.dispatch_quickdraw(true, 0x244, &mut cpu, &mut bus);
    assert!(get_result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), get_sp + 8);
    assert_eq!(bus.read_long(aux_out), aux_after.handle);
    assert_eq!(
        bus.read_word(get_sp + 8),
        0x0100,
        "GetAuxCtl should return TRUE after SetCtlColor installs custom colors"
    );
}

#[test]
fn disposecontrol_releases_aux_record_and_repairs_auxctl_chain() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_ptr = bus.alloc(200);
    let first = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 0);
    let second = new_control_handle(&mut disp, &mut cpu, &mut bus, window_ptr, false, 1);
    let first_ctab = make_control_ctab_handle(&mut bus);
    let second_ctab = make_control_ctab_handle(&mut bus);

    for (ctrl_handle, ctab_handle) in [(first, first_ctab), (second, second_ctab)] {
        let sp = if ctrl_handle == first {
            0x300180
        } else {
            0x3001A0
        };
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, ctab_handle);
        bus.write_long(sp + 4, ctrl_handle);
        let result = disp.dispatch_control(true, 0x243, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
    }

    let first_state = disp
        .control_aux_state(first)
        .expect("first control aux record");
    let second_state = disp
        .control_aux_state(second)
        .expect("second control aux record");
    let first_aux_ptr = bus.read_long(first_state.handle);
    let second_aux_ptr = bus.read_long(second_state.handle);
    assert_eq!(
        bus.read_long(second_aux_ptr),
        first_state.handle,
        "second AuxCtlRec should link to the first via acNext"
    );

    let sp = 0x3001C0;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, first);

    let result = disp.dispatch_control(true, 0x155, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert!(
        disp.control_aux_state(first).is_none(),
        "DisposeControl should drop the released control from the aux-state map"
    );
    assert_eq!(
        bus.read_long(0x0CD4),
        second_state.handle,
        "AuxCtlHead should remain pointed at the surviving control's aux record"
    );
    assert_eq!(
        bus.read_long(second_aux_ptr),
        0,
        "DisposeControl should repair the acNext chain when unlinking a non-head aux record"
    );
    assert_eq!(
        bus.get_alloc_size(first_state.handle),
        None,
        "DisposeControl should free the released AuxCtlHandle"
    );
    assert_eq!(
        bus.get_alloc_size(first_aux_ptr),
        None,
        "DisposeControl should free the released AuxCtlRec"
    );
}

// IM:I I-331: NewControl takes nine arguments and returns a ControlHandle.
#[test]
fn newcontrol_consumes_arguments_and_writes_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(16);

    bus.write_word(bounds_ptr, 12);
    bus.write_word(bounds_ptr + 2, 18);
    bus.write_word(bounds_ptr + 4, 40);
    bus.write_word(bounds_ptr + 6, 90);
    bus.write_byte(title_ptr, 4);
    bus.write_bytes(title_ptr + 1, b"Play");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x1020_3040);
    bus.write_word(sp + 4, 0);
    bus.write_word(sp + 6, 99);
    bus.write_word(sp + 8, 0);
    bus.write_word(sp + 10, 1);
    // Pascal BOOLEAN high byte convention for visible = TRUE.
    bus.write_byte(sp + 12, 1);
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, window_ptr);

    disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 26);
    assert_ne!(bus.read_long(sp + 26), 0);
}

// IM:I I-331: NewControl adds the allocated control to the window's
// control list.
#[test]
fn newcontrol_prepends_new_handle_into_window_control_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(16);
    let (old_head, old_ptr) = alloc_control_handle(&mut bus, (5, 5, 15, 50), 255, 0);
    bus.write_long(old_ptr + 4, window_ptr);
    bus.write_long(old_ptr, 0);
    bus.write_long(window_ptr + 140, old_head);

    bus.write_word(bounds_ptr, 20);
    bus.write_word(bounds_ptr + 2, 24);
    bus.write_word(bounds_ptr + 4, 60);
    bus.write_word(bounds_ptr + 6, 120);
    bus.write_byte(title_ptr, 3);
    bus.write_bytes(title_ptr + 1, b"New");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xA0B0_C0D0);
    bus.write_word(sp + 4, 0);
    bus.write_word(sp + 6, 100);
    bus.write_word(sp + 8, 0);
    bus.write_word(sp + 10, 10);
    bus.write_byte(sp + 12, 1);
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, window_ptr);

    disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let new_head = bus.read_long(sp + 26);
    let new_ptr = bus.read_long(new_head);
    assert_eq!(bus.read_long(window_ptr + 140), new_head);
    assert_eq!(bus.read_long(new_ptr), old_head);
    assert_eq!(bus.read_long(new_ptr + 4), window_ptr);
}

#[test]
fn newcontrol_custom_cdef_arms_init_then_draw_pascal_callbacks() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let return_pc = 0x1234_5678;
    let window_ptr = bus.alloc(200);
    let bounds_ptr = bus.alloc(8);
    let title_ptr = bus.alloc(16);
    let proc_id = (160i16 << 4) | 5;
    let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);

    for (offset, value) in [10i16, 20, 40, 100].into_iter().enumerate() {
        bus.write_word(bounds_ptr + offset as u32 * 2, value as u16);
    }
    bus.write_byte(title_ptr, 4);
    bus.write_bytes(title_ptr + 1, b"Play");
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, proc_id as u16);
    bus.write_word(sp + 6, 1);
    bus.write_word(sp + 8, 0);
    bus.write_word(sp + 10, 0);
    bus.write_word(sp + 12, 1);
    bus.write_long(sp + 14, title_ptr);
    bus.write_long(sp + 18, bounds_ptr);
    bus.write_long(sp + 22, window_ptr);

    disp.dispatch_control(true, 0x154, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let ctrl_handle = bus.read_long(sp + 26);
    let tramp = disp.control_def_trampoline;
    assert_ne!(ctrl_handle, 0);
    assert_ne!(tramp, 0);
    assert_eq!(cpu.read_reg(Register::PC), tramp);
    assert_eq!(cpu.read_reg(Register::A7), sp + 22);
    assert_eq!(bus.read_long(sp + 22), return_pc);
    assert_eq!(bus.read_word(tramp + 12), 5);
    assert_eq!(bus.read_long(tramp + 16), ctrl_handle);
    assert_eq!(
        bus.read_word(tramp + 22),
        super::super::TrapDispatcher::CDEF_INIT_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(tramp + 26), 0);
    assert_eq!(bus.read_long(tramp + 32), cdef_proc);
    assert_eq!(bus.read_word(tramp + 54), 0x4EF9);

    let draw_tramp = bus.read_long(tramp + 56);
    assert_ne!(draw_tramp, 0);
    assert_eq!(bus.read_word(draw_tramp + 12), 5);
    assert_eq!(bus.read_long(draw_tramp + 16), ctrl_handle);
    assert_eq!(
        bus.read_word(draw_tramp + 22),
        super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(draw_tramp + 26), 0);
    assert_eq!(bus.read_long(draw_tramp + 32), cdef_proc);
    assert_eq!(bus.read_word(draw_tramp + 54), 0x2F3C);
    assert_eq!(bus.read_word(draw_tramp + 60), 0xA873);
    assert_eq!(bus.read_word(draw_tramp + 68), 0xAA31);
    assert_eq!(bus.read_word(draw_tramp + 70), 0x4E75);
}

#[test]
fn high_bit_control_definition_id_loads_and_dispatches_unsigned_cdef_resource() {
    // Macintosh Toolbox Essentials (1992), pp. 5-12 to 5-13: the upper
    // 12 bits of the control definition ID are the CDEF resource ID.
    let (mut disp, mut cpu, mut bus) = setup();
    let cdef_id = 0x0800i16;
    let variant = 0x000B;
    let proc_id = ((cdef_id as u16) << 4 | variant) as i16;
    let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", cdef_id, &[0x4E, 0x56, 0, 0]);
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    disp.initialize_control_record(
        &mut bus,
        ctrl_ptr,
        0,
        (10, 20, 40, 100),
        b"High",
        true,
        0,
        0,
        1,
        proc_id,
        0,
    );

    let cdef_handle = bus.read_long(ctrl_ptr + 24);
    assert_ne!(cdef_handle, 0);
    assert_eq!(bus.read_long(cdef_handle), cdef_proc);

    let sp = 0x300000u32;
    let return_pc = 0x1234_5678;
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let trampoline = disp.control_def_trampoline;
    assert_eq!(cpu.read_reg(Register::PC), trampoline);
    assert_eq!(bus.read_word(trampoline + 12), variant);
    assert_eq!(bus.read_long(trampoline + 16), ctrl_handle);
    assert_eq!(bus.read_long(trampoline + 32), cdef_proc);
}

// IM:I I-332: DisposeControl takes one ControlHandle argument.
#[test]
fn disposecontrol_consumes_control_handle_argument() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x155, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

// IM:I I-332: DisposeControl removes the control from the window's
// control list.
#[test]
fn disposecontrol_unlinks_control_from_owner_window_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let (tail_handle, tail_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    let (head_handle, head_ptr) = alloc_control_handle(&mut bus, (12, 24, 36, 72), 255, 0);
    bus.write_long(tail_ptr + 4, window_ptr);
    bus.write_long(head_ptr + 4, window_ptr);
    bus.write_long(tail_ptr, 0);
    bus.write_long(head_ptr, tail_handle);
    bus.write_long(window_ptr + 140, head_handle);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, head_handle);
    disp.dispatch_control(true, 0x155, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(window_ptr + 140), tail_handle);
    assert_eq!(bus.read_long(tail_ptr), 0);
}

// IM:I I-332: KillControls takes one WindowPtr argument.
#[test]
fn killcontrols_consumes_window_argument() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    disp.dispatch_control(true, 0x156, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

// IM:I I-332: KillControls disposes every control owned by the window.
#[test]
fn killcontrols_clears_window_control_list_head() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let (tail_handle, tail_ptr) = alloc_control_handle(&mut bus, (8, 12, 24, 80), 255, 0);
    let (head_handle, head_ptr) = alloc_control_handle(&mut bus, (16, 20, 36, 100), 255, 0);
    bus.write_long(tail_ptr + 4, window_ptr);
    bus.write_long(head_ptr + 4, window_ptr);
    bus.write_long(tail_ptr, 0);
    bus.write_long(head_ptr, tail_handle);
    bus.write_long(window_ptr + 140, head_handle);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    disp.dispatch_control(true, 0x156, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(window_ptr + 140), 0);
}

// IM:I I-336: SetCtlAction takes actionProc and control-handle arguments.
#[test]
fn setctlaction_consumes_control_and_action_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x00AA_BBCC);
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x16B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

// IM:I I-336: SetCtlAction stores the control's action procedure pointer.
#[test]
fn setctlaction_updates_control_action_procptr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x0012_3456);
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x16B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(ctrl_ptr + 32), 0x0012_3456);
}

// IM:I I-336: GetCtlAction takes one ControlHandle argument and returns
// the action ProcPtr.
#[test]
fn getctlaction_consumes_control_argument_and_writes_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    bus.write_long(ctrl_ptr + 32, 0x00AB_CDEF);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_long(sp + 4), 0x00AB_CDEF);
}

// IM:I I-336: GetCtlAction returns the action procedure currently stored
// in the control record.
#[test]
fn getctlaction_returns_control_action_procptr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    bus.write_long(ctrl_ptr + 32, 0x00FE_DCBA);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(sp + 4), 0x00FE_DCBA);
}

#[test]
fn getnewcontrol_parses_cntl_resource_and_links_at_window_head() {
    // IM:I (1985) pp. I-321 and I-333: GetNewControl copies CNTL fields
    // into a control record and links it into the window's control list.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let old_ctrl_ptr = bus.alloc(40);
    let old_ctrl_handle = bus.alloc(4);
    bus.write_long(old_ctrl_handle, old_ctrl_ptr);
    bus.write_long(window_ptr + 140, old_ctrl_handle);

    let cntl = build_cntl_resource((10, 20, 40, 100), 7, true, 2, 99, 0, 0x1234_5678, b"Play");
    disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    bus.write_word(sp + 4, 128);
    bus.write_long(sp + 6, 0xDEAD_BEEF);
    disp.dispatch_control(true, 0x1BE, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let handle = bus.read_long(sp + 6);
    let ctrl_ptr = bus.read_long(handle);
    assert_ne!(handle, 0);
    assert_ne!(ctrl_ptr, 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert_eq!(bus.read_long(window_ptr + 140), handle);
    assert_eq!(bus.read_long(ctrl_ptr), old_ctrl_handle);
    assert_eq!(bus.read_long(ctrl_ptr + 4), window_ptr);
    assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 10);
    assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 20);
    assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 40);
    assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 100);
    assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 7);
    assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 2);
    assert_eq!(bus.read_word(ctrl_ptr + 22) as i16, 99);
    assert_eq!(bus.read_long(ctrl_ptr + 36), 0x1234_5678);
    assert_eq!(bus.read_byte(ctrl_ptr + 40), 4);
    assert_eq!(bus.read_bytes(ctrl_ptr + 41, 4), b"Play");
}

#[test]
fn getnewcontrol_missing_cntl_returns_nil_and_preserves_window_list() {
    // IM:I (1985) p. I-321: if the control template can't be read,
    // GetNewControl returns NIL.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let old_ctrl_ptr = bus.alloc(40);
    let old_ctrl_handle = bus.alloc(4);
    bus.write_long(old_ctrl_handle, old_ctrl_ptr);
    bus.write_long(window_ptr + 140, old_ctrl_handle);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    bus.write_word(sp + 4, 999);
    bus.write_long(sp + 6, 0xDEAD_BEEF);
    disp.dispatch_control(true, 0x1BE, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert_eq!(bus.read_long(sp + 6), 0);
    assert_eq!(bus.read_long(window_ptr + 140), old_ctrl_handle);
}

#[test]
fn control_accessors_round_trip_action_refcon_title_and_bounds() {
    let (mut disp, mut cpu, mut bus) = setup();
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    let title_ptr = bus.alloc(16);
    let out_title_ptr = bus.alloc(16);
    let sp = 0x300000;

    bus.write_long(ctrl_handle, ctrl_ptr);
    bus.write_long(ctrl_ptr + 36, 0xCAFE_BABE);
    bus.write_long(ctrl_ptr + 32, 0x0012_3456);
    bus.write_word(ctrl_ptr + 8, 10);
    bus.write_word(ctrl_ptr + 10, 20);
    bus.write_word(ctrl_ptr + 12, 30);
    bus.write_word(ctrl_ptr + 14, 60);
    bus.write_byte(ctrl_ptr + 16, 255);
    bus.write_word(ctrl_ptr + 20, 3);
    bus.write_word(ctrl_ptr + 22, 11);
    bus.write_byte(title_ptr, 3);
    bus.write_bytes(title_ptr + 1, b"EV!");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, title_ptr);
    bus.write_long(sp + 4, ctrl_handle);
    let result = disp.dispatch_control(true, 0x15F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_byte(ctrl_ptr + 40), 3);
    assert_eq!(bus.read_bytes(ctrl_ptr + 41, 3), b"EV!");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, out_title_ptr);
    bus.write_long(sp + 4, ctrl_handle);
    let result = disp.dispatch_control(true, 0x15E, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_byte(out_title_ptr), 3);
    assert_eq!(bus.read_bytes(out_title_ptr + 1, 3), b"EV!");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    let result = disp.dispatch_control(true, 0x15A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_long(sp + 4), 0xCAFE_BABE);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    let result = disp.dispatch_control(true, 0x16A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_long(sp + 4), 0x0012_3456);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 10);
    bus.write_long(sp + 2, ctrl_handle);
    let result = disp.dispatch_control(true, 0x164, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 21);
    bus.write_long(sp + 2, ctrl_handle);
    let result = disp.dispatch_control(true, 0x165, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    let result = disp.dispatch_control(true, 0x161, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 4), 10);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    let result = disp.dispatch_control(true, 0x162, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 4), 21);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 15);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, ctrl_handle);
    let result = disp.dispatch_control(true, 0x166, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 8), 10);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0);
    bus.write_word(sp + 2, 0);
    bus.write_long(sp + 4, ctrl_handle);
    let result = disp.dispatch_control(true, 0x166, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 8), 0);
}

// SizeControl/GetCTitle/SetCTitle semantics per Inside Macintosh
// Volume I (1985), pp. I-321 and I-325.
#[test]
fn sizecontrol_sets_bottom_right_from_requested_width_height_and_pops_args() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 40, 70), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 18); // h
    bus.write_word(sp + 2, 44); // w
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x15C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 10);
    assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 20);
    assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 28);
    assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 64);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn getctitle_copies_control_title_to_output_str255_and_pops_args() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    let out_title = bus.alloc(16);

    bus.write_byte(ctrl_ptr + 40, 5);
    bus.write_bytes(ctrl_ptr + 41, b"Hello");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, out_title);
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x15E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(out_title), 5);
    assert_eq!(bus.read_bytes(out_title + 1, 5), b"Hello");
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn setctitle_updates_control_title_from_input_str255_and_pops_args() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    let in_title = bus.alloc(16);

    bus.write_word(ctrl_ptr + 8, 10);
    bus.write_word(ctrl_ptr + 10, 20);
    bus.write_word(ctrl_ptr + 12, 40);
    bus.write_word(ctrl_ptr + 14, 70);
    bus.write_byte(ctrl_ptr + 16, 255);
    bus.write_byte(ctrl_ptr + 40, 3);
    bus.write_bytes(ctrl_ptr + 41, b"Old");
    bus.write_byte(in_title, 7);
    bus.write_bytes(in_title + 1, b"Options");

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, in_title);
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x15F, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(ctrl_ptr + 40), 7);
    assert_eq!(bus.read_bytes(ctrl_ptr + 41, 7), b"Options");
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

// SetCtlValue clamps to [contrlMin, contrlMax] per IM:I I-328.

#[test]
fn setctlvalue_clamps_above_max() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 20, 0);
    bus.write_word(ctrl_ptr + 22, 100);
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 500);
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 100);
}

#[test]
fn setctlvalue_clamps_below_min() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 20, 50);
    bus.write_word(ctrl_ptr + 22, 100);
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 10);
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 50);
}

#[test]
fn setctlvalue_passes_through_inrange() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 20, 0);
    bus.write_word(ctrl_ptr + 22, 100);
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 42);
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 42);
}

#[test]
fn setctlvalue_popupmenuproc_variant_does_not_clamp_to_menu_id_min() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 20, 1300); // popupMenuProc stores MENU id in min
    bus.write_word(ctrl_ptr + 22, 0);
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    disp.control_manager.set_proc_id(ctrl_ptr, 1009);
    bus.write_long(ctrl_ptr + 32, u32::MAX);
    bus.write_word(sp, 3);
    bus.write_long(sp + 2, handle);

    disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 3);
}

// SetCtlMin / SetCtlMax must bump the current value into the new range if it's outside.
#[test]
fn setctlmin_bumps_below_value_up() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 18, 5); // current value
    bus.write_word(ctrl_ptr + 20, 0); // min
    bus.write_word(ctrl_ptr + 22, 100); // max
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 20); // new min
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x164, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 20);
    assert_eq!(
        bus.read_word(ctrl_ptr + 18) as i16,
        20,
        "current value must be bumped up to the new minimum"
    );
}

#[test]
fn setctlmax_pulls_above_value_down() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 18, 80); // current value
    bus.write_word(ctrl_ptr + 20, 0); // min
    bus.write_word(ctrl_ptr + 22, 100); // max
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 50); // new max
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x165, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 22) as i16, 50);
    assert_eq!(
        bus.read_word(ctrl_ptr + 18) as i16,
        50,
        "current value must be pulled down to the new maximum"
    );
}

#[test]
fn setctlmin_inrange_value_untouched() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    let ctrl_ptr = bus.alloc(40);
    bus.write_word(ctrl_ptr + 18, 50); // current value
    bus.write_word(ctrl_ptr + 20, 0);
    bus.write_word(ctrl_ptr + 22, 100);
    let handle = bus.alloc(4);
    bus.write_long(handle, ctrl_ptr);
    bus.write_word(sp, 10); // new min, still below value
    bus.write_long(sp + 2, handle);
    disp.dispatch_control(true, 0x164, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 10);
    assert_eq!(
        bus.read_word(ctrl_ptr + 18) as i16,
        50,
        "current value in range must stay unchanged"
    );
}

// ShowControl/HideControl/MoveControl semantics per Inside Macintosh
// Volume I (1985), pp. I-328 to I-329.
#[test]
fn showcontrol_draws_standard_control_and_pops_handle_arg() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let window = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
    let screen_base = bus.read_long(window + 2);
    let row_bytes = u32::from(bus.read_word(window + 6) & 0x3FFF);
    disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
    bus.write_long(ctrl_ptr + 4, window);
    disp.control_manager.set_proc_id(ctrl_ptr, 0);
    let before: Vec<u8> = (0..row_bytes * 342)
        .map(|offset| bus.read_byte(screen_base + offset))
        .collect();

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_ne!(
        (0..row_bytes * 342)
            .map(|offset| bus.read_byte(screen_base + offset))
            .collect::<Vec<_>>(),
        before,
        "ShowControl should immediately draw a newly visible standard control"
    );
}

#[test]
fn showcontrol_arms_application_cdef_draw_in_owner_port() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let return_pc = 0x1234_5678;
    let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
    let cdef_proc = bus.alloc(4);
    let cdef_handle = bus.alloc(4);
    bus.write_word(cdef_proc, 0x4E56); // LINK.W A6,#imm
    bus.write_long(cdef_handle, cdef_proc);
    bus.write_long(ctrl_ptr + 4, owner);
    bus.write_long(ctrl_ptr + 24, cdef_handle);
    disp.control_manager.set_proc_id(ctrl_ptr, (160 << 4) | 5);

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let trampoline = disp.control_def_trampoline;
    assert_eq!(bus.read_byte(ctrl_ptr + 16), 255);
    assert_ne!(trampoline, 0);
    assert_eq!(cpu.read_reg(Register::PC), trampoline);
    assert_eq!(bus.read_word(trampoline + 12), 5);
    assert_eq!(bus.read_long(trampoline + 16), ctrl_handle);
    assert_eq!(
        bus.read_word(trampoline + 22),
        super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(trampoline + 26), 0);
    assert_eq!(bus.read_long(trampoline + 32), cdef_proc);
    assert_eq!(*disp.current_port, owner);

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x157, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(
        cpu.read_reg(Register::PC),
        return_pc,
        "ShowControl should not redraw a control that is already visible"
    );
}

#[test]
fn hidecontrol_clears_contrlvis_and_pops_handle_arg() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x158, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(ctrl_ptr + 16), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn movecontrol_repositions_rect_preserves_size_and_pops_args() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 40, 70), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 35); // v
    bus.write_word(sp + 2, 90); // h
    bus.write_long(sp + 4, ctrl_handle);
    disp.dispatch_control(true, 0x159, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 35);
    assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 90);
    assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 65);
    assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 140);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

// HiliteControl semantics per Macintosh Toolbox Essentials 1992, p. 5-98.
#[test]
fn hilite_control_writes_contrlhilite_and_pops_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1);
    bus.write_long(sp + 2, ctrl_handle);
    disp.dispatch_control(true, 0x15D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
}

#[test]
fn hilite_control_255_marks_control_inactive() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 255);
    bus.write_long(sp + 2, ctrl_handle);
    disp.dispatch_control(true, 0x15D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_byte(ctrl_ptr + 17), 255);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
}

#[test]
fn inactive_checkbox_and_radio_titles_use_gray_device_index() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let screen_base = bus.alloc(128 * 128);
    let row_bytes = 128u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 8);
    disp.device_clut.replace([[0x2020, 0x4040, 0x6060]; 256]);
    disp.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
    disp.device_clut.set_entry(42, [0x7FFF, 0x7FFF, 0x7FFF]);
    disp.device_clut.set_entry(255, [0, 0, 0]);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);

    let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
    bus.write_long(0x08A4, gdevice_handle);
    bus.write_long(0x0CC8, gdevice_handle);
    let gdevice = bus.read_long(gdevice_handle);
    let pixmap_handle = bus.read_long(gdevice + 22);
    let pixmap = bus.read_long(pixmap_handle);
    let ctab_handle = bus.read_long(pixmap + 42);
    let ctab = bus.read_long(ctab_handle);
    for index in 0u32..256 {
        let entry = ctab + 8 + index * 8;
        bus.write_word(entry, index as u16);
        bus.write_word(entry + 2, 0x2020);
        bus.write_word(entry + 4, 0x4040);
        bus.write_word(entry + 6, 0x6060);
    }
    let white_entry = ctab + 8;
    bus.write_word(white_entry, 0);
    bus.write_word(white_entry + 2, 0xFFFF);
    bus.write_word(white_entry + 4, 0xFFFF);
    bus.write_word(white_entry + 6, 0xFFFF);
    // Deliberately put the exact gray at a different GDevice-table index.
    // The inactive title draw path should resolve against device_clut,
    // matching screen export, not this current-GDevice table.
    let gray_entry = ctab + 8 + 17 * 8;
    bus.write_word(gray_entry, 17);
    bus.write_word(gray_entry + 2, 0x7FFF);
    bus.write_word(gray_entry + 4, 0x7FFF);
    bus.write_word(gray_entry + 6, 0x7FFF);
    let black_entry = ctab + 8 + 255 * 8;
    bus.write_word(black_entry, 255);
    bus.write_word(black_entry + 2, 0);
    bus.write_word(black_entry + 4, 0);
    bus.write_word(black_entry + 6, 0);

    for offset in 0..(row_bytes * 128) {
        bus.write_byte(screen_base + offset, 0);
    }
    let window_ptr = *disp.current_port;
    bus.write_word(window_ptr + 8, 0);
    bus.write_word(window_ptr + 10, 0);
    bus.write_word(window_ptr + 16, 0);
    bus.write_word(window_ptr + 18, 0);
    bus.write_word(window_ptr + 20, 128);
    bus.write_word(window_ptr + 22, 128);

    let mut control_ptrs = Vec::new();
    for (proc_id, rect, value) in [(1, (20, 20, 40, 118), 1), (2, (52, 20, 72, 118), 1)] {
        let ctrl_ptr = bus.alloc(296);
        disp.initialize_control_record(
            &mut bus, ctrl_ptr, window_ptr, rect, b"Sound", true, value, 0, 1, proc_id, 0,
        );
        bus.write_byte(ctrl_ptr + 17, 255);
        disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
        control_ptrs.push(ctrl_ptr);
    }

    let checkbox_gray = count_pixel_index(&bus, screen_base, row_bytes, 20, 35, 40, 118, 42);
    let checkbox_black = count_pixel_index(&bus, screen_base, row_bytes, 20, 35, 40, 118, 255);
    let radio_gray = count_pixel_index(&bus, screen_base, row_bytes, 52, 35, 72, 118, 42);
    let radio_black = count_pixel_index(&bus, screen_base, row_bytes, 52, 35, 72, 118, 255);

    assert!(
        checkbox_gray > 12,
        "inactive checkbox title should draw with the device gray index"
    );
    assert_eq!(
        checkbox_black, 0,
        "inactive checkbox title must not leave black glyph pixels"
    );
    assert!(
        radio_gray > 12,
        "inactive radio title should draw with the device gray index"
    );
    assert_eq!(
        radio_black, 0,
        "inactive radio title must not leave black glyph pixels"
    );

    // A custom palette with only neutral white/black endpoints still
    // maps grayishTextOr's blend to a visible tinted intermediate entry.
    disp.device_clut.replace([[0x2020, 0x4040, 0x6060]; 256]);
    disp.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
    disp.device_clut.set_entry(255, [0, 0, 0]);
    for offset in 0..(row_bytes * 128) {
        bus.write_byte(screen_base + offset, 0);
    }
    for ctrl_ptr in control_ptrs {
        disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
    }
    for (label, top) in [("checkbox", 20), ("radio", 52)] {
        assert!(
            count_pixel_index(&bus, screen_base, row_bytes, top, 35, top + 20, 118, 1) > 12,
            "inactive {label} title should use a visible palette blend without a gray ramp"
        );
    }
}

#[test]
fn inactive_popup_label_and_selected_title_use_live_device_palette() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let screen_base = bus.alloc(256 * 128);
    let row_bytes = 256u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 256, 128, 8);
    disp.device_clut.replace([[0x2020, 0x4040, 0x6060]; 256]);
    disp.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
    disp.device_clut.set_entry(255, [0, 0, 0]);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);
    for offset in 0..(row_bytes * 128) {
        bus.write_byte(screen_base + offset, 0);
    }

    let window_ptr = *disp.current_port;
    bus.write_word(window_ptr + 8, 0);
    bus.write_word(window_ptr + 10, 0);
    bus.write_word(window_ptr + 16, 0);
    bus.write_word(window_ptr + 18, 0);
    bus.write_word(window_ptr + 20, 128);
    bus.write_word(window_ptr + 22, 256);
    let menu_ptr = bus.alloc(256);
    let menu_handle = bus.alloc(4);
    bus.write_long(menu_handle, menu_ptr);
    disp.menus.push(Menu {
        id: 900,
        title: "Files".to_string(),
        items: vec![MenuItem {
            text: "Demo Map".to_string(),
            icon: 0,
            key_equiv: 0,
            mark: 0,
            style: 0,
            enabled: true,
        }],
        enabled: true,
        handle: menu_handle,
        in_menu_bar: false,
        hierarchical: false,
        visible_in_menu_bar: false,
    });

    for (top, hilite) in [(20, 255), (52, 0)] {
        let (_handle, ctrl_ptr) = disp.create_control_record(
            &mut bus,
            window_ptr,
            (top, 20, top + 20, 220),
            b"Map:",
            true,
            1,
            900,
            60,
            1009,
            0,
        );
        assert_eq!(bus.read_word(ctrl_ptr + 20), 1);
        assert_eq!(bus.read_word(ctrl_ptr + 22), 1);
        bus.write_byte(ctrl_ptr + 17, hilite);
        assert_eq!(disp.popup_control_title_width(ctrl_ptr, 1), 60);
        disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);
    }

    assert!(
        count_pixel_index(&bus, screen_base, row_bytes, 20, 20, 40, 80, 1) > 8,
        "inactive popup label should use a visible palette blend"
    );
    assert!(
        count_pixel_index(&bus, screen_base, row_bytes, 20, 90, 40, 170, 1) > 8,
        "inactive popup selected title should use a visible palette blend"
    );
    assert!(
        count_pixel_index(&bus, screen_base, row_bytes, 52, 20, 72, 80, 255) > 8,
        "enabled popup label should retain active black ink"
    );
    assert!(
        count_pixel_index(&bus, screen_base, row_bytes, 52, 90, 72, 170, 255) > 8,
        "enabled popup selected title should retain active black ink"
    );
}

/// An active push-button title must resolve its ink against the colour
/// table the screen is displayed with, not the current GDevice table.
///
/// Games that install their own palette leave the two disagreeing. Dark
/// Castle's GDevice table calls index 1 black while the live device CLUT
/// has index 1 as pure green, so every control title was painted green.
/// The inactive-title path already resolved against `device_clut`; this
/// pins the active path to the same rule.
#[test]
fn active_button_title_uses_the_device_clut_black_index() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let screen_base = bus.alloc(128 * 128);
    let row_bytes = 128u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 8);

    // Live device CLUT: black lives at 255, and index 1 is bright green.
    disp.device_clut.replace([[0x8080, 0x8080, 0x8080]; 256]);
    disp.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
    disp.device_clut.set_entry(1, [0, 0xFFFF, 0]);
    disp.device_clut.set_entry(255, [0, 0, 0]);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);

    // GDevice table disagrees: it claims index 1 is the black one, which
    // is what the old ink lookup would have latched onto.
    let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
    bus.write_long(0x08A4, gdevice_handle);
    bus.write_long(0x0CC8, gdevice_handle);
    let gdevice = bus.read_long(gdevice_handle);
    let pixmap_handle = bus.read_long(gdevice + 22);
    let pixmap = bus.read_long(pixmap_handle);
    let ctab_handle = bus.read_long(pixmap + 42);
    let ctab = bus.read_long(ctab_handle);
    for index in 0u32..256 {
        let entry = ctab + 8 + index * 8;
        bus.write_word(entry, index as u16);
        bus.write_word(entry + 2, 0x8080);
        bus.write_word(entry + 4, 0x8080);
        bus.write_word(entry + 6, 0x8080);
    }
    let black_claim = ctab + 8 + 8;
    bus.write_word(black_claim, 1);
    bus.write_word(black_claim + 2, 0);
    bus.write_word(black_claim + 4, 0);
    bus.write_word(black_claim + 6, 0);

    for offset in 0..(row_bytes * 128) {
        bus.write_byte(screen_base + offset, 0);
    }
    let window_ptr = *disp.current_port;
    bus.write_word(window_ptr + 8, 0);
    bus.write_word(window_ptr + 10, 0);
    bus.write_word(window_ptr + 16, 0);
    bus.write_word(window_ptr + 18, 0);
    bus.write_word(window_ptr + 20, 128);
    bus.write_word(window_ptr + 22, 128);

    let rect = (20i16, 10i16, 44i16, 118i16);
    let ctrl_ptr = bus.alloc(296);
    disp.initialize_control_record(
        &mut bus, ctrl_ptr, window_ptr, rect, b"Play", true, 0, 0, 1, 0, 0,
    );
    disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);

    let black = count_pixel_index(&bus, screen_base, row_bytes, 20, 10, 44, 118, 255);
    let green = count_pixel_index(&bus, screen_base, row_bytes, 20, 10, 44, 118, 1);
    assert!(
        black > 12,
        "title glyphs should use the device CLUT's black index, got {black} px"
    );
    assert_eq!(
        green, 0,
        "title must not be painted in the GDevice table's mistaken black"
    );
}

// TrackControl semantics per Macintosh Toolbox Essentials 1992, pp. 5-89 to 5-90.
#[test]
fn track_control_visible_active_button_hit_returns_inbutton() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.enable_input_trace_capture();
    let sp = 0x300000u32;
    let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, ctrl_handle);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(sp + 12), 10);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    let trace = disp.input_trace_text();
    assert!(trace.contains("A968 action=start start=(15,25)"));
    assert!(trace.contains("part=10 highlighted_item=none outcome=visible_active_hit"));
}

#[test]
fn track_control_scrollbar_arrow_calls_action_proc_with_part_code() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let trap_pc = 0x0012_3454;
    let action_proc = bus.alloc(2);
    bus.write_word(action_proc, 0x4E75); // RTS
    let (ctrl_handle, _) =
        alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);

    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, action_proc);
    bus.write_word(sp + 4, 210);
    bus.write_word(sp + 6, 248);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);

    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(sp + 12), 21, "down arrow is inDownButton");
    let trampoline = disp.control_def_trampoline;
    assert_ne!(trampoline, 0);
    assert_eq!(cpu.read_reg(Register::PC), trampoline);
    assert_eq!(cpu.read_reg(Register::A7), sp - 4);
    assert_eq!(bus.read_long(sp - 4), trap_pc);
    assert_eq!(bus.read_word(trampoline), 0x48E7);
    assert_eq!(bus.read_word(trampoline + 4), 0x2F3C);
    assert_eq!(bus.read_long(trampoline + 6), ctrl_handle);
    assert_eq!(bus.read_word(trampoline + 10), 0x3F3C);
    assert_eq!(bus.read_word(trampoline + 12), 21);
    assert_eq!(bus.read_word(trampoline + 14), 0x4EB9);
    assert_eq!(bus.read_long(trampoline + 16), action_proc);
    assert_eq!(bus.read_word(trampoline + 30), 0x4E75);

    let tracking = disp.control_tracking.as_ref().unwrap();
    assert!(tracking.scrollbar_callback_pending);
    assert_eq!(tracking.scrollbar_part, 21);

    // Returning from the initial action callback retains TrackControl.
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(Register::A7, sp);
    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state.set_mouse_position_for_test((210, 248));
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(
        !disp
            .control_tracking
            .as_ref()
            .unwrap()
            .scrollbar_callback_pending
    );
    assert_eq!(cpu.read_reg(Register::A7), sp);

    // A held arrow repeats at the classic 20 Hz cadence.
    let next_tick = disp
        .current_tick()
        .wrapping_add(TrapDispatcher::SCROLLBAR_ACTION_REPEAT_TICKS);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    cpu.write_reg(Register::PC, trap_pc + 2);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(
        disp.control_tracking
            .as_ref()
            .unwrap()
            .scrollbar_callback_pending
    );
    assert_eq!(cpu.read_reg(Register::PC), trampoline);

    // Once that callback returns, mouse-up completes the retained trap.
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(Register::A7, sp);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    disp.input_state.set_mouse_button_for_test(false);
    cpu.write_reg(Register::PC, trap_pc + 2);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(disp.control_tracking.is_none());
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_word(sp + 12), 21);
}

#[test]
fn track_control_button_systemless_theme_tracks_pressed_state_until_release() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.enable_input_trace_capture();
    let sp = 0x300000u32;
    let window = *disp.current_port;
    let row_bytes = 64u32;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    let (ctrl_handle, ctrl_ptr) =
        alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 80));
    let probe_x = 30;
    let probe_y = 30;

    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state
        .set_mouse_position_for_test((probe_y, probe_x));
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, probe_y as u16);
    bus.write_word(sp + 6, probe_x as u16);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);

    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp,
        "simple TrackControl should defer the stack pop until mouse-up"
    );
    assert!(disp.control_tracking.is_some());
    assert_eq!(bus.read_word(sp + 12), 0xBEEF);
    assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
        "held simple TrackControl should route pressed button chrome through the provider"
    );

    disp.input_state.set_mouse_position_for_test((10, 10));
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(ctrl_ptr + 17), 0);
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
        "dragging outside should redraw unpressed provider chrome"
    );

    disp.input_state
        .set_mouse_position_for_test((probe_y, probe_x));
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(ctrl_ptr + 17), 1);
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
        "dragging back inside should restore provider pressed chrome"
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_word(sp + 12), 10);
    assert_eq!(bus.read_byte(ctrl_ptr + 17), 0);
    assert!(disp.control_tracking.is_none());
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, probe_x, probe_y),
        "release should restore unpressed provider chrome before returning"
    );
    let trace = disp.input_trace_text();
    assert!(trace.contains("outcome=simple_tracking_started"));
    assert!(trace.contains("outcome=simple_no_part"));
    assert!(trace.contains("outcome=simple_part_highlighted"));
    assert!(trace.contains("part=10 highlighted_item=none outcome=simple_part_selected"));
}

fn trackcontrol_button_release_result_for_theme(
    theme_id: UiThemeId,
    release_inside: bool,
) -> (u16, u32, u8, bool) {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(theme_id);
    let sp = 0x300000u32;
    let window = *disp.current_port;
    let screen_base = bus.read_long(window + 2);
    let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
    let (ctrl_handle, ctrl_ptr) =
        alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 80));

    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state.set_mouse_position_for_test((30, 30));
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 30);
    bus.write_word(sp + 6, 30);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    if !release_inside {
        disp.input_state.set_mouse_position_for_test((10, 10));
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
    }
    disp.input_state.set_mouse_button_for_test(false);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    (
        bus.read_word(sp + 12),
        cpu.read_reg(Register::A7),
        bus.read_byte(ctrl_ptr + 17),
        disp.control_tracking.is_some(),
    )
}

#[test]
fn systemless_theme_does_not_change_trackcontrol_part_codes() {
    // IM:I I-323: TrackControl returns the same part code on inside
    // release and 0 on outside release. Theme chrome must not alter those
    // guest-visible Control Manager results.
    let classic_inside =
        trackcontrol_button_release_result_for_theme(UiThemeId::ClassicSystem7, true);
    let themed_inside =
        trackcontrol_button_release_result_for_theme(UiThemeId::SystemlessDefault, true);
    let classic_outside =
        trackcontrol_button_release_result_for_theme(UiThemeId::ClassicSystem7, false);
    let themed_outside =
        trackcontrol_button_release_result_for_theme(UiThemeId::SystemlessDefault, false);

    assert_eq!(classic_inside, (10, 0x30000C, 0, false));
    assert_eq!(classic_outside, (0, 0x30000C, 0, false));
    assert_eq!(
        themed_inside, classic_inside,
        "systemless-default must not change TrackControl inside-release part codes"
    );
    assert_eq!(
        themed_outside, classic_outside,
        "systemless-default must not change TrackControl outside-release part codes"
    );
}

fn trackcontrol_popup_value_result_for_theme(
    theme_id: UiThemeId,
    select_second_item: bool,
) -> (u16, u32, i16, bool) {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(theme_id);
    let sp = 0x300000u32;
    let window = *disp.current_port;
    let screen_base = bus.read_long(window + 2);
    let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);

    let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
    bus.write_long(ctrl_ptr + 4, owner);
    bus.write_word(ctrl_ptr + 18, 1);
    bus.write_word(ctrl_ptr + 20, 902);
    bus.write_word(ctrl_ptr + 22, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 1009);
    bus.write_long(ctrl_ptr + 32, u32::MAX);
    disp.menus.push(Menu {
        id: 902,
        title: "Mode".to_string(),
        items: vec![
            MenuItem {
                text: "First".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
            MenuItem {
                text: "Second".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
        ],
        enabled: true,
        handle: 0,
        in_menu_bar: false,
        visible_in_menu_bar: false,
        hierarchical: false,
    });

    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state.set_mouse_position_for_test((15, 25));
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xFFFF_FFFF);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp);

    let (dropdown_top, dropdown_left, dropdown_bottom, _) = disp
        .control_tracking
        .as_ref()
        .map(|tracking| tracking.dropdown_rect)
        .expect("popup tracking should open a dropdown");
    if select_second_item {
        disp.input_state
            .set_mouse_position_for_test((dropdown_top + 1 + 16 + 1, dropdown_left + 5));
    } else {
        disp.input_state
            .set_mouse_position_for_test((dropdown_bottom + 8, dropdown_left + 5));
    }
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.input_state.set_mouse_button_for_test(false);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    (
        bus.read_word(sp + 12),
        cpu.read_reg(Register::A7),
        bus.read_word(ctrl_ptr + 18) as i16,
        disp.control_tracking.is_some(),
    )
}

#[test]
fn systemless_theme_does_not_change_popup_trackcontrol_value_results() {
    // MTE 1992 describes popup contrlValue as the chosen menu item
    // number, and TrackControl as returning the release part code.
    // Theme chrome must not alter either guest-visible result.
    let classic_select = trackcontrol_popup_value_result_for_theme(UiThemeId::ClassicSystem7, true);
    let themed_select =
        trackcontrol_popup_value_result_for_theme(UiThemeId::SystemlessDefault, true);
    let classic_no_selection =
        trackcontrol_popup_value_result_for_theme(UiThemeId::ClassicSystem7, false);
    let themed_no_selection =
        trackcontrol_popup_value_result_for_theme(UiThemeId::SystemlessDefault, false);

    assert_eq!(classic_select, (10, 0x30000C, 2, false));
    assert_eq!(classic_no_selection, (0, 0x30000C, 1, false));
    assert_eq!(
        themed_select, classic_select,
        "systemless-default must not change popup TrackControl selected values"
    );
    assert_eq!(
        themed_no_selection, classic_no_selection,
        "systemless-default must not change popup TrackControl no-selection values"
    );
}

#[test]
fn track_control_popup_nil_action_does_not_open_menu() {
    for (requested, stored) in [(0, u32::MAX), (u32::MAX, 0)] {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = 0x300000;
        let owner = *disp.current_port;
        let (handle, control) = disp.create_control_record(
            &mut bus,
            owner,
            (10, 20, 30, 130),
            b"Popup",
            true,
            1,
            902,
            0,
            1008,
            0,
        );
        assert_eq!(bus.read_long(control + 32), u32::MAX);
        disp.menus.push(Menu {
            id: 902,
            title: "Popup".to_string(),
            items: vec![MenuItem {
                text: "One".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            }],
            enabled: true,
            handle: 0,
            in_menu_bar: false,
            visible_in_menu_bar: false,
            hierarchical: false,
        });
        bus.write_long(control + 32, stored);
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((15, 25));
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, requested);
        bus.write_word(sp + 4, 15);
        bus.write_word(sp + 6, 25);
        bus.write_long(sp + 8, handle);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp
            .control_tracking
            .as_ref()
            .is_some_and(|state| !state.popup_tracking));
        assert_eq!(bus.read_word(control + 18), 1);
        disp.input_state.set_mouse_button_for_test(false);
        disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp.control_tracking.is_none());
        assert_eq!(bus.read_word(control + 18), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }
}

#[test]
fn track_control_popup_menu_samples_final_release_point() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.enable_input_trace_capture();
    let sp = 0x300000u32;
    let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
    bus.write_long(ctrl_ptr + 4, owner);
    bus.write_word(ctrl_ptr + 18, 1);
    bus.write_word(ctrl_ptr + 20, 900); // popupMenuProc stores MENU id in min
    bus.write_word(ctrl_ptr + 22, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 1009);
    bus.write_long(ctrl_ptr + 32, u32::MAX);
    disp.menus.push(Menu {
        id: 900,
        title: "Squadies".to_string(),
        items: vec![
            MenuItem {
                text: "Duke".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
            MenuItem {
                text: "Carnage".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
        ],
        enabled: true,
        handle: 0,
        in_menu_bar: false,
        hierarchical: false,
        visible_in_menu_bar: false,
    });

    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state.set_mouse_position_for_test((15, 25));
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, u32::MAX);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);

    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp,
        "popup tracking should defer the TrackControl stack pop"
    );
    let dropdown_rect = disp
        .control_tracking
        .as_ref()
        .map(|tracking| tracking.dropdown_rect)
        .expect("popup tracking should open a dropdown");
    let (dropdown_top, dropdown_left, dropdown_bottom, _) = dropdown_rect;
    assert_eq!(
        (dropdown_top, dropdown_left, dropdown_bottom),
        (10, 20, 42),
        "popup tracking should align selected item 1 with the control box, \
             not open below the control bottom"
    );
    assert_eq!(bus.read_word(sp + 12), 0xBEEF);

    disp.input_state
        .set_mouse_position_for_test((dropdown_top + 16 + 1, dropdown_left + 5));
    disp.input_state.set_mouse_button_for_test(false);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_word(sp + 12), 10);
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
    assert!(disp.control_tracking.is_none());
    let trace = disp.input_trace_text();
    assert!(trace.contains("A968 action=start start=(15,25)"));
    assert!(trace.contains("outcome=open_popup_tracking"));
    assert!(trace.contains("A968 action=tracking_finish"));
    assert!(trace.contains("part=10 highlighted_item=2 outcome=popup_item_selected"));
}

#[test]
fn track_control_popup_systemless_theme_routes_highlight_through_menu_provider() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.enable_input_trace_capture();
    let sp = 0x300000u32;
    let window = *disp.current_port;
    let screen_base = bus.read_long(window + 2);
    let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, screen_base, row_bytes, 342);

    let owner = bus.read_long(bus.read_long(cpu.read_reg(Register::A5)));
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 130), 255, 0);
    bus.write_long(ctrl_ptr + 4, owner);
    bus.write_word(ctrl_ptr + 18, 1);
    bus.write_word(ctrl_ptr + 20, 901);
    bus.write_word(ctrl_ptr + 22, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 1009);
    bus.write_long(ctrl_ptr + 32, u32::MAX);
    disp.menus.push(Menu {
        id: 901,
        title: "Formation".to_string(),
        items: vec![
            MenuItem {
                text: "Duke".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
            MenuItem {
                text: "Carnage".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
        ],
        enabled: true,
        handle: 0,
        in_menu_bar: false,
        visible_in_menu_bar: false,
        hierarchical: false,
    });

    disp.input_state.set_mouse_button_for_test(true);
    disp.input_state.set_mouse_position_for_test((15, 25));
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, u32::MAX);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, ctrl_handle);
    bus.write_word(sp + 12, 0xBEEF);

    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (dropdown_top, dropdown_left, _dropdown_bottom, dropdown_right) = disp
        .control_tracking
        .as_ref()
        .map(|tracking| tracking.dropdown_rect)
        .expect("popup tracking should open a dropdown");
    let item_two_top = dropdown_top + 1 + 16;
    let raw_inversion_probe_x = dropdown_right - 10;
    let raw_inversion_probe_y = item_two_top + 5;
    assert!(
        !screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            raw_inversion_probe_x,
            raw_inversion_probe_y
        ),
        "unhighlighted popup item row should leave blank row space clear"
    );

    disp.input_state
        .set_mouse_position_for_test((item_two_top + 1, dropdown_left + 5));
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        disp.control_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item),
        Some(2)
    );
    let provider_highlight_pixels = count_set_pixels(
        &bus,
        screen_base,
        row_bytes,
        item_two_top,
        dropdown_left,
        item_two_top + 16,
        dropdown_left + 12,
    );
    assert!(
        provider_highlight_pixels > 0,
        "systemless-default popup tracking should redraw provider-owned item highlight chrome"
    );
    assert!(
        screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            raw_inversion_probe_x,
            raw_inversion_probe_y
        ),
        "systemless-default popup tracking should fill the complete highlighted row"
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_word(sp + 12), 10);
    assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
    assert!(disp.control_tracking.is_none());
    let trace = disp.input_trace_text();
    assert!(trace.contains("A968 action=tracking_update"));
    assert!(trace.contains("highlighted_item=2 outcome=popup_item_highlighted"));
    assert!(trace.contains("part=10 highlighted_item=2 outcome=popup_item_selected"));
}

#[test]
fn track_control_point_outside_returns_zero() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 5);
    bus.write_word(sp + 6, 5);
    bus.write_long(sp + 8, ctrl_handle);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
}

#[test]
fn track_control_inactive_or_invisible_control_returns_zero() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (inactive_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 255);
    let (invisible_handle, _) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, inactive_handle);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 12), 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 15);
    bus.write_word(sp + 6, 25);
    bus.write_long(sp + 8, invisible_handle);
    disp.dispatch_control(true, 0x168, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
}

// DrawControls semantics per IM:I (1985) I-322 and MTE (1992) 5-87..5-88.
#[test]
fn drawcontrols_pops_window_arg_and_preserves_control_list_links() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let a5 = cpu.read_reg(Register::A5);
    let qd_globals = bus.read_long(a5);
    let window_ptr = bus.read_long(qd_globals);

    let (first_handle, first_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    let (second_handle, second_ptr) = alloc_control_handle(&mut bus, (18, 28, 38, 68), 255, 0);
    bus.write_long(first_ptr + 4, window_ptr);
    bus.write_long(second_ptr + 4, window_ptr);
    bus.write_long(first_ptr, 0);
    bus.write_long(second_ptr, first_handle);
    bus.write_long(window_ptr + 140, second_handle);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_long(window_ptr + 140), second_handle);
    assert_eq!(bus.read_long(second_ptr), first_handle);
    assert_eq!(bus.read_long(first_ptr), 0);
}

#[test]
fn drawcontrols_dispatches_visible_application_cdefs_in_control_list_draw_order() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let return_pc = 0x1234_5678;
    let a5 = cpu.read_reg(Register::A5);
    let qd_globals = bus.read_long(a5);
    let window_ptr = bus.read_long(qd_globals);
    let proc_id = (160i16 << 4) | 5;
    let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);

    let first_ptr = bus.alloc(296);
    let first_handle = bus.alloc(4);
    bus.write_long(first_handle, first_ptr);
    disp.initialize_control_record(
        &mut bus,
        first_ptr,
        window_ptr,
        (10, 20, 40, 80),
        b"",
        true,
        0,
        0,
        1,
        proc_id,
        0,
    );
    let second_ptr = bus.alloc(296);
    let second_handle = bus.alloc(4);
    bus.write_long(second_handle, second_ptr);
    disp.initialize_control_record(
        &mut bus,
        second_ptr,
        window_ptr,
        (50, 20, 80, 80),
        b"",
        true,
        0,
        0,
        1,
        proc_id,
        0,
    );
    bus.write_long(first_ptr, 0);
    bus.write_long(second_ptr, first_handle);
    bus.write_long(window_ptr + 140, second_handle);

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, window_ptr);
    disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let first_tramp = disp.control_def_trampoline;
    assert_eq!(cpu.read_reg(Register::PC), first_tramp);
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert_eq!(bus.read_long(sp), return_pc);
    assert_eq!(bus.read_long(first_tramp + 16), first_handle);
    assert_eq!(
        bus.read_word(first_tramp + 22),
        super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(first_tramp + 32), cdef_proc);

    let second_tramp = bus.read_long(first_tramp + 56);
    assert_ne!(second_tramp, 0);
    assert_eq!(bus.read_long(second_tramp + 16), second_handle);
    assert_eq!(
        bus.read_word(second_tramp + 22),
        super::super::TrapDispatcher::CDEF_DRAW_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(second_tramp + 32), cdef_proc);
    assert_eq!(bus.read_word(second_tramp + 54), 0x2F3C);
    assert_eq!(bus.read_word(second_tramp + 60), 0xA873);
    assert_eq!(bus.read_word(second_tramp + 68), 0xAA31);
    assert_eq!(bus.read_word(second_tramp + 70), 0x4E75);
}

// FindControl semantics per IM:I (1985) I-323 and MTE (1992) 5-89.
#[test]
fn findcontrol_visible_active_hit_returns_inbutton_and_control_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let which_ctrl_out = bus.alloc(4);
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    bus.write_long(ctrl_ptr, 0);
    bus.write_long(window_ptr + 140, ctrl_handle);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, which_ctrl_out);
    bus.write_long(sp + 4, window_ptr);
    bus.write_word(sp + 8, 15);
    bus.write_word(sp + 10, 25);
    disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(which_ctrl_out), ctrl_handle);
    assert_eq!(bus.read_word(sp + 12), 10);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
}

#[test]
fn findcontrol_inactive_invisible_or_miss_returns_zero_and_nil() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let window_ptr = bus.alloc(200);
    let which_ctrl_out = bus.alloc(4);

    let (inactive_handle, inactive_ptr) =
        alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 255);
    bus.write_long(inactive_ptr, 0);
    bus.write_long(window_ptr + 140, inactive_handle);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
    bus.write_long(sp, which_ctrl_out);
    bus.write_long(sp + 4, window_ptr);
    bus.write_word(sp + 8, 15);
    bus.write_word(sp + 10, 25);
    disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(which_ctrl_out), 0);
    assert_eq!(bus.read_word(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);

    let (invisible_handle, invisible_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 0, 0);
    bus.write_long(invisible_ptr, 0);
    bus.write_long(window_ptr + 140, invisible_handle);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
    bus.write_long(sp, which_ctrl_out);
    bus.write_long(sp + 4, window_ptr);
    bus.write_word(sp + 8, 15);
    bus.write_word(sp + 10, 25);
    disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(which_ctrl_out), 0);
    assert_eq!(bus.read_word(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);

    let (visible_handle, visible_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    bus.write_long(visible_ptr, 0);
    bus.write_long(window_ptr + 140, visible_handle);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(which_ctrl_out, 0xDEAD_BEEF);
    bus.write_long(sp, which_ctrl_out);
    bus.write_long(sp + 4, window_ptr);
    bus.write_word(sp + 8, 5);
    bus.write_word(sp + 10, 5);
    disp.dispatch_control(true, 0x16C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(which_ctrl_out), 0);
    assert_eq!(bus.read_word(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
}

#[test]
fn testcontrol_returns_inbutton_for_push_buttons_and_incheckbox_for_check_family() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;

    let (button_handle, button_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    disp.control_manager.set_proc_id(button_ptr, 0);
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 15);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, button_handle);
    bus.write_word(sp + 8, 0xDEAD);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 10);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);

    let (checkbox_handle, checkbox_ptr) = alloc_control_handle(&mut bus, (40, 50, 80, 120), 255, 0);
    disp.control_manager.set_proc_id(checkbox_ptr, 1);
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 60);
    bus.write_word(sp + 2, 70);
    bus.write_long(sp + 4, checkbox_handle);
    bus.write_word(sp + 8, 0xBEEF);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 11);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);

    let (radio_handle, radio_ptr) = alloc_control_handle(&mut bus, (90, 30, 120, 100), 255, 0);
    disp.control_manager.set_proc_id(radio_ptr, 2);
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 100);
    bus.write_word(sp + 2, 40);
    bus.write_long(sp + 4, radio_handle);
    bus.write_word(sp + 8, 0xCAFE);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 11);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn testcontrol_returns_zero_for_inactive_invisible_or_miss() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;

    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 5);
    bus.write_word(sp + 2, 5);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xDEAD);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 0);

    bus.write_byte(ctrl_ptr + 17, 255);
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 15);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xBEEF);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 0);

    bus.write_byte(ctrl_ptr + 17, 0);
    bus.write_byte(ctrl_ptr + 16, 0);
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 15);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xCAFE);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn testcontrol_function_protocol_consumes_controlhandle_and_point_and_writes_integer_result() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 15);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xCAFE);
    bus.write_word(sp + 10, 0xBEEF);

    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(bus.read_word(sp + 8), 10);
    assert_eq!(
        bus.read_word(sp + 10),
        0xBEEF,
        "TestControl must only overwrite the INTEGER result slot"
    );
}

#[test]
fn testcontrol_returns_standard_scrollbar_part_codes_for_vertical_scrollbar() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let (scroll_handle, _) =
        alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);

    for (pt_v, pt_h, expected) in [
        (70i16, 248i16, 20u16),
        (95, 248, 22),
        (126, 248, 129),
        (150, 248, 23),
        (210, 248, 21),
    ] {
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, pt_v as u16);
        bus.write_word(sp + 2, pt_h as u16);
        bus.write_long(sp + 4, scroll_handle);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_word(sp + 10, 0xCAFE);

        disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), expected);
        assert_eq!(bus.read_word(sp + 10), 0xCAFE);
    }
}

fn dispatch_testcontrol_part<C: CpuOps>(
    disp: &mut TrapDispatcher,
    cpu: &mut C,
    bus: &mut MacMemoryBus,
    sp: u32,
    ctrl_handle: u32,
    point: (i16, i16),
) -> (u16, u32, u16) {
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, point.0 as u16);
    bus.write_word(sp + 2, point.1 as u16);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0xBEEF);
    bus.write_word(sp + 10, 0xCAFE);

    disp.dispatch_control(true, 0x166, cpu, bus)
        .unwrap()
        .unwrap();

    (
        bus.read_word(sp + 8),
        cpu.read_reg(Register::A7),
        bus.read_word(sp + 10),
    )
}

fn testcontrol_results_for_theme(theme_id: UiThemeId) -> Vec<(&'static str, u16, u32, u16)> {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_ui_theme_id(theme_id);
    let sp = 0x300000u32;
    let mut results = Vec::new();

    let (button_handle, button_ptr) = alloc_control_handle(&mut bus, (10, 20, 30, 60), 255, 0);
    disp.control_manager.set_proc_id(button_ptr, 0);
    let (part, stack, tail) =
        dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, button_handle, (15, 25));
    results.push(("button", part, stack, tail));

    let (checkbox_handle, checkbox_ptr) = alloc_control_handle(&mut bus, (40, 50, 80, 120), 255, 0);
    disp.control_manager.set_proc_id(checkbox_ptr, 1);
    let (part, stack, tail) =
        dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, checkbox_handle, (60, 70));
    results.push(("checkbox", part, stack, tail));

    let (radio_handle, radio_ptr) = alloc_control_handle(&mut bus, (90, 30, 120, 100), 255, 0);
    disp.control_manager.set_proc_id(radio_ptr, 2);
    let (part, stack, tail) =
        dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, radio_handle, (100, 40));
    results.push(("radio", part, stack, tail));

    let (scroll_handle, _) =
        alloc_scrollbar_control(&mut disp, &mut bus, (60, 240, 220, 256), 255, 0, 40, 0, 100);
    for (name, point) in [
        ("scroll_up_arrow", (70i16, 248i16)),
        ("scroll_page_up", (95, 248)),
        ("scroll_thumb", (126, 248)),
        ("scroll_page_down", (150, 248)),
        ("scroll_down_arrow", (210, 248)),
    ] {
        let (part, stack, tail) =
            dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, scroll_handle, point);
        results.push((name, part, stack, tail));
    }

    let (outside_handle, outside_ptr) = alloc_control_handle(&mut bus, (130, 20, 160, 80), 255, 0);
    disp.control_manager.set_proc_id(outside_ptr, 0);
    let (part, stack, tail) =
        dispatch_testcontrol_part(&mut disp, &mut cpu, &mut bus, sp, outside_handle, (120, 15));
    results.push(("outside", part, stack, tail));

    let (inactive_handle, inactive_ptr) =
        alloc_control_handle(&mut bus, (170, 20, 200, 80), 255, 255);
    disp.control_manager.set_proc_id(inactive_ptr, 0);
    let (part, stack, tail) = dispatch_testcontrol_part(
        &mut disp,
        &mut cpu,
        &mut bus,
        sp,
        inactive_handle,
        (180, 30),
    );
    results.push(("inactive", part, stack, tail));

    let (invisible_handle, invisible_ptr) =
        alloc_control_handle(&mut bus, (210, 20, 240, 80), 0, 0);
    disp.control_manager.set_proc_id(invisible_ptr, 0);
    let (part, stack, tail) = dispatch_testcontrol_part(
        &mut disp,
        &mut cpu,
        &mut bus,
        sp,
        invisible_handle,
        (220, 30),
    );
    results.push(("invisible", part, stack, tail));

    results
}

#[test]
fn systemless_theme_does_not_change_testcontrol_part_codes() {
    // IM:I I-315 defines standard Control Manager part codes. IM:I I-325
    // specifies that TestControl returns those codes for visible active
    // controls, or 0 for misses, invisible controls, and inactive controls.
    // Theme chrome must not alter those guest-visible hit-test results.
    let classic = testcontrol_results_for_theme(UiThemeId::ClassicSystem7);
    let themed = testcontrol_results_for_theme(UiThemeId::SystemlessDefault);
    let expected_stack = 0x300008u32;
    let expected_tail = 0xCAFEu16;

    assert_eq!(
        classic,
        vec![
            ("button", 10, expected_stack, expected_tail),
            ("checkbox", 11, expected_stack, expected_tail),
            ("radio", 11, expected_stack, expected_tail),
            ("scroll_up_arrow", 20, expected_stack, expected_tail),
            ("scroll_page_up", 22, expected_stack, expected_tail),
            ("scroll_thumb", 129, expected_stack, expected_tail),
            ("scroll_page_down", 23, expected_stack, expected_tail),
            ("scroll_down_arrow", 21, expected_stack, expected_tail),
            ("outside", 0, expected_stack, expected_tail),
            ("inactive", 0, expected_stack, expected_tail),
            ("invisible", 0, expected_stack, expected_tail),
        ]
    );
    assert_eq!(
        themed, classic,
        "systemless-default must not change TestControl part codes or stack protocol"
    );
}

// Draw1Control signature/no-op baseline per MTE (1992) 5-88.
#[test]
fn draw1control_nil_handle_is_noop_and_pops_arg() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);

    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn testcontrol_custom_cdef_arms_test_message_with_point_and_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let return_pc = 0x8765_4321;
    let proc_id = (160i16 << 4) | 7;
    let cdef_proc = disp.install_test_resource(&mut bus, *b"CDEF", 160, &[0x4E, 0x56, 0, 0]);
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    disp.initialize_control_record(
        &mut bus,
        ctrl_ptr,
        0,
        (10, 20, 40, 80),
        b"",
        true,
        0,
        0,
        1,
        proc_id,
        0,
    );
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_word(sp, 23);
    bus.write_word(sp + 2, 45);
    bus.write_long(sp + 4, ctrl_handle);
    bus.write_word(sp + 8, 0);

    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let tramp = disp.control_def_trampoline;
    assert_eq!(cpu.read_reg(Register::PC), tramp);
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_long(sp + 4), return_pc);
    assert_eq!(bus.read_word(tramp + 12), 7);
    assert_eq!(bus.read_long(tramp + 16), ctrl_handle);
    assert_eq!(
        bus.read_word(tramp + 22),
        super::super::TrapDispatcher::CDEF_TEST_CNTL_MSG as u16
    );
    assert_eq!(bus.read_long(tramp + 26), (23u32 << 16) | 45);
    assert_eq!(bus.read_long(tramp + 32), cdef_proc);
    assert_eq!(bus.read_long(tramp + 40), sp + 8);
}

#[test]
fn draw1control_treats_any_nonzero_contrlvis_as_visible() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let screen_base = bus.alloc(16 * 128);
    let row_bytes = 16u32;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 128, 1);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);
    clear_1bpp_screen(&mut bus, screen_base, row_bytes, 128);

    let window_ptr = *disp.current_port;
    bus.write_word(window_ptr + 16, 0);
    bus.write_word(window_ptr + 18, 0);
    bus.write_word(window_ptr + 20, 128);
    bus.write_word(window_ptr + 22, 128);
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    disp.initialize_control_record(
        &mut bus,
        ctrl_ptr,
        window_ptr,
        (20, 20, 44, 100),
        b"Play",
        false,
        0,
        0,
        1,
        0,
        0,
    );
    bus.write_byte(ctrl_ptr + 16, 1);

    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert!(
        count_set_pixels(&bus, screen_base, row_bytes, 20, 20, 44, 100) > 40,
        "drawCntl skips only contrlVis == 0 (Inside Macintosh I-329)"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn draw1control_unknown_cdef_pict_title_draws_resource_pair() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let window = 0x181000u32;
    disp.set_current_port_for_test(window);
    let base = bus.read_long(window + 2);
    let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);

    disp.install_test_resource(
        &mut bus,
        *b"PICT",
        4096,
        &fill_rect_pict_resource((0, 0, 8, 8), (0, 0, 8, 4)),
    );
    disp.install_test_resource(
        &mut bus,
        *b"PICT",
        4097,
        &fill_rect_pict_resource((0, 0, 8, 8), (0, 4, 8, 8)),
    );

    let (ctrl_handle, ctrl_ptr) =
        alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 100));
    disp.control_manager.set_proc_id(ctrl_ptr, 3216);
    bus.write_byte(ctrl_ptr + 40, 4);
    bus.write_word(ctrl_ptr + 41, 4096);
    bus.write_word(ctrl_ptr + 43, 4097);

    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 35, 30),
        "normal unknown CDEF with a PICT-pair title should draw the first PICT"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 85, 30),
        "normal state should not draw the highlighted PICT"
    );

    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_byte(ctrl_ptr + 17, 1);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);
    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 35, 30),
        "highlighted state should not draw the normal PICT"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 85, 30),
        "highlighted unknown CDEF with a PICT-pair title should draw the second PICT"
    );
}

#[test]
fn draw1control_unknown_cdef_falls_back_to_button_title_chrome() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let window = 0x181000u32;
    disp.set_current_port_for_test(window);
    let base = bus.read_long(window + 2);
    let row_bytes = (bus.read_word(window + 6) & 0x3FFF) as u32;
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let (blank_handle, blank_ptr) =
        alloc_button_control(&mut disp, &mut bus, window, (20, 20, 40, 140));
    let (titled_handle, titled_ptr) =
        alloc_button_control(&mut disp, &mut bus, window, (60, 20, 80, 140));
    for ptr in [blank_ptr, titled_ptr] {
        disp.control_manager.set_proc_id(ptr, 3216);
    }
    bus.write_byte(titled_ptr + 40, 4);
    bus.write_bytes(titled_ptr + 41, b"PLAY");

    for handle in [blank_handle, titled_handle] {
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    let blank_chrome = count_set_pixels(&bus, base, row_bytes, 20, 20, 40, 140);
    assert!(
        blank_chrome > 0,
        "unknown CDEF fallback should draw visible control chrome"
    );

    let mut title_differences = 0;
    for dy in 0..20 {
        for dx in 0..120 {
            let blank_pixel = screen_pixel_is_set(&bus, base, row_bytes, 20 + dx, 20 + dy);
            let titled_pixel = screen_pixel_is_set(&bus, base, row_bytes, 20 + dx, 60 + dy);
            if blank_pixel != titled_pixel {
                title_differences += 1;
            }
        }
    }
    assert!(
        title_differences > 8,
        "unknown CDEF fallback should draw the ControlRecord title"
    );

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 65);
    bus.write_word(sp + 2, 30);
    bus.write_long(sp + 4, titled_handle);
    bus.write_word(sp + 8, 0xDEAD);
    disp.dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 10);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_popup_control_preserves_current_gdevice_and_pops_arg() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let window_ptr = *disp.current_port;
    let gdevice_before = *disp.current_gdevice;
    let (ctrl_handle, ctrl_ptr) =
        alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 20, 260, 60));
    disp.control_manager.set_proc_id(ctrl_ptr, 1008);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, ctrl_handle);

    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(*disp.current_gdevice, gdevice_before);
}

#[test]
fn background_scrollbar_drawing_is_clipped_behind_front_window() {
    // Standard CDEF drawing is clipped by the owner window's visRgn.
    // A background window's scrollbar must therefore never paint through
    // a front window even though the HLE scrollbar renderer writes direct
    // framebuffer pixels. IM:I I-145 and I-326; MTE 1992 pp. 4-69..4-70.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = 0x300000u32;
    disp.set_screen_mode_for_test(screen_base, 320, 320, 240, 8);
    bus.write_long(0x0824, screen_base);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let back = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        screen_base,
        40,
        40,
        220,
        280,
        "Back",
        0,
        true,
        false,
        false,
        0,
    );
    let front = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        screen_base,
        90,
        90,
        200,
        220,
        "Front",
        0,
        true,
        false,
        false,
        0,
    );
    assert_eq!(disp.window_list, vec![front, back]);

    // Back-local (60,40..76,200) maps to global (100,80..116,240),
    // crossing straight through the front content.
    let (_handle, ctrl_ptr) =
        alloc_scrollbar_control(&mut disp, &mut bus, (60, 40, 76, 200), 0xFF, 0, 5, 0, 10);
    bus.write_long(ctrl_ptr + 4, back);

    for y in 100u32..116 {
        for x in 90u32..220 {
            bus.write_byte(screen_base + y * 320 + x, 0x4D);
        }
    }
    // An uncovered portion should still prove the scrollbar was drawn.
    for y in 100u32..116 {
        for x in 80u32..90 {
            bus.write_byte(screen_base + y * 320 + x, 0x4D);
        }
    }

    disp.draw_control(&mut cpu, &mut bus, ctrl_ptr);

    for y in 100u32..116 {
        for x in 90u32..220 {
            assert_eq!(
                bus.read_byte(screen_base + y * 320 + x),
                0x4D,
                "background scrollbar painted through front window at ({x},{y})"
            );
        }
    }
    assert!(
        (100u32..116)
            .any(|y| { (80u32..90).any(|x| bus.read_byte(screen_base + y * 320 + x) != 0x4D) }),
        "visible portion of the background scrollbar should still draw"
    );
}

#[test]
fn draw1control_systemless_theme_routes_popup_control_chrome_through_provider() {
    let sp = 0x300000u32;
    let bounds = (20, 20, 40, 120);

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_window = *classic.current_port;
    let classic_base = classic_bus.read_long(classic_window + 2);
    let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    let (classic_handle, classic_ptr) =
        alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
    classic.control_manager.set_proc_id(classic_ptr, 1008);
    classic_cpu.write_reg(Register::A7, sp);
    classic_bus.write_long(sp, classic_handle);
    classic
        .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .unwrap();

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let themed_window = *themed.current_port;
    let themed_base = themed_bus.read_long(themed_window + 2);
    let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    let (themed_handle, themed_ptr) =
        alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
    themed.control_manager.set_proc_id(themed_ptr, 1008);
    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_long(sp, themed_handle);
    themed
        .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();

    assert!(
        screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 100, 37),
        "classic popup CDEF should draw its resolved auto-width offset shadow"
    );
    assert!(
        screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 101, 37),
        "classic popup CDEF should extend the resolved shadow one pixel past the provider frame"
    );
    assert!(
        !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 101, 37),
        "systemless-default popup provider should not draw the classic resolved shadow extension"
    );

    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_word(sp, 25);
    themed_bus.write_word(sp + 2, 25);
    themed_bus.write_long(sp + 4, themed_handle);
    themed_bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();
    assert_eq!(themed_bus.read_word(sp + 8), 10);
    assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_systemless_theme_routes_popup_control_hilite_states_through_provider() {
    let sp = 0x300000u32;
    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let themed_window = *themed.current_port;
    let row_bytes = 64u32;
    let base = themed_bus.alloc(row_bytes * 342);
    themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, base, row_bytes, 342);

    let (normal_handle, normal_ptr) = alloc_button_control(
        &mut themed,
        &mut themed_bus,
        themed_window,
        (20, 20, 40, 120),
    );
    let (pressed_handle, pressed_ptr) = alloc_button_control(
        &mut themed,
        &mut themed_bus,
        themed_window,
        (52, 20, 72, 120),
    );
    let (inactive_handle, inactive_ptr) = alloc_button_control(
        &mut themed,
        &mut themed_bus,
        themed_window,
        (84, 20, 104, 120),
    );
    for ptr in [normal_ptr, pressed_ptr, inactive_ptr] {
        themed.control_manager.set_proc_id(ptr, 1008);
    }
    themed_bus.write_byte(pressed_ptr + 17, 1);
    themed_bus.write_byte(inactive_ptr + 17, 255);

    for handle in [normal_handle, pressed_handle, inactive_handle] {
        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_long(sp, handle);
        themed
            .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
    }

    assert!(
        !screen_pixel_is_set(&themed_bus, base, row_bytes, 22, 30),
        "normal systemless-default popup should keep the state inset clear"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, base, row_bytes, 30, 57),
        "pressed systemless-default popup should route hilite state to provider fill"
    );
    let normal_chrome = count_set_pixels(&themed_bus, base, row_bytes, 21, 20, 38, 100);
    let inactive_chrome = count_set_pixels(&themed_bus, base, row_bytes, 85, 20, 102, 100);
    assert!(
            inactive_chrome > normal_chrome,
            "inactive systemless-default popup should route HiliteControl state to provider chrome ({inactive_chrome} <= {normal_chrome})"
        );
}

#[test]
fn draw1control_systemless_theme_routes_push_button_chrome_through_provider() {
    let sp = 0x300000u32;
    let bounds = (20, 20, 40, 80);

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_window = *classic.current_port;
    let classic_base = classic_bus.read_long(classic_window + 2);
    let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    let (classic_handle, _) =
        alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
    classic_cpu.write_reg(Register::A7, sp);
    classic_bus.write_long(sp, classic_handle);
    classic
        .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .unwrap();

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let themed_window = *themed.current_port;
    let themed_base = themed_bus.read_long(themed_window + 2);
    let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    let (themed_handle, _) =
        alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_long(sp, themed_handle);
    themed
        .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();

    assert!(
        !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 20, 20),
        "classic round-rect CDEF should leave the outer corner as background"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 20, 20),
        "systemless-default provider chrome should own the HLE control corner pixel"
    );

    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_word(sp, 25);
    themed_bus.write_word(sp + 2, 25);
    themed_bus.write_long(sp + 4, themed_handle);
    themed_bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();
    assert_eq!(themed_bus.read_word(sp + 8), 10);
    assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_systemless_theme_routes_button_hilite_states_through_provider() {
    let sp = 0x300000u32;
    let (mut themed, mut cpu, mut bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let window = *themed.current_port;
    let row_bytes = 64u32;
    let base = bus.alloc(row_bytes * 342);
    themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let (enabled_handle, _enabled_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (20, 20, 40, 80));
    let (pressed_handle, pressed_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (60, 20, 80, 80));
    let (inactive_handle, inactive_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (100, 20, 120, 80));
    bus.write_byte(pressed_ptr + 17, 1);
    bus.write_byte(inactive_ptr + 17, 255);

    for handle in [enabled_handle, pressed_handle, inactive_handle] {
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        themed
            .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    let enabled_body = count_set_pixels(&bus, base, row_bytes, 23, 23, 37, 77);
    let pressed_body = count_set_pixels(&bus, base, row_bytes, 63, 23, 77, 77);
    assert!(
            pressed_body > enabled_body,
            "systemless-default should route contrlHilite=1 into the provider pressed state ({pressed_body} <= {enabled_body})"
        );

    // MTE 1992 glossary, "inactive control": inactive controls are
    // visually indicated and do not respond as active controls.
    let inactive_body = count_set_pixels(&bus, base, row_bytes, 103, 23, 117, 77);
    assert!(
        pressed_body > inactive_body,
        "systemless-default should keep inactive button chrome distinct from pressed chrome"
    );

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 65);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, pressed_handle);
    bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 10);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 105);
    bus.write_word(sp + 2, 25);
    bus.write_long(sp + 4, inactive_handle);
    bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_systemless_theme_routes_checkbox_and_radio_chrome_through_provider() {
    let sp = 0x300000u32;

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_window = *classic.current_port;
    let classic_base = classic_bus.read_long(classic_window + 2);
    let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    let (classic_checkbox_handle, classic_checkbox_ptr) = alloc_button_control(
        &mut classic,
        &mut classic_bus,
        classic_window,
        (20, 20, 40, 120),
    );
    classic.control_manager.set_proc_id(classic_checkbox_ptr, 1);
    classic_bus.write_word(classic_checkbox_ptr + 18, 1);
    let (classic_radio_handle, classic_radio_ptr) = alloc_button_control(
        &mut classic,
        &mut classic_bus,
        classic_window,
        (50, 20, 70, 120),
    );
    classic.control_manager.set_proc_id(classic_radio_ptr, 2);
    classic_bus.write_word(classic_radio_ptr + 18, 1);
    for handle in [classic_checkbox_handle, classic_radio_handle] {
        classic_cpu.write_reg(Register::A7, sp);
        classic_bus.write_long(sp, handle);
        classic
            .dispatch_control(true, 0x16D, &mut classic_cpu, &mut classic_bus)
            .unwrap()
            .unwrap();
    }

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let themed_window = *themed.current_port;
    let themed_base = themed_bus.read_long(themed_window + 2);
    let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    let (themed_checkbox_handle, themed_checkbox_ptr) = alloc_button_control(
        &mut themed,
        &mut themed_bus,
        themed_window,
        (20, 20, 40, 120),
    );
    themed.control_manager.set_proc_id(themed_checkbox_ptr, 1);
    themed_bus.write_word(themed_checkbox_ptr + 18, 1);
    let (themed_radio_handle, themed_radio_ptr) = alloc_button_control(
        &mut themed,
        &mut themed_bus,
        themed_window,
        (50, 20, 70, 120),
    );
    themed.control_manager.set_proc_id(themed_radio_ptr, 2);
    themed_bus.write_word(themed_radio_ptr + 18, 1);
    for handle in [themed_checkbox_handle, themed_radio_handle] {
        themed_cpu.write_reg(Register::A7, sp);
        themed_bus.write_long(sp, handle);
        themed
            .dispatch_control(true, 0x16D, &mut themed_cpu, &mut themed_bus)
            .unwrap()
            .unwrap();
    }

    let classic_checkbox_pixels = count_set_pixels(
        &classic_bus,
        classic_base,
        classic_row_bytes,
        24,
        22,
        36,
        34,
    );
    let themed_checkbox_pixels =
        count_set_pixels(&themed_bus, themed_base, themed_row_bytes, 24, 22, 36, 34);
    assert!(
            themed_checkbox_pixels > classic_checkbox_pixels,
            "systemless-default checkbox provider should draw a denser selected mark ({themed_checkbox_pixels} <= {classic_checkbox_pixels})"
        );
    let classic_radio_pixels = count_set_pixels(
        &classic_bus,
        classic_base,
        classic_row_bytes,
        54,
        22,
        66,
        34,
    );
    let themed_radio_pixels =
        count_set_pixels(&themed_bus, themed_base, themed_row_bytes, 54, 22, 66, 34);
    assert!(
            themed_radio_pixels > classic_radio_pixels,
            "systemless-default radio provider should draw a denser selected mark ({themed_radio_pixels} <= {classic_radio_pixels})"
        );

    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_word(sp, 25);
    themed_bus.write_word(sp + 2, 25);
    themed_bus.write_long(sp + 4, themed_checkbox_handle);
    themed_bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();
    assert_eq!(themed_bus.read_word(sp + 8), 11);
    assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_systemless_theme_routes_checkbox_radio_hilite_states_through_provider() {
    let sp = 0x300000u32;
    let (mut themed, mut cpu, mut bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let window = *themed.current_port;
    let row_bytes = 64u32;
    let base = bus.alloc(row_bytes * 342);
    themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let (checkbox_enabled_handle, checkbox_enabled_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (20, 20, 40, 100));
    let (checkbox_pressed_handle, checkbox_pressed_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (52, 20, 72, 100));
    let (checkbox_inactive_handle, checkbox_inactive_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (84, 20, 104, 100));
    for ptr in [
        checkbox_enabled_ptr,
        checkbox_pressed_ptr,
        checkbox_inactive_ptr,
    ] {
        themed.control_manager.set_proc_id(ptr, 1);
    }
    bus.write_byte(checkbox_pressed_ptr + 17, 1);
    bus.write_byte(checkbox_inactive_ptr + 17, 255);

    let (radio_enabled_handle, radio_enabled_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (20, 140, 40, 220));
    let (radio_pressed_handle, radio_pressed_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (52, 140, 72, 220));
    let (radio_inactive_handle, radio_inactive_ptr) =
        alloc_button_control(&mut themed, &mut bus, window, (84, 140, 104, 220));
    for ptr in [radio_enabled_ptr, radio_pressed_ptr, radio_inactive_ptr] {
        themed.control_manager.set_proc_id(ptr, 2);
    }
    bus.write_byte(radio_pressed_ptr + 17, 1);
    bus.write_byte(radio_inactive_ptr + 17, 255);

    for handle in [
        checkbox_enabled_handle,
        checkbox_pressed_handle,
        checkbox_inactive_handle,
        radio_enabled_handle,
        radio_pressed_handle,
        radio_inactive_handle,
    ] {
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        themed
            .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    let checkbox_enabled_body = count_set_pixels(&bus, base, row_bytes, 24, 22, 36, 34);
    let checkbox_pressed_body = count_set_pixels(&bus, base, row_bytes, 56, 22, 68, 34);
    let checkbox_inactive_body = count_set_pixels(&bus, base, row_bytes, 88, 22, 100, 34);
    assert!(
            checkbox_pressed_body > checkbox_enabled_body,
            "systemless-default should route checkbox contrlHilite=1 to provider pressed fill ({checkbox_pressed_body} <= {checkbox_enabled_body})"
        );
    assert!(
        checkbox_pressed_body > checkbox_inactive_body,
        "systemless-default should keep inactive checkbox chrome distinct from pressed chrome"
    );

    let radio_enabled_body = count_set_pixels(&bus, base, row_bytes, 24, 142, 36, 154);
    let radio_pressed_body = count_set_pixels(&bus, base, row_bytes, 56, 142, 68, 154);
    let radio_inactive_body = count_set_pixels(&bus, base, row_bytes, 88, 142, 100, 154);
    assert!(
            radio_pressed_body > radio_enabled_body,
            "systemless-default should route radio contrlHilite=1 to provider pressed fill ({radio_pressed_body} <= {radio_enabled_body})"
        );
    assert!(
        radio_pressed_body > radio_inactive_body,
        "systemless-default should keep inactive radio chrome distinct from pressed chrome"
    );

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 57);
    bus.write_word(sp + 2, 145);
    bus.write_long(sp + 4, radio_pressed_handle);
    bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 11);
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn scrollbar_themes_share_the_presentation_provider_and_keep_part_codes() {
    let sp = 0x300000u32;
    let bounds = (20, 20, 180, 36);

    let (mut classic, _classic_cpu, mut classic_bus) = setup_with_port();
    let classic_window = *classic.current_port;
    let classic_base = classic_bus.read_long(classic_window + 2);
    let classic_row_bytes = (classic_bus.read_word(classic_window + 6) & 0x3FFF) as u32;
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    let (_classic_handle, classic_ptr) =
        alloc_button_control(&mut classic, &mut classic_bus, classic_window, bounds);
    classic.control_manager.set_proc_id(classic_ptr, 16);
    classic_bus.write_word(classic_ptr + 18, 40);
    classic_bus.write_word(classic_ptr + 20, 0);
    classic_bus.write_word(classic_ptr + 22, 100);
    assert!(
        classic.draw_theme_scrollbar_chrome(
            &mut classic_bus,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            40,
            0,
            100,
            0,
        ),
        "classic-system7 should use the architecture-neutral scrollbar renderer"
    );

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let themed_window = *themed.current_port;
    let themed_base = themed_bus.read_long(themed_window + 2);
    let themed_row_bytes = (themed_bus.read_word(themed_window + 6) & 0x3FFF) as u32;
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    let (themed_handle, themed_ptr) =
        alloc_button_control(&mut themed, &mut themed_bus, themed_window, bounds);
    themed.control_manager.set_proc_id(themed_ptr, 16);
    themed_bus.write_word(themed_ptr + 18, 40);
    themed_bus.write_word(themed_ptr + 20, 0);
    themed_bus.write_word(themed_ptr + 22, 100);
    assert!(
        themed.draw_theme_scrollbar_chrome(
            &mut themed_bus,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
            40,
            0,
            100,
            0,
        ),
        "systemless-default routes scrollBarProc chrome through the theme provider"
    );

    themed_cpu.write_reg(Register::A7, sp);
    themed_bus.write_word(sp, 85);
    themed_bus.write_word(sp + 2, 28);
    themed_bus.write_long(sp + 4, themed_handle);
    themed_bus.write_word(sp + 8, 0xDEAD);
    themed
        .dispatch_control(true, 0x166, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();
    assert_eq!(themed_bus.read_word(sp + 8), 129);
    assert_eq!(themed_cpu.read_reg(Register::A7), sp + 8);
}

#[test]
fn draw1control_systemless_theme_routes_scrollbar_hilite_states_through_provider() {
    let sp = 0x300000u32;
    let (mut themed, mut cpu, mut bus) = setup_with_port();
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    let row_bytes = 64u32;
    let base = bus.alloc(row_bytes * 342);
    themed.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let (normal_handle, normal_ptr) =
        alloc_scrollbar_control(&mut themed, &mut bus, (20, 20, 100, 36), 255, 0, 40, 0, 100);
    let (arrow_handle, _) = alloc_scrollbar_control(
        &mut themed,
        &mut bus,
        (20, 52, 100, 68),
        255,
        20,
        40,
        0,
        100,
    );
    let (page_handle, _) = alloc_scrollbar_control(
        &mut themed,
        &mut bus,
        (20, 84, 100, 100),
        255,
        22,
        40,
        0,
        100,
    );
    let (inactive_handle, inactive_ptr) = alloc_scrollbar_control(
        &mut themed,
        &mut bus,
        (20, 116, 100, 132),
        255,
        255,
        40,
        0,
        100,
    );

    for handle in [normal_handle, arrow_handle, page_handle, inactive_handle] {
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        themed
            .dispatch_control(true, 0x16D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    let normal_arrow_fill = count_set_pixels(&bus, base, row_bytes, 21, 21, 35, 35);
    let pressed_arrow_fill = count_set_pixels(&bus, base, row_bytes, 21, 53, 35, 67);
    assert!(
            pressed_arrow_fill > normal_arrow_fill,
            "systemless-default should route scrollbar arrow part hilite through provider fill ({pressed_arrow_fill} <= {normal_arrow_fill})"
        );

    let normal_page_before_fill = count_set_pixels(&bus, base, row_bytes, 37, 22, 47, 34);
    let pressed_page_before_fill = count_set_pixels(&bus, base, row_bytes, 37, 86, 47, 98);
    assert!(
            pressed_page_before_fill > normal_page_before_fill,
            "systemless-default should route scrollbar page-region hilite through provider fill ({pressed_page_before_fill} <= {normal_page_before_fill})"
        );

    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 28, 48),
        "enabled scrollbar should draw its provider thumb frame"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 124, 48),
        "inactive scrollbar should suppress the provider thumb"
    );

    // IM:I I-313 / I-323: hilite values 1..253 name active parts,
    // while 255 makes the control inactive and hit testing returns no part.
    for (handle, pt_v, pt_h, expected) in [
        (arrow_handle, 24i16, 60i16, 20u16),
        (page_handle, 40, 92, 22),
        (normal_handle, 56, 28, 129),
        (inactive_handle, 56, 124, 0),
    ] {
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, pt_v as u16);
        bus.write_word(sp + 2, pt_h as u16);
        bus.write_long(sp + 4, handle);
        bus.write_word(sp + 8, 0xDEAD);
        themed
            .dispatch_control(true, 0x166, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 8), expected);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 20);
    bus.write_long(sp + 2, normal_handle);
    themed
        .dispatch_control(true, 0x15D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(normal_ptr + 17), 20);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert!(
        count_set_pixels(&bus, base, row_bytes, 21, 21, 35, 35) > normal_arrow_fill,
        "HiliteControl should redraw scroll-bar part hilites through the provider"
    );

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 255);
    bus.write_long(sp + 2, normal_handle);
    themed
        .dispatch_control(true, 0x15D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(normal_ptr + 17), 255);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 28, 56),
        "HiliteControl(255) should redraw the inactive provider state"
    );
    assert_eq!(bus.read_byte(inactive_ptr + 17), 255);
}

#[test]
fn draw1control_out_of_range_handle_is_noop_and_pops_arg() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, bus.ram_size() + 4);

    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn draw1control_handle_pointing_at_out_of_range_control_ptr_is_noop_and_pops_arg() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let handle = bus.alloc(4);
    assert_ne!(handle, 0);
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, handle);
    bus.write_long(handle, bus.ram_size() + 0x1000);

    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn draw1control_handle_pointing_to_truncated_control_record_is_noop_and_pops_arg() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let handle = bus.alloc(4);
    let ctrl_ptr = bus.ram_size().saturating_sub(41);
    assert_ne!(handle, 0);
    assert!(ctrl_ptr > 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, handle);
    bus.write_long(handle, ctrl_ptr);
    bus.write_byte(ctrl_ptr + 40, 16);

    disp.dispatch_control(true, 0x16D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
}

#[test]
fn dragcontrol_pops_18_bytes_and_leaves_contrlrect_unchanged_on_no_drag_path() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let sp = 0x300000u32;
    let window_ptr = *disp.current_port;
    let (ctrl_handle, ctrl_ptr) = alloc_control_handle(&mut bus, (88, 96, 132, 220), 0, 0);
    let limit_rect_ptr = bus.alloc(8);
    let slop_rect_ptr = bus.alloc(8);
    let top_before = bus.read_word(ctrl_ptr + 8);
    let left_before = bus.read_word(ctrl_ptr + 10);
    let bottom_before = bus.read_word(ctrl_ptr + 12);
    let right_before = bus.read_word(ctrl_ptr + 14);

    bus.write_long(ctrl_ptr + 4, window_ptr);
    bus.write_long(window_ptr + 140, ctrl_handle);

    bus.write_word(limit_rect_ptr, 0);
    bus.write_word(limit_rect_ptr + 2, 0);
    bus.write_word(limit_rect_ptr + 4, 400);
    bus.write_word(limit_rect_ptr + 6, 640);
    bus.write_word(slop_rect_ptr, (-40i16) as u16);
    bus.write_word(slop_rect_ptr + 2, (-40i16) as u16);
    bus.write_word(slop_rect_ptr + 4, (-20i16) as u16);
    bus.write_word(slop_rect_ptr + 6, (-20i16) as u16);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0); // axis
    bus.write_long(sp + 2, slop_rect_ptr);
    bus.write_long(sp + 6, limit_rect_ptr);
    bus.write_word(sp + 10, 12); // startPt.v
    bus.write_word(sp + 12, 18); // startPt.h
    bus.write_long(sp + 14, ctrl_handle);

    disp.dispatch_control(true, 0x167, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 18);
    assert_eq!(bus.read_word(ctrl_ptr + 8), top_before);
    assert_eq!(bus.read_word(ctrl_ptr + 10), left_before);
    assert_eq!(bus.read_word(ctrl_ptr + 12), bottom_before);
    assert_eq!(bus.read_word(ctrl_ptr + 14), right_before);
}

fn alloc_button_control(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut MacMemoryBus,
    window_ptr: u32,
    bounds: (i16, i16, i16, i16),
) -> (u32, u32) {
    let ctrl_ptr = bus.alloc(48);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    bus.write_long(ctrl_ptr, 0);
    bus.write_long(ctrl_ptr + 4, window_ptr);
    bus.write_word(ctrl_ptr + 8, bounds.0 as u16);
    bus.write_word(ctrl_ptr + 10, bounds.1 as u16);
    bus.write_word(ctrl_ptr + 12, bounds.2 as u16);
    bus.write_word(ctrl_ptr + 14, bounds.3 as u16);
    bus.write_byte(ctrl_ptr + 16, 255);
    bus.write_byte(ctrl_ptr + 17, 0);
    bus.write_word(ctrl_ptr + 18, 0);
    bus.write_word(ctrl_ptr + 20, 0);
    bus.write_word(ctrl_ptr + 22, 1);
    bus.write_long(ctrl_ptr + 24, 0);
    bus.write_long(ctrl_ptr + 28, 0);
    bus.write_long(ctrl_ptr + 32, 0);
    bus.write_long(ctrl_ptr + 36, 0);
    bus.write_byte(ctrl_ptr + 40, 0);
    disp.control_manager.set_proc_id(ctrl_ptr, 0);
    (ctrl_handle, ctrl_ptr)
}

fn screen_pixel_is_set(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> bool {
    let byte = bus.read_byte(base + (y as u32 * row_bytes) + ((x as u32) / 8));
    byte & (0x80u8 >> ((x as u8) & 7)) != 0
}

fn count_set_pixels(
    bus: &MacMemoryBus,
    base: u32,
    row_bytes: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> u32 {
    let mut count = 0;
    for y in top..bottom {
        for x in left..right {
            if screen_pixel_is_set(bus, base, row_bytes, x, y) {
                count += 1;
            }
        }
    }
    count
}

fn count_pixel_index(
    bus: &MacMemoryBus,
    base: u32,
    row_bytes: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
    pixel_index: u8,
) -> u32 {
    let mut count = 0;
    for y in top..bottom {
        for x in left..right {
            if bus.read_byte(base + y as u32 * row_bytes + x as u32) == pixel_index {
                count += 1;
            }
        }
    }
    count
}

fn clear_1bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32) {
    for y in 0..height {
        for x in 0..row_bytes {
            bus.write_byte(base + y * row_bytes + x, 0);
        }
    }
}

#[test]
fn updtcontrol_repaints_intersecting_controls_and_empty_regions() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let window_ptr = 0x181000u32;
    let screen_base = bus.read_long(window_ptr + 2);
    let row_bytes = (bus.read_word(window_ptr + 6) & 0x3FFF) as u32;

    let (left_handle, left_ptr) =
        alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 20, 60, 120));
    let (right_handle, right_ptr) =
        alloc_button_control(&mut disp, &mut bus, window_ptr, (20, 150, 60, 250));

    bus.write_long(window_ptr + 140, right_handle);
    bus.write_long(right_ptr, left_handle);
    bus.write_long(left_ptr, 0);
    bus.write_long(left_ptr + 4, window_ptr);
    bus.write_long(right_ptr + 4, window_ptr);

    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    disp.dispatch_control(true, 0x169, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let left_probe = (70, 40);
    let right_probe = (200, 40);
    assert!(!screen_pixel_is_set(
        &bus,
        screen_base,
        row_bytes,
        left_probe.0,
        left_probe.1
    ));
    assert!(!screen_pixel_is_set(
        &bus,
        screen_base,
        row_bytes,
        right_probe.0,
        right_probe.1
    ));

    bus.write_byte(left_ptr + 17, 1);
    let narrow_rgn = bus.alloc(10);
    bus.write_word(narrow_rgn, 10);
    bus.write_word(narrow_rgn + 2, 20);
    bus.write_word(narrow_rgn + 4, 20);
    bus.write_word(narrow_rgn + 6, 60);
    bus.write_word(narrow_rgn + 8, 120);
    let narrow_handle = bus.alloc(4);
    bus.write_long(narrow_handle, narrow_rgn);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, narrow_handle);
    bus.write_long(sp + 4, window_ptr);
    let sp_before = cpu.read_reg(Register::A7);
    disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
    assert!(
        screen_pixel_is_set(&bus, screen_base, row_bytes, left_probe.0, left_probe.1),
        "left control should redraw when its rect intersects the update region"
    );
    assert!(
        !screen_pixel_is_set(&bus, screen_base, row_bytes, right_probe.0, right_probe.1),
        "right control should remain unchanged when it does not intersect the update region"
    );

    bus.write_byte(right_ptr + 17, 1);
    let empty_rgn = bus.alloc(10);
    bus.write_word(empty_rgn, 10);
    bus.write_word(empty_rgn + 2, 0);
    bus.write_word(empty_rgn + 4, 0);
    bus.write_word(empty_rgn + 6, 0);
    bus.write_word(empty_rgn + 8, 0);
    let empty_handle = bus.alloc(4);
    bus.write_long(empty_handle, empty_rgn);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, empty_handle);
    bus.write_long(sp + 4, window_ptr);
    let sp_before = cpu.read_reg(Register::A7);
    disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
    assert!(
        screen_pixel_is_set(&bus, screen_base, row_bytes, left_probe.0, left_probe.1),
        "empty update regions should leave previously repainted controls unchanged"
    );
    assert!(
        !screen_pixel_is_set(&bus, screen_base, row_bytes, right_probe.0, right_probe.1),
        "empty update regions should not repaint controls outside the update region"
    );
}

#[test]
fn updtcontrol_out_of_range_window_pointer_is_noop_and_pops_args() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, bus.ram_size() + 0x1000);

    let sp_before = cpu.read_reg(Register::A7);
    disp.dispatch_control(true, 0x153, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before + 8,
        "UpdtControl should still consume its Pascal frame"
    );
}

// GetCVariant per Inside Macintosh Volume V (1986), p. V-222: returns the
// variation code packed in the low 4 bits of the control's procID (Inside
// Macintosh Volume I 1985, p. I-323: control definition ID = 16 *
// resourceID + variation_code).
fn install_control_for_variant_test(
    disp: &mut crate::trap::TrapDispatcher,
    bus: &mut MacMemoryBus,
    proc_id: i16,
) -> u32 {
    let ctrl_ptr = bus.alloc(296);
    let ctrl_handle = bus.alloc(4);
    bus.write_long(ctrl_handle, ctrl_ptr);
    disp.control_manager.set_proc_id(ctrl_ptr, proc_id);
    ctrl_handle
}

#[test]
fn getcvariant_returns_variation_code_from_low_four_bits_of_proc_id() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;

    // pushButProc = 0 → variant 0
    let h_button = install_control_for_variant_test(&mut disp, &mut bus, 0);
    // checkBoxProc = 1 → variant 1
    let h_check = install_control_for_variant_test(&mut disp, &mut bus, 1);
    // radioButProc = 2 → variant 2
    let h_radio = install_control_for_variant_test(&mut disp, &mut bus, 2);
    // scrollBarProc = 16 → variant 0 (CDEF resource 1, variant 0)
    let h_scroll = install_control_for_variant_test(&mut disp, &mut bus, 16);
    // popupMenuProc + popupFixedWidth = 1008 + 1 = 1009 → variant 1
    let h_popup_fixed = install_control_for_variant_test(&mut disp, &mut bus, 1009);

    for (handle, expected) in [
        (h_button, 0i16),
        (h_check, 1),
        (h_radio, 2),
        (h_scroll, 0),
        (h_popup_fixed, 1),
    ] {
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, handle);
        bus.write_word(sp + 4, 0xDEAD);
        disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_word(sp + 4) as i16,
            expected,
            "GetCVariant must return low 4 bits of procID; got wrong value"
        );
    }
}

#[test]
fn getcvariant_returns_zero_for_nil_control_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 0xBEEF);

    disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(
        bus.read_word(sp + 4) as i16,
        0,
        "NIL theControl must return 0 (no variant) without crashing"
    );
}

#[test]
fn getcvariant_function_protocol_pops_handle_and_writes_integer_result() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = 0x300000u32;
    let handle = install_control_for_variant_test(&mut disp, &mut bus, 2);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, handle);
    bus.write_word(sp + 4, 0xCAFE);
    bus.write_word(sp + 6, 0xBABE);

    disp.dispatch_control(true, 0x009, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    // A7 advances exactly 4 bytes (handle popped); INTEGER result at SP+4.
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_word(sp + 4) as i16, 2);
    // Sentinel just past the 2-byte result slot survives.
    assert_eq!(
        bus.read_word(sp + 6),
        0xBABE,
        "trap must not write past the 2-byte INTEGER result slot"
    );
}
