use super::super::dispatch::{QueuedEvent, TrapDispatcher};
use super::super::test_helpers::setup;
use crate::cpu::{CpuOps, Register};
use crate::memory::globals::addr;
use crate::memory::{MacMemoryBus, MemoryBus};

// ---- FlushEvents ($A032) ----

#[test]
fn native_menu_mouse_down_is_latched_and_rate_limited_until_menuselect() {
    let (mut disp, _cpu, mut bus) = setup();
    disp.set_tick_count_for_test(&mut bus, 100);
    disp.pending_native_menu_event = Some(QueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 10,
        where_h: 42,
        modifiers: 0,
    });

    let first = disp.dequeue_event(&bus, 1 << 1);
    assert!(first.6);
    assert_eq!((first.0, first.3, first.4), (1, 10, 42));
    assert!(disp.pending_native_menu_event.is_some());

    let same_tick = disp.dequeue_event(&bus, 1 << 1);
    assert!(!same_tick.6, "latched click must not spin an event loop");

    let next_tick = disp.current_tick().wrapping_add(1);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    let next_tick = disp.dequeue_event(&bus, 1 << 1);
    assert!(next_tick.6, "ignored menu click must be presented again");
    assert!(disp.pending_native_menu_event.is_some());
}

#[test]
fn native_menu_mouse_down_follows_older_low_level_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_tick_count_for_test(&mut bus, 100);
    disp.event_queue.push_back(QueuedEvent {
        what: 3,
        message: 0x1234,
        when: 0,
        where_v: 20,
        where_h: 30,
        modifiers: 0,
    });
    disp.pending_native_menu_event = Some(QueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 10,
        where_h: 42,
        modifiers: 0,
    });

    assert_eq!(disp.peek_event(&bus, u16::MAX).unwrap().what, 3);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (3, 0x1234));
    let event = disp.dequeue_event(&bus, u16::MAX);
    assert_eq!((event.0, event.3, event.4), (1, 10, 42));

    let next_tick = disp.current_tick().wrapping_add(1);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    disp.event_queue.push_back(QueuedEvent {
        what: 4,
        message: 0x5678,
        when: 0,
        where_v: 20,
        where_h: 30,
        modifiers: 0,
    });
    assert_eq!(disp.peek_event(&bus, u16::MAX).unwrap().what, 1);
    assert_eq!(disp.dequeue_event(&bus, u16::MAX).0, 1);
}

#[test]
fn flush_events_clears_queue_and_sets_d0_zero() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Push a couple of events into the queue
    disp.event_queue.push_back(QueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 10,
        where_h: 20,
        modifiers: 0,
    });
    disp.event_queue.push_back(QueuedEvent {
        what: 3,
        message: 42,
        when: 0,
        where_v: 30,
        where_h: 40,
        modifiers: 0,
    });
    assert_eq!(disp.event_queue.len(), 2);

    // D0 low-order word = eventMask, high-order word = stopMask (IM:II II-69)
    cpu.write_reg(Register::D0, 0xFFFFu32);

    let result = disp.dispatch_event(false, 0x32, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert!(disp.event_queue.is_empty());
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn initevents_returns_noerr_and_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_3000;
    cpu.write_reg(Register::D0, 0xFFFF_FFEC);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x6D, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitEvents should be handled");
    assert!(result.unwrap().is_ok(), "InitEvents should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "InitEvents should return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "InitEvents should preserve A7"
    );
}

#[test]
fn held_key_generates_autokey_after_default_threshold() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x7D, 31); // Down Arrow
    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event, "initial keyDown should be delivered");
    assert_eq!(what, 3);
    assert_eq!(message, 0x0000_7D1F);

    let first_repeat_tick = disp.current_tick() + TrapDispatcher::AUTO_KEY_THRESHOLD_TICKS;

    disp.set_tick_count_for_test(&mut bus, first_repeat_tick - 1);
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(
        !has_event,
        "autoKey should wait for the 16-tick default threshold"
    );

    disp.set_tick_count_for_test(&mut bus, first_repeat_tick);
    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(has_event, "held key should generate autoKey at threshold");
    assert_eq!(what, 5);
    assert_eq!(message, 0x0000_7D1F);

    let second_repeat_tick = disp.current_tick() + TrapDispatcher::AUTO_KEY_RATE_TICKS;

    disp.set_tick_count_for_test(&mut bus, second_repeat_tick - 1);
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(
        !has_event,
        "autoKey should wait for the 4-tick default repeat rate"
    );

    disp.set_tick_count_for_test(&mut bus, second_repeat_tick);
    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(has_event, "held key should repeat after the rate interval");
    assert_eq!(what, 5);
    assert_eq!(message, 0x0000_7D1F);

    bus.write_word(
        crate::memory::globals::addr::SYS_EVT_MASK,
        bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK) | (1 << 4),
    );
    disp.push_key_up_with_system_event_mask(
        bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK),
        0x7D,
        31,
    );
    let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0010);
    assert!(has_event, "keyUp should be delivered");
    assert_eq!(what, 4);

    disp.set_tick_count_for_test(&mut bus, 100);
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(!has_event, "released key should not keep auto-keying");
}

#[test]
fn autokey_is_posted_when_ticks_advance_before_the_next_poll() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x00, b'a');
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event, "initial keyDown should be delivered");

    let next_tick = disp
        .current_tick()
        .wrapping_add(TrapDispatcher::AUTO_KEY_THRESHOLD_TICKS);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    disp.post_auto_key_if_due(bus.read_word(crate::memory::globals::addr::SYS_EVT_MASK));

    disp.push_key_up(0x00, b'a');
    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0020);
    assert!(
        has_event,
        "elapsed autoKey must survive a later key release"
    );
    assert_eq!(what, 5);
    assert_eq!(message, 0x0000_0061);
}

#[test]
fn default_system_event_mask_suppresses_keyup_but_clears_keymap() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x24, 13);
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(
        has_event,
        "keyDown should use the default system event mask"
    );
    assert!(disp.key_is_down(0x24));

    disp.push_key_up(0x24, 13);
    assert!(!disp.key_is_down(0x24), "keyUp must clear physical state");
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0010);
    assert!(
        !has_event,
        "the default SysEvtMask must not post keyUp events"
    );
}

#[test]
fn keys_use_classic_keymap_bit_order() {
    let (mut disp, _, _) = setup();

    for (key_code, expected_byte_index, expected_mask) in [
        (0x00, 0, 0x01),  // A
        (0x31, 6, 0x02),  // Space
        (0x37, 6, 0x80),  // Command
        (0x7B, 15, 0x08), // Left Arrow
        (0x7C, 15, 0x10), // Right Arrow
        (0x7D, 15, 0x20), // Down Arrow
        (0x7E, 15, 0x40), // Up Arrow
    ] {
        disp.push_key_down(key_code, 0);
        assert_eq!(
            disp.key_map_bytes()[expected_byte_index],
            expected_mask,
            "wrong KeyMap bit for key code {key_code:#04X}"
        );
        disp.push_key_up(key_code, 0);
        assert_eq!(disp.key_map_bytes()[expected_byte_index], 0);
    }
}

#[test]
fn repeated_host_keydown_does_not_duplicate_keydown_or_restart_autokey() {
    // Browsers emit repeated keydown callbacks for a held key. Classic
    // Event Manager emits one keyDown followed by timed autoKey records.
    // Inside Macintosh Volume I, I-246.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x30, 9); // Tab
    let first_repeat_tick = disp
        .input_state
        .key_repeat()
        .expect("Tab should arm autoKey")
        .next_tick();
    let next_tick = disp.current_tick().wrapping_add(5);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    disp.push_key_down(0x30, 9); // host repeat while still held

    assert_eq!(disp.event_queue.len(), 1, "only one keyDown may be queued");
    assert_eq!(
        disp.input_state
            .key_repeat()
            .expect("autoKey should remain armed")
            .next_tick(),
        first_repeat_tick,
        "a repeated host callback must not postpone autoKey"
    );
    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event);
    assert_eq!((what, message), (3, 0x0000_3009));
}

#[test]
fn event_avail_peeks_autokey_without_duplicating_it() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x7D, 31);
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event, "initial keyDown should be drained");

    let next_tick = disp
        .current_tick()
        .wrapping_add(TrapDispatcher::AUTO_KEY_THRESHOLD_TICKS);
    disp.set_tick_count_for_test(&mut bus, next_tick);
    let first = disp
        .peek_toolbox_event(&bus, 0x0020)
        .expect("EventAvail should see due autoKey");
    assert_eq!(first.what, 5);
    assert_eq!(first.message, 0x0000_7D1F);
    assert_eq!(disp.event_queue.len(), 1);

    let second = disp
        .peek_toolbox_event(&bus, 0x0020)
        .expect("second EventAvail should see same autoKey");
    assert_eq!(second.what, 5);
    assert_eq!(second.message, 0x0000_7D1F);
    assert_eq!(
        disp.event_queue.len(),
        1,
        "peek should not duplicate autoKey"
    );
}

#[test]
fn modifier_keys_update_keymap_without_generating_keyboard_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x38, 0); // Shift
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(disp.key_is_down(0x38), "Shift must update the KeyMap");
    assert!(
        !has_event,
        "modifier keys must not generate standalone keyDown events"
    );

    disp.push_key_up(0x38, 0);
    assert!(
        !disp.key_is_down(0x38),
        "Shift release must clear the KeyMap"
    );
    let (_, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0010);
    assert!(
        !has_event,
        "modifier keys must not generate standalone keyUp events"
    );
}

#[test]
fn printable_key_event_carries_held_shift_modifier() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x38, 0); // Shift
    disp.push_key_down(0x00, b'e');
    let (what, message, _, _, _, modifiers, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);

    assert!(has_event);
    assert_eq!(what, 3);
    assert_eq!(message, 0x0000_0065);
    assert_ne!(modifiers & 0x0200, 0, "shiftKey must be carried by keyDown");
}

#[test]
fn caps_lock_latches_keymap_and_alpha_lock_until_second_press() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x39, 0);
    assert!(disp.key_is_down(0x39), "first press should latch Caps Lock");
    assert_eq!(disp.current_event_modifiers() & 0x0400, 0x0400);
    assert!(
        !disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008).6,
        "Caps Lock must not post a standalone keyDown event"
    );

    disp.push_key_down(0x39, 0);
    assert!(
        disp.key_is_down(0x39),
        "a repeated host keydown while physically held must not toggle the latch"
    );
    disp.push_key_up(0x39, 0);
    assert!(
        disp.key_is_down(0x39),
        "physical release must preserve the logical Caps Lock latch"
    );
    assert_eq!(disp.current_event_modifiers() & 0x0400, 0x0400);
    disp.push_key_down(0x00, b'a');
    let (what, _, _, _, _, modifiers, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event);
    assert_eq!(what, 3);
    assert_eq!(
        modifiers & 0x0400,
        0x0400,
        "character events must carry alphaLock while Caps Lock is latched"
    );
    disp.push_key_up(0x00, b'a');

    disp.push_key_down(0x39, 0);
    assert!(
        !disp.key_is_down(0x39),
        "second physical press should release Caps Lock"
    );
    assert_eq!(disp.current_event_modifiers() & 0x0400, 0);
    disp.push_key_up(0x39, 0);
    assert!(
        !disp.key_is_down(0x39),
        "second physical release must leave Caps Lock clear"
    );
    disp.push_key_down(0x00, b'a');
    let (_, _, _, _, _, modifiers, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event);
    assert_eq!(
        modifiers & 0x0400,
        0,
        "character events must clear alphaLock after Caps Lock is released"
    );
}

#[test]
fn command_modifier_is_reported_on_following_character_event() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_sent_open_app_event_for_test(true);

    disp.push_key_down(0x37, 0); // Command
    disp.push_key_down(0x01, b's');

    let (what, message, _, _, _, modifiers, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0x0008);
    assert!(has_event);
    assert_eq!(what, 3, "only the character key should post keyDown");
    assert_eq!(message, 0x0000_0173);
    assert_ne!(modifiers & 0x0100, 0, "cmdKey must be set on Cmd-S");
}

// ---- Device Manager DCE/unit-table state ($A03D/$A43D/$A03E) ----

fn seed_device_unit_table(bus: &mut MacMemoryBus, count: u16) -> u32 {
    let table = bus.alloc(u32::from(count) * 4);
    bus.fill_zeros(table, u32::from(count) * 4);
    bus.write_long(addr::U_TABLE_BASE, table);
    bus.write_word(addr::UNIT_NTRY_CNT, count);
    table
}

#[test]
fn drvrinstall_uses_a0_driverptr_d0_refnum_and_returns_oserr_in_d0() {
    // Inside Macintosh: Devices (1994), pp. 1-83 to 1-84:
    // _DrvrInstall uses A0=drvrPtr, D0=refNum, and returns OSErr in D0.
    let (mut disp, mut cpu, mut bus) = setup();
    let table = seed_device_unit_table(&mut bus, 96);
    let driver_ptr = 0x320000;
    let stack_ptr = 0x00F0_1000;
    cpu.write_reg(Register::A0, driver_ptr);
    cpu.write_reg(Register::D0, 0xFFFF_FFEC);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DrvrInstall should be handled");
    assert!(
        result.unwrap().is_ok(),
        "DrvrInstall should return normally"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DrvrInstall should return noErr in D0 for nominal calls"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        driver_ptr,
        "DrvrInstall should not rewrite the A0 driver pointer"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "DrvrInstall is register-based and should preserve A7"
    );
    let unit = !(0xFFECu16);
    let dce_handle = bus.read_long(table + u32::from(unit) * 4);
    let dce = bus.read_long(dce_handle);
    assert_ne!(dce_handle, 0);
    assert_ne!(dce, 0);
    assert_eq!(bus.read_long(dce), 0, "dCtlDriver starts clear");
    assert_eq!(bus.read_word(dce + 4), 0x0040, "only dRAMBased is set");
    assert_eq!(bus.read_word(dce + 24), 0xFFEC, "dCtlRefNum is copied");
    assert!(
        bus.read_bytes(dce + 6, 18).iter().all(|&byte| byte == 0)
            && bus.read_bytes(dce + 26, 26).iter().all(|&byte| byte == 0),
        "all remaining AuxDCE fields start clear"
    );
}

#[test]
fn drvrremove_uses_d0_refnum_and_returns_oserr_in_d0() {
    // Inside Macintosh: Devices (1994), pp. 1-85 to 1-86:
    // _DrvrRemove uses D0=refNum and returns OSErr in D0.
    let (mut disp, mut cpu, mut bus) = setup();
    seed_device_unit_table(&mut bus, 96);
    let stack_ptr = 0x00F0_2000;
    cpu.write_reg(Register::D0, 0xFFFF_FFEC);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x3E, &mut cpu, &mut bus);
    assert!(result.is_some(), "DrvrRemove should be handled");
    assert!(result.unwrap().is_ok(), "DrvrRemove should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "an empty valid unit-table slot is a successful no-op"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "DrvrRemove is register-based and should preserve A7"
    );
}

#[test]
fn drvrinstall_drvrremove_install_then_remove_composition_balances_stack() {
    // Dispatch _DrvrInstall + _DrvrRemove in sequence against the
    // same refNum=-50 slot, with a poisoned
    // sentinel above SP to verify neither trap walks past the
    // caller's stack window. Per IM:Devices 1994 pp. 1-83..1-86
    // both traps are register-only OS-bit FUNCTIONs with no Pascal
    // stack frame.
    let (mut disp, mut cpu, mut bus) = setup();
    let table = seed_device_unit_table(&mut bus, 96);
    let driver_ptr: u32 = 0x320200;
    let stack_ptr: u32 = 0x200000;
    let sentinel_addr = stack_ptr;
    let sentinel: u32 = 0xBADC_0DE0;
    let ref_num: u32 = 0xFFFF_FFCE; // -50 sign-extended to 32 bits

    bus.write_long(sentinel_addr, sentinel);

    cpu.write_reg(Register::A7, stack_ptr);

    // DrvrInstall
    cpu.write_reg(Register::A0, driver_ptr);
    cpu.write_reg(Register::D0, ref_num);
    let r1 = disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus);
    assert!(r1.is_some());
    assert!(r1.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DrvrInstall composition: noErr"
    );

    // DrvrRemove against the same refNum
    cpu.write_reg(Register::D0, ref_num);
    let r2 = disp.dispatch_event(false, 0x3E, &mut cpu, &mut bus);
    assert!(r2.is_some());
    assert!(r2.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DrvrRemove composition: noErr"
    );

    // A7 unchanged after both calls.
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "Install + Remove must leave A7 untouched"
    );

    // Caller's stack memory at SP+0 untouched (no spurious writes).
    assert_eq!(
        bus.read_long(sentinel_addr),
        sentinel,
        "Install + Remove must not clobber caller memory above SP"
    );
    let unit = !(ref_num as u16);
    assert_eq!(
        bus.read_long(table + u32::from(unit) * 4),
        0,
        "DriverRemove clears the installed unit-table entry"
    );
}

#[test]
fn drvrinstall_rejects_out_of_range_refnum_without_allocating() {
    // Devices 1994, pp. 1-83--1-84: an unmatched reference number
    // returns badUnitErr (-21).
    let (mut disp, mut cpu, mut bus) = setup();
    let table = seed_device_unit_table(&mut bus, 32);
    let allocation_end = bus.heap_bump_ptr();
    cpu.write_reg(Register::A0, 0x0032_0000);
    cpu.write_reg(Register::D0, (-50i32) as u32);

    disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-21i32) as u32);
    assert_eq!(bus.heap_bump_ptr(), allocation_end);
    assert!(
        bus.read_bytes(table, 32 * 4).iter().all(|&byte| byte == 0),
        "badUnitErr must leave the unit table unchanged"
    );
}

#[test]
fn driver_install_reserve_memory_raw_word_reaches_shared_dce_semantics() {
    // Devices 1994, pp. 1-84--1-85 and UI 3.4 Devices.h lines
    // 1126--1141: bit 10 selects DriverInstallReserveMem at $A43D.
    let (mut disp, mut cpu, mut bus) = setup();
    let table = seed_device_unit_table(&mut bus, 96);
    let ref_num = (-51i32) as u32;
    cpu.write_reg(Register::A0, 0x0032_1000);
    cpu.write_reg(Register::D0, ref_num);
    cpu.write_reg(Register::A7, 0x00F0_1800);

    disp.dispatch(0xA43D, &mut cpu, &mut bus).unwrap();

    let slot = table + u32::from(!(ref_num as u16)) * 4;
    let handle = bus.read_long(slot);
    let dce = bus.read_long(handle);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_ne!(handle, 0);
    assert_eq!(bus.read_word(dce + 4), 0x0040);
    assert_eq!(bus.read_word(dce + 24), ref_num as u16);
    assert_eq!(cpu.read_reg(Register::A7), 0x00F0_1800);
}

#[test]
fn drvrinstall_reuses_existing_dce_and_drvrremove_refuses_open_driver() {
    // Devices 1994, pp. 1-83--1-86: install initializes the selected DCE;
    // remove returns dRemoveErr (-25) without disposing an open DCE.
    let (mut disp, mut cpu, mut bus) = setup();
    let table = seed_device_unit_table(&mut bus, 96);
    let ref_num = (-50i32) as u32;
    let slot = table + u32::from(!(ref_num as u16)) * 4;
    let dce = bus.alloc(52);
    let handle = bus.alloc(4);
    bus.fill_bytes(dce, 52, 0xA5);
    bus.write_long(handle, dce);
    bus.write_long(slot, handle);

    cpu.write_reg(Register::D0, ref_num);
    disp.dispatch_event(false, 0x3D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(slot), handle, "install reuses the DCE handle");
    assert_eq!(bus.read_long(handle), dce);
    assert_eq!(bus.read_word(dce + 4), 0x0040);

    bus.write_word(dce + 4, 0x0060);
    cpu.write_reg(Register::D0, ref_num);
    disp.dispatch_event(false, 0x3E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), (-25i32) as u32);
    assert_eq!(bus.read_long(slot), handle, "open DCE remains installed");
    assert_eq!(bus.read_long(handle), dce, "open DCE remains allocated");
}

#[test]
fn dovbltask_uses_d0_slot_and_returns_oserr_in_d0() {
    // Inside Macintosh: Processes (1994), p. 4-27:
    // DoVBLTask uses D0=slot and returns OSErr in D0.
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_3000;
    cpu.write_reg(Register::D0, 0x0000_0009);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
    assert!(result.is_some(), "DoVBLTask should be handled");
    assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DoVBLTask should return noErr in D0 for nominal calls"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "DoVBLTask is register-based and should preserve A7"
    );
}

#[test]
fn dovbltask_register_only_calling_convention_preserves_stack_across_mixed_slots() {
    // A 5-call composition cycling slot inputs 0 → 1 → 0 → 2 → 0
    // must leave A7 unchanged across each dispatch (register-only
    // OS-bit FUNCTION calling convention per IM:Processes 1994
    // p. 4-27 — no Pascal stack frame consumed).
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_3000;
    cpu.write_reg(Register::A7, stack_ptr);

    for slot in [0i32, 1, 0, 2, 0] {
        cpu.write_reg(Register::D0, slot as u32);
        let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
        assert!(result.is_some(), "DoVBLTask should be handled");
        assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "DoVBLTask must preserve A7 across dispatch with slot={slot}"
        );
    }
}

#[test]
fn dovbltask_invalid_slot_returns_slotnumerr_without_consuming_stack() {
    // Inside Macintosh: Processes (1994), p. 4-27:
    // DoVBLTask returns slotNumErr for invalid slot numbers.
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_5000;
    cpu.write_reg(Register::D0, 16);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x72, &mut cpu, &mut bus);
    assert!(result.is_some(), "DoVBLTask should be handled");
    assert!(result.unwrap().is_ok(), "DoVBLTask should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -360,
        "DoVBLTask should return slotNumErr for an invalid slot"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "DoVBLTask should preserve A7 on the invalid-slot path"
    );
}

#[test]
fn attachvbl_uses_d0_slot_and_updates_primary_vbl_slot() {
    // Inside Macintosh: Processes (1994), p. 4-26:
    // AttachVBL takes a slot number in D0, returns OSErr in D0,
    // and does not consume a Pascal stack frame.
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_6000;
    cpu.write_reg(Register::D0, 10);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
    assert!(result.is_some(), "AttachVBL should be handled");
    assert!(result.unwrap().is_ok(), "AttachVBL should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "AttachVBL should return noErr in D0 for a valid slot"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "AttachVBL is register-based and should preserve A7"
    );
    assert_eq!(
        disp.callback_scheduling.primary_vbl_slot(),
        10,
        "AttachVBL should record the newly selected primary slot"
    );
}

#[test]
fn attachvbl_invalid_slot_returns_slotnumerr_without_changing_primary_slot() {
    // Invalid slots should reject cleanly and leave the recorded
    // primary slot unchanged.
    let (mut disp, mut cpu, mut bus) = setup();
    let stack_ptr = 0x00F0_7000;
    disp.callback_scheduling.set_primary_vbl_slot(7);
    cpu.write_reg(Register::D0, 16);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
    assert!(result.is_some(), "AttachVBL should be handled");
    assert!(result.unwrap().is_ok(), "AttachVBL should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -360,
        "AttachVBL should return slotNumErr for an invalid slot"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "AttachVBL should preserve A7 on an invalid-slot path"
    );
    assert_eq!(
        disp.callback_scheduling.primary_vbl_slot(),
        7,
        "AttachVBL should not mutate the recorded primary slot on error"
    );
}

#[test]
fn sintinstall_uses_a0_qelemptr_d0_slot_and_returns_oserr_in_d0() {
    // Inside Macintosh: Devices (1994), pp. 2-70 to 2-71:
    // _SIntInstall uses A0=slot-queue element pointer, D0=slot number, and returns OSErr in D0.
    let (mut disp, mut cpu, mut bus) = setup();
    let queue_elem_ptr = 0x320100;
    let stack_ptr = 0x00F0_4000;
    bus.write_long(queue_elem_ptr, 0xA5A5_5A5A);
    bus.write_long(queue_elem_ptr + 4, 0x1122_3344);
    cpu.write_reg(Register::A0, queue_elem_ptr);
    cpu.write_reg(Register::D0, 0x0000_0009);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x75, &mut cpu, &mut bus);
    assert!(result.is_some(), "SIntInstall should be handled");
    assert!(
        result.unwrap().is_ok(),
        "SIntInstall should return normally"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SIntInstall should return noErr in D0 for nominal calls"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        queue_elem_ptr,
        "SIntInstall should not rewrite the queue-element pointer"
    );
    assert_eq!(
        bus.read_long(queue_elem_ptr),
        0xA5A5_5A5A,
        "SIntInstall no-op path should not mutate queue-element memory"
    );
    assert_eq!(
        bus.read_long(queue_elem_ptr + 4),
        0x1122_3344,
        "SIntInstall no-op path should preserve queue-element fields"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "SIntInstall is register-based and should preserve A7"
    );
}

#[test]
fn sintremove_uses_a0_qelemptr_d0_slot_and_returns_oserr_in_d0() {
    // Inside Macintosh: Devices (1994), p. 2-71:
    // _SIntRemove uses A0=slot-queue element pointer, D0=slot number, and returns OSErr in D0.
    let (mut disp, mut cpu, mut bus) = setup();
    let queue_elem_ptr = 0x320200;
    let stack_ptr = 0x00F0_5000;
    bus.write_long(queue_elem_ptr, 0x55AA_33CC);
    bus.write_long(queue_elem_ptr + 4, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, queue_elem_ptr);
    cpu.write_reg(Register::D0, 0x0000_000A);
    cpu.write_reg(Register::A7, stack_ptr);

    let result = disp.dispatch_event(false, 0x76, &mut cpu, &mut bus);
    assert!(result.is_some(), "SIntRemove should be handled");
    assert!(result.unwrap().is_ok(), "SIntRemove should return normally");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SIntRemove should return noErr in D0 for nominal calls"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        queue_elem_ptr,
        "SIntRemove should not rewrite the queue-element pointer"
    );
    assert_eq!(
        bus.read_long(queue_elem_ptr),
        0x55AA_33CC,
        "SIntRemove no-op path should not mutate queue-element memory"
    );
    assert_eq!(
        bus.read_long(queue_elem_ptr + 4),
        0xDEAD_BEEF,
        "SIntRemove no-op path should preserve queue-element fields"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        stack_ptr,
        "SIntRemove is register-based and should preserve A7"
    );
}

const EVENT_PTR: u32 = 0x300000;
const QHDR_FLAGS_OFFSET: u32 = 0;
const QHDR_HEAD_OFFSET: u32 = 2;
const QHDR_TAIL_OFFSET: u32 = 6;
const QELEM_LINK_OFFSET: u32 = 0;

/// Helper to read back an EventRecord from guest memory.
fn read_event_record(
    bus: &crate::memory::MacMemoryBus,
    ptr: u32,
) -> (u16, u32, u32, u16, u16, u16) {
    let what = bus.read_word(ptr);
    let message = bus.read_long(ptr + 2);
    let when = bus.read_long(ptr + 6);
    let where_v = bus.read_word(ptr + 10);
    let where_h = bus.read_word(ptr + 12);
    let modifiers = bus.read_word(ptr + 14);
    (what, message, when, where_v, where_h, modifiers)
}

// ---- Enqueue ($A96F) / Dequeue ($A96E) ----

#[test]
fn enqueue_on_empty_queue_sets_qhead_qtail_and_terminal_link() {
    // Inside Macintosh: Operating System Utilities (1994), pp. 6-15..6-16:
    // Enqueue adds qElement to the end of qHeader and updates the queue header.
    let (mut disp, mut cpu, mut bus) = setup();
    let q_header = 0x310000;
    let q_entry = 0x310100;

    bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0xA5A5);
    bus.write_long(q_header + QHDR_HEAD_OFFSET, 0);
    bus.write_long(q_header + QHDR_TAIL_OFFSET, 0);
    bus.write_long(q_entry + QELEM_LINK_OFFSET, 0xDEAD_BEEF);

    cpu.write_reg(Register::A0, q_entry);
    cpu.write_reg(Register::A1, q_header);
    let result = disp.dispatch_event(true, 0x16F, &mut cpu, &mut bus);
    assert!(result.is_some(), "Enqueue should be handled");
    assert!(result.unwrap().is_ok(), "Enqueue should succeed");
    assert_eq!(
        bus.read_long(q_header + QHDR_HEAD_OFFSET),
        q_entry,
        "Enqueue should set qHead to inserted entry for an empty queue"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_TAIL_OFFSET),
        q_entry,
        "Enqueue should set qTail to inserted entry for an empty queue"
    );
    assert_eq!(
        bus.read_long(q_entry + QELEM_LINK_OFFSET),
        0,
        "Enqueue should terminate the inserted entry with qLink=NIL"
    );
    assert_eq!(
        bus.read_word(q_header + QHDR_FLAGS_OFFSET),
        0xA5A5,
        "Enqueue should not modify qFlags"
    );
}

#[test]
fn enqueue_appends_after_existing_tail_and_preserves_qflags() {
    // Inside Macintosh: Operating System Utilities (1994), pp. 6-13..6-16:
    // QHdr stores qFlags/qHead/qTail and Enqueue appends at queue end.
    let (mut disp, mut cpu, mut bus) = setup();
    let q_header = 0x310200;
    let first_entry = 0x310300;
    let second_entry = 0x310400;

    bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x55AA);
    bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
    bus.write_long(q_header + QHDR_TAIL_OFFSET, first_entry);
    bus.write_long(first_entry + QELEM_LINK_OFFSET, 0);
    bus.write_long(second_entry + QELEM_LINK_OFFSET, 0x1111_2222);

    cpu.write_reg(Register::A0, second_entry);
    cpu.write_reg(Register::A1, q_header);
    let result = disp.dispatch_event(true, 0x16F, &mut cpu, &mut bus);
    assert!(result.is_some(), "Enqueue should be handled");
    assert!(result.unwrap().is_ok(), "Enqueue should succeed");
    assert_eq!(
        bus.read_long(q_header + QHDR_HEAD_OFFSET),
        first_entry,
        "Enqueue should preserve qHead when appending"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_TAIL_OFFSET),
        second_entry,
        "Enqueue should move qTail to the appended entry"
    );
    assert_eq!(
        bus.read_long(first_entry + QELEM_LINK_OFFSET),
        second_entry,
        "Enqueue should link the previous tail to the appended entry"
    );
    assert_eq!(
        bus.read_long(second_entry + QELEM_LINK_OFFSET),
        0,
        "Enqueue should terminate the appended entry with qLink=NIL"
    );
    assert_eq!(
        bus.read_word(q_header + QHDR_FLAGS_OFFSET),
        0x55AA,
        "Enqueue should not modify qFlags"
    );
}

#[test]
fn dequeue_present_entry_unlinks_element_and_returns_noerr() {
    // Inside Macintosh: Operating System Utilities (1994), p. 6-16:
    // Dequeue removes a found element, adjusts the queue, and returns noErr.
    let (mut disp, mut cpu, mut bus) = setup();
    let q_header = 0x310500;
    let first_entry = 0x310600;
    let second_entry = 0x310700;

    bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x0F0F);
    bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
    bus.write_long(q_header + QHDR_TAIL_OFFSET, second_entry);
    bus.write_long(first_entry + QELEM_LINK_OFFSET, second_entry);
    bus.write_long(second_entry + QELEM_LINK_OFFSET, 0);

    cpu.write_reg(Register::A0, first_entry);
    cpu.write_reg(Register::A1, q_header);
    let result = disp.dispatch_event(true, 0x16E, &mut cpu, &mut bus);
    assert!(result.is_some(), "Dequeue should be handled");
    assert!(
        result.unwrap().is_ok(),
        "Dequeue should return from dispatch"
    );
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "Dequeue should return noErr when entry is present"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_HEAD_OFFSET),
        second_entry,
        "Dequeue should promote next entry to qHead when removing head"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_TAIL_OFFSET),
        second_entry,
        "Dequeue should update qTail when head removal leaves one entry"
    );
    assert_eq!(
        bus.read_word(q_header + QHDR_FLAGS_OFFSET),
        0x0F0F,
        "Dequeue should not modify qFlags"
    );
}

#[test]
fn dequeue_missing_entry_returns_qerr_and_preserves_queue() {
    // Inside Macintosh: Operating System Utilities (1994), p. 6-16:
    // Dequeue returns qErr (-1) when the entry is not in the queue.
    let (mut disp, mut cpu, mut bus) = setup();
    let q_header = 0x310800;
    let first_entry = 0x310900;
    let second_entry = 0x310A00;
    let missing_entry = 0x310B00;

    bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0xAAAA);
    bus.write_long(q_header + QHDR_HEAD_OFFSET, first_entry);
    bus.write_long(q_header + QHDR_TAIL_OFFSET, second_entry);
    bus.write_long(first_entry + QELEM_LINK_OFFSET, second_entry);
    bus.write_long(second_entry + QELEM_LINK_OFFSET, 0);

    cpu.write_reg(Register::A0, missing_entry);
    cpu.write_reg(Register::A1, q_header);
    let result = disp.dispatch_event(true, 0x16E, &mut cpu, &mut bus);
    assert!(result.is_some(), "Dequeue should be handled");
    assert!(
        result.unwrap().is_ok(),
        "Dequeue should return from dispatch"
    );
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -1,
        "Dequeue should return qErr for missing entry"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_HEAD_OFFSET),
        first_entry,
        "Missing-entry Dequeue should preserve qHead"
    );
    assert_eq!(
        bus.read_long(q_header + QHDR_TAIL_OFFSET),
        second_entry,
        "Missing-entry Dequeue should preserve qTail"
    );
    assert_eq!(
        bus.read_long(first_entry + QELEM_LINK_OFFSET),
        second_entry,
        "Missing-entry Dequeue should preserve existing queue links"
    );
    assert_eq!(
        bus.read_long(second_entry + QELEM_LINK_OFFSET),
        0,
        "Missing-entry Dequeue should preserve existing terminal qLink"
    );
    assert_eq!(
        bus.read_word(q_header + QHDR_FLAGS_OFFSET),
        0xAAAA,
        "Missing-entry Dequeue should preserve qFlags"
    );
}

#[test]
fn enqueue_dequeue_dispatcher_convention_preserves_register_only_abi() {
    // A single in-Rust sequence pinning the
    // Tool-bit Enqueue PROCEDURE / Dequeue FUNCTION register
    // calling convention (A0=qElement, A1=qHeader; D0=OSErr for
    // Dequeue). Per IM:OSUtils 1994 pp. 6-13..6-17 + IM:II 1985
    // p. II-374.
    let (mut disp, mut cpu, mut bus) = setup();
    let q_header = 0x320000;
    let elem_a = 0x320100;
    let elem_b = 0x320200;
    let elem_c = 0x320300;

    bus.write_word(q_header + QHDR_FLAGS_OFFSET, 0x5A5A);
    bus.write_long(q_header + QHDR_HEAD_OFFSET, 0);
    bus.write_long(q_header + QHDR_TAIL_OFFSET, 0);
    bus.write_long(elem_a + QELEM_LINK_OFFSET, 0xDEAD_BEEF);
    bus.write_long(elem_b + QELEM_LINK_OFFSET, 0xCAFE_F00D);
    bus.write_long(elem_c + QELEM_LINK_OFFSET, 0xBAAD_F00D);

    // B1: Enqueue elem_a onto empty queue.
    cpu.write_reg(Register::A0, elem_a);
    cpu.write_reg(Register::A1, q_header);
    assert!(disp
        .dispatch_event(true, 0x16F, &mut cpu, &mut bus)
        .is_some());
    assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_a);
    assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_a);
    assert_eq!(bus.read_long(elem_a + QELEM_LINK_OFFSET), 0);

    // B2: Enqueue elem_b — appends after elem_a.
    cpu.write_reg(Register::A0, elem_b);
    cpu.write_reg(Register::A1, q_header);
    assert!(disp
        .dispatch_event(true, 0x16F, &mut cpu, &mut bus)
        .is_some());
    assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_a);
    assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);
    assert_eq!(bus.read_long(elem_a + QELEM_LINK_OFFSET), elem_b);
    assert_eq!(bus.read_long(elem_b + QELEM_LINK_OFFSET), 0);
    assert_eq!(bus.read_word(q_header + QHDR_FLAGS_OFFSET), 0x5A5A);

    // B3: Dequeue elem_a — present, returns noErr in D0.
    cpu.write_reg(Register::D0, 0x3FFF_3FFF); // poison D0
    cpu.write_reg(Register::A0, elem_a);
    cpu.write_reg(Register::A1, q_header);
    assert!(disp
        .dispatch_event(true, 0x16E, &mut cpu, &mut bus)
        .is_some());
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_b);
    assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);

    // B4: Dequeue elem_c — not in queue, returns qErr -1 in D0.
    cpu.write_reg(Register::D0, 0x3FFF_3FFF); // poison D0
    cpu.write_reg(Register::A0, elem_c);
    cpu.write_reg(Register::A1, q_header);
    assert!(disp
        .dispatch_event(true, 0x16E, &mut cpu, &mut bus)
        .is_some());
    assert_eq!(cpu.read_reg(Register::D0) as i32, -1);
    assert_eq!(bus.read_long(q_header + QHDR_HEAD_OFFSET), elem_b);
    assert_eq!(bus.read_long(q_header + QHDR_TAIL_OFFSET), elem_b);
    assert_eq!(bus.read_word(q_header + QHDR_FLAGS_OFFSET), 0x5A5A);
}

// ---- AttachVBL ($A071, OS) ----

#[test]
fn attach_vbl_returns_noerr_for_primary_slot() {
    let (mut disp, mut cpu, mut bus) = setup();

    let result = disp.dispatch_event(false, 0x71, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn rdrvrinstall_returns_d0_zero_and_preserves_stack_pointer() {
    // Inside Macintosh Volume III (1986), p. III-21: RDrvrInstall is the
    // ROM-driver install variant of DrvrInstall and returns noErr.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0x1234_5678);
    cpu.write_reg(Register::D0, 7);

    let result = disp.dispatch_event(false, 0x4F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp_pre);
}

// ---- PostEvent ($A02F) ----

#[test]
fn post_event_returns_d0_zero() {
    let (mut disp, mut cpu, mut bus) = setup();

    let result = disp.dispatch_event(false, 0x2F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn post_event_observes_direct_sys_evt_mask_writes() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 4);
    cpu.write_reg(Register::D0, 0x1234);

    let result = disp.dispatch_event(false, 0x2F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), TrapDispatcher::EVT_NOT_ENB);
    assert!(disp.event_queue.is_empty());

    bus.write_word(crate::memory::globals::addr::SYS_EVT_MASK, 0xffff);
    cpu.write_reg(Register::A0, 4);
    cpu.write_reg(Register::D0, 0x5678);
    let result = disp.dispatch_event(false, 0x2F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(disp.event_queue.front().map(|event| event.what), Some(4));
    assert_eq!(
        disp.event_queue.front().map(|event| event.message),
        Some(0x5678)
    );
}

#[test]
fn posted_event_timestamp_survives_later_retrieval() {
    let (mut disp, mut cpu, mut bus) = setup();
    let posted_at = 0x1020_3040;
    let retrieved_at = 0x5566_7788;

    bus.write_word(crate::memory::globals::addr::SYS_EVT_MASK, u16::MAX);
    disp.set_tick_count_for_test(&mut bus, posted_at);
    cpu.write_reg(Register::A0, 4);
    cpu.write_reg(Register::D0, 0xA1B2_C3D4);
    let result = disp.dispatch_event(false, 0x2F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        disp.event_queue.front().map(|event| event.when),
        Some(posted_at)
    );

    disp.set_tick_count_for_test(&mut bus, retrieved_at);
    let (what, message, when, where_v, where_h, modifiers, has_event) =
        disp.dequeue_event(&bus, 1 << 4);
    assert!(has_event);
    assert_eq!(
        (what, message, when),
        (4, 0xA1B2_C3D4, posted_at),
        "EventRecord.when must retain the posting tick rather than retrieval time"
    );
    assert_eq!((where_v, where_h), (0, 0));
    assert_eq!(modifiers & 0x0080, 0x0080);
}

// ---- OSEventAvail ($A030) ----

#[test]
fn os_event_avail_returns_d0_ffff_when_empty() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xFFFF);
    cpu.write_reg(Register::A0, EVENT_PTR);

    let result = disp.dispatch_event(false, 0x30, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    // D0=$FFFF = null event (FALSE) per TB Essentials 1992, p. 2-98
    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

    let (what, _message, _when, _where_v, _where_h, modifiers) = read_event_record(&bus, EVENT_PTR);
    assert_eq!(what, 0);
    assert_eq!(modifiers & 0x0080, 0x0080);
}

#[test]
fn os_event_avail_peeks_latched_native_menu_mouse_down() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_tick_count_for_test(&mut bus, 100);
    disp.pending_native_menu_event = Some(QueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 10,
        where_h: 42,
        modifiers: 0,
    });
    cpu.write_reg(Register::D0, 1 << 1);
    cpu.write_reg(Register::A0, EVENT_PTR);

    let result = disp.dispatch_event(false, 0x30, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(read_event_record(&bus, EVENT_PTR).0, 1);
    assert!(disp.pending_native_menu_event.is_some());

    cpu.write_reg(Register::D0, 1 << 1);
    let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(read_event_record(&bus, EVENT_PTR).0, 1);
}

// ---- GetOSEvent ($A031) ----

#[test]
fn get_os_event_returns_d0_ffff_when_empty() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xFFFF);
    cpu.write_reg(Register::A0, EVENT_PTR);

    let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    // D0=$FFFF = null event (FALSE) per TB Essentials 1992, p. 2-97
    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);

    let (what, _message, _when, _where_v, _where_h, modifiers) = read_event_record(&bus, EVENT_PTR);
    assert_eq!(what, 0);
    assert_eq!(modifiers & 0x0080, 0x0080);
}

#[test]
fn get_os_event_skips_toolbox_and_high_level_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: 0x1000,
        when: 0,
        where_v: 10,
        where_h: 20,
        modifiers: 0,
    });
    disp.event_queue.push_back(QueuedEvent {
        what: 23,
        message: u32::from_be_bytes(*b"aevt"),
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });
    disp.push_key_down(0x00, b'a');
    cpu.write_reg(Register::D0, 0xFFFF);
    cpu.write_reg(Register::A0, EVENT_PTR);

    // Macintosh Toolbox Essentials (1992), pp. 2-97--2-99:
    // GetOSEvent and OSEventAvail return only low-level events from the
    // Operating System event queue, never update or high-level events.
    let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(read_event_record(&bus, EVENT_PTR).0, 3);
    assert_eq!(disp.event_queue.len(), 2);
    assert_eq!(disp.event_queue.get(0).unwrap().what, 6);
    assert_eq!(disp.event_queue.get(1).unwrap().what, 23);

    cpu.write_reg(Register::D0, 0xFFFF);
    let result = disp.dispatch_event(false, 0x30, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF);
    assert_eq!(read_event_record(&bus, EVENT_PTR).0, 0);
    assert_eq!(disp.event_queue.len(), 2);
}

#[test]
fn toolbox_event_accessors_apply_documented_event_priority() {
    let (mut disp, mut cpu, mut bus) = setup();
    for (what, message) in [
        (6, 0x1000),
        (23, u32::from_be_bytes(*b"aevt")),
        (1, 0),
        (8, 0x2000),
    ] {
        disp.event_queue.push_back(QueuedEvent {
            what,
            message,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
    }

    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 8, "activate events have highest priority");
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 1, "user input precedes update events");
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 6, "update events precede high-level events");
    assert_eq!(disp.event_queue.front().map(|event| event.what), Some(23));
}

#[test]
fn low_level_toolbox_events_preserve_fifo_order() {
    let (mut disp, mut cpu, mut bus) = setup();
    for (what, message) in [(3, 0x3000), (2, 0x2000), (7, 0x7000)] {
        disp.event_queue.push_back(QueuedEvent {
            what,
            message,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
    }

    for expected in [(3, 0x3000), (2, 0x2000), (7, 0x7000)] {
        let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
        assert_eq!((peeked.what, peeked.message), expected);
        let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
        assert_eq!((event.0, event.1), expected);
    }
}

fn make_dirty_visible_window(bus: &mut crate::memory::MacMemoryBus) -> u32 {
    let window = bus.alloc(160);
    bus.write_byte(window + 110, 0xFF); // WindowRecord.visible
    let region = bus.alloc(10);
    bus.write_word(region, 10);
    bus.write_word(region + 2, 0);
    bus.write_word(region + 4, 0);
    bus.write_word(region + 6, 20);
    bus.write_word(region + 8, 20);
    let update_handle = bus.alloc(4);
    bus.write_long(update_handle, region);
    bus.write_long(window + 122, update_handle); // WindowRecord.updateRgn
    window
}

#[test]
fn flushed_update_keeps_priority_between_autokey_and_os_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = make_dirty_visible_window(&mut bus);
    disp.window_list.push(window);
    disp.queue_window_update_event(window);
    disp.flush_events_with_masks(1 << 6, 0);
    assert!(disp.event_queue.is_empty());
    assert_eq!(disp.flushed_update_events.len(), 1);

    for what in [15, 5] {
        disp.event_queue.push_back(QueuedEvent {
            what,
            message: 0,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });
    }

    let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
    assert_eq!(peeked.what, 5, "autoKey precedes recovered updateEvt");
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 5);

    let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
    assert_eq!(peeked.what, 6, "recovered updateEvt precedes OS events");
    assert_eq!(peeked.message, window);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (6, window));

    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 15);
}

#[test]
fn mixed_source_updates_follow_front_to_back_window_order() {
    let (mut disp, mut cpu, mut bus) = setup();
    let front = make_dirty_visible_window(&mut bus);
    let back = make_dirty_visible_window(&mut bus);
    disp.window_list.replace(vec![front, back]);

    disp.queue_window_update_event(front);
    disp.flush_events_with_masks(1 << 6, 0);
    disp.queue_window_update_event(back);
    assert_eq!(disp.flushed_update_events.len(), 1);
    assert_eq!(disp.event_queue.len(), 1);

    let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
    assert_eq!(peeked.message, front, "recovered front update must win");
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (6, front));

    let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
    assert_eq!(peeked.message, back);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (6, back));
}

#[test]
fn stale_ordinary_update_does_not_precede_valid_recovered_update() {
    let (mut disp, mut cpu, mut bus) = setup();
    let hidden = make_dirty_visible_window(&mut bus);
    bus.write_byte(hidden + 110, 0);
    let visible = make_dirty_visible_window(&mut bus);
    disp.window_list.replace(vec![hidden, visible]);
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: hidden,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });
    disp.queue_window_update_event(visible);
    disp.flush_events_with_masks(1 << 6, 0);
    // Restore the stale ordinary marker after FlushEvents recovered the
    // valid marker; this models an independently queued stale source.
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: hidden,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });

    let peeked = disp.peek_toolbox_event(&bus, u16::MAX).unwrap();
    assert_eq!(peeked.message, visible);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (6, visible));
}

#[test]
fn explicitly_posted_orphan_update_remains_deliverable() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: 0x1234_5678,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });

    assert_eq!(
        disp.peek_toolbox_event(&bus, u16::MAX).unwrap().message,
        0x1234_5678
    );
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!((event.0, event.1), (6, 0x1234_5678));
}

#[test]
fn picture_backed_update_peek_and_dequeue_skip_the_same_marker() {
    let (mut disp, mut cpu, mut bus) = setup();
    let picture_window = make_dirty_visible_window(&mut bus);
    let picture_handle = bus.alloc(4);
    bus.write_long(picture_handle, 0x0010_0000);
    bus.write_long(picture_window + 148, picture_handle);
    disp.window_list.push(picture_window);
    disp.queue_window_update_event(picture_window);
    disp.event_queue.push_back(QueuedEvent {
        what: 15,
        message: 0,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });

    assert_eq!(disp.peek_toolbox_event(&bus, u16::MAX).unwrap().what, 15);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 15);
}

#[test]
fn dequeue_does_not_synthesize_markerless_dirty_window_updates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = make_dirty_visible_window(&mut bus);
    disp.window_list.push(window);
    disp.event_queue.push_back(QueuedEvent {
        what: 15,
        message: 0,
        when: 0,
        where_v: 10,
        where_h: 20,
        modifiers: 0,
    });

    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert_eq!(event.0, 15);
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert!(!event.6, "a dirty region alone must not stream updateEvts");
}

#[test]
fn peek_and_dequeue_agree_on_markerless_dirty_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = make_dirty_visible_window(&mut bus);
    disp.window_list.push(window);

    assert!(disp.peek_toolbox_event(&bus, u16::MAX).is_none());
    let event = disp.dequeue_toolbox_event(&mut cpu, &mut bus, u16::MAX);
    assert!(!event.6);
    assert_eq!(event.0, 0);
}

#[test]
fn get_os_event_mouse_up_reports_button_up_modifier() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.push_mouse_down(75, 150);
    let _ = disp.dequeue_event(&bus, 0xFFFF);
    disp.push_mouse_up(75, 150);

    cpu.write_reg(Register::D0, 0x0004);
    cpu.write_reg(Register::A0, EVENT_PTR);

    let result = disp.dispatch_event(false, 0x31, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    // D0=0 = event found (TRUE) per TB Essentials 1992, p. 2-97
    assert_eq!(cpu.read_reg(Register::D0), 0);

    let (what, _message, _when, where_v, where_h, modifiers) = read_event_record(&bus, EVENT_PTR);
    assert_eq!(what, 2);
    assert_eq!(where_v, 75);
    assert_eq!(where_h, 150);
    assert_eq!(modifiers & 0x0080, 0x0080);
}
