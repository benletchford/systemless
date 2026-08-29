//! Scripted, deterministic trap-interaction replays that emit input
//! traces. Each builds a full dispatcher/CPU/bus and drives menu/dialog/
//! control tracking through the Toolbox traps, so they reach deep into
//! dispatcher state and live in this crate, gated behind the off-by-default
//! `test-support` feature.

use crate::cpu::{M68kCpu, Register};
use crate::memory::{globals::addr, MacMemoryBus, MemoryBus};
use crate::menu_manager::TrackedMenuPaneView;
use crate::trap::dispatch::{DialogItem, QueuedEvent};
use crate::trap::TrapDispatcher;

const SCRIPT_SP: u32 = 0x100000;
const SCRIPT_PORT_PTR: u32 = 0x181000;

/// Replay a deterministic MenuSelect interaction that selects an enabled item.
///
/// Inside Macintosh Volume I, I-355 documents `MenuSelect` as a tracking call:
/// it takes a global start point, keeps control while the mouse button remains
/// down, highlights enabled menu items under the cursor, and returns the menu ID
/// in the high word plus the item number in the low word after release.
pub fn scripted_menuselect_enabled_item_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();
    dispatcher.menu_bar_hidden = false;
    bus.write_word(addr::MBAR_HEIGHT, 20);

    let menu_handle =
        scripted_new_menu(&mut dispatcher, &mut cpu, &mut bus, 520, 0x30B900, "File")?;
    scripted_append_menu(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        menu_handle,
        0x30B940,
        "Open/O;Close/W",
    )?;
    scripted_insert_menu(&mut dispatcher, &mut cpu, &mut bus, menu_handle)?;
    scripted_call_menu_trap(&mut dispatcher, &mut cpu, &mut bus, 0x137, "DrawMenuBar")?;

    let (_, title_left, title_right) = scripted_menu_title_regions(&dispatcher, &bus)
        .into_iter()
        .next()
        .ok_or_else(|| "scripted MenuSelect replay could not find menu title region".to_string())?;
    let title_mid_h = (title_left + title_right) / 2;

    dispatcher.push_mouse_down(10, title_mid_h);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_word(SCRIPT_SP, 10);
    bus.write_word(SCRIPT_SP + 2, title_mid_h as u16);
    bus.write_long(SCRIPT_SP + 4, 0xA5A5_A5A5);
    scripted_call_menu_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x13D,
        "MenuSelect start",
    )?;

    let (dropdown_top, dropdown_left, _, _) = dispatcher
        .menu_tracking
        .as_ref()
        .map(|tracking| tracking.dropdown_rect())
        .ok_or_else(|| "scripted MenuSelect replay did not enter tracking".to_string())?;
    dispatcher.set_mouse_position(dropdown_top + 17, dropdown_left + 8);
    scripted_call_menu_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x13D,
        "MenuSelect tracking update",
    )?;

    dispatcher.push_mouse_up(dropdown_top + 17, dropdown_left + 8);
    scripted_call_menu_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x13D,
        "MenuSelect release",
    )?;

    for _ in 0..40 {
        if dispatcher.menu_tracking.is_none() {
            break;
        }
        scripted_call_menu_trap(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0x13D,
            "MenuSelect flash",
        )?;
    }

    if dispatcher.menu_tracking.is_some() {
        return Err("scripted MenuSelect replay did not finish flash tracking".to_string());
    }
    let result = bus.read_long(SCRIPT_SP + 4);
    if result != 0x0208_0002 {
        return Err(format!(
            "scripted MenuSelect replay returned ${result:08X}, expected $02080002"
        ));
    }
    Ok(dispatcher.input_trace_text())
}

/// Replay a deterministic popup-menu `TrackControl` interaction.
///
/// Inside Macintosh Volume I, I-323 documents `TrackControl` as a tracking
/// call: it takes a local start point while the mouse is down, follows mouse
/// movement until release, and returns the tracked control part code. Systemless
/// also supports the popup-menu CDEF path used by classic controls; this replay
/// proves the popup tracking state crosses start, drag, highlight, and release.
pub fn scripted_trackcontrol_popup_menu_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();

    let menu_handle = scripted_new_menu(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        900,
        0x30BA00,
        "Squadies",
    )?;
    scripted_append_menu(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        menu_handle,
        0x30BA40,
        "Duke;Carnage",
    )?;

    let ctrl_handle = scripted_new_popup_control(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        SCRIPT_PORT_PTR,
        (10, 20, 30, 130),
        900,
        1,
        1009,
    )?;
    let ctrl_ptr = bus.read_long(ctrl_handle);
    if ctrl_ptr == 0 {
        return Err("scripted TrackControl popup replay created a NIL control".to_string());
    }

    dispatcher.push_mouse_down(15, 25);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, 0);
    bus.write_word(SCRIPT_SP + 4, 15);
    bus.write_word(SCRIPT_SP + 6, 25);
    bus.write_long(SCRIPT_SP + 8, ctrl_handle);
    bus.write_word(SCRIPT_SP + 12, 0xBEEF);
    scripted_call_control_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x168,
        "TrackControl popup start",
    )?;

    let (dropdown_top, dropdown_left, _, _) = dispatcher
        .control_tracking
        .as_ref()
        .map(|tracking| tracking.dropdown_rect)
        .ok_or_else(|| "scripted TrackControl popup replay did not enter tracking".to_string())?;
    if cpu.read_reg(Register::A7) != SCRIPT_SP {
        return Err(
            "scripted TrackControl popup replay popped the stack before release".to_string(),
        );
    }

    dispatcher.set_mouse_position(dropdown_top + 5, dropdown_left - 4);
    scripted_call_control_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x168,
        "TrackControl popup outside update",
    )?;

    dispatcher.set_mouse_position(dropdown_top + 17, dropdown_left + 5);
    scripted_call_control_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x168,
        "TrackControl popup item update",
    )?;

    dispatcher.push_mouse_up(dropdown_top + 17, dropdown_left + 5);
    scripted_call_control_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x168,
        "TrackControl popup release",
    )?;

    if dispatcher.control_tracking.is_some() {
        return Err("scripted TrackControl popup replay did not finish tracking".to_string());
    }
    let part = bus.read_word(SCRIPT_SP + 12);
    if part != 10 {
        return Err(format!(
            "scripted TrackControl popup replay returned part {part}, expected 10"
        ));
    }
    let value = bus.read_word(ctrl_ptr + 18);
    if value != 2 {
        return Err(format!(
            "scripted TrackControl popup replay left control value {value}, expected 2"
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 12 {
        return Err(format!(
            "scripted TrackControl popup replay left A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 12
        ));
    }

    Ok(dispatcher.input_trace_text())
}

/// Replay a deterministic `ModalDialog` button click.
///
/// Macintosh Toolbox Essentials 1992, pp. 6-83 to 6-88 describes
/// `ModalDialog` as the Dialog Manager event loop for modal dialogs. It waits
/// for user action, tracks button clicks, flashes a button selection, writes
/// `itemHit`, and returns to the caller.
pub fn scripted_modaldialog_button_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();

    let dialog_ptr = scripted_install_modal_button_dialog(
        &mut dispatcher,
        &mut bus,
        (60, 70, 180, 260),
        (70, 95, 94, 165),
    );
    let item_hit_ptr = 0x30BC00;
    bus.write_word(item_hit_ptr, 0xBEEF);

    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog start",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err("scripted ModalDialog replay did not enter tracking".to_string());
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP {
        return Err("scripted ModalDialog replay popped the stack before item hit".to_string());
    }

    dispatcher.push_mouse_down(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog mouseDown",
    )?;
    if dispatcher
        .dialog_tracking
        .as_ref()
        .and_then(|tracking| tracking.active_button.as_ref())
        .is_none()
    {
        return Err("scripted ModalDialog replay did not start button tracking".to_string());
    }

    dispatcher.set_mouse_position(142, 84);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog button drag outside",
    )?;

    dispatcher.set_mouse_position(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog button drag inside",
    )?;

    dispatcher.push_mouse_up(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog mouseUp",
    )?;

    for _ in 0..40 {
        if dispatcher.dialog_tracking.is_none() {
            break;
        }
        scripted_call_dialog_trap(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0x191,
            "ModalDialog flash",
        )?;
    }

    if dispatcher.dialog_tracking.is_some() {
        return Err("scripted ModalDialog replay did not finish flash tracking".to_string());
    }
    let item_hit = bus.read_word(item_hit_ptr);
    if item_hit != 1 {
        return Err(format!(
            "scripted ModalDialog replay returned itemHit {item_hit}, expected 1"
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 8 {
        return Err(format!(
            "scripted ModalDialog replay left A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 8
        ));
    }
    if dispatcher.front_window != dialog_ptr {
        return Err(
            "scripted ModalDialog replay changed the front dialog unexpectedly".to_string(),
        );
    }

    Ok(dispatcher.input_trace_text())
}

/// Replay a deterministic `ModalDialog` editText key entry followed by OK.
///
/// Inside Macintosh Volume I, I-415 and Macintosh Toolbox Essentials 1992,
/// pp. 6-135 to 6-137 document that `ModalDialog` uses TextEdit to handle
/// keyDown events in editable text items and returns the editable item number
/// when that item is enabled. The dialog remains visible so the application can
/// continue calling `ModalDialog` until OK or Cancel is selected.
pub fn scripted_modaldialog_edit_text_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();

    let (_dialog_ptr, edit_handle) = scripted_install_modal_edit_text_dialog(
        &mut dispatcher,
        &mut bus,
        (60, 70, 190, 280),
        (70, 95, 94, 165),
        (104, 40, 124, 180),
        "AB",
    );
    let item_hit_ptr = 0x30BC20;
    bus.write_word(item_hit_ptr, 0xBEEF);

    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog editText start",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err("scripted ModalDialog editText replay did not enter tracking".to_string());
    }

    dispatcher.push_key_down(0x06, b'Z');
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog editText keyDown",
    )?;
    if dispatcher.dialog_tracking.is_some() {
        return Err(
            "scripted ModalDialog editText replay did not return after enabled editText keyDown"
                .to_string(),
        );
    }
    let item_hit = bus.read_word(item_hit_ptr);
    if item_hit != 2 {
        return Err(format!(
            "scripted ModalDialog editText replay returned itemHit {item_hit}, expected 2"
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 8 {
        return Err(format!(
            "scripted ModalDialog editText replay left A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 8
        ));
    }
    let edited_text = scripted_text_handle_text(&bus, edit_handle);
    if edited_text != "Z" {
        return Err(format!(
            "scripted ModalDialog editText replay left text {edited_text:?}, expected \"Z\""
        ));
    }

    dispatcher.push_key_up(0x06, b'Z');
    bus.write_word(item_hit_ptr, 0xBEEF);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog editText re-enter",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err("scripted ModalDialog editText replay did not re-enter tracking".to_string());
    }

    dispatcher.push_mouse_down(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog editText OK mouseDown",
    )?;
    dispatcher.push_mouse_up(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog editText OK mouseUp",
    )?;
    for _ in 0..40 {
        if dispatcher.dialog_tracking.is_none() {
            break;
        }
        scripted_call_dialog_trap(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0x191,
            "ModalDialog editText OK flash",
        )?;
    }

    if dispatcher.dialog_tracking.is_some() {
        return Err(
            "scripted ModalDialog editText replay did not finish OK flash tracking".to_string(),
        );
    }
    let item_hit = bus.read_word(item_hit_ptr);
    if item_hit != 1 {
        return Err(format!(
            "scripted ModalDialog editText replay final itemHit {item_hit}, expected 1"
        ));
    }
    if scripted_text_handle_text(&bus, edit_handle) != "Z" {
        return Err("scripted ModalDialog editText replay lost edited text after OK".to_string());
    }

    Ok(dispatcher.input_trace_text())
}

/// Replay a deterministic `ModalDialog` filter return followed by re-entry.
///
/// Inside Macintosh Volume I, I-415 and Macintosh Toolbox Essentials 1992,
/// pp. 6-135 to 6-137 document that `ModalDialog` passes events to a modal
/// filter before default handling. A TRUE filter result returns the filter's
/// `itemHit`; a FALSE result lets the Dialog Manager handle the same event.
pub fn scripted_modaldialog_filter_retained_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();

    let dialog_ptr = scripted_install_modal_filter_dialog(
        &mut dispatcher,
        &mut bus,
        (60, 70, 190, 280),
        (70, 95, 94, 165),
        (104, 40, 134, 180),
    );
    let item_hit_ptr = 0x30BC40;
    let result_addr = 0x30BC48;
    let filter_proc = bus.alloc(8);
    bus.write_word(filter_proc, 0x4E56);
    dispatcher.dialog_filter_result_addr = result_addr;
    bus.write_word(item_hit_ptr, 0xBEEF);
    bus.write_word(result_addr, 0);

    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, filter_proc);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog filter start",
    )?;
    if !dispatcher
        .dialog_tracking
        .as_ref()
        .is_some_and(|tracking| tracking.filter_proc == filter_proc)
    {
        return Err("scripted ModalDialog filter replay did not enter filter tracking".to_string());
    }

    dispatcher.push_key_down(0x0C, b'Q');
    let key_event = dispatcher
        .event_queue
        .pop_back()
        .ok_or_else(|| "scripted ModalDialog filter replay did not queue keyDown".to_string())?;
    scripted_set_modal_filter_event(&mut dispatcher, key_event)?;
    bus.write_word(result_addr, 0xFFFF);
    bus.write_word(item_hit_ptr, 2);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog filter keyDown handled",
    )?;
    if dispatcher.dialog_tracking.is_some() {
        return Err(
            "scripted ModalDialog filter replay did not return after TRUE filter".to_string(),
        );
    }
    if bus.read_word(item_hit_ptr) != 2 {
        return Err(format!(
            "scripted ModalDialog filter replay returned itemHit {}, expected 2",
            bus.read_word(item_hit_ptr)
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 8 {
        return Err(format!(
            "scripted ModalDialog filter replay left A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 8
        ));
    }
    if !dispatcher
        .dialog_visible_snapshots
        .contains_key(&dialog_ptr)
    {
        return Err("scripted ModalDialog filter replay did not retain visible dialog".to_string());
    }

    dispatcher.push_key_up(0x0C, b'Q');
    dispatcher.event_queue.pop_back();

    bus.write_word(item_hit_ptr, 0xBEEF);
    bus.write_word(result_addr, 0);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, filter_proc);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog filter re-enter",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err("scripted ModalDialog filter replay did not re-enter tracking".to_string());
    }

    dispatcher.push_mouse_down(142, 200);
    let mouse_down_event = dispatcher
        .event_queue
        .pop_back()
        .ok_or_else(|| "scripted ModalDialog filter replay did not queue mouseDown".to_string())?;
    scripted_set_modal_filter_event(&mut dispatcher, mouse_down_event)?;
    bus.write_word(result_addr, 0);
    bus.write_word(item_hit_ptr, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog filter mouseDown declined",
    )?;
    if dispatcher
        .dialog_tracking
        .as_ref()
        .and_then(|tracking| tracking.active_button.as_ref())
        .is_none()
    {
        return Err(
            "scripted ModalDialog filter replay did not start button tracking after FALSE filter"
                .to_string(),
        );
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP {
        return Err(
            "scripted ModalDialog filter replay returned before button release".to_string(),
        );
    }

    dispatcher.push_mouse_up(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog filter OK mouseUp",
    )?;
    for _ in 0..40 {
        if dispatcher.dialog_tracking.is_none() {
            break;
        }
        scripted_call_dialog_trap(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0x191,
            "ModalDialog filter OK flash",
        )?;
    }

    if dispatcher.dialog_tracking.is_some() {
        return Err("scripted ModalDialog filter replay did not finish OK tracking".to_string());
    }
    if bus.read_word(item_hit_ptr) != 1 {
        return Err(format!(
            "scripted ModalDialog filter replay final itemHit {}, expected 1",
            bus.read_word(item_hit_ptr)
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 8 {
        return Err(format!(
            "scripted ModalDialog filter replay final A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 8
        ));
    }

    Ok(dispatcher.input_trace_text())
}

/// Replay a Preferences-style retained modal loop with an app-mutated checkbox.
///
/// Inside Macintosh Volume I, I-415 documents `ModalDialog` returning the item
/// hit in a modal dialog, while Macintosh Toolbox Essentials 1992, pp. 6-148
/// to 6-153 defines checkbox dialog items and their Dialog Manager item
/// numbers. Many apps respond to a non-OK item by updating control state and
/// calling `ModalDialog` again while the same dialog remains visible.
pub fn scripted_modaldialog_preferences_checkbox_input_trace() -> Result<String, String> {
    let (mut dispatcher, mut cpu, mut bus) = scripted_menu_setup();
    dispatcher.enable_input_trace_capture();

    let bounds = (60, 70, 190, 300);
    let (dialog_ptr, checkbox_handle) = scripted_install_modal_preferences_checkbox_dialog(
        &mut dispatcher,
        &mut bus,
        bounds,
        (70, 95, 94, 165),
        (104, 40, 124, 180),
        "Strict play",
    );
    let item_hit_ptr = 0x30BC60;
    bus.write_word(item_hit_ptr, 0xBEEF);

    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog preferences start",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err("scripted ModalDialog preferences replay did not enter tracking".to_string());
    }

    dispatcher.push_mouse_down(174, 180);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog preferences checkbox mouseDown",
    )?;
    if dispatcher.dialog_tracking.is_some() {
        return Err(
            "scripted ModalDialog preferences replay did not return after checkbox hit".to_string(),
        );
    }
    if bus.read_word(item_hit_ptr) != 2 {
        return Err(format!(
            "scripted ModalDialog preferences replay returned itemHit {}, expected 2",
            bus.read_word(item_hit_ptr)
        ));
    }
    if cpu.read_reg(Register::A7) != SCRIPT_SP + 8 {
        return Err(format!(
            "scripted ModalDialog preferences replay left A7=${:08X}, expected ${:08X}",
            cpu.read_reg(Register::A7),
            SCRIPT_SP + 8
        ));
    }
    if !dispatcher
        .dialog_visible_snapshots
        .contains_key(&dialog_ptr)
    {
        return Err(
            "scripted ModalDialog preferences replay did not retain visible dialog".to_string(),
        );
    }

    dispatcher.push_mouse_up(174, 180);
    dispatcher.event_queue.pop_back();
    scripted_set_control_value(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        checkbox_handle,
        1,
        "SetCtlValue preferences checkbox",
    )?;
    if dispatcher
        .dialog_control_values
        .get(&(dialog_ptr, 2))
        .copied()
        != Some(1)
    {
        return Err("scripted ModalDialog preferences replay did not store checkbox value".into());
    }
    let checkbox_ptr = bus.read_long(checkbox_handle);
    if checkbox_ptr == 0 || bus.read_word(checkbox_ptr + 18) != 1 {
        return Err(
            "scripted ModalDialog preferences replay did not update ControlRecord value"
                .to_string(),
        );
    }
    scripted_record_modal_app_control_value_trace(
        &mut dispatcher,
        &bus,
        dialog_ptr,
        bounds,
        2,
        Some(5),
        checkbox_handle,
        1,
        "checkbox_value_toggled",
    );

    bus.write_word(item_hit_ptr, 0xBEEF);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, item_hit_ptr);
    bus.write_long(SCRIPT_SP + 4, 0);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog preferences re-enter",
    )?;
    if dispatcher.dialog_tracking.is_none() {
        return Err(
            "scripted ModalDialog preferences replay did not re-enter tracking".to_string(),
        );
    }
    if dispatcher
        .dialog_control_values
        .get(&(dialog_ptr, 2))
        .copied()
        != Some(1)
    {
        return Err(
            "scripted ModalDialog preferences replay lost checkbox value on re-entry".to_string(),
        );
    }

    dispatcher.push_mouse_down(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog preferences OK mouseDown",
    )?;
    if dispatcher
        .dialog_tracking
        .as_ref()
        .and_then(|tracking| tracking.active_button.as_ref())
        .is_none()
    {
        return Err(
            "scripted ModalDialog preferences replay did not start OK tracking".to_string(),
        );
    }
    dispatcher.push_mouse_up(142, 200);
    scripted_call_dialog_trap(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0x191,
        "ModalDialog preferences OK mouseUp",
    )?;
    for _ in 0..40 {
        if dispatcher.dialog_tracking.is_none() {
            break;
        }
        scripted_call_dialog_trap(
            &mut dispatcher,
            &mut cpu,
            &mut bus,
            0x191,
            "ModalDialog preferences OK flash",
        )?;
    }

    if dispatcher.dialog_tracking.is_some() {
        return Err(
            "scripted ModalDialog preferences replay did not finish OK tracking".to_string(),
        );
    }
    if bus.read_word(item_hit_ptr) != 1 {
        return Err(format!(
            "scripted ModalDialog preferences replay final itemHit {}, expected 1",
            bus.read_word(item_hit_ptr)
        ));
    }
    if dispatcher
        .dialog_control_values
        .get(&(dialog_ptr, 2))
        .copied()
        != Some(1)
    {
        return Err(
            "scripted ModalDialog preferences replay lost final checkbox value".to_string(),
        );
    }

    Ok(dispatcher.input_trace_text())
}

fn scripted_menu_setup() -> (TrapDispatcher, M68kCpu, MacMemoryBus) {
    let mut dispatcher = TrapDispatcher::new();
    dispatcher.scrap_clipboard_writable = true;
    let mut cpu = M68kCpu::new();
    let mut bus = MacMemoryBus::new(4 * 1024 * 1024);

    cpu.write_reg(Register::A7, SCRIPT_SP);
    let a5_addr = 0x180000u32;
    let qd_globals = 0x180004u32;
    cpu.write_reg(Register::A5, a5_addr);
    bus.write_long(a5_addr, qd_globals);
    bus.write_long(0x016A, 100);
    // MBState ($0172) defaults to mouse-up after startup; keep scripted
    // dispatcher-only mouse transitions from inheriting zero-filled RAM as a
    // held button. Inside Macintosh Volume II, II-371.
    bus.write_byte(addr::MB_STATE, 0x80);
    bus.write_word(
        addr::MENU_FLASH,
        crate::memory::globals::DEFAULT_MENU_FLASH_COUNT,
    );
    dispatcher.tick_count = 100;

    let screen_base = 0x300000u32;
    let row_bytes = 64u32;
    dispatcher.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
    bus.write_long(0x0824, screen_base);
    bus.write_word(0x0828, row_bytes as u16);

    let port_ptr = SCRIPT_PORT_PTR;
    bus.write_word(port_ptr, 0);
    bus.write_long(port_ptr + 2, screen_base);
    bus.write_word(port_ptr + 6, row_bytes as u16);
    bus.write_word(port_ptr + 8, 0);
    bus.write_word(port_ptr + 10, 0);
    bus.write_word(port_ptr + 12, 342);
    bus.write_word(port_ptr + 14, 512);
    bus.write_word(port_ptr + 16, 0);
    bus.write_word(port_ptr + 18, 0);
    bus.write_word(port_ptr + 20, 342);
    bus.write_word(port_ptr + 22, 512);

    let vis_rgn = 0x182000u32;
    bus.write_word(vis_rgn, 10);
    bus.write_word(vis_rgn + 2, 0);
    bus.write_word(vis_rgn + 4, 0);
    bus.write_word(vis_rgn + 6, 342);
    bus.write_word(vis_rgn + 8, 512);
    let vis_rgn_handle = 0x182100u32;
    bus.write_long(vis_rgn_handle, vis_rgn);
    bus.write_long(port_ptr + 24, vis_rgn_handle);

    let clip_rgn = 0x182200u32;
    bus.write_word(clip_rgn, 10);
    bus.write_word(clip_rgn + 2, 0);
    bus.write_word(clip_rgn + 4, 0);
    bus.write_word(clip_rgn + 6, 342);
    bus.write_word(clip_rgn + 8, 512);
    let clip_rgn_handle = 0x182300u32;
    bus.write_long(clip_rgn_handle, clip_rgn);
    bus.write_long(port_ptr + 28, clip_rgn_handle);

    bus.write_bytes(port_ptr + 32, &[0x00; 8]);
    bus.write_bytes(port_ptr + 40, &[0xFF; 8]);
    bus.write_long(port_ptr + 48, 0);
    bus.write_word(port_ptr + 52, 1);
    bus.write_word(port_ptr + 54, 1);
    bus.write_word(port_ptr + 56, 8);
    bus.write_bytes(port_ptr + 58, &[0xFF; 8]);
    bus.write_word(port_ptr + 66, 0);
    bus.write_word(port_ptr + 68, 0);
    bus.write_word(port_ptr + 70, 0);
    bus.write_word(port_ptr + 72, 1);
    bus.write_word(port_ptr + 74, 0);

    bus.write_long(qd_globals, port_ptr);
    dispatcher.set_current_port_for_test(port_ptr);
    (dispatcher, cpu, bus)
}

fn scripted_install_modal_button_dialog(
    dispatcher: &mut TrapDispatcher,
    bus: &mut MacMemoryBus,
    bounds: (i16, i16, i16, i16),
    button_rect: (i16, i16, i16, i16),
) -> u32 {
    let dialog_ptr = bus.alloc(256);
    let height = bounds.2 - bounds.0;
    let width = bounds.3 - bounds.1;
    bus.write_word(dialog_ptr + 6, 0);
    bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
    bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
    bus.write_word(dialog_ptr + 16, 0);
    bus.write_word(dialog_ptr + 18, 0);
    bus.write_word(dialog_ptr + 20, height as u16);
    bus.write_word(dialog_ptr + 22, width as u16);
    bus.write_word(dialog_ptr + 164, 0xFFFF);
    bus.write_word(dialog_ptr + 168, 1);

    dispatcher.front_window = dialog_ptr;
    dispatcher.window_bounds = bounds;
    dispatcher.window_proc_id = 1;
    dispatcher.window_title = "Modal Trace".to_string();
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![DialogItem {
            item_type: 4,
            rect: button_rect,
            text: "OK".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }],
    );
    dialog_ptr
}

fn scripted_install_modal_filter_dialog(
    dispatcher: &mut TrapDispatcher,
    bus: &mut MacMemoryBus,
    bounds: (i16, i16, i16, i16),
    button_rect: (i16, i16, i16, i16),
    user_item_rect: (i16, i16, i16, i16),
) -> u32 {
    let dialog_ptr = bus.alloc(256);
    let height = bounds.2 - bounds.0;
    let width = bounds.3 - bounds.1;
    bus.write_word(dialog_ptr + 6, 0);
    bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
    bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
    bus.write_word(dialog_ptr + 16, 0);
    bus.write_word(dialog_ptr + 18, 0);
    bus.write_word(dialog_ptr + 20, height as u16);
    bus.write_word(dialog_ptr + 22, width as u16);
    bus.write_word(dialog_ptr + 164, 0xFFFF);
    bus.write_word(dialog_ptr + 168, 1);

    dispatcher.front_window = dialog_ptr;
    dispatcher.window_bounds = bounds;
    dispatcher.window_proc_id = 1;
    dispatcher.window_title = "Modal Filter Trace".to_string();
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![
            DialogItem {
                item_type: 4,
                rect: button_rect,
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0,
                rect: user_item_rect,
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ],
    );
    dialog_ptr
}

fn scripted_install_modal_preferences_checkbox_dialog(
    dispatcher: &mut TrapDispatcher,
    bus: &mut MacMemoryBus,
    bounds: (i16, i16, i16, i16),
    button_rect: (i16, i16, i16, i16),
    checkbox_rect: (i16, i16, i16, i16),
    checkbox_title: &str,
) -> (u32, u32) {
    let dialog_ptr = bus.alloc(256);
    let height = bounds.2 - bounds.0;
    let width = bounds.3 - bounds.1;
    bus.write_word(dialog_ptr + 6, 0);
    bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
    bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
    bus.write_word(dialog_ptr + 16, 0);
    bus.write_word(dialog_ptr + 18, 0);
    bus.write_word(dialog_ptr + 20, height as u16);
    bus.write_word(dialog_ptr + 22, width as u16);
    bus.write_word(dialog_ptr + 164, 0xFFFF);
    bus.write_word(dialog_ptr + 168, 1);

    let title_len = checkbox_title.len().min(255);
    let checkbox_ptr = bus.alloc(42 + title_len as u32);
    bus.write_long(checkbox_ptr, 0);
    bus.write_long(checkbox_ptr + 4, dialog_ptr);
    bus.write_word(checkbox_ptr + 8, checkbox_rect.0 as u16);
    bus.write_word(checkbox_ptr + 10, checkbox_rect.1 as u16);
    bus.write_word(checkbox_ptr + 12, checkbox_rect.2 as u16);
    bus.write_word(checkbox_ptr + 14, checkbox_rect.3 as u16);
    bus.write_byte(checkbox_ptr + 16, 255);
    bus.write_byte(checkbox_ptr + 17, 0);
    bus.write_word(checkbox_ptr + 18, 0);
    bus.write_word(checkbox_ptr + 20, 0);
    bus.write_word(checkbox_ptr + 22, 1);
    bus.write_byte(checkbox_ptr + 40, title_len as u8);
    bus.write_bytes(checkbox_ptr + 41, &checkbox_title.as_bytes()[..title_len]);

    let checkbox_handle = bus.alloc(4);
    bus.write_long(checkbox_handle, checkbox_ptr);

    dispatcher.front_window = dialog_ptr;
    dispatcher.window_bounds = bounds;
    dispatcher.window_proc_id = 1;
    dispatcher.window_title = "Preferences".to_string();
    dispatcher.control_proc_ids.insert(checkbox_ptr, 1);
    dispatcher
        .dialog_control_handles
        .insert(checkbox_handle, (dialog_ptr, 2));
    dispatcher.dialog_control_values.insert((dialog_ptr, 2), 0);
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![
            DialogItem {
                item_type: 4,
                rect: button_rect,
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 5,
                rect: checkbox_rect,
                text: checkbox_title.to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ],
    );
    (dialog_ptr, checkbox_handle)
}

fn scripted_install_modal_edit_text_dialog(
    dispatcher: &mut TrapDispatcher,
    bus: &mut MacMemoryBus,
    bounds: (i16, i16, i16, i16),
    button_rect: (i16, i16, i16, i16),
    edit_rect: (i16, i16, i16, i16),
    edit_text: &str,
) -> (u32, u32) {
    let dialog_ptr = bus.alloc(256);
    let height = bounds.2 - bounds.0;
    let width = bounds.3 - bounds.1;
    bus.write_word(dialog_ptr + 6, 0);
    bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
    bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
    bus.write_word(dialog_ptr + 16, 0);
    bus.write_word(dialog_ptr + 18, 0);
    bus.write_word(dialog_ptr + 20, height as u16);
    bus.write_word(dialog_ptr + 22, width as u16);
    bus.write_word(dialog_ptr + 164, 1);
    bus.write_word(dialog_ptr + 168, 1);

    let edit_bytes = edit_text.as_bytes();
    let edit_data_len = edit_bytes.len();
    let edit_handle = bus.alloc(4);
    let edit_ptr = bus.alloc(edit_data_len as u32);
    bus.write_bytes(edit_ptr, edit_bytes);
    bus.write_long(edit_handle, edit_ptr);

    let items_handle = bus.alloc(4);
    let button_data_len = 2usize;
    let button_padded_len = (button_data_len + 1) & !1;
    let edit_padded_len = (edit_data_len + 1) & !1;
    let ditl_len = 2 + (4 + 8 + 2 + button_padded_len) + (4 + 8 + 2 + edit_padded_len);
    let ditl_ptr = bus.alloc(ditl_len as u32);
    bus.write_long(items_handle, ditl_ptr);
    bus.write_long(dialog_ptr + 156, items_handle);
    bus.write_word(ditl_ptr, 1);

    let mut offset = ditl_ptr + 2;
    bus.write_long(offset, 0);
    offset += 4;
    for value in [button_rect.0, button_rect.1, button_rect.2, button_rect.3] {
        bus.write_word(offset, value as u16);
        offset += 2;
    }
    bus.write_byte(offset, 4);
    bus.write_byte(offset + 1, 2);
    offset += 2;
    bus.write_bytes(offset, b"OK");
    offset += button_padded_len as u32;

    bus.write_long(offset, edit_handle);
    offset += 4;
    for value in [edit_rect.0, edit_rect.1, edit_rect.2, edit_rect.3] {
        bus.write_word(offset, value as u16);
        offset += 2;
    }
    bus.write_byte(offset, 16);
    bus.write_byte(offset + 1, edit_data_len as u8);
    offset += 2;
    bus.write_bytes(offset, edit_bytes);

    dispatcher.front_window = dialog_ptr;
    dispatcher.window_bounds = bounds;
    dispatcher.window_proc_id = 1;
    dispatcher.window_title = "Modal Edit Trace".to_string();
    dispatcher
        .dialog_item_handles
        .insert(edit_handle, (dialog_ptr, 1));
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![
            DialogItem {
                item_type: 4,
                rect: button_rect,
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 16,
                rect: edit_rect,
                text: edit_text.to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: edit_data_len.min(i16::MAX as usize) as i16,
            },
        ],
    );
    (dialog_ptr, edit_handle)
}

fn scripted_set_modal_filter_event(
    dispatcher: &mut TrapDispatcher,
    event: QueuedEvent,
) -> Result<(), String> {
    let Some(tracking) = dispatcher.dialog_tracking.as_mut() else {
        return Err("scripted ModalDialog filter replay has no active tracking".to_string());
    };
    tracking.last_filter_event = Some(event);
    Ok(())
}

fn scripted_text_handle_text(bus: &MacMemoryBus, handle: u32) -> String {
    let ptr = bus.read_long(handle);
    if ptr == 0 {
        return String::new();
    }
    let len = bus.get_alloc_size(ptr).unwrap_or(0) as usize;
    String::from_utf8_lossy(&bus.read_bytes(ptr, len)).to_string()
}

fn scripted_trace_nonzero(value: u32) -> String {
    if value == 0 {
        "$00000000".to_string()
    } else {
        "$NONZERO".to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn scripted_record_modal_app_control_value_trace(
    dispatcher: &mut TrapDispatcher,
    bus: &MacMemoryBus,
    dialog_ptr: u32,
    bounds: (i16, i16, i16, i16),
    item_no: i16,
    item_type: Option<u8>,
    ctrl_handle: u32,
    value: i16,
    outcome: &str,
) {
    let item_type = item_type
        .map(|value| format!("${value:02X}"))
        .unwrap_or_else(|| "none".to_string());
    let ctrl_ptr = bus.read_long(ctrl_handle);
    dispatcher.record_input_trace_line(format!(
        "A991 action=app_set_control_value live_mouse=({},{}) {} dialog={} bounds=({},{},{},{}) item_hit={} item_type={} control_handle={} control_ptr={} control_value={} result=app outcome={}",
        dispatcher.mouse_pos.0,
        dispatcher.mouse_pos.1,
        dispatcher.input_trace_state_fields(),
        scripted_trace_nonzero(dialog_ptr),
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3,
        item_no,
        item_type,
        scripted_trace_nonzero(ctrl_handle),
        scripted_trace_nonzero(ctrl_ptr),
        value,
        outcome,
    ));
}

fn scripted_call_control_trap(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    trap_num: u16,
    label: &str,
) -> Result<(), String> {
    dispatcher
        .dispatch_control(true, trap_num, cpu, bus)
        .ok_or_else(|| format!("{label} was not handled"))?
        .map_err(|err| format!("{label} failed: {err}"))
}

fn scripted_set_control_value(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    ctrl_handle: u32,
    value: i16,
    label: &str,
) -> Result<(), String> {
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_word(SCRIPT_SP, value as u16);
    bus.write_long(SCRIPT_SP + 2, ctrl_handle);
    scripted_call_control_trap(dispatcher, cpu, bus, 0x163, label)
}

fn scripted_call_dialog_trap(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    trap_num: u16,
    label: &str,
) -> Result<(), String> {
    dispatcher
        .dispatch_dialog(true, trap_num, cpu, bus)
        .ok_or_else(|| format!("{label} was not handled"))?
        .map_err(|err| format!("{label} failed: {err}"))
}

fn scripted_new_popup_control(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    window_ptr: u32,
    bounds: (i16, i16, i16, i16),
    menu_id: i16,
    value: i16,
    proc_id: i16,
) -> Result<u32, String> {
    let bounds_ptr = 0x30BB00;
    let title_ptr = 0x30BB20;
    bus.write_word(bounds_ptr, bounds.0 as u16);
    bus.write_word(bounds_ptr + 2, bounds.1 as u16);
    bus.write_word(bounds_ptr + 4, bounds.2 as u16);
    bus.write_word(bounds_ptr + 6, bounds.3 as u16);
    scripted_write_pstring(bus, title_ptr, "");

    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, 0);
    bus.write_word(SCRIPT_SP + 4, proc_id as u16);
    bus.write_word(SCRIPT_SP + 6, 0);
    bus.write_word(SCRIPT_SP + 8, menu_id as u16);
    bus.write_word(SCRIPT_SP + 10, value as u16);
    bus.write_byte(SCRIPT_SP + 12, 1);
    bus.write_long(SCRIPT_SP + 14, title_ptr);
    bus.write_long(SCRIPT_SP + 18, bounds_ptr);
    bus.write_long(SCRIPT_SP + 22, window_ptr);

    scripted_call_control_trap(dispatcher, cpu, bus, 0x154, "NewControl popup")?;
    Ok(bus.read_long(cpu.read_reg(Register::A7)))
}

fn scripted_write_pstring(bus: &mut MacMemoryBus, ptr: u32, s: &str) {
    let bytes = s.as_bytes();
    bus.write_byte(ptr, bytes.len().min(255) as u8);
    for (i, b) in bytes.iter().take(255).enumerate() {
        bus.write_byte(ptr + 1 + i as u32, *b);
    }
}

fn scripted_call_menu_trap(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    trap_num: u16,
    label: &str,
) -> Result<(), String> {
    dispatcher
        .dispatch_menu(true, trap_num, cpu, bus)
        .ok_or_else(|| format!("{label} was not handled"))?
        .map_err(|err| format!("{label} failed: {err}"))
}

fn scripted_new_menu(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    menu_id: i16,
    title_ptr: u32,
    title: &str,
) -> Result<u32, String> {
    scripted_write_pstring(bus, title_ptr, title);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, title_ptr);
    bus.write_word(SCRIPT_SP + 4, menu_id as u16);
    scripted_call_menu_trap(dispatcher, cpu, bus, 0x131, "NewMenu")?;
    Ok(bus.read_long(cpu.read_reg(Register::A7)))
}

fn scripted_append_menu(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    menu_handle: u32,
    data_ptr: u32,
    data: &str,
) -> Result<(), String> {
    scripted_write_pstring(bus, data_ptr, data);
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_long(SCRIPT_SP, data_ptr);
    bus.write_long(SCRIPT_SP + 4, menu_handle);
    scripted_call_menu_trap(dispatcher, cpu, bus, 0x133, "AppendMenu")
}

fn scripted_insert_menu(
    dispatcher: &mut TrapDispatcher,
    cpu: &mut M68kCpu,
    bus: &mut MacMemoryBus,
    menu_handle: u32,
) -> Result<(), String> {
    cpu.write_reg(Register::A7, SCRIPT_SP);
    bus.write_word(SCRIPT_SP, 0);
    bus.write_long(SCRIPT_SP + 2, menu_handle);
    scripted_call_menu_trap(dispatcher, cpu, bus, 0x135, "InsertMenu")
}

fn scripted_menu_title_regions(
    dispatcher: &TrapDispatcher,
    bus: &MacMemoryBus,
) -> Vec<(usize, i16, i16)> {
    dispatcher
        .current_menu_title_regions_with_indices(bus)
        .into_iter()
        .map(|(index, region)| (index, region.left, region.right))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_menuselect_enabled_item_trace_records_full_tracking_lifecycle() {
        let trace = scripted_menuselect_enabled_item_input_trace().unwrap();

        assert!(trace.contains("A93D action=start"));
        assert!(trace.contains("outcome=open_tracking"));
        assert!(trace.contains("A93D action=tracking_entered"));
        assert!(trace.contains("tracking=menu:active"));
        assert!(trace.contains("A93D action=tracking_update"));
        assert!(
            trace.contains("highlighted_item=2 result=pending outcome=enabled_item_highlighted")
        );
        assert!(trace.contains("A93D action=release"));
        assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=start_flash"));
        assert!(trace.contains("A93D action=finish"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=enabled_item_selected"));
    }

    #[test]
    fn scripted_trackcontrol_popup_menu_trace_records_full_tracking_lifecycle() {
        let trace = scripted_trackcontrol_popup_menu_input_trace().unwrap();

        assert!(trace.contains("A968 action=start"));
        assert!(trace.contains("outcome=open_popup_tracking"));
        assert!(trace.contains("A968 action=tracking_update"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:active"));
        assert!(trace.contains("highlighted_item=0 outcome=popup_no_item"));
        assert!(trace.contains("highlighted_item=2 outcome=popup_item_highlighted"));
        assert!(trace.contains("A968 action=tracking_finish"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("part=10 highlighted_item=2 outcome=popup_item_selected"));
    }

    #[test]
    fn scripted_modaldialog_button_trace_records_full_tracking_lifecycle() {
        let trace = scripted_modaldialog_button_input_trace().unwrap();

        assert!(trace.contains("A991 action=start"));
        assert!(trace.contains("outcome=open_modal_tracking"));
        assert!(trace.contains("A991 action=mouse_down"));
        assert!(trace.contains("item_hit=1 item_type=$04 highlighted=true"));
        assert!(trace.contains("outcome=button_tracking_started"));
        assert!(trace.contains("A991 action=tracking_update"));
        assert!(trace.contains("highlighted=false result=pending outcome=button_unhighlighted"));
        assert!(trace.contains("highlighted=true result=pending outcome=button_highlighted"));
        assert!(trace.contains("A991 action=release"));
        assert!(trace.contains("highlighted=true result=pending outcome=start_flash"));
        assert!(trace.contains("A991 action=finish"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains(
            "item_hit=1 item_type=$04 highlighted=none result=returned outcome=button_item_hit"
        ));
    }

    #[test]
    fn scripted_modaldialog_edit_text_trace_records_key_return_and_reentry() {
        let trace = scripted_modaldialog_edit_text_input_trace().unwrap();

        assert!(trace.matches("A991 action=start").count() >= 2);
        assert!(trace.contains("outcome=open_modal_tracking"));
        assert!(trace.contains("A991 action=key_down"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("edit_item=2 item_type=$10"));
        assert!(trace.contains("key_code=$06 char_code=$5A"));
        assert!(trace.contains("text_before=hex:4142 text_after=hex:5A"));
        assert!(trace.contains("result=returned outcome=enabled_edittext_item_hit"));
        assert!(trace.contains("A991 action=mouse_down"));
        assert!(trace.contains("outcome=button_tracking_started"));
        assert!(trace.contains("A991 action=finish"));
        assert!(trace.contains(
            "item_hit=1 item_type=$04 highlighted=none result=returned outcome=button_item_hit"
        ));
    }

    #[test]
    fn scripted_modaldialog_filter_retained_trace_records_return_and_passthrough() {
        let trace = scripted_modaldialog_filter_retained_input_trace().unwrap();

        assert!(trace.matches("A991 action=start").count() >= 2);
        assert!(trace.contains("A991 action=filter_result"));
        assert!(trace.contains("event=keyDown(3)"));
        assert!(trace.contains("message=$00000C51"));
        assert!(trace.contains("item_hit=2 item_type=$00"));
        assert!(
            trace.contains("dialog_retained=true result=returned outcome=filter_item_hit_retained")
        );
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("event=mouseDown(1)"));
        assert!(trace.contains("result=passed outcome=filter_declined"));
        assert!(trace.contains("A991 action=mouse_down"));
        assert!(trace.contains("outcome=button_tracking_started"));
        assert!(trace.contains("A991 action=finish"));
        assert!(trace.contains(
            "item_hit=1 item_type=$04 highlighted=none result=returned outcome=button_item_hit"
        ));
    }

    #[test]
    fn scripted_modaldialog_preferences_checkbox_trace_records_app_retained_loop() {
        let trace = scripted_modaldialog_preferences_checkbox_input_trace().unwrap();

        assert!(trace.matches("A991 action=start").count() >= 2);
        assert!(trace.contains("A991 action=mouse_down"));
        assert!(trace.contains(
            "item_hit=2 item_type=$05 highlighted=none result=returned outcome=checkbox_item_hit_retained"
        ));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("A991 action=app_set_control_value"));
        assert!(trace.contains("item_hit=2 item_type=$05"));
        assert!(trace.contains("control_value=1 result=app outcome=checkbox_value_toggled"));
        assert!(trace.contains("A991 action=finish"));
        assert!(trace.contains(
            "item_hit=1 item_type=$04 highlighted=none result=returned outcome=button_item_hit"
        ));
    }
}
