use super::super::dispatch::{LoadedResources, ResourceFileMap};
use super::super::test_helpers::{setup, setup_with_trap_tables, TEST_SP};
use super::QUICKTIME_NUM_VERSION_6_0_FINAL;
use crate::cpu::{CpuOps, Register};
use crate::memory::globals::addr;
use crate::memory::{GuestAddressSpace, MemoryBus};
use crate::process_context::{ProcessContext, ProcessNativeHeapState};
use ppc::PpcMemory;
use std::collections::HashMap;

use super::super::test_helpers::MockCpu;

// ================================================================
// Helper: call dispatch_resource and unwrap
// ================================================================
fn call(
    dispatcher: &mut super::super::TrapDispatcher,
    is_tool: bool,
    trap_num: u16,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> crate::Result<()> {
    dispatcher
        .dispatch_resource(is_tool, trap_num, cpu, bus)
        .expect("trap arm should be handled")
}

fn call_trap_word(
    dispatcher: &mut super::super::TrapDispatcher,
    trap_word: u16,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> crate::Result<()> {
    dispatcher.dispatch(trap_word, cpu, bus)
}

#[test]
fn server_dispatch_reports_absent_file_server_in_parameter_block_and_d0() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x2A_0000;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x7FFF);
    bus.write_word(pb + 26, 2); // SCShutDown

    call_trap_word(&mut disp, 0xA094, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(pb + 16) as i16, -50, "ioResult is paramErr");
    assert_eq!(cpu.read_reg(Register::D0) as i32, -50, "D0 is paramErr");
    assert_eq!(
        cpu.read_reg(Register::A0),
        pb,
        "parameter block is preserved"
    );
}

#[test]
fn server_dispatch_rejects_a_null_parameter_block() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);

    call(&mut disp, false, 0x94, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -50, "D0 is paramErr");
}

#[test]
fn initpack_pops_pack_id_and_records_last_pack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;

    bus.write_word(sp, 0x1234);
    bus.write_long(sp + 2, 0xDEAD_BEEF);

    let sp_pre = cpu.read_reg(Register::A7);
    let result = call_trap_word(&mut disp, 0xA9E5, &mut cpu, &mut bus);
    assert!(result.is_ok(), "InitPack should return cleanly");
    assert_eq!(cpu.read_reg(Register::A7), sp_pre + 2);
    assert_eq!(disp.last_init_pack_id, Some(0x1234_i16));
    assert_eq!(bus.read_long(sp + 2), 0xDEAD_BEEF);
}

#[test]
fn phantom_script_aliases_clear_d0_and_balance_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;

    // AA87 GetScript alias: selector word + script word + result slot.
    bus.write_word(sp, 0x0001);
    bus.write_word(sp + 2, 0x1234);
    bus.write_word(sp + 4, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    call_trap_word(&mut disp, 0xAA87, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0, "AA87 clears D0");
    assert_eq!(bus.read_word(sp + 4), 0, "AA87 writes zero result");
    assert_eq!(cpu.read_reg(Register::A7), sp + 4, "AA87 pops 4 bytes");

    // AA88 SetScript alias: value, selector, script; no result slot.
    bus.write_word(sp, 0x5678);
    bus.write_word(sp + 2, 0x0002);
    bus.write_word(sp + 4, 0x1234);
    bus.write_word(sp + 6, 0xCAFE);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    call_trap_word(&mut disp, 0xAA88, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0, "AA88 clears D0");
    assert_eq!(
        bus.read_word(sp + 6),
        0xCAFE,
        "AA88 leaves trailing memory untouched"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp + 6, "AA88 pops 6 bytes");

    // AA89 GetEnvirons alias: selector word + result slot.
    bus.write_word(sp, 0x0005);
    bus.write_word(sp + 2, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    call_trap_word(&mut disp, 0xAA89, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0, "AA89 clears D0");
    assert_eq!(bus.read_word(sp + 2), 0, "AA89 writes zero result");
    assert_eq!(cpu.read_reg(Register::A7), sp + 2, "AA89 pops 2 bytes");
}

#[test]
fn cmpstring_variants_apply_documented_case_and_marks_sensitivity() {
    // Inside Macintosh: Text (1993), pp. 5-51--5-52: MARKS and CASE
    // independently enable diacritic-sensitive and case-sensitive matching.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let accented_lower = bus.alloc(1);
    let plain_lower = bus.alloc(1);
    let plain_upper = bus.alloc(1);
    bus.write_byte(accented_lower, 0x8E); // Mac Roman e acute
    bus.write_byte(plain_lower, b'e');
    bus.write_byte(plain_upper, b'E');

    for (trap_word, left, right, expected) in [
        (0xA03C, accented_lower, plain_upper, 0),
        (0xA23C, accented_lower, plain_upper, 1),
        (0xA43C, accented_lower, plain_upper, 1),
        (0xA63C, accented_lower, plain_upper, 1),
        (0xA03C, plain_lower, plain_upper, 0),
        (0xA23C, plain_lower, plain_upper, 0),
        (0xA43C, plain_lower, plain_upper, 1),
        (0xA63C, plain_lower, plain_upper, 1),
    ] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, left);
        cpu.write_reg(Register::A1, right);
        cpu.write_reg(Register::D0, 0x0001_0001);
        call(&mut dispatcher, false, 0x3C, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0),
            expected,
            "trap ${trap_word:04X}"
        );
    }
}

/// Write a Pascal string (length-prefixed) into memory at `addr`.
fn write_pstring(bus: &mut crate::memory::MacMemoryBus, addr: u32, s: &[u8]) {
    bus.write_byte(addr, s.len() as u8);
    for (i, &b) in s.iter().enumerate() {
        bus.write_byte(addr + 1 + i as u32, b);
    }
}

/// Write an FSSpec at `spec_ptr`: vRefNum(2) + dirID(4) + name(pstring, 64 bytes).
fn write_fsspec(
    bus: &mut crate::memory::MacMemoryBus,
    spec_ptr: u32,
    vref: u16,
    dir_id: u32,
    name: &[u8],
) {
    bus.write_word(spec_ptr, vref);
    bus.write_long(spec_ptr + 2, dir_id);
    let len = name.len().min(63) as u8;
    bus.write_byte(spec_ptr + 6, len);
    for (i, &byte) in name.iter().take(len as usize).enumerate() {
        bus.write_byte(spec_ptr + 7 + i as u32, byte);
    }
}

fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
    let data_offset = 16u32;
    let data_length = (4 + data.len()) as u32;
    let map_offset = data_offset + data_length;
    let type_list_offset = 30u16;
    let ref_list_offset = 10u16;
    let name_list_offset = 40u16;
    let map_length = 52u32;

    let mut bytes = vec![0u8; (map_offset + map_length) as usize];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&data_offset.to_be_bytes());
    header[4..8].copy_from_slice(&map_offset.to_be_bytes());
    header[8..12].copy_from_slice(&data_length.to_be_bytes());
    header[12..16].copy_from_slice(&map_length.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);

    let data_start = data_offset as usize;
    bytes[data_start..data_start + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
    bytes[data_start + 4..data_start + 4 + data.len()].copy_from_slice(data);

    let map_start = map_offset as usize;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 16..map_start + 20].copy_from_slice(&0u32.to_be_bytes());
    bytes[map_start + 20..map_start + 22].copy_from_slice(&0u16.to_be_bytes());
    bytes[map_start + 22..map_start + 24].copy_from_slice(&0u16.to_be_bytes());
    bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());
    bytes[map_start + 28..map_start + 30].copy_from_slice(&0u16.to_be_bytes());

    let type_list_start = map_start + type_list_offset as usize;
    bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
    bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 8..type_list_start + 10]
        .copy_from_slice(&ref_list_offset.to_be_bytes());

    let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
    bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
    bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
    bytes[ref_list_start + 4] = 0;
    bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

    bytes
}

#[test]
fn systemevent_uses_event_pointer_pascal_frame() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;
    let event_ptr = bus.alloc(16);
    bus.write_word(event_ptr, 1); // what = mouseDown
    bus.write_long(event_ptr + 2, 0x1122_3344); // message
    bus.write_long(event_ptr + 6, 0x5566_7788); // when
    bus.write_long(event_ptr + 10, 0x99AA_BBCC); // where
    bus.write_word(event_ptr + 14, 0x0102); // modifiers
    bus.write_long(sp - 4, 0xDEAD_BEEF);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0xBEEF); // result sentinel
    bus.write_long(sp + 6, 0xCAFE_BABE);
    bus.write_long(sp + 16, 0x5A5A_C0DE); // catches the former result location
    let preserved = [
        (Register::D3, 0xD300_0003),
        (Register::D4, 0xD400_0004),
        (Register::D5, 0xD500_0005),
        (Register::D6, 0xD600_0006),
        (Register::D7, 0xD700_0007),
        (Register::A2, 0xA200_0002),
        (Register::A3, event_ptr),
        (Register::A4, 0xA400_0004),
        (Register::A5, 0xA500_0005),
        (Register::A6, 0xA600_0006),
    ];
    for (register, value) in preserved {
        cpu.write_reg(register, value);
    }

    let result = disp.dispatch_resource(true, 0x1B2, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_long(sp - 4), 0xDEAD_BEEF);
    assert_eq!(bus.read_long(sp), event_ptr);
    assert_eq!(bus.read_word(sp + 4), 0);
    assert_eq!(bus.read_long(sp + 6), 0xCAFE_BABE);
    assert_eq!(bus.read_long(sp + 16), 0x5A5A_C0DE);
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    for (register, value) in preserved {
        assert_eq!(
            cpu.read_reg(register),
            value,
            "stack-based SystemEvent must preserve {register:?}"
        );
    }
}

/// Set up loaded resources with one entry.
fn setup_resources(
    dispatcher: &mut super::super::TrapDispatcher,
    bus: &mut crate::memory::MacMemoryBus,
    res_type: &[u8; 4],
    res_id: i16,
    data: &[u8],
) -> u32 {
    let data_ptr = bus.alloc(data.len() as u32);
    bus.write_bytes(data_ptr, data);
    let mut loaded = HashMap::new();
    loaded.insert((*res_type, res_id), data_ptr);
    let file = ResourceFileMap {
        loaded,
        named: HashMap::new(),
        names_by_id: HashMap::new(),
        attrs: HashMap::new(),
        map_attrs: 0,
    };
    dispatcher.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(0, file)]),
        names: HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });
    dispatcher.remember_resource_backing_data(0, *res_type, res_id, data.to_vec());
    data_ptr
}

#[test]
fn movehhi_respects_setrespurge_for_changed_resource_handles() {
    // Inside Macintosh Volume I (1985), p. I-126 and Memory 1992,
    // pp. 2-18 / 2-91: SetResPurge installs the purge hook so a
    // changed resource can be written out when the Memory Manager
    // moves a resource handle.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 32000, &[0x41, 0x42, 0x43]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 32000, data_ptr);
    let changed = super::super::TrapDispatcher::RES_CHANGED_ATTR;
    let sp = TEST_SP;

    // FALSE (high byte 0) keeps the changed resource marked as changed.
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0x00FF);
    let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert!(!disp.policy.res_purge());

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, handle);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
        0
    );

    cpu.write_reg(Register::A0, handle);
    let result = disp.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
    assert!(result.is_some(), "MoveHHi should be handled");
    assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
        0,
        "SetResPurge(FALSE) should keep the resource changed after MoveHHi"
    );

    // TRUE (high byte 1) clears the changed flag through the purge hook.
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0x0100);
    let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert!(disp.policy.res_purge());

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::A0, handle);
    let result = disp.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
    assert!(result.is_some(), "MoveHHi should be handled");
    assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
    assert_eq!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0) & changed,
        0,
        "SetResPurge(TRUE) should clear the changed flag through MoveHHi"
    );
}

#[test]
fn find_vfs_file_prefers_relative_path_components_before_basename() {
    let disp = super::super::TrapDispatcher::new();
    disp.vfs
        .insert("MacPopulous/BigColourData/LOAD.PIC".to_string(), vec![1]);
    disp.vfs
        .insert("MacPopulous/ColourData/LOAD.PIC".to_string(), vec![2]);

    assert_eq!(
        disp.find_vfs_file(":ColourData:load.pic"),
        Some("MacPopulous/ColourData/LOAD.PIC".to_string())
    );
}

#[test]
fn find_vfs_file_does_not_discard_explicit_path_components() {
    let disp = super::super::TrapDispatcher::new();
    disp.vfs
        .insert("Character Files/Data CD".to_string(), vec![1]);

    assert_eq!(disp.find_vfs_file(":Data Files:Data CD"), None);
    assert_eq!(
        disp.find_vfs_file("Data CD"),
        Some("Character Files/Data CD".to_string())
    );
}

#[test]
fn find_vfs_rsrc_file_prefers_relative_path_components_before_basename() {
    let disp = super::super::TrapDispatcher::new();
    disp.vfs_rsrc
        .insert("MacPopulous/BigColourData/LOAD.PIC".to_string(), vec![1]);
    disp.vfs_rsrc
        .insert("MacPopulous/ColourData/LOAD.PIC".to_string(), vec![2]);

    assert_eq!(
        disp.find_vfs_rsrc_file(":ColourData:load.pic"),
        Some("MacPopulous/ColourData/LOAD.PIC".to_string())
    );
}

#[test]
fn find_vfs_rsrc_file_does_not_discard_explicit_path_components() {
    let disp = super::super::TrapDispatcher::new();
    disp.vfs_rsrc
        .insert("Character Files/Data CD".to_string(), vec![1]);

    assert_eq!(disp.find_vfs_rsrc_file(":Data Files:Data CD"), None);
    assert_eq!(
        disp.find_vfs_rsrc_file("Data CD"),
        Some("Character Files/Data CD".to_string())
    );
}

// ================================================================
// 0. CreateResFile (0x1B1)
// ================================================================
#[test]
fn createresfile_missing_file_creates_data_and_resource_forks() {
    // IM:More Macintosh Toolbox 1993, p. 1-57: CreateResFile creates a
    // file with a zero-length data fork and an empty resource fork map
    // when no matching file exists.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;
    let name_ptr = 0x230000u32;
    write_pstring(&mut bus, name_ptr, b"Prefs.RSRC");
    bus.write_long(sp, name_ptr);

    call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp + 4,
        "CreateResFile pops fileName ptr"
    );
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
    assert!(
        disp.vfs.contains_key("Prefs.RSRC"),
        "missing file should create data fork"
    );
    assert!(
        disp.vfs_rsrc.contains_key("Prefs.RSRC"),
        "missing file should create resource fork"
    );
    assert_eq!(disp.vfs.get("Prefs.RSRC").unwrap(), &Vec::<u8>::new());
    assert!(
        crate::managers::resource::ResourceFork::parse(disp.vfs_rsrc.get("Prefs.RSRC").unwrap())
            .is_some(),
        "CreateResFile should seed a structurally valid empty resource map"
    );
}

#[test]
fn createresfile_existing_data_fork_adds_resource_fork_without_truncating_data() {
    // IM:More Macintosh Toolbox 1993, p. 1-57: if data fork exists with
    // zero-length/missing resource fork, CreateResFile adds the resource
    // fork map and does not destroy file data.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs
        .insert("DATAONLY".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]);

    let sp = TEST_SP;
    let name_ptr = 0x230100u32;
    write_pstring(&mut bus, name_ptr, b"DATAONLY");
    bus.write_long(sp, name_ptr);

    call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
    assert_eq!(
        disp.vfs.get("DATAONLY").unwrap(),
        &vec![0xDE, 0xAD, 0xBE, 0xEF],
        "CreateResFile should not truncate existing data fork bytes"
    );
    assert!(
        crate::managers::resource::ResourceFork::parse(disp.vfs_rsrc.get("DATAONLY").unwrap())
            .is_some(),
        "CreateResFile should add a resource fork map for existing data file"
    );
}

#[test]
fn createresfile_existing_zero_length_resource_fork_returns_noerr() {
    // IM:More Macintosh Toolbox 1993, p. 1-57: if the data fork exists
    // and the resource fork is zero-length, CreateResFile creates the
    // empty resource map instead of returning dupFNErr.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs
        .insert("ZERORSRC".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    disp.vfs_rsrc.insert("ZERORSRC".to_string(), Vec::new());

    let sp = TEST_SP;
    let name_ptr = 0x230180u32;
    write_pstring(&mut bus, name_ptr, b"ZERORSRC");
    bus.write_long(sp, name_ptr);

    call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
    assert_eq!(
        disp.vfs.get("ZERORSRC").unwrap(),
        &vec![0xDE, 0xAD, 0xBE, 0xEF],
        "CreateResFile should preserve existing data fork bytes"
    );
    assert!(
        crate::managers::resource::ResourceFork::parse(disp.vfs_rsrc.get("ZERORSRC").unwrap())
            .is_some(),
        "CreateResFile should turn a zero-length resource fork into an empty mapped fork"
    );
}

#[test]
fn createresfile_existing_resource_fork_returns_dupfnerr_and_leaves_file_unchanged() {
    // IM:More Macintosh Toolbox 1993, p. 1-57 (Special Considerations):
    // if a matching resource file is found during PBOpenRF search,
    // CreateResFile does nothing and ResError reports dupFNErr (-48).
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("DUPLICATE".to_string(), vec![0x11, 0x22]);
    disp.vfs_rsrc
        .insert("DUPLICATE".to_string(), vec![0x33, 0x44]);

    let sp = TEST_SP;
    let name_ptr = 0x230200u32;
    write_pstring(&mut bus, name_ptr, b"DUPLICATE");
    bus.write_long(sp, name_ptr);

    call(&mut disp, true, 0x1B1, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        -48,
        "ResErr should be dupFNErr"
    );
    assert_eq!(disp.vfs.get("DUPLICATE").unwrap(), &vec![0x11, 0x22]);
    assert_eq!(disp.vfs_rsrc.get("DUPLICATE").unwrap(), &vec![0x33, 0x44]);
}

// ================================================================
// 1. GetResource (0x1A0) — found
// ================================================================
#[test]
fn get_resource_found() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"STR#", 128, &[0; 10]);

    // Push params: SP+0 = id(2), SP+2 = type(4)
    let sp = TEST_SP;
    bus.write_word(sp, 128u16); // id
    bus.write_long(sp + 2, u32::from_be_bytes(*b"STR#")); // type

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
    let handle = bus.read_long(new_sp);
    assert_ne!(handle, 0, "handle should be non-zero");
    // Dereference handle -> ptr should equal data_ptr
    let deref = bus.read_long(handle);
    assert_eq!(deref, data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");
}

#[test]
fn get_resource_synthesizes_system_clut_depth_id() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP;
    bus.write_word(sp, 4u16); // standard 4-bit System color table ID
    bus.write_long(sp + 2, u32::from_be_bytes(*b"clut"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
    let handle = bus.read_long(new_sp);
    assert_ne!(handle, 0, "synthetic clut must return a handle");
    assert_eq!(cpu.read_reg(Register::A0), handle);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

    let ctab = bus.read_long(handle);
    assert_ne!(ctab, 0, "synthetic clut handle must be loaded");
    assert_eq!(bus.read_long(ctab), 4, "ctSeed should match the depth ID");
    assert_eq!(bus.read_word(ctab + 4), 0, "ctFlags should be zero");
    assert_eq!(bus.read_word(ctab + 6), 15, "ctSize should hold 16 entries");
    assert_eq!(bus.read_word(ctab + 8), 0, "entry 0 value");
    assert_eq!(bus.read_word(ctab + 10), 0xFFFF, "entry 0 red");
    assert_eq!(bus.read_word(ctab + 12), 0xFFFF, "entry 0 green");
    assert_eq!(bus.read_word(ctab + 14), 0xFFFF, "entry 0 blue");
    let black = ctab + 8 + 15 * 8;
    assert_eq!(bus.read_word(black), 15, "entry 15 value");
    assert_eq!(bus.read_word(black + 2), 0, "entry 255 red");
    assert_eq!(bus.read_word(black + 4), 0, "entry 255 green");
    assert_eq!(bus.read_word(black + 6), 0, "entry 255 blue");
}

#[test]
fn get_resource_synthesizes_us_system_intl_zero() {
    // IM:I pp. I-495..I-499: INTL ID 0 contains the active numeric,
    // currency, short-date, and time conventions.
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"INTL"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(disp.resource_handle_files.get(&handle), Some(&0));
    assert_eq!(bus.read_word(0x0A60), 0);

    let intl = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(intl), Some(32));
    assert_eq!(bus.read_bytes(intl, 12), b".,;$\0\0\xF0\0\0/\xFF\x60");
    assert_eq!(bus.read_bytes(intl + 12, 4), b" AM\0");
    assert_eq!(bus.read_bytes(intl + 16, 4), b" PM\0");
    assert_eq!(bus.read_byte(intl + 20), b':');
    assert_eq!(bus.read_word(intl + 30), 0);
}

#[test]
fn get_resource_synthesizes_standard_system_pattern_list() {
    // IM:I pp. I-475..I-476: PAT# ID 0 is a count word followed by the
    // 38 eight-byte patterns in the standard MacPaint palette.
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PAT#"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(disp.resource_handle_files.get(&handle), Some(&0));
    assert_eq!(bus.read_word(0x0A60), 0);

    let list = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(list), Some(306));
    assert_eq!(bus.read_word(list), 38);
    let body_hash = bus
        .read_bytes(list, 306)
        .iter()
        .fold(0xCBF2_9CE4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01B3)
        });
    assert_eq!(body_hash, 0x3A52_B0D8_76F0_2FE3);
    assert_eq!(bus.read_bytes(list + 2, 8), &[0xFF; 8]);
    assert_eq!(
        bus.read_bytes(list + 2 + 19 * 8, 8),
        &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        bus.read_bytes(list + 2 + 37 * 8, 8),
        &[0x00, 0x08, 0x14, 0x2A, 0x55, 0x2A, 0x14, 0x08]
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PAT#"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_long(TEST_SP + 6), handle);
}

#[test]
fn get_resource_does_not_synthesize_unknown_system_pattern_list() {
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PAT#"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(TEST_SP + 6), 0);
    // Match the observed System 7.5.3 GetResource miss contract.
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn get_resource_synthesizes_standard_system_window_color_table() {
    // IM:V pp. V-201..V-203: InitWindows falls back to the System-file
    // or ROM `'wctb'` ID 0 when the application does not provide one.
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"wctb"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(cpu.read_reg(Register::A0), handle);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(bus.read_word(0x0A60), 0);

    let table = bus.read_long(handle);
    assert_ne!(table, 0);
    assert_eq!(bus.get_alloc_size(table), Some(112));
    assert_eq!(bus.read_long(table), 0);
    assert_eq!(bus.read_word(table + 4), 0);
    assert_eq!(bus.read_word(table + 6), 12);
    assert_eq!(bus.read_word(table + 8), 0); // wContentColor
    assert_eq!(bus.read_word(table + 10), 0xFFFF);
    assert_eq!(bus.read_word(table + 12), 0xFFFF);
    assert_eq!(bus.read_word(table + 14), 0xFFFF);
    assert_eq!(bus.read_word(table + 16), 1); // wFrameColor
    assert_eq!(bus.read_word(table + 18), 0);
    assert_eq!(bus.read_word(table + 104), 12); // wTingeDark
    assert_eq!(bus.read_word(table + 106), 0x3333);
    assert_eq!(bus.read_word(table + 108), 0x3333);
    assert_eq!(bus.read_word(table + 110), 0x6666);
}

#[test]
fn get_resource_synthesizes_standard_wdef_zero() {
    // Inside Macintosh Volume V, V-32 lists WDEF 0 as the default
    // document-window definition function stored in the Macintosh ROM.
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP;
    bus.write_word(sp, 0u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"WDEF"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
    let handle = bus.read_long(new_sp);
    assert_ne!(handle, 0, "synthetic WDEF 0 must return a handle");
    assert_eq!(cpu.read_reg(Register::A0), handle);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

    let wdef = bus.read_long(handle);
    assert_ne!(wdef, 0, "synthetic WDEF handle must be loaded");
    assert_eq!(
        bus.read_word(wdef),
        0x205F,
        "standard WDEF shim should first recover the JSR return address"
    );
    assert_eq!(
        bus.read_word(wdef + 2),
        0xDEFC,
        "standard WDEF shim should discard its 12-byte Pascal parameter block"
    );
    assert_eq!(
        bus.read_word(wdef + 8),
        0x4ED0,
        "standard WDEF shim should return through the recovered address"
    );
}

#[test]
fn get_resource_synthesizes_standard_roman_kchr_id_zero() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP;
    bus.write_word(sp, 0u16); // standard U.S. Roman keyboard layout
    bus.write_long(sp + 2, u32::from_be_bytes(*b"KCHR"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6, "SP should advance by 6");
    let handle = bus.read_long(new_sp);
    assert_ne!(handle, 0, "synthetic KCHR must return a handle");
    assert_eq!(cpu.read_reg(Register::A0), handle);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(0x0A60), 0, "ResErr should be noErr");

    let kchr = bus.read_long(handle);
    assert_ne!(kchr, 0, "synthetic KCHR handle must be loaded");
    assert_eq!(bus.get_alloc_size(kchr), Some(2 + 256 + 2 + 128 * 2 + 2));
    assert_eq!(bus.read_word(kchr), 0, "KCHR version word");
    assert_eq!(
        bus.read_byte(kchr + 2),
        0,
        "modifier byte 0 should select the unshifted table"
    );
    assert_eq!(
        bus.read_byte(kchr + 3),
        0,
        "modifier byte 1 is Command-only and should stay unshifted"
    );
    assert_eq!(
        bus.read_byte(kchr + 4),
        1,
        "modifier byte 2 should select the shifted table"
    );

    assert_eq!(bus.read_word(kchr + 2 + 256), 2, "KCHR table count");
    let table0 = kchr + 2 + 256 + 2;
    let table1 = table0 + 128;
    assert_eq!(bus.read_byte(table0), b'a');
    assert_eq!(bus.read_byte(table1), b'A');
    assert_eq!(bus.read_byte(table0 + 0x0C), b'q');
    assert_eq!(bus.read_byte(table1 + 0x0C), b'Q');
    assert_eq!(bus.read_byte(table0 + 0x7B), 0x1C, "left arrow");
    assert_eq!(bus.read_byte(table0 + 0x7C), 0x1D, "right arrow");
    assert_eq!(bus.read_byte(table0 + 0x7D), 0x1F, "down arrow");
    assert_eq!(bus.read_byte(table0 + 0x7E), 0x1E, "up arrow");
    assert_eq!(
        bus.read_bytes(table1 + 0x7B, 4),
        vec![0x1C, 0x1D, 0x1F, 0x1E],
        "shifted arrow translations should remain navigation characters"
    );
    assert_eq!(bus.read_word(table1 + 128), 0, "KCHR dead-key count");
}

#[test]
fn get_resource_synthesizes_standard_adb_kmap_id_zero() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP;
    bus.write_word(sp, 0);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"KMAP"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0, "synthetic KMAP must return a handle");
    let kmap = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(kmap), Some(4 + 128 + 2));
    assert_eq!(bus.read_word(kmap), 0, "KMAP identifier");
    assert_eq!(bus.read_word(kmap + 2), 0, "KMAP version");
    let remapped = [
        (0x36u32, 0x3Bu8),
        (0x3B, 0x7B),
        (0x3C, 0x7C),
        (0x3D, 0x7D),
        (0x3E, 0x7E),
        (0x7B, 0x3C),
        (0x7C, 0x3D),
        (0x7D, 0x3E),
        (0x7E, 0x36),
    ];
    for keycode in 0..128u32 {
        let expected = remapped
            .iter()
            .find_map(|&(raw, virtual_key)| (raw == keycode).then_some(virtual_key))
            .unwrap_or(keycode as u8);
        assert_eq!(
            bus.read_byte(kmap + 4 + keycode),
            expected,
            "standard raw keycode {keycode:#04X} should use its ADB virtual-key mapping"
        );
    }
    assert_eq!(bus.read_word(kmap + 4 + 128), 0, "KMAP exception count");
}

#[test]
fn sethandlesize_resource_handle_keeps_resource_maps_current() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 77, &[1, 2, 3, 4]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"TEST", 77, data_ptr);

    disp.insert_named_resource_for_test(0, (*b"TEST", "ResizeMe".to_string()), (77, data_ptr));

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 64);
    let resize = disp.dispatch_memory(false, 0x24, &mut cpu, &mut bus);
    assert!(resize.is_some(), "SetHandleSize should be handled");
    assert!(resize.unwrap().is_ok(), "SetHandleSize should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);

    let new_ptr = bus.read_long(handle);
    assert_ne!(new_ptr, 0);
    assert_ne!(new_ptr, data_ptr);
    assert_eq!(bus.read_bytes(new_ptr, 4), vec![1, 2, 3, 4]);
    assert_eq!(bus.get_alloc_size(new_ptr), Some(64));
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|(ptr, _, _)| *ptr),
        Some(new_ptr)
    );
    assert_eq!(disp.handle_for_ptr(data_ptr), None);
    assert_eq!(disp.handle_for_ptr(new_ptr), Some(handle));

    let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
    assert_eq!(file.loaded.get(&(*b"TEST", 77)).copied(), Some(new_ptr));
    assert_eq!(
        file.named.get(&(*b"TEST", "ResizeMe".to_string())).copied(),
        Some((77, new_ptr))
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 77u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let returned_handle = bus.read_long(TEST_SP + 6);
    assert_eq!(
        returned_handle, handle,
        "GetResource should return the resized live handle, not a duplicate"
    );
    assert_eq!(bus.read_long(returned_handle), new_ptr);
}

#[test]
fn resizing_empty_resource_handle_does_not_load_unrelated_resources() {
    let (mut disp, mut cpu, mut bus) = setup();
    let first_data = setup_resources(&mut disp, &mut bus, b"TEST", 1, &[1, 2]);
    let second_data = disp.install_test_resource(&mut bus, *b"TEST", 2, &[3, 4]);
    disp.insert_resource_pointer_for_test(0, (*b"TEST", 1), 0);
    disp.insert_resource_pointer_for_test(0, (*b"TEST", 2), 0);
    disp.insert_named_resource_for_test(0, (*b"TEST", "First".to_string()), (1, 0));
    disp.insert_named_resource_for_test(0, (*b"TEST", "Second".to_string()), (2, 0));

    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"TEST", 1, first_data);
    assert_eq!(bus.read_long(handle), 0);

    let resized = disp.resize_resource_allocation(&mut bus, handle, 0, 8);
    assert_ne!(resized, 0);
    let file = &disp.resources.as_ref().unwrap().files[&0];
    assert_eq!(file.loaded[&(*b"TEST", 1)], resized);
    assert_eq!(file.loaded[&(*b"TEST", 2)], 0);
    assert_eq!(file.named[&(*b"TEST", "First".to_string())], (1, resized));
    assert_eq!(file.named[&(*b"TEST", "Second".to_string())], (2, 0));

    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));
    call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();
    let second_handle = bus.read_long(TEST_SP + 6);
    assert_ne!(second_handle, 0);
    assert_eq!(bus.read_bytes(bus.read_long(second_handle), 2), vec![3, 4]);
    assert_ne!(bus.read_long(second_handle), resized);
    assert_eq!(bus.read_bytes(second_data, 2), vec![3, 4]);
}

#[test]
fn sethandlesize_resizes_native_resource_through_process_manager() {
    const NATIVE_HEAP_BASE: u32 = 0x22_0000;
    const NATIVE_HEAP_LIMIT: u32 = 0x30_0000;

    let (mut disp, mut cpu, mut bus) = setup();
    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let shared = bus
        .shared_ram_region(0, 4 * 1024 * 1024)
        .expect("test RAM should be shareable");
    let mut native = GuestAddressSpace::new();
    context.attach_memory(0, shared, &mut native);
    let foreign = native.shared_view();
    bus.attach_guest_address_space(foreign);
    disp.attach_unconverted_process_services(&mut context);

    let memory_manager = disp.process_memory_manager();
    let handle = {
        let mut memory_manager = memory_manager.borrow_mut();
        memory_manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: NATIVE_HEAP_BASE,
                heap_cursor: NATIVE_HEAP_BASE,
                heap_limit: NATIVE_HEAP_LIMIT,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let handle = memory_manager.new_native_resource_handle(&mut native, Some(&[1, 2, 3, 4]));
        assert_ne!(handle, 0);
        assert_ne!(memory_manager.new_native_ptr(&mut native, 32, true), 0);
        handle
    };
    let original = memory_manager.borrow().native_allocation(handle).unwrap();
    let detached = memory_manager.borrow().detached_clone();
    disp.insert_loaded_resource_handle_for_test(handle, (original.ptr, *b"TEST", 77));

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 64);
    let resize = disp.dispatch_memory(false, 0x24, &mut cpu, &mut bus);

    assert!(resize.is_some(), "SetHandleSize should be handled");
    assert!(resize.unwrap().is_ok(), "SetHandleSize should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let resized = memory_manager.borrow().native_allocation(handle).unwrap();
    assert_eq!(resized.size, 64);
    assert_ne!(resized.ptr, original.ptr);
    assert_eq!(native.read_u32_be(handle), Some(resized.ptr));
    assert_eq!(
        (0..4)
            .map(|offset| native.read_u8(resized.ptr + offset).unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        memory_manager.borrow().recover_handle(resized.ptr),
        Some(handle)
    );
    assert_eq!(memory_manager.borrow().recover_handle(original.ptr), None);
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|entry| entry.0),
        Some(resized.ptr)
    );

    assert_eq!(
        memory_manager
            .borrow_mut()
            .empty_process_handle(&mut bus, handle),
        0
    );
    disp.empty_resource_handle_residency(handle);
    assert_eq!(native.read_u32_be(handle), Some(0));
    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 16);
    let refill = disp.dispatch_memory(false, 0x24, &mut cpu, &mut bus);
    assert!(
        refill.is_some(),
        "SetHandleSize should refill an empty handle"
    );
    assert!(refill.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let refilled = memory_manager.borrow().native_allocation(handle).unwrap();
    assert_eq!(refilled.size, 16);
    assert_ne!(refilled.ptr, 0);
    assert_eq!(native.read_u32_be(handle), Some(refilled.ptr));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x60));
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|entry| entry.0),
        Some(refilled.ptr)
    );
    assert_eq!(detached.native_allocation(handle), Some(original));
    assert_ne!(detached.native_allocation(handle), Some(refilled));
}

#[test]
fn get_resource_respects_setresload_false_until_loadresource() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 501, &[0xCA, 0xFE]);

    // IM:More Macintosh Toolbox 1993, 1-79 to 1-80: after
    // SetResLoad(FALSE), GetResource returns an empty handle for data
    // that is not already in memory; LoadResource later fills it.
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);

    let sp = TEST_SP;
    bus.write_word(sp, 501u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"LOAD"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0, "GetResource should still return a handle");
    assert_eq!(
        bus.read_long(handle),
        0,
        "master pointer should be nil while automatic loading is disabled"
    );
    assert_eq!(
        disp.take_hle_tick_cost(),
        0,
        "empty SetResLoad(FALSE) handles should not charge materialization work"
    );
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);

    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert!(
        disp.take_hle_tick_cost() > 0,
        "LoadResource should charge for filling an empty resource handle"
    );
}

#[test]
fn get_resource_autoload_records_hle_work_once() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"LOAD", 503, &[0x5A; 1024]);

    bus.write_word(TEST_SP, 503u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_ne!(bus.read_long(handle), 0);
    assert!(
        disp.take_hle_tick_cost()
            >= super::super::TrapDispatcher::resource_load_tick_cost(1024) as i32,
        "first autoloaded GetResource should charge by resource size"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 503u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        disp.take_hle_tick_cost(),
        0,
        "reusing an already-loaded resource handle should not charge another materialization"
    );
}

#[test]
fn get_resource_autoloads_existing_empty_handle_after_setresload_true() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 502, &[0xBE, 0xEF]);

    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);
    bus.write_word(TEST_SP, 502u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let empty_handle = bus.read_long(TEST_SP + 6);
    assert_ne!(empty_handle, 0);
    assert_eq!(bus.read_long(empty_handle), 0);

    // IM:More Macintosh Toolbox 1993, 1-79: SetResLoad(TRUE) returns
    // resource-returning calls to automatic loading behavior.
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 502u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"LOAD"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let loaded_handle = bus.read_long(TEST_SP + 6);
    assert_eq!(loaded_handle, empty_handle);
    assert_eq!(bus.read_long(loaded_handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A0, data_ptr);
    let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
    assert!(recovered.is_some(), "RecoverHandle should be handled");
    assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
    assert_eq!(cpu.read_reg(Register::A0), loaded_handle);
    assert_eq!(disp.handle_for_ptr(data_ptr), Some(loaded_handle));
}

// ================================================================
// 1b. GetResource (0x1A0) — not found
// ================================================================
#[test]
fn get_resource_returns_standard_system_icon_one_after_application_chain_miss() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(TEST_SP, 1u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"ICON"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    let ptr = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(ptr), Some(128));
    assert_eq!(bus.read_long(ptr), 0xFFFF_FFFF);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn get_resource_miss_returns_nil_with_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0xAA, 0xBB]);

    let sp = TEST_SP;
    bus.write_word(sp, 999u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"TEST"));

    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6);
    let handle = bus.read_long(new_sp);
    assert_eq!(handle, 0, "handle should be NULL");
    // BasiliskII/System 7.5.3: missing ID under an existing type is NIL
    // with noErr despite IM:I I-119 documenting resNotFound.
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

// More Macintosh Toolbox (1993), p. 1-78: RGetResource follows the
// GetResource search contract (plus ROM retry on real hardware).
#[test]
fn rgetresource_returns_handle_for_open_chain_match() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0x41, 0x42, 0x43, 0x44]);

    bus.write_word(TEST_SP, 128u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));

    call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// More Macintosh Toolbox (1993), p. 1-78: failed RGetResource requests
// return NIL and surface not-found through ResError.
#[test]
fn rgetresource_miss_returns_nil_with_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 777u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"MISS"));

    call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(bus.read_long(TEST_SP + 6), 0);
    assert_eq!(bus.read_word(0x0A60), (-192i16) as u16);
}

// More Macintosh Toolbox (1993), p. 1-78: RGetResource delegates to
// GetResource chain semantics before any ROM retry.
#[test]
fn rgetresource_walks_chain_not_just_current_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    let chain_ptr = bus.alloc(4);
    bus.write_bytes(chain_ptr, &[0xAA, 0xBB, 0xCC, 0xDD]);

    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([
            (
                100,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"PICT", 300), chain_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            ),
            (
                200,
                ResourceFileMap {
                    loaded: HashMap::new(),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            ),
        ]),
        names: HashMap::new(),
        search_order: vec![100, 200],
        current_file: 200,
    });

    bus.write_word(TEST_SP, 300u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));

    call(&mut disp, true, 0x00C, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(bus.read_long(handle), chain_ptr);
    assert_eq!(disp.resource_handle_files.get(&handle).copied(), Some(100));
    assert_eq!(bus.read_word(0x0A60), 0);
}

// ================================================================
// 2. Get1Resource (0x01F) — found
// ================================================================
#[test]
fn get1_resource_found() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);

    let sp = TEST_SP;
    bus.write_word(sp, 200u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"DLOG"));

    call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6);
    let handle = bus.read_long(new_sp);
    assert_ne!(handle, 0);
    let deref = bus.read_long(handle);
    assert_eq!(deref, data_ptr);
}

#[test]
fn get1_resource_uses_current_resource_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    let app_ptr = bus.alloc(4);
    bus.write_bytes(app_ptr, &[0xAA; 4]);
    let images_ptr = bus.alloc(4);
    bus.write_bytes(images_ptr, &[0xBB; 4]);

    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([
            (
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"PICT", 1114), app_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            ),
            (
                101,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"PICT", 1114), images_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            ),
        ]),
        names: HashMap::new(),
        search_order: vec![0, 101],
        current_file: 101,
    });

    let sp = TEST_SP;
    bus.write_word(sp, 1114u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"PICT"));

    call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    assert_ne!(handle, 0);
    assert_eq!(bus.read_long(handle), images_ptr);
    assert_eq!(disp.resource_handle_files.get(&handle).copied(), Some(101));
}

// ================================================================
// 2b. Get1Resource (0x01F) — not found
// ================================================================
#[test]
fn get1_resource_miss_returns_nil_with_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"TEST", 128, &[0x11, 0x22]);

    let sp = TEST_SP;
    bus.write_word(sp, 999u16);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"TEST"));

    call(&mut disp, true, 0x01F, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6);
    let handle = bus.read_long(new_sp);
    assert_eq!(handle, 0);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

// ================================================================
// 3. DetachResource (0x192)
// ================================================================
#[test]
fn detach_resource() {
    let (mut disp, mut cpu, mut bus) = setup();

    let shared_ptr = bus.alloc(8);
    for (offset, byte) in [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]
        .into_iter()
        .enumerate()
    {
        bus.write_byte(shared_ptr + offset as u32, byte);
    }
    let handle = bus.alloc(4);
    bus.write_long(handle, shared_ptr);
    disp.insert_loaded_resource_handle_for_test(handle, (shared_ptr, *b"TEST", 7));

    let sp = TEST_SP;
    bus.write_long(sp, handle);

    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);

    let detached_ptr = bus.read_long(handle);
    assert_ne!(detached_ptr, shared_ptr);
    assert_eq!(
        bus.read_bytes(detached_ptr, 8),
        bus.read_bytes(shared_ptr, 8)
    );
    assert!(!disp.loaded_handles.contains_key(&handle));
    assert_eq!(bus.read_word(0x0A60), 0);

    bus.write_byte(detached_ptr + 3, 0xEE);
    assert_eq!(bus.read_byte(shared_ptr + 3), 0x40);
}

#[test]
fn detach_resource_keeps_unloaded_resource_map_reference_reloadable() {
    // Inside Macintosh Volume I (1985), p. I-122: DetachResource makes
    // the handle ordinary, but the resource remains in its file. A later
    // lookup must therefore still count and reload the resource.
    let (mut disp, mut cpu, mut bus) = setup();
    let original = [0x10u8, 0x20, 0x30, 0x40];
    let shared_ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &original);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, shared_ptr);

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    assert_eq!(disp.count_resources(*b"CODE", true), 1);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .loaded
            .get(&(*b"CODE", 1)),
        Some(&0)
    );
    let (refnum, reloaded_ptr) = disp
        .find_or_load_resource_current(&mut bus, *b"CODE", 1)
        .unwrap();
    assert_eq!(refnum, 0);
    assert_ne!(reloaded_ptr, shared_ptr);
    assert_eq!(bus.read_bytes(reloaded_ptr, original.len()), original);
}

// Inside Macintosh Volume I (1985), p. I-122: DetachResource must not
// detach resChanged resources, but ResError still returns noErr.
#[test]
fn detach_resource_reschanged_handle_returns_noerr_without_detaching() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 201, &[0xAA; 8]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 201, data_ptr);
    let original_ptr = bus.read_long(handle);

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_long(handle), original_ptr);
    assert!(disp.loaded_handles.contains_key(&handle));
    assert!(disp.resource_handle_files.contains_key(&handle));
}

// Inside Macintosh Volume I (1985), p. I-122: DetachResource does nothing
// and returns resNotFound when the handle is not a resource handle.
#[test]
fn detach_resource_non_resource_handle_returns_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_ptr = bus.alloc(6);
    bus.write_bytes(fake_ptr, &[1, 2, 3, 4, 5, 6]);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(fake_handle), fake_ptr);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// Per BasiliskII/System 7.5.3: DetachResource(nil) returns
// resNotFound, matching the non-resource non-nil handle path.
#[test]
fn detach_resource_nil_handle_returns_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(0x0A60, 0);
    bus.write_long(TEST_SP, 0);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        super::super::TrapDispatcher::RES_NOT_FOUND
    );
}

#[test]
fn detach_resource_unloads_map_data_but_keeps_reference_reloadable() {
    let (mut disp, mut cpu, mut bus) = setup();

    let original = [0x50, 0x61, 0x6B, 0x20, 0xAA, 0xBB];
    let data_ptr = disp.install_test_resource(&mut bus, *b"Pak ", 12000, &original);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"Pak ", 12000, data_ptr);

    let decoded_ptr = disp.resize_resource_allocation(&mut bus, handle, data_ptr, 10);
    bus.write_bytes(decoded_ptr, &[0x00, 0x02, 0x00, 0x0B, 1, 2, 3, 4, 5, 6]);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .loaded
            .get(&(*b"Pak ", 12000))
            .copied(),
        Some(decoded_ptr)
    );

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(0x0A60), 0);
    let detached_ptr = bus.read_long(handle);
    assert_ne!(detached_ptr, decoded_ptr);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .loaded
            .get(&(*b"Pak ", 12000)),
        Some(&0)
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 12000u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"Pak "));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let reloaded_handle = bus.read_long(TEST_SP + 6);
    assert_ne!(reloaded_handle, 0);
    assert_ne!(reloaded_handle, handle);
    let reloaded_ptr = bus.read_long(reloaded_handle);
    assert_ne!(reloaded_ptr, decoded_ptr);
    assert_eq!(bus.read_bytes(reloaded_ptr, original.len()), original);
}

// ================================================================
// 4. LoadResource (0x1A2)
// ================================================================
#[test]
fn load_resource() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 200, data_ptr);
    assert_eq!(
        disp.resource_handles_by_key
            .get(&(0, *b"DLOG", 200))
            .copied(),
        Some(handle)
    );

    let sp = TEST_SP;
    bus.write_long(sp, handle);

    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn load_resource_loaded_handle_is_noop_and_recoverable() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 203, &[0xCA, 0xFE]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 203, data_ptr);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(
        disp.resource_handle_files.get(&handle).copied(),
        Some(disp.current_resource_refnum()),
        "LoadResource should leave the resource-file binding intact"
    );

    cpu.write_reg(Register::A0, data_ptr);
    let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
    assert!(recovered.is_some(), "RecoverHandle should be handled");
    assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
    assert_eq!(cpu.read_reg(Register::A0), handle);
}

#[test]
fn load_resource_rewrites_stale_master_pointer_after_reload() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 204, &[0xAB, 0xCD]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 204, data_ptr);

    bus.write_long(handle, 0x1234_5678);
    assert_ne!(bus.read_long(handle), data_ptr);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(
        disp.handle_for_ptr(data_ptr),
        Some(handle),
        "LoadResource should restore RecoverHandle-style ownership"
    );
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn load_resource_repopulates_handle_after_emptyhandle_purge() {
    // More Macintosh Toolbox (1993), p. 1-80: LoadResource reads
    // resource data into memory for a resource handle. Memory (1992),
    // p. 2-52 explicitly directs purged resources to LoadResource.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 202, &[0xCA, 0xFE]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 202, data_ptr);
    let refnum = disp
        .resource_handle_files
        .get(&handle)
        .copied()
        .expect("resource handle should have a file binding");

    cpu.write_reg(Register::A0, handle);
    let emptied = disp.dispatch_memory(false, 0x2B, &mut cpu, &mut bus);
    assert!(emptied.is_some(), "EmptyHandle should be handled");
    assert!(emptied.unwrap().is_ok(), "EmptyHandle should succeed");
    assert_eq!(
        bus.read_long(handle),
        0,
        "resource handle should be empty after purge"
    );
    assert_eq!(
        bus.get_alloc_size(data_ptr),
        None,
        "EmptyHandle should return the resource block to the process heap"
    );
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|entry| entry.0),
        Some(0),
        "resource identity should remain registered without a resident pointer"
    );
    assert_eq!(
        disp.resources
            .as_ref()
            .and_then(|resources| resources.files.get(&refnum))
            .and_then(|file| file.loaded.get(&(*b"DLOG", 202)))
            .copied(),
        Some(0)
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(
        bus.read_long(handle),
        data_ptr,
        "LoadResource should restore the purged resource's master pointer"
    );
    assert_eq!(
        disp.handle_for_ptr(data_ptr),
        Some(handle),
        "LoadResource should restore RecoverHandle-style ptr ownership"
    );
    assert!(
        !disp.detached_handles.contains_key(&handle),
        "LoadResource should clear detached handle state after reloading"
    );
    assert!(
        !disp.detached_handle_files.contains_key(&handle),
        "LoadResource should clear detached file ownership after reloading"
    );
    cpu.write_reg(Register::A0, data_ptr);
    let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
    assert!(recovered.is_some(), "RecoverHandle should be handled");
    assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
    assert_eq!(
        cpu.read_reg(Register::A0),
        handle,
        "RecoverHandle should rediscover the reloaded resource handle"
    );
    assert_eq!(
        disp.resource_handle_files.get(&handle).copied(),
        Some(refnum),
        "LoadResource should restore the resource-file binding after reloading a purged handle"
    );
    assert_eq!(bus.read_word(0x0A60), 0, "LoadResource should report noErr");
}

#[test]
fn load_resource_populates_empty_handle_after_setresload_false() {
    // IM:More Macintosh Toolbox 1993, p. 1-80: SetResLoad(FALSE) can
    // return an empty handle for a present resource; LoadResource later
    // fills the master pointer back in.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"LOAD", 205, &[0x12, 0x34]);

    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"LOAD", 205, data_ptr);
    assert_eq!(bus.read_long(handle), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert!(
        disp.loaded_handles.contains_key(&handle),
        "LoadResource should keep the reloaded resource registered as loaded"
    );
    assert!(
        disp.resource_handle_files.contains_key(&handle),
        "LoadResource should keep the resource-file ownership for the reloaded handle"
    );
    cpu.write_reg(Register::A0, data_ptr);
    let recovered = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
    assert!(recovered.is_some(), "RecoverHandle should be handled");
    assert!(recovered.unwrap().is_ok(), "RecoverHandle should succeed");
    assert_eq!(cpu.read_reg(Register::A0), handle);
}

#[test]
fn load_resource_restores_stale_detached_file_binding_when_reloading() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 206, &[0xDE, 0xAD]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 206, data_ptr);
    let refnum = disp
        .resource_handle_files
        .get(&handle)
        .copied()
        .expect("resource handle should have a file binding");

    disp.remove_resource_handle_file_for_test(handle);
    disp.insert_detached_resource_handle_file_for_test(handle, refnum);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_handle_files.get(&handle).copied(),
        Some(refnum)
    );
    assert!(!disp.detached_handle_files.contains_key(&handle));
}

#[test]
fn load_resource_non_resource_handle_leaves_handle_intact_and_reserr_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, 0x00AB_CDEF);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x1A2, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(fake_handle), 0x00AB_CDEF);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// ================================================================
// 5. ReleaseResource (0x1A3)
// ================================================================
#[test]
fn release_resource_invalidates_handle_and_getresource_allocates_fresh_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 200, &[0xAB; 8]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 200, data_ptr);

    let sp = TEST_SP;
    bus.write_long(sp, handle);

    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), 0);
    assert!(
        !disp.loaded_handles.contains_key(&handle),
        "ReleaseResource should invalidate the old handle's resource identity"
    );
    assert!(
        !disp.resource_handle_files.contains_key(&handle),
        "ReleaseResource should remove the old handle's resource file binding"
    );
    assert!(
        !disp
            .resource_handles_by_key
            .contains_key(&(0, *b"DLOG", 200)),
        "ReleaseResource should remove the stale resource handle index"
    );
    assert_eq!(
        disp.handle_for_ptr(data_ptr),
        None,
        "released resource data should not be recoverable until reload"
    );
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        -192,
        "a stale released handle is no longer a resource handle"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 200u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DLOG"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    let reloaded_handle = bus.read_long(TEST_SP + 6);
    assert_ne!(reloaded_handle, 0);
    assert_ne!(
        reloaded_handle, handle,
        "GetResource after ReleaseResource should allocate a fresh handle"
    );
    assert_eq!(bus.read_long(reloaded_handle), data_ptr);
    assert_eq!(
        disp.handle_for_ptr(data_ptr),
        Some(reloaded_handle),
        "GetResource should restore RecoverHandle ownership through the fresh handle"
    );
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn release_resource_unloads_vfs_backed_resource_and_getresource_reloads() {
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0xAB; 8];
    let rsrc_bytes = make_single_resource_fork_bytes(*b"PICT", 23002, &bytes);
    disp.vfs_rsrc.insert("Reloadable".to_string(), rsrc_bytes);
    let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Reloadable", false);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 23002u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    let old_ptr = bus.read_long(handle);
    assert_ne!(handle, 0);
    assert_ne!(old_ptr, 0);
    assert_eq!(bus.get_alloc_size(old_ptr), Some(bytes.len() as u32));

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), 0);
    assert_eq!(bus.get_alloc_size(old_ptr), None);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&refnum)
            .unwrap()
            .loaded
            .get(&(*b"PICT", 23002))
            .copied(),
        Some(0),
        "ReleaseResource should mark VFS-backed data unloaded"
    );
    assert!(!disp.loaded_handles.contains_key(&handle));
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 23002u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let reloaded_handle = bus.read_long(TEST_SP + 6);
    let reloaded_ptr = bus.read_long(reloaded_handle);
    assert_ne!(reloaded_handle, 0);
    assert_ne!(reloaded_handle, handle);
    assert_ne!(reloaded_ptr, 0);
    assert_eq!(bus.get_alloc_size(reloaded_ptr), Some(bytes.len() as u32));
    assert_eq!(bus.read_bytes(reloaded_ptr, bytes.len()), bytes);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn released_resource_reloads_from_backing_cache_without_reparsing_fork() {
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0xCD; 8];
    let rsrc_bytes = make_single_resource_fork_bytes(*b"PICT", 23003, &bytes);
    disp.vfs_rsrc.insert("CachedReload".to_string(), rsrc_bytes);
    let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "CachedReload", false);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 23003u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 6);
    let old_ptr = bus.read_long(handle);
    assert_ne!(handle, 0);
    assert_ne!(old_ptr, 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(handle), 0);
    assert_eq!(bus.get_alloc_size(old_ptr), None);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&refnum)
            .unwrap()
            .loaded
            .get(&(*b"PICT", 23003))
            .copied(),
        Some(0)
    );

    disp.vfs_rsrc.remove("CachedReload");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 23003u16);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PICT"));
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    let reloaded_handle = bus.read_long(TEST_SP + 6);
    let reloaded_ptr = bus.read_long(reloaded_handle);
    assert_ne!(reloaded_handle, 0);
    assert_ne!(reloaded_ptr, 0);
    assert_eq!(bus.read_bytes(reloaded_ptr, bytes.len()), bytes);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn release_resource_reschanged_handle_is_not_released_and_reserr_is_noerr() {
    // IM:I I-121: ReleaseResource does not release a resource whose
    // resChanged attribute is set, but ResError still reports noErr.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 201, &[0xAA; 12]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 201, data_ptr);
    let original_ptr = bus.read_long(handle);

    // Mark resource changed via ChangedResource ($A9AA).
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(handle), original_ptr);
    assert!(disp.loaded_handles.contains_key(&handle));
    assert!(disp.resource_handle_files.contains_key(&handle));
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn release_resource_non_resource_handle_sets_resnotfound() {
    // IM:I I-121: non-resource handles are ignored and report resNotFound.
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_ptr = bus.alloc(4);
    bus.write_bytes(fake_ptr, &[0xDE, 0xAD, 0xBE, 0xEF]);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(fake_handle), fake_ptr);
    assert!(!disp.loaded_handles.contains_key(&fake_handle));
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// ================================================================
// 5b. SizeResource (0x1A5)
// ================================================================
#[test]
fn size_resource_loaded_resource_handle_returns_byte_size() {
    // IM:I I-121: SizeResource returns the resource size in bytes.
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70];
    let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 77, &bytes);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 77, data_ptr);

    bus.write_word(0x0A60, (-192i16) as u16); // prove success clears stale error
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn size_resource_empty_handle_from_setresload_false_style_still_reports_size() {
    // IM:I I-118..I-121: SetResLoad(FALSE) can yield an empty resource
    // handle, but SizeResource still reports the resource-file byte size.
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0x41u8, 0x42, 0x43, 0x44, 0x45];
    let data_ptr = setup_resources(&mut disp, &mut bus, b"ALRT", 90, &bytes);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"ALRT", 90, data_ptr);

    // Match the state produced by GetResource/Get1IndResource while
    // automatic loading is disabled: the map-owned handle and backing
    // bytes remain known, but both recorded and live data pointers are NIL.
    disp.insert_loaded_resource_handle_for_test(handle, (0, *b"ALRT", 90));
    disp.insert_resource_pointer_for_test(0, (*b"ALRT", 90), 0);
    bus.write_long(handle, 0);
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_long(handle), 0, "size query must not load data");
    assert_eq!(disp.loaded_handles.get(&handle).unwrap().0, 0);
}

#[test]
fn size_resource_reloads_nil_backing_pointer_when_loading_is_enabled() {
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0x11u8, 0x22, 0x33, 0x44];
    let data_ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &bytes);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, data_ptr);
    disp.insert_loaded_resource_handle_for_test(handle, (0, *b"CODE", 1));
    disp.insert_resource_pointer_for_test(0, (*b"CODE", 1), 0);
    bus.write_long(handle, 0);
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(TEST_SP + 4), bytes.len() as u32);
    assert_ne!(bus.read_long(handle), 0);
    assert_eq!(bus.read_bytes(bus.read_long(handle), bytes.len()), bytes);
}

#[test]
fn size_resource_non_resource_handles_return_minus_one_and_resnotfound() {
    // IM:I I-121: SizeResource returns -1 and resNotFound for handles that
    // are not Resource Manager handles (including detached handles).
    let (mut disp, mut cpu, mut bus) = setup();

    // Unknown handle case.
    let fake_ptr = bus.alloc(3);
    bus.write_bytes(fake_ptr, &[0x01, 0x02, 0x03]);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
    let mut new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, -1);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);

    // Detached resource handle case.
    let data_ptr = setup_resources(&mut disp, &mut bus, b"DLOG", 301, &[0xAA; 6]);
    let res_handle = disp.get_or_create_resource_handle(&mut bus, *b"DLOG", 301, data_ptr);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, res_handle);
    call(&mut disp, true, 0x192, &mut cpu, &mut bus).unwrap(); // DetachResource

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, res_handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
    new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, -1);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// ================================================================
// 5c. MaxSizeRsrc (0x021)
// ================================================================
#[test]
fn maxsizersrc_returns_resource_size_for_loaded_resource_handle() {
    // IM:IV IV-16: MaxSizeRsrc returns resource size from the map.
    let (mut disp, mut cpu, mut bus) = setup();
    let bytes = [0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let data_ptr = setup_resources(&mut disp, &mut bus, b"MSIZ", 42, &bytes);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"MSIZ", 42, data_ptr);

    bus.write_word(0x0A60, (-192i16) as u16); // stale error should clear
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, bytes.len() as i32);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn maxsizersrc_returns_minus_one_and_sets_resnotfound_for_non_resource_handle() {
    // IM:I I-121 + IM:IV IV-16: non-resource handles report
    // "not found" and return -1 size.
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_ptr = bus.alloc(5);
    bus.write_bytes(fake_ptr, &[1, 2, 3, 4, 5]);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert_eq!(bus.read_long(new_sp) as i32, -1);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

#[test]
fn maxsizersrc_consumes_handle_argument_and_writes_function_result_slot() {
    // FUNCTION MaxSizeRsrc(theResource: Handle): LONGINT
    // (IM:IV IV-16): pop 4-byte arg, write 4-byte result at new SP.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"MSZ2", 7, &[0x10, 0x20, 0x30]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"MSZ2", 7, data_ptr);

    bus.write_long(TEST_SP, handle);
    bus.write_long(TEST_SP + 4, 0xDEADBEEF); // result slot sentinel
    call(&mut disp, true, 0x021, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(
        bus.read_long(TEST_SP + 4),
        3,
        "MaxSizeRsrc writes result to function-result slot at post-pop SP"
    );
}

// ================================================================
// 5d. ResourceDispatch (0x022) — selectors 1/2/3
// ================================================================
#[test]
fn resourcedispatch_generated_selector_routes_are_sorted_unique_and_complete() {
    assert_eq!(super::RESOURCE_DISPATCH_OPERATION_ROUTES.len(), 3);
    assert!(super::RESOURCE_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    let read = super::resource_dispatch_operation_route(0xA822, 0x0001)
        .expect("ReadPartialResource route");
    assert_eq!(read.routine_name, "ReadPartialResource");
    assert_eq!(
        read.operation_id,
        "selector-operation:_ResourceDispatch:0x0001:d0-moveq-immediate:8"
    );

    assert!(super::resource_dispatch_operation_route(0xA822, 0x7001).is_none());
    assert!(super::resource_dispatch_operation_route(0xA822, 0x0001_0001).is_none());
    assert!(super::resource_dispatch_operation_route(0xAA22, 0x0001).is_none());
}

#[test]
fn resourcedispatch_records_every_generated_operation_and_rejects_opcode_spelling() {
    let (mut disp, mut cpu, mut bus) = setup();
    for route in super::RESOURCE_DISPATCH_OPERATION_ROUTES {
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, u32::from(route.selector));
        bus.write_bytes(TEST_SP, &[0; 16]);
        call_trap_word(&mut disp, 0xA822, &mut cpu, &mut bus).unwrap();
        assert_eq!(disp.current_selector_operation, Some(route.operation_id));
    }

    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x7001);
    bus.write_bytes(TEST_SP, &[0; 16]);
    call_trap_word(&mut disp, 0xA822, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.current_selector_operation, None);

    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x0001_0001);
    bus.write_bytes(TEST_SP, &[0; 16]);
    call_trap_word(&mut disp, 0xA822, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.current_selector_operation, None);
}

#[test]
fn readpartialresource_selector_1_copies_requested_bytes_and_pops_16_bytes() {
    // MMTB 1993 pp. 1-111 to 1-113 prints the MOVEQ opcode $7001;
    // this compatibility probe preserves the handler's historical
    // low-byte dispatch while generated runtime identity uses D0 = 1.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(
        &mut disp,
        &mut bus,
        b"PART",
        90,
        &[0x10, 0x20, 0x30, 0x40, 0x50],
    );
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"PART", 90, data_ptr);
    let buffer = bus.alloc(6);
    bus.write_bytes(buffer, &[0xEE; 6]);

    bus.write_long(TEST_SP, 3); // count
    bus.write_long(TEST_SP + 4, buffer);
    bus.write_long(TEST_SP + 8, 1); // offset
    bus.write_long(TEST_SP + 12, handle);
    cpu.write_reg(Register::D0, 0x7001); // low-byte selector form

    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_bytes(buffer, 3), vec![0x20, 0x30, 0x40]);
    assert_eq!(bus.read_word(0x0A60) as i16, -188);
}

#[test]
fn getresource_obeys_direct_resload_writes_for_empty_and_resident_handles() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"PART", 94, &[1, 2, 3, 4]);
    let res_load = crate::memory::globals::addr::RES_LOAD;
    let mut handle = 0;

    // Assembly callers save, clear, and restore the ResLoad byte without
    // calling SetResLoad. An adjacent nonzero byte is not part of the flag.
    for (flag, expected_ptr) in [(0u8, 0), (0, 0), (0xFF, data_ptr), (0, data_ptr)] {
        bus.write_word(res_load, (u16::from(flag) << 8) | 0xFF);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 94);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"PART"));
        call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();
        let returned = bus.read_long(TEST_SP + 6);
        assert_ne!(returned, 0);
        if handle == 0 {
            handle = returned;
        }
        assert_eq!(returned, handle, "resource identity must remain stable");
        assert_eq!(bus.read_long(handle), expected_ptr);
        assert_eq!(bus.read_word(0x0A60), 0);
    }
}

#[test]
fn readpartialresource_empty_handle_reads_backing_resource_and_reports_noerr() {
    // MMTB 1993 1-41: the partial-resource workflow calls
    // SetResLoad(FALSE), gets an empty resource handle, restores
    // SetResLoad(TRUE), then calls ReadPartialResource. That read
    // should stream from the resource on disk and report noErr.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(
        &mut disp,
        &mut bus,
        b"PART",
        94,
        &[0x10, 0x20, 0x30, 0x40, 0x50],
    );
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"PART", 94, data_ptr);
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    assert_eq!(bus.read_long(handle), 0, "handle should be empty");
    let buffer = bus.alloc(4);
    bus.write_bytes(buffer, &[0xEE; 4]);

    bus.write_long(TEST_SP, 4); // count
    bus.write_long(TEST_SP + 4, buffer);
    bus.write_long(TEST_SP + 8, 1); // offset
    bus.write_long(TEST_SP + 12, handle);
    cpu.write_reg(Register::D0, 0x0001);

    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_bytes(buffer, 4), vec![0x20, 0x30, 0x40, 0x50]);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_long(handle), 0, "partial read should not load it");
}

#[test]
fn writepartialresource_selector_2_extends_resource_and_sets_writingpastend() {
    // MMTB 1993 1-70: writes past end extend the resource and report
    // writingPastEnd (-189).
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"WRPT", 91, &[0xAA, 0xBB, 0xCC, 0xDD]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"WRPT", 91, data_ptr);
    let src = bus.alloc(4);
    bus.write_bytes(src, &[1, 2, 3, 4]);

    bus.write_long(TEST_SP, 4); // count
    bus.write_long(TEST_SP + 4, src);
    bus.write_long(TEST_SP + 8, 3); // offset
    bus.write_long(TEST_SP + 12, handle);
    cpu.write_reg(Register::D0, 0x0002);

    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(0x0A60) as i16, -189);

    let live_ptr = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(live_ptr).unwrap_or(0), 7);
    assert_eq!(
        bus.read_bytes(live_ptr, 7),
        vec![0xAA, 0xBB, 0xCC, 1, 2, 3, 4]
    );
}

#[test]
fn writepartialresource_empty_handle_updates_backing_resource_and_reports_noerr() {
    // MMTB 1993 1-41 describes using SetResLoad(FALSE) and an empty
    // handle for the partial-resource family; writes should still use
    // the resource map entry rather than treating the handle as missing.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"WEPT", 95, &[0xAA, 0xBB, 0xCC, 0xDD]);
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 0);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"WEPT", 95, data_ptr);
    bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    assert_eq!(bus.read_long(handle), 0, "handle should be empty");
    let src = bus.alloc(2);
    bus.write_bytes(src, &[0x11, 0x22]);

    bus.write_long(TEST_SP, 2); // count
    bus.write_long(TEST_SP + 4, src);
    bus.write_long(TEST_SP + 8, 1); // offset
    bus.write_long(TEST_SP + 12, handle);
    cpu.write_reg(Register::D0, 0x0002);

    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_bytes(data_ptr, 4), vec![0xAA, 0x11, 0x22, 0xDD]);
    assert_eq!(bus.read_long(handle), 0, "partial write should not load it");
}

#[test]
fn writepartialresource_selector_2_zero_count_does_not_resize_resource() {
    // MMTB 1993 1-70: a zero-length write is a no-op and does not
    // grow the resource or report writingPastEnd.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"ZCNT", 93, &[0x11, 0x22, 0x33, 0x44]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"ZCNT", 93, data_ptr);
    let src = bus.alloc(4);
    bus.write_bytes(src, &[9, 8, 7, 6]);

    bus.write_long(TEST_SP, 0); // count
    bus.write_long(TEST_SP + 4, src);
    bus.write_long(TEST_SP + 8, 7); // offset past end
    bus.write_long(TEST_SP + 12, handle);
    cpu.write_reg(Register::D0, 0x0002);

    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(0x0A60), 0);

    let live_ptr = bus.read_long(handle);
    assert_eq!(bus.get_alloc_size(live_ptr).unwrap_or(0), 4);
    assert_eq!(bus.read_bytes(live_ptr, 4), vec![0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn setresourcesize_selector_3_updates_resource_size_visible_to_sizeresource() {
    // MMTB 1993 1-71: selector 3 updates resource size.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"RSIZ", 92, &[0x10, 0x20, 0x30]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"RSIZ", 92, data_ptr);

    bus.write_long(TEST_SP, 9); // newSize
    bus.write_long(TEST_SP + 4, handle);
    cpu.write_reg(Register::D0, 0x0003);
    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap(); // SizeResource
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 9);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn setresourcesize_selector_3_resizes_emptied_resource_handle_in_place() {
    // IM:VI 13-23: SetResourceSize changes the size of a resource
    // without writing data. An emptied resource handle should still
    // stay tied to the same logical resource after resizing.
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"RSZ0", 93, &[0xAA, 0xBB, 0xCC]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"RSZ0", 93, data_ptr);

    bus.write_long(handle, 0);

    bus.write_long(TEST_SP, 7);
    bus.write_long(TEST_SP + 4, handle);
    cpu.write_reg(Register::D0, 0x0003);
    call(&mut disp, true, 0x022, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        bus.read_long(handle),
        0,
        "resized empty handle should be repopulated"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 93);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"RSZ0"));
    bus.write_long(TEST_SP + 6, 0xDEADBEEF);
    call(&mut disp, true, 0x1A0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(
        bus.read_long(TEST_SP + 6),
        handle,
        "relookup should resolve to the same handle after resize"
    );
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1A5, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 7);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// ================================================================
// 6. ResError (0x1AF)
// ================================================================
#[test]
fn res_error() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Set ResErr to a known value
    bus.write_word(0x0A60, (-108i16) as u16);

    call(&mut disp, true, 0x1AF, &mut cpu, &mut bus).unwrap();

    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP, "SP should be unchanged");
    let result = bus.read_word(sp) as i16;
    assert_eq!(result, -108);
}

fn addreference_setup() -> (
    super::super::TrapDispatcher,
    MockCpu,
    crate::memory::MacMemoryBus,
    u32,
    u32,
) {
    let (mut disp, cpu, mut bus) = setup();
    let source_ptr = bus.alloc(8);
    bus.write_bytes(
        source_ptr,
        &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
    );

    let mut file0_loaded = HashMap::new();
    file0_loaded.insert((*b"CURS", 1), source_ptr);
    let mut file0 = ResourceFileMap {
        loaded: file0_loaded,
        named: HashMap::new(),
        names_by_id: HashMap::new(),
        attrs: HashMap::from([((*b"CURS", 1), 0x0004u8)]),
        map_attrs: 0,
    };
    file0
        .named
        .insert((*b"CURS", "SystemCursor".to_string()), (1, source_ptr));

    let file1 = ResourceFileMap::default();
    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(0, file0), (1, file1)]),
        names: HashMap::new(),
        search_order: vec![0, 1],
        current_file: 1,
    });

    let source_handle =
        disp.get_or_create_resource_handle_in_file(&mut bus, *b"CURS", 1, source_ptr, 0);

    (disp, cpu, bus, source_handle, source_ptr)
}

#[test]
fn addreference_adds_system_reference_and_marks_current_file_changed() {
    let (mut disp, mut cpu, mut bus, source_handle, source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x200000u32;
    write_pstring(&mut bus, name_ptr, b"AddedReference");
    let ref_id = 200i16;
    let current_before = disp.current_resource_refnum();

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);

    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(disp.current_resource_refnum(), current_before);
    assert_eq!(
        disp.resource_handle_files.get(&source_handle).copied(),
        Some(0)
    );

    let file = disp.resources.as_ref().unwrap().files.get(&1).unwrap();
    assert_eq!(
        disp.loaded_handles.get(&source_handle).copied(),
        Some((source_ptr, *b"CURS", 1))
    );
    assert_eq!(file.loaded.get(&(*b"CURS", ref_id)), Some(&source_ptr));
    assert_eq!(
        file.named.get(&(*b"CURS", "AddedReference".to_string())),
        Some(&(ref_id, source_ptr))
    );
    assert_eq!(
        file.attrs.get(&(*b"CURS", ref_id)).copied().unwrap_or(0)
            & (super::super::TrapDispatcher::RES_CHANGED_ATTR as u8
                | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8),
        (super::super::TrapDispatcher::RES_CHANGED_ATTR as u8
            | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8)
    );
    assert_ne!(
        file.map_attrs & super::super::TrapDispatcher::RES_MAP_CHANGED_ATTR,
        0
    );
}

#[test]
fn addreference_persists_current_map_on_close_without_write_refnum_marker() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    disp.vfs_rsrc.insert(
        "Prefs".to_string(),
        super::super::TrapDispatcher::empty_resource_fork_bytes(),
    );
    let prefs_refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", false);
    assert!(!disp.write_refnums.contains(&prefs_refnum));

    let sp = TEST_SP;
    let name_ptr = 0x200020u32;
    write_pstring(&mut bus, name_ptr, b"PrefsCursor");
    let ref_id = 210i16;

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, prefs_refnum);
    let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert!(!disp
        .resources
        .as_ref()
        .unwrap()
        .files
        .contains_key(&prefs_refnum));

    let fork = crate::managers::resource::ResourceFork::parse(
        disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
    )
    .expect("serialized resource fork should parse after CloseResFile");
    let resource = fork
        .resources()
        .get(&(*b"CURS", ref_id))
        .expect("AddReference entry should survive CloseResFile");
    assert_eq!(
        resource.data,
        vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
    );
    assert_eq!(resource.name.as_deref(), Some("PrefsCursor"));
    assert_eq!(
        resource.attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u8,
        0
    );
    assert_ne!(
        resource.attrs & super::super::TrapDispatcher::RES_SYS_REF_ATTR as u8,
        0
    );
}

// Inside Macintosh Volume I (1985), p. I-124: RmvResource removes the
// current-file reference, leaves the handle's data allocated, and keeps
// the current resource file unchanged.
#[test]
fn rmvereference_removes_current_file_reference_without_disposing_handle_data() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = setup_resources(
        &mut disp,
        &mut bus,
        b"STR ",
        30000,
        &[0x10, 0x20, 0x30, 0x40],
    );
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 30000, data_ptr);
    let current_before = disp.current_resource_refnum();

    bus.write_long(TEST_SP, handle);
    call_trap_word(&mut disp, 0xA9AE, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(disp.current_resource_refnum(), current_before);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert!(!disp.loaded_handles.contains_key(&handle));
    assert!(!disp.resource_handle_files.contains_key(&handle));
    assert!(!disp
        .resources
        .as_ref()
        .unwrap()
        .files
        .get(&0)
        .unwrap()
        .loaded
        .contains_key(&(*b"STR ", 30000)));
}

#[test]
fn addreference_shadows_system_handle_metadata_in_current_file() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let add_name_ptr = 0x200020u32;
    let info_name_ptr = 0x200040u32;
    let info_type_ptr = 0x200060u32;
    let info_id_ptr = 0x200080u32;
    write_pstring(&mut bus, add_name_ptr, b"AddedReference");
    let ref_id = 206i16;

    bus.write_long(sp, add_name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(0x0A60), 0);

    bus.write_long(sp, info_name_ptr);
    bus.write_long(sp + 4, info_type_ptr);
    bus.write_long(sp + 8, info_id_ptr);
    bus.write_long(sp + 12, source_handle);
    cpu.write_reg(Register::A7, sp);
    call_trap_word(&mut disp, 0xA9A8, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_handle_files.get(&source_handle).copied(),
        Some(0)
    );
    assert_eq!(bus.read_word(info_id_ptr), ref_id as u16);
    assert_eq!(bus.read_long(info_type_ptr), u32::from_be_bytes(*b"CURS"));
    assert_eq!(bus.read_pstring(info_name_ptr), b"AddedReference".to_vec());

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, source_handle);
    bus.write_word(sp + 4, 0xBEEF);
    call_trap_word(&mut disp, 0xA9A6, &mut cpu, &mut bus).unwrap();

    let attrs = bus.read_word(sp + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        attrs
            & (super::super::TrapDispatcher::RES_CHANGED_ATTR as u16
                | super::super::TrapDispatcher::RES_SYS_REF_ATTR as u16),
        0
    );
    assert_ne!(
        attrs & super::super::TrapDispatcher::RES_SYS_REF_ATTR as u16,
        0
    );
    assert_ne!(
        attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u16,
        0
    );
}

#[test]
fn addreference_preserves_a5_a6_and_pops_success_frame() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x200180u32;
    write_pstring(&mut bus, name_ptr, b"Regs");
    let ref_id = 204i16;

    cpu.write_reg(Register::A5, 0x1A5A_0000);
    cpu.write_reg(Register::A6, 0x1A6A_0000);

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    assert_eq!(cpu.read_reg(Register::A5), 0x1A5A_0000);
    assert_eq!(cpu.read_reg(Register::A6), 0x1A6A_0000);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn addreference_preserves_all_nonvolatile_registers_on_success() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x2001C0u32;
    write_pstring(&mut bus, name_ptr, b"AllRegs");
    let ref_id = 205i16;

    let sentinels = [
        (Register::D2, 0xD202_0202),
        (Register::D3, 0xD303_0303),
        (Register::D4, 0xD404_0404),
        (Register::D5, 0xD505_0505),
        (Register::D6, 0xD606_0606),
        (Register::D7, 0xD707_0707),
        (Register::A2, 0xA202_0202),
        (Register::A3, 0xA303_0303),
        (Register::A4, 0xA404_0404),
        (Register::A5, 0xA505_0505),
        (Register::A6, 0xA606_0606),
    ];

    for &(reg, value) in &sentinels {
        cpu.write_reg(reg, value);
    }

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    for &(reg, value) in &sentinels {
        assert_eq!(cpu.read_reg(reg), value, "{reg:?} should be preserved");
    }
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn addreference_duplicate_reference_sets_addreffailed() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x200200u32;
    write_pstring(&mut bus, name_ptr, b"DuplicateRef");
    let ref_id = 201i16;

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(0x0A60), 0);

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, ref_id as u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(0x0A60) as i16, -195);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&1)
            .unwrap()
            .loaded
            .len(),
        1
    );
}

#[test]
fn addreference_non_system_handle_sets_addreffailed() {
    let (mut disp, mut cpu, mut bus, _source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x200300u32;
    write_pstring(&mut bus, name_ptr, b"RawHandle");

    let raw_ptr = bus.alloc(4);
    bus.write_long(raw_ptr, 0xCAFEBABE);
    let raw_handle = bus.alloc(4);
    bus.write_long(raw_handle, raw_ptr);

    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, 202u16);
    bus.write_long(sp + 6, raw_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    assert_eq!(bus.read_word(0x0A60) as i16, -195);
    assert!(!disp
        .resources
        .as_ref()
        .unwrap()
        .files
        .get(&1)
        .unwrap()
        .loaded
        .contains_key(&(*b"CURS", 202)));
}

#[test]
fn addreference_current_system_file_sets_addreffailed() {
    let (mut disp, mut cpu, mut bus, source_handle, _source_ptr) = addreference_setup();
    let sp = TEST_SP;
    let name_ptr = 0x200400u32;
    write_pstring(&mut bus, name_ptr, b"SysFileRef");

    bus.write_word(sp, 0);
    let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(disp.current_resource_refnum(), 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, name_ptr);
    bus.write_word(sp + 4, 203u16);
    bus.write_long(sp + 6, source_handle);
    call_trap_word(&mut disp, 0xA9AC, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    assert_eq!(bus.read_word(0x0A60) as i16, -195);
    assert_eq!(disp.current_resource_refnum(), 0);
}

// ================================================================
// 7. Get1NamedResource (0x020) — found
// ================================================================
#[test]
fn get1_named_resource_found() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = bus.alloc(16);
    bus.write_bytes(data_ptr, &[0x42; 16]);

    let mut loaded = HashMap::new();
    loaded.insert((*b"STR ", 500i16), data_ptr);
    let mut named = HashMap::new();
    named.insert((*b"STR ", "MyString".to_string()), (500i16, data_ptr));
    let mut names_by_id = HashMap::new();
    names_by_id.insert((*b"STR ", 500i16), "MyString".to_string());
    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(
            0,
            ResourceFileMap {
                loaded,
                named,
                names_by_id,
                attrs: HashMap::new(),
                map_attrs: 0,
            },
        )]),
        names: HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });

    // Write Pascal string name at 0x200000
    let name_addr = 0x200000u32;
    write_pstring(&mut bus, name_addr, b"MyString");

    let sp = TEST_SP;
    bus.write_long(sp, name_addr); // name_ptr
    bus.write_long(sp + 4, u32::from_be_bytes(*b"STR ")); // type

    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 8);
    let handle = bus.read_long(new_sp);
    assert_ne!(
        handle, 0,
        "handle should be non-zero for found named resource"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        handle,
        "found named resource handle should be returned in A0"
    );
}

#[test]
fn get1_named_resource_matches_name_without_case() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"PICT", 10002, b"picture");
    disp.insert_named_resource_for_test(0, (*b"PICT", "HIDDEN".to_string()), (10002, data_ptr));
    let name_ptr = 0x200000u32;
    write_pstring(&mut bus, name_ptr, b"hidden");
    bus.write_long(TEST_SP, name_ptr);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"PICT"));
    bus.write_word(0x0A60, (-192i16) as u16);

    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 8);
    assert_ne!(handle, 0);
    assert_eq!(bus.read_long(handle), data_ptr);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn get1_named_resource_miss_returns_nil_in_a0() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"STR ", 500, b"present type");
    let name_addr = 0x200000u32;
    write_pstring(&mut bus, name_addr, b"Missing");

    bus.write_long(TEST_SP, name_addr);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"STR "));

    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(TEST_SP + 8), 0);
    assert_eq!(bus.read_word(0x0A60) as i16, super::RES_NOT_FOUND_ERR);
}

#[test]
fn get1_named_resource_absent_type_returns_nil_without_error() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"VPIC", 500, b"other type");
    let name_addr = 0x200000u32;
    write_pstring(&mut bus, name_addr, b"hidden");
    bus.write_long(TEST_SP, name_addr);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"PICT"));
    bus.write_word(0x0A60, super::RES_NOT_FOUND_ERR as u16);

    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(bus.read_long(TEST_SP + 8), 0);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn get1_named_resource_reloads_after_release() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data = [0x42, 0x43, 0x44, 0x45];
    let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 500, &data);
    disp.insert_named_resource_for_test(0, (*b"TEST", "ReloadMe".to_string()), (500, data_ptr));
    disp.insert_resource_name_for_test(0, (*b"TEST", 500), "ReloadMe".to_string());

    let name_ptr = 0x300000u32;
    write_pstring(&mut bus, name_ptr, b"ReloadMe");
    bus.write_long(TEST_SP, name_ptr);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"TEST"));
    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();
    let first_handle = bus.read_long(TEST_SP + 8);
    assert_eq!(bus.read_long(first_handle), data_ptr);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, first_handle);
    call(&mut disp, true, 0x1A3, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_long(first_handle), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, name_ptr);
    bus.write_long(TEST_SP + 4, u32::from_be_bytes(*b"TEST"));
    call(&mut disp, true, 0x020, &mut cpu, &mut bus).unwrap();
    let second_handle = bus.read_long(TEST_SP + 8);
    let second_ptr = bus.read_long(second_handle);
    assert_ne!(second_handle, first_handle);
    assert_ne!(second_ptr, 0);
    assert_eq!(bus.read_bytes(second_ptr, data.len()), data);
}

// ================================================================
// 7b. GetResInfo (0x1A8) — named resource metadata
// ================================================================
#[test]
fn get_res_info_named_resource_returns_id_type_and_name() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = setup_resources(&mut disp, &mut bus, b"STR ", 500, &[0x42; 16]);
    disp.insert_named_resource_for_test(0, (*b"STR ", "MyString".to_string()), (500, data_ptr));
    disp.insert_resource_name_for_test(0, (*b"STR ", 500), "MyString".to_string());

    let handle = disp.get_or_create_resource_handle(&mut bus, *b"STR ", 500, data_ptr);
    let name_ptr = 0x200000u32;
    let type_ptr = 0x200100u32;
    let id_ptr = 0x200104u32;

    let sp = TEST_SP;
    bus.write_long(sp, name_ptr);
    bus.write_long(sp + 4, type_ptr);
    bus.write_long(sp + 8, id_ptr);
    bus.write_long(sp + 12, handle);

    call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(id_ptr) as i16, 500);
    assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"STR "));
    assert_eq!(bus.read_byte(name_ptr), 8);
    assert_eq!(bus.read_bytes(name_ptr + 1, 8), b"MyString");
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn get_res_info_preserves_duplicate_resource_names_by_id() {
    let (mut disp, mut cpu, mut bus) = setup();

    let first_ptr = setup_resources(&mut disp, &mut bus, b"m\x95sn", 128, &[0x11; 4]);
    let second_ptr = disp.install_named_test_resource_in_file(
        &mut bus,
        0,
        *b"m\x95sn",
        129,
        "Ferry Passengers to <DST>",
        &[0x22; 4],
    );
    disp.insert_named_resource_for_test(
        0,
        (*b"m\x95sn", "Ferry Passengers to <DST>".to_string()),
        (128, first_ptr),
    );
    disp.insert_resource_name_for_test(
        0,
        (*b"m\x95sn", 128),
        "Ferry Passengers to <DST>".to_string(),
    );

    let first_handle = disp.get_or_create_resource_handle(&mut bus, *b"m\x95sn", 128, first_ptr);
    let second_handle = disp.get_or_create_resource_handle(&mut bus, *b"m\x95sn", 129, second_ptr);
    let name_ptr = 0x200000u32;
    let type_ptr = 0x200100u32;
    let id_ptr = 0x200104u32;
    let sp = TEST_SP;

    for (handle, expected_id) in [(first_handle, 128i16), (second_handle, 129i16)] {
        bus.write_byte(name_ptr, 0);
        bus.write_long(sp, name_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, id_ptr);
        bus.write_long(sp + 12, handle);

        call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(bus.read_word(id_ptr) as i16, expected_id);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"m\x95sn"));
        let name_len = bus.read_byte(name_ptr) as usize;
        assert_eq!(
            bus.read_bytes(name_ptr + 1, name_len),
            b"Ferry Passengers to <DST>"
        );
        assert_eq!(bus.read_word(0x0A60), 0);
        cpu.write_reg(Register::A7, TEST_SP);
    }
}

// ================================================================
// 7c. GetResInfo (0x1A8) — unnamed resource clears stale Str255
// ================================================================
#[test]
fn get_res_info_unnamed_resource_clears_name_buffer() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0x10, 0x20, 0x30]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"TEST", 7, data_ptr);
    let name_ptr = 0x200200u32;
    let type_ptr = 0x200300u32;
    let id_ptr = 0x200304u32;

    bus.write_bytes(name_ptr, b"urer\0");

    let sp = TEST_SP;
    bus.write_long(sp, name_ptr);
    bus.write_long(sp + 4, type_ptr);
    bus.write_long(sp + 8, id_ptr);
    bus.write_long(sp + 12, handle);

    call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(id_ptr) as i16, 7);
    assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"TEST"));
    assert_eq!(bus.read_byte(name_ptr), 0);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// More Macintosh Toolbox (1993), pp. 1-81 to 1-82: if the handle is not
// a resource handle, GetResInfo does nothing and ResError is resNotFound.
#[test]
fn get_res_info_non_resource_handle_sets_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();

    let fake_ptr = bus.alloc(4);
    bus.write_long(fake_ptr, 0xDEADBEEF);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    let name_ptr = 0x200400u32;
    let type_ptr = 0x200500u32;
    let id_ptr = 0x200504u32;
    bus.write_pstring(name_ptr, b"keepme");
    bus.write_long(type_ptr, 0x11223344);
    bus.write_word(id_ptr, 0x5566);

    bus.write_long(TEST_SP, name_ptr);
    bus.write_long(TEST_SP + 4, type_ptr);
    bus.write_long(TEST_SP + 8, id_ptr);
    bus.write_long(TEST_SP + 12, fake_handle);

    call(&mut disp, true, 0x1A8, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    assert_eq!(bus.read_word(0x0A60), (-192i16) as u16);
    assert_eq!(bus.read_byte(name_ptr), 6);
    assert_eq!(bus.read_long(type_ptr), 0x11223344);
    assert_eq!(bus.read_word(id_ptr), 0x5566);
}

// ================================================================
// 7d. SetResInfo (0x1A9)
// ================================================================
// Inside Macintosh Volume I (1985), p. I-122: SetResInfo takes
// (Handle, Integer, Str255 ptr) and pops 10 bytes.
#[test]
fn setresinfo_consumes_handle_id_and_name_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"SINF", 7, &[0x10, 0x20]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"SINF", 7, data_ptr);

    let name_ptr = 0x200600u32;
    write_pstring(&mut bus, name_ptr, b"Renamed");

    bus.write_long(TEST_SP, name_ptr);
    bus.write_word(TEST_SP + 4, 11u16);
    bus.write_long(TEST_SP + 6, handle);
    bus.write_word(TEST_SP + 10, 0xBEEF);

    call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(TEST_SP + 10), 0xBEEF);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// Inside Macintosh Volume I (1985), p. I-122: SetResInfo updates
// resource map metadata (ID and name) for the specified resource.
#[test]
fn setresinfo_updates_resource_id_and_name_for_unprotected_resource() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"NAME", 7, &[0x31, 0x32, 0x33]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"NAME", 7, data_ptr);
    disp.insert_named_resource_for_test(0, (*b"NAME", "OldName".to_string()), (7, data_ptr));

    let name_ptr = 0x200700u32;
    write_pstring(&mut bus, name_ptr, b"NewName");
    bus.write_long(TEST_SP, name_ptr);
    bus.write_word(TEST_SP + 4, 11u16);
    bus.write_long(TEST_SP + 6, handle);

    call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

    let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|entry| entry.2),
        Some(11i16)
    );
    assert!(file.loaded.contains_key(&(*b"NAME", 11)));
    assert!(!file.loaded.contains_key(&(*b"NAME", 7)));
    assert_eq!(
        file.named.get(&(*b"NAME", "NewName".to_string())),
        Some(&(11, data_ptr))
    );
    assert!(!file.named.contains_key(&(*b"NAME", "OldName".to_string())));
}

// Inside Macintosh Volume I (1985), p. I-122: resource metadata APIs
// report missing-handle failures via ResError.
#[test]
fn setresinfo_invalid_handle_sets_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_ptr = bus.alloc(4);
    bus.write_long(fake_ptr, 0xCAFEBABE);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    let name_ptr = 0x200800u32;
    write_pstring(&mut bus, name_ptr, b"NeverUsed");
    bus.write_long(TEST_SP, name_ptr);
    bus.write_word(TEST_SP + 4, 19u16);
    bus.write_long(TEST_SP + 6, fake_handle);

    call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// Inside Macintosh Volume IV (1986), p. IV-16: resProtected blocks
// metadata mutation for SetResInfo and returns resAttrErr.
#[test]
fn setresinfo_protected_resource_is_noop_with_resattrerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 3, &[0x01, 0x02]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 3, data_ptr);
    disp.insert_named_resource_for_test(0, (*b"PROT", "KeepName".to_string()), (3, data_ptr));
    disp.insert_resource_attrs_for_test(0, (*b"PROT", 3), 0x0008u8);

    let name_ptr = 0x200900u32;
    write_pstring(&mut bus, name_ptr, b"NewName");
    bus.write_long(TEST_SP, name_ptr);
    bus.write_word(TEST_SP + 4, 22u16);
    bus.write_long(TEST_SP + 6, handle);

    call(&mut disp, true, 0x1A9, &mut cpu, &mut bus).unwrap();

    let file = disp.resources.as_ref().unwrap().files.get(&0).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        super::super::TrapDispatcher::RES_ATTR_ERR
    );
    assert_eq!(
        disp.loaded_handles.get(&handle).map(|entry| entry.2),
        Some(3i16)
    );
    assert!(file.loaded.contains_key(&(*b"PROT", 3)));
    assert!(!file.loaded.contains_key(&(*b"PROT", 22)));
    assert!(file.named.contains_key(&(*b"PROT", "KeepName".to_string())));
    assert!(!file.named.contains_key(&(*b"PROT", "NewName".to_string())));
}

// ================================================================
// 7e. GetResFileAttrs (0x1F6) / SetResFileAttrs (0x1F7)
// ================================================================
// Inside Macintosh Volume I (1985), p. I-126: GetResFileAttrs returns
// the file map attribute word for a valid resource-file refNum.
#[test]
fn getresfileattrs_returns_only_documented_map_attr_bits_for_open_resource_file() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);
    disp.set_resource_map_attrs_for_test(0, 0x0234);

    bus.write_word(TEST_SP, 0u16);
    bus.write_word(TEST_SP + 2, 0xBEEF);

    call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(TEST_SP + 2), 0x0020);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// Inside Macintosh Volume I (1985), p. I-126: if refNum is unknown,
// GetResFileAttrs does nothing and returns resFNotFound in ResError.
#[test]
fn getresfileattrs_missing_refnum_sets_resfnotfound_and_preserves_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);

    bus.write_word(TEST_SP, 99u16);
    bus.write_word(TEST_SP + 2, 0xBEEF);

    call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(TEST_SP + 2), 0xBEEF);
    assert_eq!(bus.read_word(0x0A60) as i16, -193);
}

// Inside Macintosh Volume I (1985), p. I-126: GetResFileAttrs is a
// Pascal FUNCTION that consumes one INTEGER refNum and writes one
// INTEGER result slot.
#[test]
fn getresfileattrs_consumes_refnum_and_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"RFAT", 1, &[0xAA]);
    disp.set_resource_map_attrs_for_test(0, 0x0234);

    bus.write_word(TEST_SP, 0u16);
    bus.write_word(TEST_SP + 2, 0xBEEF);

    let sp_pre = cpu.read_reg(Register::A7);
    call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp_pre + 2);
    assert_eq!(bus.read_word(TEST_SP + 2), 0x0020);
}

// Inside Macintosh Volume I (1985), p. I-126: SetResFileAttrs takes
// refNum + attrs and mutates the target file map attributes.
#[test]
fn setresfileattrs_consumes_refnum_and_attrs_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"SRAF", 2, &[0xBB]);

    bus.write_word(TEST_SP, 0x01A5u16);
    bus.write_word(TEST_SP + 2, 0u16);
    bus.write_word(TEST_SP + 4, 0xBEEF);

    call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(TEST_SP + 4), 0xBEEF);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .map_attrs,
        0x00A0
    );
}

// Inside Macintosh Volume I (1985), p. I-126 lists only the
// mapReadOnly/mapCompact/mapChanged bits as public resource-file
// attributes; undefined bits are ignored by SetResFileAttrs and not
// surfaced by GetResFileAttrs.
#[test]
fn setresfileattrs_getresfileattrs_roundtrip_only_documented_bits() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"MASK", 3, &[0xCC]);

    bus.write_word(TEST_SP, 0xFFFFu16);
    bus.write_word(TEST_SP + 2, 0u16);
    call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0u16);
    bus.write_word(TEST_SP + 2, 0xBEEFu16);
    call(&mut disp, true, 0x1F6, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .map_attrs,
        0x00E0
    );
    assert_eq!(bus.read_word(TEST_SP + 2), 0x00E0);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// Inside Macintosh Volume I (1985), p. I-126: for unknown refNum,
// SetResFileAttrs does nothing and still returns noErr.
#[test]
fn setresfileattrs_missing_refnum_is_noop_with_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    setup_resources(&mut disp, &mut bus, b"SRAF", 2, &[0xBB]);
    disp.set_resource_map_attrs_for_test(0, 0x0033);

    bus.write_word(TEST_SP, 0x0F0Fu16);
    bus.write_word(TEST_SP + 2, 77u16);

    call(&mut disp, true, 0x1F7, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resources
            .as_ref()
            .unwrap()
            .files
            .get(&0)
            .unwrap()
            .map_attrs,
        0x0033
    );
}

fn add_resource_for_writeback_test(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    res_type: [u8; 4],
    res_id: i16,
    data: &[u8],
    name: &[u8],
) -> u32 {
    let data_ptr = bus.alloc(data.len() as u32);
    bus.write_bytes(data_ptr, data);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);
    let name_ptr = 0x250000u32;
    write_pstring(bus, name_ptr, name);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, name_ptr);
    bus.write_word(TEST_SP + 4, res_id as u16);
    bus.write_long(TEST_SP + 6, u32::from_be_bytes(res_type));
    bus.write_long(TEST_SP + 10, handle);
    call(disp, true, 0x1AB, cpu, bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );
    handle
}

// Inside Macintosh Volume I (1985), p. I-125: WriteResource clears
// resChanged on successful writes for changed resources.
#[test]
fn writeresource_changed_resource_clears_reschanged_attribute() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"WRIT", 77, &[0x10, 0x20, 0x30]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"WRIT", 77, data_ptr);

    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );
}

#[test]
fn writeresource_persists_added_resource_to_writable_vfs_resource_fork() {
    // IM:I I-124..I-125: WriteResource writes changed resource data to
    // the resource file and clears resChanged. Systemless must mirror the
    // write into vfs_rsrc so a later OpenRFPerm/Get1Resource sees it.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs_rsrc.insert(
        "Prefs".to_string(),
        super::super::TrapDispatcher::empty_resource_fork_bytes(),
    );
    let _refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
    let handle = add_resource_for_writeback_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        *b"CAsp",
        20000,
        &[0x10, 0x20, 0x30, 0x40],
        b"Config",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );

    let fork = crate::managers::resource::ResourceFork::parse(
        disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
    )
    .expect("serialized resource fork should parse");
    let resource = fork
        .resources()
        .get(&(*b"CAsp", 20000))
        .expect("added resource should be serialized");
    assert_eq!(resource.data, vec![0x10, 0x20, 0x30, 0x40]);
    assert_eq!(resource.name.as_deref(), Some("Config"));
    assert_eq!(
        resource.attrs & super::super::TrapDispatcher::RES_CHANGED_ATTR as u8,
        0
    );
}

#[test]
fn closeresfile_persists_added_resource_before_closing_writable_map() {
    // CloseResFile performs UpdateResFile before removing the map from
    // the open-resource chain; the VFS mirror must be updated before the
    // in-memory file map is freed.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs_rsrc.insert(
        "Prefs".to_string(),
        super::super::TrapDispatcher::empty_resource_fork_bytes(),
    );
    let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
    let _handle = add_resource_for_writeback_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        *b"KMpt",
        20000,
        &[0x55, 0x66, 0x77],
        b"Keys",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, refnum);
    let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert!(!disp.resources.as_ref().unwrap().files.contains_key(&refnum));

    let fork = crate::managers::resource::ResourceFork::parse(
        disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
    )
    .expect("serialized resource fork should parse after CloseResFile");
    let resource = fork
        .resources()
        .get(&(*b"KMpt", 20000))
        .expect("added resource should survive CloseResFile");
    assert_eq!(resource.data, vec![0x55, 0x66, 0x77]);
    assert_eq!(resource.name.as_deref(), Some("Keys"));
}

#[test]
fn updateresfile_persists_added_resource_to_writable_vfs_resource_fork() {
    // UpdateResFile writes changed resources and the resource map for the
    // specified refNum. This is the normal batching path used by apps that
    // add several preference resources before closing the file.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs_rsrc.insert(
        "Prefs".to_string(),
        super::super::TrapDispatcher::empty_resource_fork_bytes(),
    );
    let refnum = disp.open_resource_file_from_vfs_key(&mut bus, "Prefs", true);
    let handle = add_resource_for_writeback_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        *b"MDta",
        128,
        &[0xA1, 0xB2, 0xC3, 0xD4],
        b"Meta",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, refnum);
    let result = disp.dispatch_toolbox(true, 0x199, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & super::super::TrapDispatcher::RES_CHANGED_ATTR,
        0
    );

    let fork = crate::managers::resource::ResourceFork::parse(
        disp.vfs_rsrc.get("Prefs").expect("Prefs resource fork"),
    )
    .expect("serialized resource fork should parse after UpdateResFile");
    let resource = fork
        .resources()
        .get(&(*b"MDta", 128))
        .expect("added resource should survive UpdateResFile");
    assert_eq!(resource.data, vec![0xA1, 0xB2, 0xC3, 0xD4]);
    assert_eq!(resource.name.as_deref(), Some("Meta"));
}

// Inside Macintosh Volume I (1985), p. I-125: WriteResource is a no-op
// with noErr for protected resources and for resources not marked changed.
#[test]
fn writeresource_protected_or_unchanged_resource_is_noop_with_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let changed = super::super::TrapDispatcher::RES_CHANGED_ATTR;

    let prot_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 1, &[0xAA; 4]);
    let prot_handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 1, prot_ptr);
    bus.write_long(TEST_SP, prot_handle);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap(); // ChangedResource
    disp.with_resource_file_mut_for_test(0, |file| {
        let attrs = file.attrs.entry((*b"PROT", 1)).or_insert(0);
        *attrs |= 0x0008u8; // resProtected
    })
    .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, prot_handle);
    call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        disp.resource_attributes_for_handle(prot_handle)
            .unwrap_or(0)
            & changed,
        0
    );

    let plain_ptr = setup_resources(&mut disp, &mut bus, b"PLAI", 2, &[0xBB; 4]);
    let plain_handle = disp.get_or_create_resource_handle(&mut bus, *b"PLAI", 2, plain_ptr);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, plain_handle);
    call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(
        disp.resource_attributes_for_handle(plain_handle)
            .unwrap_or(0)
            & changed,
        0
    );
}

// Inside Macintosh Volume I (1985), p. I-125: non-resource handles cause
// WriteResource to do nothing and return resNotFound.
#[test]
fn writeresource_non_resource_handle_returns_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();
    let fake_ptr = bus.alloc(8);
    bus.write_bytes(fake_ptr, &[0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4]);
    let fake_handle = bus.alloc(4);
    bus.write_long(fake_handle, fake_ptr);

    bus.write_long(TEST_SP, fake_handle);
    call(&mut disp, true, 0x1B0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
    assert_eq!(bus.read_long(fake_handle), fake_ptr);
}

// ================================================================
// 8. CurResFile (0x194)
// ================================================================
#[test]
fn cur_res_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    call(&mut disp, true, 0x194, &mut cpu, &mut bus).unwrap();

    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP, "SP should be unchanged");
    let refnum = bus.read_word(sp);
    assert_eq!(refnum, 0);
}

#[test]
fn cur_res_file_translates_the_internal_application_map_key_to_its_fcb_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(addr::CUR_APREF_NUM, 2);

    call(&mut disp, true, 0x194, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(TEST_SP), 2);
}

// ================================================================
// 9. HomeResFile (0x1A4)
// ================================================================
#[test]
fn home_res_file_returns_loaded_resource_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = 0x1234u32;
    disp.insert_loaded_resource_handle_for_test(handle, (0x200000, *b"STR ", 1));
    disp.insert_resource_handle_file_for_test(handle, 128);

    let sp = TEST_SP;
    bus.write_long(sp, handle);

    call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    let refnum = bus.read_word(new_sp);
    assert_eq!(refnum, 128);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn home_res_file_translates_the_internal_application_map_key_to_its_fcb_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = 0x1234u32;
    disp.insert_loaded_resource_handle_for_test(handle, (0x200000, *b"CODE", 1));
    disp.insert_resource_handle_file_for_test(handle, 0);
    bus.write_word(addr::CUR_APREF_NUM, 2);
    bus.write_long(TEST_SP, handle);

    call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(TEST_SP + 4), 2);
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn home_res_file_returns_minus_one_for_detached_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = 0x5678u32;
    disp.insert_detached_resource_handle_for_test(handle, (*b"STR ", 2));
    disp.insert_detached_resource_handle_file_for_test(handle, 202);

    let sp = TEST_SP;
    bus.write_long(sp, handle);

    call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    let refnum = bus.read_word(new_sp) as i16;
    assert_eq!(refnum, -1);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

#[test]
fn home_res_file_returns_minus_one_for_unknown_handle() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP;
    bus.write_long(sp, 0x9ABCu32);

    call(&mut disp, true, 0x1A4, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    let refnum = bus.read_word(new_sp) as i16;
    assert_eq!(refnum, -1);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// ================================================================
// 10. LoadSeg (0x1F0)
// ================================================================
#[test]
fn loadseg_patches_calling_jump_table_entry_to_jmp_loaded_code() {
    // Inside Macintosh Volume II (1985), p. II-61: LoadSeg rewrites an
    // unloaded jump-table entry to the loaded JMP form.
    //
    // Use the documented MPW entry layout:
    //   [offset, MOVE.W #segNum,-(SP), segNum, _LoadSeg]
    // so the trap can consume segNum from A7 and patch the entry.
    let (mut disp, mut cpu, mut bus) = setup();

    // Set up segment 1 at address 0x210000
    let seg_addr = 0x210000u32;
    // Write a non-0xFFFF header word so header_size = 4
    // Segment header: taboff=0, nentries=0 (no JT entries to patch)
    bus.write_word(seg_addr, 0x0000); // taboff
    bus.write_word(seg_addr + 2, 0x0000); // nentries
    let mut seg_map = HashMap::new();
    seg_map.insert(1i16, seg_addr);
    disp.register_segments(seg_map);

    // Set up a standard MPW jump table entry at 0x200000:
    //   +0: 0000  routine offset within segment
    //   +2: 3F3C  MOVE.W #imm, -(SP)
    //   +4: 0001  segment number
    //   +6: A9F0  _LoadSeg trap
    // After trap executes, the entry is patched to:
    //   +0: seg#
    //   +2: 4EF9  JMP.L
    //   +4: code_addr
    // Inside Macintosh Volume II, II-61
    let island_addr = 0x200000u32;
    bus.write_word(island_addr, 0x0000); // routine offset
    bus.write_word(island_addr + 2, 0x3F3C); // MOVE.W
    bus.write_word(island_addr + 4, 0x0001); // seg#
    bus.write_word(island_addr + 6, 0xA9F0); // _LoadSeg

    // PC points past the A9F0 instruction (as if CPU just executed it)
    cpu.write_reg(Register::PC, island_addr + 8);

    // Standard format: segment number is pushed on stack by MOVE.W
    let sp = TEST_SP;
    bus.write_word(sp, 1u16); // segment number

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    // Standard MPW format pops the segment number pushed by MOVE.W.
    let sp_after = cpu.read_reg(Register::A7);
    assert_eq!(sp_after, TEST_SP + 2);

    // Handler patches the island and sets PC to island+2 (the JMP.L instruction).
    // The JMP.L target is seg_addr + header_size(4) + routine_offset(0) = seg_addr + 4.
    let pc = cpu.read_reg(Register::PC);
    assert_eq!(pc, island_addr + 2, "PC should point to patched JMP.L");

    // Verify the island was patched correctly
    assert_eq!(
        bus.read_word(island_addr),
        1,
        "island[0] should be seg number"
    );
    assert_eq!(
        bus.read_word(island_addr + 2),
        0x4EF9,
        "island[2] should be JMP.L"
    );
    assert_eq!(
        bus.read_long(island_addr + 4),
        seg_addr + 4,
        "JMP target should be code entry"
    );
}

#[test]
fn loadseg_mpw_call_consumes_segment_number_word_argument() {
    // Inside Macintosh Volume II (1985), p. II-60:
    // MPW jump-table callers push segNum with MOVE.W #segNum, -(SP)
    // before executing _LoadSeg.
    let (mut disp, mut cpu, mut bus) = setup();
    let seg_addr = 0x220000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0000);
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));

    let island_addr = 0x210000u32;
    bus.write_word(island_addr, 0x0000);
    bus.write_word(island_addr + 2, 0x3F3C);
    bus.write_word(island_addr + 4, 0x0001);
    bus.write_word(island_addr + 6, 0xA9F0);

    cpu.write_reg(Register::PC, island_addr + 8);
    bus.write_word(TEST_SP, 1u16);
    bus.write_word(TEST_SP + 2, 0xBEEF);

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(TEST_SP + 2), 0xBEEF);
}

#[test]
fn loadseg_saved_gateway_reenters_wrapper_dispatch_after_patching_segment() {
    // Inside Macintosh Volume II (1985), II-60: LoadSeg consumes a
    // segment-number word. A saved auto-pop gateway may be invoked by a
    // wrapper rather than by an unloaded jump-table entry. It must still
    // re-enter the six-byte dispatch sequence before the synthetic return.
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();
    let seg_addr = 0x230000u32;
    bus.write_word(seg_addr, 0);
    bus.write_word(seg_addr + 2, 1);
    disp.register_segments(HashMap::from([(12i16, seg_addr)]));

    let entry_addr = 0x240000u32;
    cpu.write_reg(Register::A5, entry_addr);
    bus.write_word(entry_addr, 0);
    bus.write_word(entry_addr + 2, 0x3F3C);
    bus.write_word(entry_addr + 4, 12);
    bus.write_word(entry_addr + 6, 0xA9F0);

    let gateway = bus.get_or_create_system_trap_gateway(0xADF0);
    let return_pc = 0x250000u32;
    cpu.write_reg(Register::PC, gateway + 2);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, return_pc);
    bus.write_word(TEST_SP + 4, 12);
    bus.write_word(TEST_SP + 6, 0xBEEF);

    call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::PC), return_pc - 6);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(bus.read_word(TEST_SP + 6), 0xBEEF);
    assert_eq!(bus.read_word(entry_addr), 12);
    assert_eq!(bus.read_word(entry_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(entry_addr + 4), seg_addr + 4);
    assert_eq!(bus.read_word(gateway), 0xADF0);
}

#[test]
fn loadseg_auto_pop_old_trap_uses_original_mpw_caller_entry() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();

    let seg_addr = 0x220000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0000);
    disp.register_segments(HashMap::from([(2i16, seg_addr)]));
    disp.install_trap_address(&mut bus, 0xA9A0, 0x310000)
        .unwrap();

    let entry_addr = 0x240000u32;
    bus.write_word(entry_addr, 0x0010);
    bus.write_word(entry_addr + 2, 0x3F3C);
    bus.write_word(entry_addr + 4, 0x0002);
    bus.write_word(entry_addr + 6, 0xA9F0);

    let old_trap_stub = 0x300000u32;
    bus.write_word(old_trap_stub, 0xADF0);
    bus.write_word(old_trap_stub + 2, 0x0000);
    bus.write_word(old_trap_stub + 4, 0x4EB9);
    bus.write_long(old_trap_stub + 6, 0x00011111);

    bus.write_long(TEST_SP, entry_addr + 8);
    bus.write_word(TEST_SP + 4, 2);
    bus.write_word(TEST_SP + 6, 0xBEEF);
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::PC, old_trap_stub + 2);

    call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

    assert!(
        disp.loadseg_getresource_state.is_none(),
        "native LoadSeg old-trap fallback must not recursively invoke native GetResource"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(
        cpu.read_reg(Register::PC),
        entry_addr + 2,
        "LoadSeg should re-enter the patched original jump-table entry"
    );
    assert_eq!(bus.read_word(TEST_SP + 6), 0xBEEF);

    assert_eq!(bus.read_word(entry_addr), 2);
    assert_eq!(bus.read_word(entry_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(entry_addr + 4), seg_addr + 4 + 0x0010);

    assert_eq!(bus.read_word(old_trap_stub), 0xADF0);
    assert_eq!(bus.read_word(old_trap_stub + 2), 0x0000);
    assert_eq!(bus.read_word(old_trap_stub + 4), 0x4EB9);
}

#[test]
fn loadseg_native_old_trap_recovers_the_recorded_original_call() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();
    let preserved_d_regs = [
        0xD300_0003,
        0xD400_0004,
        0xD500_0005,
        0xD600_0006,
        0xD700_0007,
    ];
    let preserved_a_regs = [
        0xA200_0002,
        0xA300_0003,
        0xA400_0004,
        0xA500_0005,
        0xA600_0006,
    ];
    for (reg, value) in [
        Register::D3,
        Register::D4,
        Register::D5,
        Register::D6,
        Register::D7,
    ]
    .into_iter()
    .zip(preserved_d_regs)
    {
        cpu.write_reg(reg, value);
    }
    for (reg, value) in [
        Register::A2,
        Register::A3,
        Register::A4,
        Register::A5,
        Register::A6,
    ]
    .into_iter()
    .zip(preserved_a_regs)
    {
        cpu.write_reg(reg, value);
    }
    let seg_addr = 0x220000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0000);
    disp.register_segments(HashMap::from([(2i16, seg_addr)]));

    let entry_addr = 0x240000u32;
    bus.write_word(entry_addr, 0x0010);
    bus.write_word(entry_addr + 2, 0x3F3C);
    bus.write_word(entry_addr + 4, 0x0002);
    bus.write_word(entry_addr + 6, 0xA9F0);

    // SetTrapAddress accepts an arbitrary routine entry. Its first
    // instruction and stack-frame convention are deliberately irrelevant
    // to recovery of the original A-line call.
    let handler = 0x300000u32;
    bus.write_word(handler, 0x4E71); // NOP
    disp.install_trap_address(&mut bus, 0xA9F0, handler)
        .unwrap();
    cpu.write_reg(Register::PC, entry_addr + 8);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_word(TEST_SP + 2, 0xBEEF);

    disp.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::PC), handler);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 4);

    let trampoline = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA9F0);
    let handler_sp = TEST_SP - 0x100;
    bus.write_long(handler_sp, 0x310000);
    for reg in [
        Register::D3,
        Register::D4,
        Register::D5,
        Register::D6,
        Register::D7,
        Register::A2,
        Register::A3,
        Register::A4,
        Register::A5,
        Register::A6,
    ] {
        cpu.write_reg(reg, 0xDEAD_BEEF);
    }
    cpu.write_reg(Register::D0, 0xCAFE_0000);
    cpu.write_reg(Register::A0, 0xCAFE_000A);
    cpu.write_reg(Register::A7, handler_sp);
    cpu.write_reg(Register::PC, trampoline + 2);

    call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::PC), entry_addr + 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(TEST_SP + 2), 0xBEEF);
    assert_eq!(bus.read_word(entry_addr), 2);
    assert_eq!(bus.read_word(entry_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(entry_addr + 4), seg_addr + 4 + 0x0010);
    for (reg, expected) in [
        Register::D3,
        Register::D4,
        Register::D5,
        Register::D6,
        Register::D7,
    ]
    .into_iter()
    .zip(preserved_d_regs)
    {
        assert_eq!(cpu.read_reg(reg), expected);
    }
    for (reg, expected) in [
        Register::A2,
        Register::A3,
        Register::A4,
        Register::A5,
        Register::A6,
    ]
    .into_iter()
    .zip(preserved_a_regs)
    {
        assert_eq!(cpu.read_reg(reg), expected);
    }
    assert_eq!(cpu.read_reg(Register::D0), 0xCAFE_0000);
    assert_eq!(cpu.read_reg(Register::A0), 0xCAFE_000A);
    assert!(!disp.pending_native_trap_calls.contains_key(&0xA9F0));
}

#[test]
fn loadseg_native_old_trap_restores_the_recorded_think_stack() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();
    let seg_addr = 0x220000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0000);
    disp.register_segments(HashMap::from([(2i16, seg_addr)]));

    let entry_addr = 0x240000u32;
    bus.write_word(entry_addr, 0xA9F0);
    bus.write_word(entry_addr + 2, 0x0000);
    bus.write_word(entry_addr + 4, 0x0010);
    bus.write_word(entry_addr + 6, 0x0002);

    let handler = 0x300000u32;
    bus.write_word(handler, 0x4E71); // NOP
    disp.install_trap_address(&mut bus, 0xA9F0, handler)
        .unwrap();
    cpu.write_reg(Register::PC, entry_addr + 2);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0xBEEF);

    disp.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::PC), handler);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 4);

    let trampoline = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA9F0);
    let handler_sp = TEST_SP - 0x100;
    bus.write_long(handler_sp, 0x310000);
    cpu.write_reg(Register::A7, handler_sp);
    cpu.write_reg(Register::PC, trampoline + 2);

    call_trap_word(&mut disp, 0xADF0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::PC), entry_addr);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(TEST_SP), 0xBEEF);
    assert_eq!(bus.read_word(entry_addr), 0x4EF9);
    assert_eq!(bus.read_long(entry_addr + 2), seg_addr + 4 + 0x0010);
    assert!(!disp.pending_native_trap_calls.contains_key(&0xA9F0));
}

#[test]
fn loadseg_patches_all_entries_for_loaded_segment() {
    // Inside Macintosh Volume II (1985), pp. II-60 to II-61:
    // LoadSeg patches every unloaded jump-table entry for the segment
    // so future cross-segment calls can run directly.
    let (mut disp, mut cpu, mut bus) = setup();

    let seg_addr = 0x230000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0002);
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));

    let island_addr = 0x240000u32;
    bus.write_word(island_addr, 0x0000); // routine offset 0
    bus.write_word(island_addr + 2, 0x3F3C);
    bus.write_word(island_addr + 4, 0x0001);
    bus.write_word(island_addr + 6, 0xA9F0);

    bus.write_word(island_addr + 8, 0x0004); // routine offset 4
    bus.write_word(island_addr + 10, 0x3F3C);
    bus.write_word(island_addr + 12, 0x0001);
    bus.write_word(island_addr + 14, 0xA9F0);

    cpu.write_reg(Register::PC, island_addr + 8);
    cpu.write_reg(Register::A5, island_addr);
    bus.write_word(TEST_SP, 1u16);

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);

    assert_eq!(bus.read_word(island_addr), 1);
    assert_eq!(bus.read_word(island_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(island_addr + 4), seg_addr + 4);

    assert_eq!(bus.read_word(island_addr + 8), 1);
    assert_eq!(bus.read_word(island_addr + 10), 0x4EF9);
    assert_eq!(bus.read_long(island_addr + 12), seg_addr + 8);
}

#[test]
fn loadseg_patches_think_far_header_entry_range() {
    // Symantec/THINK far CODE reuses the first four bytes as
    // first-JT-entry index and entry count, with flag bits set. LoadSeg
    // must not treat the flagged count word as a raw near-model count.
    let (mut disp, mut cpu, mut bus) = setup();

    let seg_addr = 0x230000u32;
    bus.write_word(seg_addr, 0x8003); // reloc flag + first JT entry index
    bus.write_word(seg_addr + 2, 0x4002); // far flag + 2 entries
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));

    let jt_base = 0x240000u32;
    bus.write_word(addr::CUR_JT_OFFSET, 0x20);
    cpu.write_reg(Register::A5, jt_base - 0x20);

    let entry_addr = jt_base + 3 * 8;
    bus.write_word(entry_addr, 0xA9F0);
    bus.write_word(entry_addr + 2, 0x0000);
    bus.write_word(entry_addr + 4, 0x0000); // routine offset
    bus.write_word(entry_addr + 6, 0x0001); // segment number

    let next_entry_addr = entry_addr + 8;
    bus.write_word(next_entry_addr, 0xA9F0);
    bus.write_word(next_entry_addr + 2, 0x0000);
    bus.write_word(next_entry_addr + 4, 0x0008); // routine offset
    bus.write_word(next_entry_addr + 6, 0x0001); // segment number

    bus.write_word(next_entry_addr + 8, 0xDEAD);

    cpu.write_reg(Register::PC, entry_addr + 2);

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "THINK-style LoadSeg entry does not pop a segment word"
    );
    assert_eq!(cpu.read_reg(Register::PC), entry_addr);

    assert_eq!(bus.read_word(entry_addr), 0x4EF9);
    assert_eq!(bus.read_long(entry_addr + 2), seg_addr + 4);
    assert_eq!(bus.read_word(entry_addr + 6), 0);

    assert_eq!(bus.read_word(next_entry_addr), 0x4EF9);
    assert_eq!(bus.read_long(next_entry_addr + 2), seg_addr + 12);
    assert_eq!(bus.read_word(next_entry_addr + 6), 0);
    assert_eq!(
        bus.read_word(next_entry_addr + 8),
        0xDEAD,
        "flagged THINK entry count should be masked before patching"
    );
}

#[test]
fn loadseg_defers_to_native_getresource_hook_when_installed() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();

    let seg_addr = 0x230000u32;
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0000);
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));
    disp.install_trap_address(&mut bus, 0xA9A0, 0x300000)
        .unwrap();

    let island_addr = 0x240000u32;
    bus.write_word(island_addr, 0x0000);
    bus.write_word(island_addr + 2, 0x3F3C);
    bus.write_word(island_addr + 4, 0x0001);
    bus.write_word(island_addr + 6, 0xA9F0);

    cpu.write_reg(Register::PC, island_addr + 8);
    cpu.write_reg(Register::D3, 0xD3D3_D3D3);
    cpu.write_reg(Register::A4, 0xA4A4_A4A4);
    bus.write_word(TEST_SP, 1u16);

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    let state = disp
        .loadseg_getresource_state
        .as_ref()
        .expect("LoadSeg should be waiting for native GetResource");
    let call_sp = state.result_sp - 10;
    assert_eq!(state.seg_num, 1);
    assert_eq!(state.entry_addr, island_addr);
    assert_eq!(state.d_regs[3], 0xD3D3_D3D3);
    assert_eq!(state.a_regs[4], 0xA4A4_A4A4);
    assert_eq!(state.a_regs[7], TEST_SP + 2);

    assert_eq!(cpu.read_reg(Register::PC), 0x300000);
    assert_eq!(cpu.read_reg(Register::A7), call_sp);
    assert_eq!(
        bus.read_long(call_sp),
        disp.loadseg_getresource_trampoline_addr.unwrap()
    );
    assert_eq!(bus.read_word(call_sp + 4), 1);
    assert_eq!(bus.read_long(call_sp + 6), u32::from_be_bytes(*b"CODE"));
    assert_eq!(bus.read_long(state.result_sp), 0);

    assert_eq!(bus.read_word(island_addr), 0x0000);
    assert_eq!(bus.read_word(island_addr + 2), 0x3F3C);
}

#[test]
fn loadseg_materializes_code_from_runtime_resource_chain() {
    let (mut disp, mut cpu, mut bus) = setup();

    let code = [
        0x00, 0x00, 0x00, 0x01, // near header: one entry
        0x4E, 0x75, // RTS
    ];
    let seg_addr = setup_resources(&mut disp, &mut bus, b"CODE", 19, &code);

    let island_addr = 0x240000u32;
    bus.write_word(island_addr, 0x0000);
    bus.write_word(island_addr + 2, 0x3F3C);
    bus.write_word(island_addr + 4, 19);
    bus.write_word(island_addr + 6, 0xA9F0);
    bus.write_word(addr::CUR_JT_OFFSET, 0);
    cpu.write_reg(Register::A5, island_addr);
    cpu.write_reg(Register::PC, island_addr + 8);
    bus.write_word(TEST_SP, 19);

    call(&mut disp, true, 0x1F0, &mut cpu, &mut bus).unwrap();

    assert_eq!(disp.segment_map.get(&19), Some(&seg_addr));
    assert_eq!(cpu.read_reg(Register::PC), island_addr + 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_word(island_addr), 19);
    assert_eq!(bus.read_word(island_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(island_addr + 4), seg_addr + 4);
}

#[test]
fn loadseg_getresource_continuation_refreshes_decoded_code_resource() {
    let (mut disp, mut cpu, mut bus) = setup();

    let seg_addr = 0x230000u32;
    bus.write_long(seg_addr - 4, 8);
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x4002); // encoded/undecoded header
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));

    let decoded = [
        0x00, 0x00, 0x00, 0x01, // near header: one entry
        0x60, 0x00, 0x00, 0x02, // decoded code bytes
    ];
    let ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &decoded);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, ptr);

    let island_addr = 0x240000u32;
    bus.write_word(island_addr, 0x0000);
    bus.write_word(island_addr + 2, 0x3F3C);
    bus.write_word(island_addr + 4, 0x0001);
    bus.write_word(island_addr + 6, 0xA9F0);
    cpu.write_reg(Register::PC, 0x29CF00);
    cpu.write_reg(Register::A5, island_addr);
    cpu.write_reg(Register::A4, 0xA4A4_A4A4);

    disp.loadseg_getresource_state = Some(super::super::dispatch::LoadSegGetResourceState {
        seg_num: 1,
        entry_addr: island_addr,
        result_sp: TEST_SP - 4,
        d_regs: [0x1010_1010; 8],
        a_regs: [
            0xA0A0_A0A0,
            0xA1A1_A1A1,
            0xA2A2_A2A2,
            0xA3A3_A3A3,
            0xA4A4_A4A4,
            island_addr,
            0xA6A6_A6A6,
            TEST_SP,
        ],
    });
    bus.write_long(TEST_SP - 4, handle);

    disp.resume_loadseg_after_getresource(&mut bus, &mut cpu)
        .unwrap();

    assert_eq!(bus.read_word(seg_addr), 0x0000);
    assert_eq!(bus.read_word(seg_addr + 2), 0x0001);
    assert_eq!(bus.read_long(seg_addr + 4), 0x6000_0002);
    assert_eq!(cpu.read_reg(Register::A4), 0xA4A4_A4A4);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(cpu.read_reg(Register::PC), island_addr + 2);
    assert_eq!(bus.read_word(island_addr), 1);
    assert_eq!(bus.read_word(island_addr + 2), 0x4EF9);
    assert_eq!(bus.read_long(island_addr + 4), seg_addr + 4);
}

#[test]
fn loadseg_getresource_continuation_preserves_header_for_implausibly_expanded_code_header() {
    let (mut disp, mut cpu, mut bus) = setup();

    let seg_addr = 0x230000u32;
    bus.write_long(seg_addr - 4, 8);
    bus.write_word(seg_addr, 0x0000);
    bus.write_word(seg_addr + 2, 0x0002);
    bus.write_long(seg_addr + 4, 0x4E56_0000);
    disp.register_segments(HashMap::from([(1i16, seg_addr)]));

    let corrupted = [
        0x06, 0x79, 0x69, 0x25, // would decode as THINK far with 10533 entries
        0x60, 0x00, 0x00, 0x02,
    ];
    let ptr = setup_resources(&mut disp, &mut bus, b"CODE", 1, &corrupted);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"CODE", 1, ptr);

    let jt_base = 0x240000u32;
    bus.write_word(addr::CUR_JT_OFFSET, 0);
    cpu.write_reg(Register::A5, jt_base);
    for i in 0..2 {
        let entry = jt_base + i * 8;
        bus.write_word(entry, 0);
        bus.write_word(entry + 2, 0x3F3C);
        bus.write_word(entry + 4, 1);
        bus.write_word(entry + 6, 0xA9F0);
    }

    disp.loadseg_getresource_state = Some(super::super::dispatch::LoadSegGetResourceState {
        seg_num: 1,
        entry_addr: jt_base,
        result_sp: TEST_SP - 4,
        d_regs: [0x1010_1010; 8],
        a_regs: [
            0xA0A0_A0A0,
            0xA1A1_A1A1,
            0xA2A2_A2A2,
            0xA3A3_A3A3,
            0xA4A4_A4A4,
            jt_base,
            0xA6A6_A6A6,
            TEST_SP,
        ],
    });
    bus.write_long(TEST_SP - 4, handle);

    disp.resume_loadseg_after_getresource(&mut bus, &mut cpu)
        .unwrap();

    assert_eq!(bus.read_word(seg_addr), 0x0000);
    assert_eq!(bus.read_word(seg_addr + 2), 0x0002);
    assert_eq!(bus.read_long(seg_addr + 4), 0x6000_0002);
    assert_eq!(bus.read_word(jt_base), 1);
    assert_eq!(bus.read_word(jt_base + 2), 0x4EF9);
    assert_eq!(bus.read_long(jt_base + 4), seg_addr + 4);
    assert_eq!(bus.read_word(jt_base + 8), 1);
    assert_eq!(bus.read_word(jt_base + 10), 0x4EF9);
    assert_eq!(bus.read_long(jt_base + 12), seg_addr + 4);
}

#[test]
fn tool_trampoline_bypasses_later_native_trap_handler() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();
    setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0xAA, 0xBB]);

    let trampoline = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA9A0);
    disp.install_trap_address(&mut bus, 0xA9A0, 0x300000)
        .unwrap();

    let return_pc = 0x12345678;
    bus.write_long(TEST_SP, return_pc);
    bus.write_word(TEST_SP + 4, 7);
    bus.write_long(TEST_SP + 6, u32::from_be_bytes(*b"TEST"));
    bus.write_long(TEST_SP + 10, 0);
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::PC, trampoline + 2);

    call_trap_word(&mut disp, 0xADA0, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::PC),
        return_pc,
        "saved Systemless trampoline should call the original HLE trap"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_ne!(cpu.read_reg(Register::A0), 0);
    assert_eq!(bus.read_long(TEST_SP + 10), cpu.read_reg(Register::A0));
    assert_eq!(bus.read_word(0x0A60), 0);
}

#[test]
fn guest_auto_pop_old_trap_stub_bypasses_native_trap_handler() {
    let (mut disp, mut cpu, mut bus) = setup_with_trap_tables();
    setup_resources(&mut disp, &mut bus, b"TEST", 7, &[0xAA, 0xBB]);
    disp.install_trap_address(&mut bus, 0xA9A0, 0x300000)
        .unwrap();

    let return_pc = 0x12345678;
    bus.write_word(TEST_SP, 7);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"TEST"));
    bus.write_long(TEST_SP + 6, 0);
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::PC, return_pc);
    disp.dispatch(0xA9A0, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::PC), 0x300000);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 4);

    cpu.write_reg(Register::PC, 0x0029D256);

    call_trap_word(&mut disp, 0xADA0, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::PC), return_pc);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_ne!(cpu.read_reg(Register::A0), 0);
    assert_eq!(bus.read_long(TEST_SP + 6), cpu.read_reg(Register::A0));
    assert!(!disp.pending_native_trap_calls.contains_key(&0xA9A0));
}

// ================================================================
// 11. GetResAttrs (0x1A6)
// ================================================================
// Inside Macintosh Volume I (1985), p. I-121: GetResAttrs returns
// the map attribute word for a live resource handle.
#[test]
fn get_res_attrs_returns_attribute_bits_for_live_resource_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"ATTR", 42, &[0x11, 0x22, 0x33]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"ATTR", 42, data_ptr);

    disp.insert_resource_attrs_for_test(0, (*b"ATTR", 42), 0x0026u8);

    let sp = TEST_SP;
    bus.write_long(sp, handle);
    bus.write_word(sp + 4, 0xBEEF);

    call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(TEST_SP + 4), 0x0026);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// BasiliskII returns resChanged in the result slot and sets ResError to
// resNotFound when the handle is not a resource handle.
#[test]
fn get_res_attrs_returns_res_changed_for_unknown_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;
    bus.write_long(sp, 0x00DEAD00);
    bus.write_word(sp + 4, 0xBEEF);

    call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(TEST_SP + 4), 0x0002);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// Inside Macintosh Volume I (1985), p. I-121 and p. I-123:
// GetResAttrs observes the resChanged bit set by AddResource on a live
// resource handle.
#[test]
fn get_res_attrs_after_add_resource_returns_res_changed() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(3);
    bus.write_bytes(data_ptr, &[0x11, 0x22, 0x33]);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);
    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(0, ResourceFileMap::default())]),
        names: HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });

    bus.write_long(TEST_SP, 0);
    bus.write_word(TEST_SP + 4, 43u16);
    bus.write_long(TEST_SP + 6, u32::from_be_bytes(*b"ATTR"));
    bus.write_long(TEST_SP + 10, handle);
    call(&mut disp, true, 0x1AB, &mut cpu, &mut bus).unwrap(); // AddResource
    assert_eq!(bus.read_word(0x0A60), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    bus.write_word(TEST_SP + 4, 0xBEEF);
    call(&mut disp, true, 0x1A6, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(TEST_SP + 4), 0x0002);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// Inside Macintosh Volume IV (1986), p. IV-16 / More Macintosh Toolbox
// 1993, p. 1-120: RsrcMapEntry returns the resource-reference offset from
// the start of the resource map for live resource handles.
#[test]
#[ignore]
fn rsrcmapentry_returns_reference_offset_for_live_resource_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let rsrc_bytes = make_single_resource_fork_bytes(*b"TEST", 43, &[0x11, 0x22]);
    disp.vfs_rsrc
        .insert("RsrcMapEntryTest".to_string(), rsrc_bytes);
    disp.open_resource_file_from_vfs_key(&mut bus, "RsrcMapEntryTest", false);

    let handle = disp
        .loaded_handles
        .iter()
        .find_map(|(handle, (_, res_type, res_id))| {
            (*res_type == *b"TEST" && *res_id == 43).then_some(*handle)
        })
        .expect("resource handle should be loaded");

    bus.write_long(TEST_SP, handle);
    bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

    call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 40);
    assert_eq!(bus.read_word(0x0A60), 0);
}

// Nil handles are tolerated by leaving the result slot untouched while
// setting ResErr to resNotFound.
#[test]
#[ignore]
fn rsrcmapentry_nil_handle_sets_resnotfound_and_leaves_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_long(TEST_SP, 0);
    bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

    call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 0xBEEF_BEEF);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// Inside Macintosh Volume IV (1986), p. IV-16 / More Macintosh Toolbox
// 1993, p. 1-120: non-resource handles report resNotFound and leave the
// caller's result slot unchanged.
#[test]
#[ignore]
fn rsrcmapentry_unknown_handle_sets_resnotfound_and_leaves_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_long(TEST_SP, 0x00DE_AD00);
    bus.write_long(TEST_SP + 4, 0xBEEF_BEEF);

    call(&mut disp, true, 0x1C5, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 0xBEEF_BEEF);
    assert_eq!(bus.read_word(0x0A60) as i16, -192);
}

// ================================================================
// 12. ChangedResource (0x1AA)
// ================================================================
// Inside Macintosh Volume I (1985), p. I-123: ChangedResource marks
// a resource as changed so UpdateResFile/WriteResource will flush it.
#[test]
fn changedresource_sets_reschanged_attribute_for_unprotected_resource() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"CHNG", 9, &[0xAA, 0xBB]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"CHNG", 9, data_ptr);

    bus.write_long(TEST_SP, handle);
    bus.write_word(0x0A60, 0xBEEF);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_ne!(
        disp.resource_attributes_for_handle(handle).unwrap_or(0)
            & u16::from(super::super::TrapDispatcher::RES_CHANGED_ATTR),
        0
    );
}

// Inside Macintosh Volume IV (1986), p. IV-16: ChangedResource does not
// modify protected resources (resProtected bit set) but returns resAttrErr.
#[test]
fn changedresource_protected_resource_is_noop_with_resattrerr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let data_ptr = setup_resources(&mut disp, &mut bus, b"PROT", 3, &[0x10, 0x20]);
    let handle = disp.get_or_create_resource_handle(&mut bus, *b"PROT", 3, data_ptr);

    disp.insert_resource_attrs_for_test(0, (*b"PROT", 3), 0x0008u8);

    bus.write_long(TEST_SP, handle);
    bus.write_word(0x0A60, 0xBEEF);
    call(&mut disp, true, 0x1AA, &mut cpu, &mut bus).unwrap();

    let attrs_after = disp.resource_attributes_for_handle(handle).unwrap_or(0);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        super::super::TrapDispatcher::RES_ATTR_ERR
    );
    assert_eq!(
        attrs_after & u16::from(super::super::TrapDispatcher::RES_CHANGED_ATTR),
        0
    );
    assert_ne!(attrs_after & 0x0008, 0);
}

// ================================================================
// 13a. HighLevelFSDispatch (0x252) selector 1 — FSMakeFSSpec
// ================================================================
#[test]
fn high_level_fs_generated_routes_preserve_exact_moveq_values() {
    assert_eq!(super::HIGH_LEVEL_FS_DISPATCH_OPERATION_ROUTES.len(), 15);
    assert!(super::HIGH_LEVEL_FS_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (1, "FSMakeFSSpec"),
        (2, "FSpOpenDF"),
        (3, "FSpOpenRF"),
        (4, "FSpCreate"),
        (5, "FSpDirCreate"),
        (6, "FSpDelete"),
        (7, "FSpGetFInfo"),
        (8, "FSpSetFInfo"),
        (9, "FSpSetFLock"),
        (10, "FSpRstFLock"),
        (11, "FSpRename"),
        (12, "FSpCatMove"),
        (13, "FSpOpenResFile"),
        (14, "FSpCreateResFile"),
        (15, "FSpExchangeFiles"),
    ] {
        let route = super::high_level_fs_dispatch_operation_route(0xAA52, selector)
            .expect("HighLevelFSDispatch route");
        assert_eq!(route.routine_name, routine_name);
    }

    for (trap_word, selector) in [
        (0xAB52, 1),
        (0xAA52, 0),
        (0xAA52, 16),
        (0xAA52, 0x0000_7001),
        (0xAA52, 0x0001_0001),
    ] {
        assert!(super::high_level_fs_dispatch_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn high_level_fs_dispatch_records_nonterminal_and_rejects_stale_high_word() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.current_trap_word = 0xAA52;

    cpu.write_reg(Register::D0, 11);
    let result = disp
        .dispatch_resource(true, 0x252, &mut cpu, &mut bus)
        .expect("HighLevelFSDispatch arm");
    assert!(matches!(result, Err(crate::Error::Halted)));
    assert_eq!(
        disp.current_selector_operation,
        Some(super::HIGH_LEVEL_FS_DISPATCH_OPERATION_ROUTES[10].operation_id)
    );

    cpu.write_reg(Register::D0, 0x0001_000B);
    let result = disp
        .dispatch_resource(true, 0x252, &mut cpu, &mut bus)
        .expect("HighLevelFSDispatch arm");
    assert!(matches!(result, Err(crate::Error::Halted)));
    assert_eq!(disp.current_selector_operation, None);
}

#[test]
fn hlfs_dispatch_fsmakefsspec() {
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(&mut bus, name_ptr, b"MyFile.txt");

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr); // spec_ptr
    bus.write_long(sp + 4, name_ptr); // name_ptr
    bus.write_long(sp + 8, 2); // dirID
    bus.write_word(sp + 12, 1); // vRefNum

    cpu.write_reg(Register::D0, 1); // selector = FSMakeFSSpec

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    // Check FSSpec was filled: vRefNum at spec+0, dirID at spec+2, name at spec+6
    assert_eq!(
        bus.read_word(spec_ptr),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
    );
    assert_eq!(bus.read_long(spec_ptr + 2), 2);
    assert_eq!(bus.read_byte(spec_ptr + 6), 10); // length of "MyFile.txt"
                                                 // File doesn't exist in VFS — result should be fnfErr (-43)
                                                 // Files 1992, 2-166
    assert_eq!(bus.read_word(new_sp) as i16, -43);
}

// ================================================================
// 13a2. FSMakeFSSpec returns noErr for existing file
// ================================================================
#[test]
fn hlfs_dispatch_fsmakefsspec_exists() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("MyFile.txt".to_string(), vec![]);

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(&mut bus, name_ptr, b"MyFile.txt");

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);
    bus.write_long(sp + 4, name_ptr);
    bus.write_long(sp + 8, 2);
    bus.write_word(sp + 12, 1);
    cpu.write_reg(Register::D0, 1);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert_eq!(bus.read_word(new_sp), 0); // noErr
}

// ================================================================
// 13a3. FSMakeFSSpec returns noErr for existing directory
// ================================================================
#[test]
fn hlfs_dispatch_fsmakefsspec_dir() {
    let (mut disp, mut cpu, mut bus) = setup();
    // Add a file with directory component
    disp.vfs.insert("MyDir/SomeFile.txt".to_string(), vec![]);

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(&mut bus, name_ptr, b":MyDir");

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);
    bus.write_long(sp + 4, name_ptr);
    bus.write_long(sp + 8, 2);
    bus.write_word(sp + 12, 1);
    cpu.write_reg(Register::D0, 1);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert_eq!(bus.read_word(new_sp), 0); // noErr — directory exists
}

#[test]
fn fsmakefsspec_decomposes_partial_pathname_to_parent_dir_and_basename() {
    // Files 1992, 2-28 and 2-34: callers may pass full or partial
    // pathnames to FSMakeFSSpec, but the resulting FSSpec stores the
    // parent directory ID and final object name, not the whole pathname.
    let (mut disp, mut cpu, mut bus) = setup();
    let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    disp.vfs.insert(
        "System Folder/Preferences/Existing Prefs".to_string(),
        vec![1, 2, 3],
    );
    disp.vfs.insert("App/App".to_string(), vec![]);
    disp.set_launched_app_path("App/App");

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(
        &mut bus,
        name_ptr,
        b":System Folder:Preferences:Existing Prefs",
    );

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);
    bus.write_long(sp + 4, name_ptr);
    bus.write_long(sp + 8, 0);
    bus.write_word(sp + 12, 0);
    cpu.write_reg(Register::D0, 1);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert_eq!(bus.read_word(new_sp) as i16, 0);
    assert_eq!(
        bus.read_word(spec_ptr),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
    );
    assert_eq!(bus.read_long(spec_ptr + 2), pref_dir_id);
    assert_eq!(
        crate::trap::types::read_fsspec_name(&bus, spec_ptr),
        "Existing Prefs"
    );
}

#[test]
fn fsmakefsspec_pathname_missing_target_does_not_match_same_basename_elsewhere() {
    // Files 1992, 2-34 to 2-35: if the parent directory exists but the
    // target object does not, FSMakeFSSpec still fills a valid FSSpec
    // and returns fnfErr. A pathname must not degrade to a basename-only
    // search in another directory.
    let (mut disp, mut cpu, mut bus) = setup();
    let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    disp.vfs.insert("App/App".to_string(), vec![]);
    disp.vfs
        .insert("App/Shared Preferences".to_string(), vec![0x42]);
    disp.set_launched_app_path("App/App");

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(
        &mut bus,
        name_ptr,
        b":System Folder:Preferences:Shared Preferences",
    );

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);
    bus.write_long(sp + 4, name_ptr);
    bus.write_long(sp + 8, 0);
    bus.write_word(sp + 12, 0);
    cpu.write_reg(Register::D0, 1);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert_eq!(bus.read_word(new_sp) as i16, -43);
    assert_eq!(
        bus.read_word(spec_ptr),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
    );
    assert_eq!(bus.read_long(spec_ptr + 2), pref_dir_id);
    assert_eq!(
        crate::trap::types::read_fsspec_name(&bus, spec_ptr),
        "Shared Preferences"
    );
}

#[test]
fn fsmakefsspec_maps_unix_tmp_path_to_temporary_items_parent() {
    // Some ports pass POSIX temp pathnames through FSMakeFSSpec. Treat
    // /tmp as the standard temporary folder rather than a missing HFS
    // directory named "tmp".
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300000u32;
    let name_ptr = 0x300100u32;
    write_pstring(&mut bus, name_ptr, b"/tmp/lcache00.tmp");

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);
    bus.write_long(sp + 4, name_ptr);
    bus.write_long(sp + 8, 0);
    bus.write_word(sp + 12, 0);
    cpu.write_reg(Register::D0, 1);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert_eq!(bus.read_word(new_sp) as i16, -43);
    assert_eq!(
        bus.read_word(spec_ptr),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
    );
    let temp_dir_id = disp
        .directory_id_for_vfs_path("Temporary Items")
        .expect("temp folder should be present");
    assert_eq!(bus.read_long(spec_ptr + 2), temp_dir_id);
    assert_eq!(
        crate::trap::types::read_fsspec_name(&bus, spec_ptr),
        "lcache00.tmp"
    );
}

// ================================================================
// 13b. HighLevelFSDispatch (0x252) selector 2 — FSpOpenDF
// ================================================================
#[test]
fn hlfs_dispatch_fspopendf() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Add file to VFS so FSpOpenDF can find it
    disp.vfs.insert("OpenMe.txt".to_string(), vec![1, 2, 3]);

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"OpenMe.txt");

    let ref_num_ptr = 0x300200u32;

    let sp = TEST_SP;
    bus.write_long(sp, ref_num_ptr); // refNumPtr
    bus.write_word(sp + 4, 1); // permission (fsRdPerm)
    bus.write_long(sp + 6, spec_ptr); // spec_ptr

    cpu.write_reg(Register::D0, 2); // selector = FSpOpenDF

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 10);
    // A refnum should have been written
    let refnum = bus.read_word(ref_num_ptr);
    assert!(
        refnum >= 100,
        "refnum should be >= 100 (initial next_refnum)"
    );
    // Result at new SP should be 0
    assert_eq!(bus.read_word(new_sp), 0);
}

#[test]
fn hlfs_dispatch_fspopenrf_opens_resource_fork_access_path() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs_rsrc
        .insert("OpenMe.rsrc".to_string(), vec![0x52, 0x53, 0x52, 0x43]);

    let spec_ptr = 0x300000u32;
    write_fsspec(
        &mut bus,
        spec_ptr,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        2,
        b"OpenMe.rsrc",
    );
    let ref_num_ptr = 0x300200u32;

    let sp = TEST_SP;
    bus.write_long(sp, ref_num_ptr);
    bus.write_word(sp + 4, 1); // fsRdPerm
    bus.write_long(sp + 6, spec_ptr);
    cpu.write_reg(Register::D0, 3); // selector = FSpOpenRF

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 10);
    let refnum = bus.read_word(ref_num_ptr);
    assert!(refnum >= 100, "resource-fork refnum should be allocated");
    assert_eq!(bus.read_word(new_sp), 0);
    assert_eq!(
        disp.open_files.get(&refnum).map(String::as_str),
        Some("__rsrc__OpenMe.rsrc")
    );
    assert_eq!(
        disp.vfs.get("__rsrc__OpenMe.rsrc"),
        Some(&vec![0x52, 0x53, 0x52, 0x43])
    );
}

#[test]
fn hlfs_dispatch_fspopenrf_does_not_escape_the_fsspec_directory() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs_rsrc
        .insert("Alpha/Duplicate".to_string(), vec![0xA1]);
    disp.vfs_rsrc
        .insert("Beta/Duplicate".to_string(), vec![0xB2]);
    let empty_dir_id = disp.ensure_vfs_directory("Empty");

    let spec_ptr = 0x300000u32;
    write_fsspec(
        &mut bus,
        spec_ptr,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        empty_dir_id,
        b"Duplicate",
    );
    let ref_num_ptr = 0x300200u32;
    bus.write_word(ref_num_ptr, 0x7FFF);
    bus.write_long(TEST_SP, ref_num_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, spec_ptr);
    cpu.write_reg(Register::D0, 3);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -43); // fnfErr
    assert_eq!(bus.read_word(ref_num_ptr), 0x7FFF);
    assert!(disp.open_files.is_empty());
}

#[test]
fn hlfs_dispatch_fspopenrf_rejects_an_unknown_volume() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs_rsrc.insert("Duplicate".to_string(), vec![0xA1]);

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1234, 2, b"Duplicate");
    let ref_num_ptr = 0x300200u32;
    bus.write_word(ref_num_ptr, 0x7FFF);
    bus.write_long(TEST_SP, ref_num_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, spec_ptr);
    cpu.write_reg(Register::D0, 3);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -35); // nsvErr
    assert_eq!(bus.read_word(ref_num_ptr), 0x7FFF);
    assert!(disp.open_files.is_empty());
}

#[test]
fn hlfs_dispatch_fspopendf_packed_signedbyte_permission_marks_write_path() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Prefs".to_string(), b"existing prefs".to_vec());

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"Prefs");

    let ref_num_ptr = 0x300200u32;
    let sp = TEST_SP;
    bus.write_long(sp, ref_num_ptr);
    bus.write_word(sp + 4, 0x0233); // fsWrPerm in high byte, nonzero padding low byte.
    bus.write_long(sp + 6, spec_ptr);

    cpu.write_reg(Register::D0, 2);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    let refnum = bus.read_word(ref_num_ptr);
    assert_eq!(new_sp, TEST_SP + 10);
    assert_eq!(bus.read_word(new_sp), 0);
    assert!(
        disp.write_refnums.contains(&refnum),
        "packed fsWrPerm must be tracked as a write-open access path"
    );
}

#[test]
fn hlfs_dispatch_fspopendf_double_exclusive_write_open_returns_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Save.dat".to_string(), vec![1, 2, 3]);

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"Save.dat");

    let first_ref_ptr = 0x300200u32;
    let sp = TEST_SP;
    bus.write_long(sp, first_ref_ptr);
    bus.write_word(sp + 4, 3); // fsRdWrPerm
    bus.write_long(sp + 6, spec_ptr);
    cpu.write_reg(Register::D0, 2);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 0);

    let second_ref_ptr = 0x300220u32;
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(sp, second_ref_ptr);
    bus.write_word(sp + 4, 0x0333); // fsRdWrPerm in high byte, non-permission padding low byte.
    bus.write_long(sp + 6, spec_ptr);
    bus.write_word(second_ref_ptr, 0x7FFF);
    cpu.write_reg(Register::D0, 2);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 10);
    assert_eq!(bus.read_word(new_sp), 0);
    let second_refnum = bus.read_word(second_ref_ptr);
    assert_ne!(
        second_refnum, 0x7FFF,
        "BasiliskII/extfs-compatible second FSpOpenDF publishes a refnum"
    );
    assert!(
        disp.write_refnums.contains(&second_refnum),
        "packed fsRdWrPerm must be tracked on the second write-open path"
    );
}

#[test]
fn hlfs_dispatch_fspopendf_locked_file_write_remains_openable() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Locked.dat".to_string(), vec![1, 2, 3]);
    disp.locked_files.insert("Locked.dat".to_string());

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"Locked.dat");

    let ref_num_ptr = 0x300200u32;
    let sp = TEST_SP;
    bus.write_long(sp, ref_num_ptr);
    bus.write_word(sp + 4, 0x0233); // fsWrPerm in high byte, non-permission padding low byte.
    bus.write_long(sp + 6, spec_ptr);
    bus.write_word(ref_num_ptr, 0x7FFF);
    cpu.write_reg(Register::D0, 2);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 10);
    assert_eq!(bus.read_word(new_sp), 0);
    let refnum = bus.read_word(ref_num_ptr);
    assert_ne!(
        refnum, 0x7FFF,
        "BasiliskII/extfs-compatible locked FSpOpenDF publishes a refnum"
    );
    assert!(
        disp.write_refnums.contains(&refnum),
        "packed fsWrPerm must still be tracked as write-open"
    );
}

// ================================================================
// 13b2. FSpOpenDF returns fnfErr for missing file
// ================================================================
#[test]
fn hlfs_dispatch_fspopendf_fnferr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"Missing.txt");

    let ref_num_ptr = 0x300200u32;

    let sp = TEST_SP;
    bus.write_long(sp, ref_num_ptr);
    bus.write_word(sp + 4, 1);
    bus.write_long(sp + 6, spec_ptr);

    cpu.write_reg(Register::D0, 2);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 10);
    // Result should be fnfErr (-43)
    assert_eq!(bus.read_word(new_sp) as i16, -43);
}

// ================================================================
// 13b3. HighLevelFSDispatch (0x252) selector 7 — FSpGetFInfo (file)
// ================================================================
#[test]
fn hlfs_dispatch_fspgetfinfo_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("InfoFile.bin".to_string(), vec![0xAA]);
    disp.set_vfs_entry_finfo(
        "InfoFile.bin",
        u32::from_be_bytes(*b"APPL"),
        u32::from_be_bytes(*b"RSED"),
        0x1234,
    );

    let spec_ptr = 0x300000u32;
    let finfo_ptr = 0x300100u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"InfoFile.bin");

    let sp = TEST_SP;
    bus.write_long(sp, finfo_ptr); // fndrInfo pointer
    bus.write_long(sp + 4, spec_ptr); // spec pointer
    cpu.write_reg(Register::D0, 7);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 8);
    assert_eq!(bus.read_word(new_sp), 0); // noErr
    assert_eq!(bus.read_long(finfo_ptr), u32::from_be_bytes(*b"APPL"));
    assert_eq!(bus.read_long(finfo_ptr + 4), u32::from_be_bytes(*b"RSED"));
    assert_eq!(bus.read_word(finfo_ptr + 8), 0x1234);
}

// ================================================================
// 13b4. HighLevelFSDispatch (0x252) selector 7 — FSpGetFInfo (directory)
// ================================================================
#[test]
fn hlfs_dispatch_fspgetfinfo_directory() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Seed a child file so the parent directory exists in the VFS tree.
    disp.vfs
        .insert("EV Override 1.0.1/EV Override".to_string(), vec![0x00]);

    let spec_ptr = 0x300000u32;
    let finfo_ptr = 0x300100u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"EV Override 1.0.1");

    let sp = TEST_SP;
    bus.write_long(sp, finfo_ptr); // fndrInfo pointer
    bus.write_long(sp + 4, spec_ptr); // spec pointer
    cpu.write_reg(Register::D0, 7);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 8);
    assert_eq!(bus.read_word(new_sp), 0); // noErr
    assert_eq!(bus.read_long(finfo_ptr), u32::from_be_bytes(*b"fold"));
    assert_eq!(bus.read_long(finfo_ptr + 4), u32::from_be_bytes(*b"MACS"));
}

// ================================================================
// 13c. HighLevelFSDispatch (0x252) selector 4 — FSpCreate
// ================================================================
#[test]
fn hlfs_dispatch_fspcreate() {
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"NewFile.dat");

    let sp = TEST_SP;
    // FSpCreate stack: SP+0..SP+9 = other params, SP+10 = spec_ptr
    // Zero out the lower bytes
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 10, spec_ptr);

    cpu.write_reg(Register::D0, 4); // selector = FSpCreate

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 14);
    assert!(
        disp.vfs.contains_key("NewFile.dat"),
        "file should be added to VFS"
    );
}

// ================================================================
// 13c2. HighLevelFSDispatch (0x252) selector 5 — FSpDirCreate
// ================================================================
#[test]
fn hlfs_dispatch_fspdircreate_creates_child_directory_and_returns_dirid() {
    let (mut disp, mut cpu, mut bus) = setup();

    let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    let spec_ptr = 0x300000u32;
    let created_dir_id_ptr = 0x300100u32;
    write_fsspec(
        &mut bus,
        spec_ptr,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        parent_dir_id,
        b"Humongous Entertainment",
    );
    bus.write_long(created_dir_id_ptr, 0xDEAD_BEEF);

    let sp = TEST_SP;
    bus.write_long(sp, created_dir_id_ptr);
    bus.write_word(sp + 4, 0); // scriptTag
    bus.write_long(sp + 6, spec_ptr);
    bus.write_word(sp + 10, 0xBEEF); // result poison
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 5);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(TEST_SP + 10) as i16, 0);
    let created_dir_id = bus.read_long(created_dir_id_ptr);
    assert_ne!(created_dir_id, parent_dir_id);
    assert_eq!(
        disp.directory_path_for_id(created_dir_id),
        Some("System Folder/Preferences/Humongous Entertainment")
    );
}

#[test]
fn hlfs_dispatch_fspdircreate_duplicate_returns_dupfneerr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    disp.ensure_vfs_directory("System Folder/Preferences/Humongous Entertainment");
    let spec_ptr = 0x300000u32;
    let created_dir_id_ptr = 0x300100u32;
    write_fsspec(
        &mut bus,
        spec_ptr,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        parent_dir_id,
        b"Humongous Entertainment",
    );
    bus.write_long(created_dir_id_ptr, 0xDEAD_BEEF);

    let sp = TEST_SP;
    bus.write_long(sp, created_dir_id_ptr);
    bus.write_word(sp + 4, 0);
    bus.write_long(sp + 6, spec_ptr);
    bus.write_word(sp + 10, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 5);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -48);
    assert_eq!(
        bus.read_long(created_dir_id_ptr),
        0xDEAD_BEEF,
        "duplicate failure should not overwrite createdDirID"
    );
}

// ================================================================
// 13d. HighLevelFSDispatch (0x252) selector 14/13 — FSpCreateResFile/FSpOpenResFile
// ================================================================
#[test]
fn hlfs_dispatch_fspcreateresfile_and_openresfile() {
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"Escape Velocity Prefs");

    // FSpCreateResFile stack: [scriptTag(2)] [fileType(4)] [creator(4)] [spec_ptr(4)]
    let sp = TEST_SP;
    bus.write_word(sp, 0);
    bus.write_long(sp + 2, 0);
    bus.write_long(sp + 6, 0);
    bus.write_long(sp + 10, spec_ptr);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 14);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
    assert!(disp.vfs.contains_key("Escape Velocity Prefs"));
    assert!(disp.vfs_rsrc.contains_key("Escape Velocity Prefs"));
    assert_eq!(bus.read_word(0x0A60), 0);

    // FSpOpenResFile stack: [permission(2)] [spec_ptr(4)] [result(2)]
    bus.write_word(sp, 1);
    bus.write_long(sp + 2, spec_ptr);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 6);
    let refnum = bus.read_word(new_sp);
    assert_eq!(refnum % 94, 2, "resource refnum is an HFS FCB offset");
    assert_eq!(cpu.read_reg(Register::D0), refnum as u32);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), refnum);
}

#[test]
fn hlfs_dispatch_fspcreateresfile_honors_parent_dir_id() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pilots_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1/Pilots");
    let spec_ptr = 0x300000u32;
    write_fsspec(
        &mut bus,
        spec_ptr,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        pilots_dir_id,
        b"Rick Hardslab",
    );

    let sp = TEST_SP;
    bus.write_word(sp, 0);
    bus.write_long(sp + 2, u32::from_be_bytes(*b"PIL "));
    bus.write_long(sp + 6, u32::from_be_bytes(*b"EVO!"));
    bus.write_long(sp + 10, spec_ptr);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 14);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
    assert_eq!(bus.read_word(0x0A60) as i16, 0);
    assert!(!disp.vfs.contains_key("Rick Hardslab"));
    assert!(!disp.vfs_rsrc.contains_key("Rick Hardslab"));
    assert!(disp
        .vfs
        .contains_key("EV Override 1.0.1/Pilots/Rick Hardslab"));
    assert!(disp
        .vfs_rsrc
        .contains_key("EV Override 1.0.1/Pilots/Rick Hardslab"));
    let metadata = disp
        .vfs_file_metadata("EV Override 1.0.1/Pilots/Rick Hardslab")
        .expect("created pilot metadata");
    assert_eq!(metadata.parent_dir_id, pilots_dir_id);
    assert_eq!(metadata.file_type, u32::from_be_bytes(*b"PIL "));
    assert_eq!(metadata.creator, u32::from_be_bytes(*b"EVO!"));
}

#[test]
fn hlfs_dispatch_fspopenresfile_dedup_keeps_current_resource_file() {
    // More Macintosh Toolbox 1993, p. 1-63: reopening an already-open
    // resource fork returns the existing refnum without making that file
    // current. This regression catches the dedup path mutating CurResFile.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs_rsrc.insert("First".to_string(), vec![]);
    disp.vfs_rsrc.insert("Second".to_string(), vec![]);

    let first_spec = 0x300080u32;
    let second_spec = 0x3000A0u32;
    write_fsspec(&mut bus, first_spec, 1, 2, b"First");
    write_fsspec(&mut bus, second_spec, 1, 2, b"Second");

    let sp = TEST_SP;
    bus.write_word(sp, 1);
    bus.write_long(sp + 2, first_spec);
    bus.write_word(sp + 6, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
    let first_ref = bus.read_word(TEST_SP + 6);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), first_ref);

    bus.write_word(sp, 1);
    bus.write_long(sp + 2, second_spec);
    bus.write_word(sp + 6, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
    let second_ref = bus.read_word(TEST_SP + 6);
    assert_ne!(second_ref, first_ref);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), second_ref);

    bus.write_word(sp, 1);
    bus.write_long(sp + 2, first_spec);
    bus.write_word(sp + 6, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);
    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(TEST_SP + 6), first_ref);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), second_ref);
}

#[test]
fn openresfile_dedup_keeps_current_resource_file() {
    // IM:Volume I p. I-115: OpenResFile reuses an already-open resource
    // fork without switching the current resource file.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs_rsrc.insert("First".to_string(), vec![]);
    disp.vfs_rsrc.insert("Second".to_string(), vec![]);

    let first_name = 0x3000C0u32;
    let second_name = 0x3000E0u32;
    write_pstring(&mut bus, first_name, b"First");
    write_pstring(&mut bus, second_name, b"Second");

    let sp = TEST_SP;

    bus.write_long(sp, first_name);
    bus.write_word(sp + 4, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
    let first_ref = bus.read_word(TEST_SP + 4);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), first_ref);

    bus.write_long(sp, second_name);
    bus.write_word(sp + 4, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
    let second_ref = bus.read_word(TEST_SP + 4);
    assert_ne!(second_ref, first_ref);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), second_ref);

    bus.write_long(sp, first_name);
    bus.write_word(sp + 4, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    call_trap_word(&mut disp, 0xA997, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(TEST_SP + 4), first_ref);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_word(0x0A60), 0);
    assert_eq!(bus.read_word(0x0A5A), second_ref);
}

#[test]
fn hlfs_dispatch_fspopenresfile_data_only_file_returns_resfnotfound() {
    // More Macintosh Toolbox 1993, p. 1-58: FSpOpenResFile opens an
    // existing resource fork. If the data fork exists but no resource fork
    // has been created, Systemless should not synthesize one on open; the
    // current selector contract reports failure via -1 plus resFNotFound.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("DataOnly".to_string(), b"DATA".to_vec());

    let spec_ptr = 0x300040u32;
    write_fsspec(&mut bus, spec_ptr, 0, 0, b"DataOnly");

    let sp = TEST_SP;
    bus.write_word(sp, 1); // fsRdPerm
    bus.write_long(sp + 2, spec_ptr);
    bus.write_word(sp + 6, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(bus.read_word(TEST_SP + 6), (-1i16) as u16);
    assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
    assert_eq!(bus.read_word(0x0A60), (-193i16) as u16);
    assert!(
        !disp.vfs_rsrc.contains_key("DataOnly"),
        "missing resource fork should not be synthesized by FSpOpenResFile"
    );
}

#[test]
fn hlfs_dispatch_fspopenresfile_missing_file_returns_fnferr() {
    // IM:Volume VI 13-20: FSpOpenResFile shares HOpenResFile result
    // codes, including fnfErr (-43) when the file itself is missing.
    let (mut disp, mut cpu, mut bus) = setup();

    let spec_ptr = 0x300060u32;
    write_fsspec(&mut bus, spec_ptr, 0, 0, b"TotallyMissing");

    let sp = TEST_SP;
    bus.write_word(sp, 1); // fsRdPerm
    bus.write_long(sp + 2, spec_ptr);
    bus.write_word(sp + 6, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 13);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    assert_eq!(bus.read_word(TEST_SP + 6), (-1i16) as u16);
    assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
    assert_eq!(bus.read_word(0x0A60), (-43i16) as u16);
}

// ================================================================
// 13e. HighLevelFSDispatch (0x252) selector 6 — FSpDelete
// ================================================================
#[test]
fn hlfs_dispatch_fspdelete() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Pre-populate VFS with the file
    disp.vfs.insert("DelMe.txt".to_string(), vec![1, 2, 3]);
    disp.vfs_rsrc.insert("DelMe.txt".to_string(), vec![4, 5, 6]);

    let spec_ptr = 0x300000u32;
    write_fsspec(&mut bus, spec_ptr, 1, 2, b"DelMe.txt");

    let sp = TEST_SP;
    bus.write_long(sp, spec_ptr);

    cpu.write_reg(Register::D0, 6); // selector = FSpDelete

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP + 4);
    assert!(
        !disp.vfs.contains_key("DelMe.txt"),
        "file should be removed from VFS"
    );
    assert!(
        !disp.vfs_rsrc.contains_key("DelMe.txt"),
        "resource fork should be removed from VFS"
    );
}

// ================================================================
// 13f. HighLevelFSDispatch (0x252) selector 15 — FSpExchangeFiles
// ================================================================
#[test]
fn hlfs_dispatch_fspexchangefiles_swaps_forks_and_preserves_file_identity() {
    // Inside Macintosh: Files (1992), pp. 2-165--2-166: both forks and
    // modification dates follow the data, while file IDs, names, parent
    // directories, creation dates, and Finder information remain with
    // their catalogue entry.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Player".to_string(), vec![0x11, 0x12]);
    disp.vfs_rsrc.insert("Player".to_string(), vec![0x13, 0x14]);
    disp.vfs
        .insert("Player SysTwi temp".to_string(), vec![0x21, 0x22, 0x23]);
    disp.vfs_rsrc
        .insert("Player SysTwi temp".to_string(), vec![0x24]);
    disp.set_vfs_entry_finfo(
        "Player",
        u32::from_be_bytes(*b"OLD "),
        u32::from_be_bytes(*b"GAME"),
        0x0100,
    );
    disp.set_vfs_entry_finfo(
        "Player SysTwi temp",
        u32::from_be_bytes(*b"NEW "),
        u32::from_be_bytes(*b"TEMP"),
        0x0200,
    );
    disp.vfs_metadata.update("Player", |metadata| {
        metadata.created_date = 100;
        metadata.modified_date = 110;
    });
    disp.vfs_metadata.update("Player SysTwi temp", |metadata| {
        metadata.created_date = 200;
        metadata.modified_date = 220;
    });
    let player_before = disp.vfs_file_metadata("Player").unwrap();
    let temp_before = disp.vfs_file_metadata("Player SysTwi temp").unwrap();

    // An open access path remains attached to the Player catalogue entry;
    // its next read observes the newly exchanged contents.
    disp.open_files.insert(128, "Player".to_string());
    disp.file_positions.insert(128, 1);

    let source_spec = 0x300000u32;
    let dest_spec = 0x300080u32;
    write_fsspec(&mut bus, source_spec, 1, 2, b"Player SysTwi temp");
    write_fsspec(&mut bus, dest_spec, 1, 2, b"Player");

    let sp = TEST_SP;
    bus.write_long(sp, dest_spec);
    bus.write_long(sp + 4, source_spec);
    bus.write_word(sp + 8, 0xBEEF);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 15);

    call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(bus.read_word(TEST_SP + 8) as i16, 0);
    assert_eq!(disp.vfs.get("Player").unwrap(), &vec![0x21, 0x22, 0x23]);
    assert_eq!(disp.vfs_rsrc.get("Player").unwrap(), &vec![0x24]);
    assert_eq!(
        disp.vfs.get("Player SysTwi temp").unwrap(),
        &vec![0x11, 0x12]
    );
    assert_eq!(
        disp.vfs_rsrc.get("Player SysTwi temp").unwrap(),
        &vec![0x13, 0x14]
    );

    let player_after = disp.vfs_file_metadata("Player").unwrap();
    let temp_after = disp.vfs_file_metadata("Player SysTwi temp").unwrap();
    assert_eq!(player_after.file_id, player_before.file_id);
    assert_eq!(player_after.parent_dir_id, player_before.parent_dir_id);
    assert_eq!(player_after.created_date, player_before.created_date);
    assert_eq!(player_after.modified_date, temp_before.modified_date);
    assert_eq!(player_after.file_type, player_before.file_type);
    assert_eq!(player_after.creator, player_before.creator);
    assert_eq!(player_after.finder_flags, player_before.finder_flags);
    assert_eq!(temp_after.file_id, temp_before.file_id);
    assert_eq!(temp_after.created_date, temp_before.created_date);
    assert_eq!(temp_after.modified_date, player_before.modified_date);
    assert_eq!(temp_after.file_type, temp_before.file_type);
    assert_eq!(temp_after.creator, temp_before.creator);
    assert_eq!(temp_after.finder_flags, temp_before.finder_flags);
    assert_eq!(
        disp.open_files.get(&128).map(String::as_str),
        Some("Player")
    );
    assert_eq!(disp.file_positions.get(&128), Some(&1));
}

#[test]
fn hlfs_dispatch_fspexchangefiles_reports_missing_and_same_files() {
    // Files 1992, p. 2-166 documents fnfErr and afpSameObjectErr for
    // missing operands and two FSSpecs identifying the same file.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("Player".to_string(), vec![0x11]);

    let player_spec = 0x300000u32;
    let missing_spec = 0x300080u32;
    write_fsspec(&mut bus, player_spec, 1, 2, b"Player");
    write_fsspec(&mut bus, missing_spec, 1, 2, b"Missing");

    for (source_spec, dest_spec, expected) in [
        (player_spec, missing_spec, -43i16),
        (player_spec, player_spec, -5038i16),
    ] {
        bus.write_long(TEST_SP, dest_spec);
        bus.write_long(TEST_SP + 4, source_spec);
        bus.write_word(TEST_SP + 8, 0xBEEF);
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 15);

        call(&mut disp, true, 0x252, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, expected);
    }
}

// ================================================================
// 14a. Gestalt (0xAD) — sysv
// ================================================================
#[test]
fn gestalt_vers() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"vers"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_reports_materialized_trap_table_bases() {
    let (mut disp, mut cpu, mut bus) = setup();

    for (selector, expected) in [
        (*b"ostt", crate::trap::dispatch::OS_TRAP_TABLE_BASE),
        (*b"tbtt", crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE),
    ] {
        cpu.write_reg(Register::D0, u32::from_be_bytes(selector));
        call_trap_word(&mut disp, 0xA1AD, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), expected);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }
}

#[test]
fn gestalt_sysv() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"sysv"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A0),
        crate::machine_profile::REFERENCE_MACHINE_PROFILE.system_version_bcd as u32
    );
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_sys_architecture_reports_68k() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"sysa"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

// ================================================================
// 14b. Gestalt (0xAD) — cput
// Per IM:Operating System Utilities 1994 line 1439 / 2299:
//   gestaltCPU68040 = $004 under the 'cput' selector.
// ================================================================
#[test]
fn gestalt_cput() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"cput"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 4);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_addressing_attributes_follow_current_mmu_mode() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"addr"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 0b111);

    disp.mmu_mode = 0;
    bus.set_addressing_32_bit(false);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"addr"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 0b110);
}

#[test]
fn gestalt_logical_memory_selectors_match_68040_profile() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"lram"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        cpu.read_reg(Register::A0),
        crate::machine_profile::REFERENCE_MACHINE_PROFILE.ram_size_bytes
    );
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"pgsz"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 4096);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_display_manager_selectors_match_basilisk753() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"dplv"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 0x0002_0006);
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"dply"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 0x0000_0007);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_quicktime_reports_final_numversion() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"qtim"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), QUICKTIME_NUM_VERSION_6_0_FINAL);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!((cpu.read_reg(Register::A0) >> 8) & 0xFF, 0x80);
}

#[test]
fn gestalt_drag_manager_absent_without_error() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0x000F);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"drag"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_speech_manager_absent_without_error() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"ttsc"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_textedit_selectors_are_known_classic_system7_values() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"te  "));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 5);
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"teat"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_thread_manager_reports_supported_critical_sections() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"thds"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "bit 0 should report Thread Manager present"
    );
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn newgestalt_rejects_builtin_thread_manager_selector() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0x0040_0000);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"thds"));

    call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_EA50);
}

#[test]
fn gestalt_resource_manager_reports_partial_resource_support() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"rsrc"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_ppc_toolbox_reports_present() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"ppc "));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_keyboard_type_reports_extended_adb_keyboard() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"kbd "));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 4);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_common_classic_environment_selectors_do_not_error() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"edtn"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"scr#"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), 1);
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"hdwr"));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        cpu.read_reg(Register::A0) & ((1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 7)),
        0x9B
    );
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_file_system_attributes_report_fsspec_and_extended_dispatch() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xBEEF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"fs  "));
    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A0), (1 << 0) | (1 << 1));
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_quickdraw_version_reports_system7_32bit_qd13() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"qd  "));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0x0230);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn gestalt_absent_screen_saver_extension_clears_response_register() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0xFFFF_FFFF);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"SAVR"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_EA51);
}

// ================================================================
// 14c. Gestalt (0xAD) — unknown selector
// ================================================================
#[test]
fn gestalt_unknown() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"zzzz"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0xFFFFEA51u32);
}

#[test]
fn gestalt_aux_absent_clears_response_register() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0x0753);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"a/ux"));

    call(&mut disp, false, 0xAD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0xFFFFEA51u32);
}

#[test]
fn newgestalt_rejects_builtin_keyboard_selector() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0x0040_0000);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"kbd "));

    call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_EA50);
}

#[test]
fn newgestalt_accepts_unregistered_screen_saver_selector() {
    let (mut disp, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0x0040_0000);
    cpu.write_reg(Register::D0, u32::from_be_bytes(*b"SAVR"));

    call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
}

/// Dispatches _NewGestalt with five distinct fictional selectors via
/// `call_trap_word(0xA3AD)` and witnesses that A7 is preserved across
/// each call. Per IM:OSUtils 1994 p. 1-32 NewGestalt is an OS-bit
/// FUNCTION with A0+D0 entry registers and D0 exit register; no Pascal
/// stack frame is consumed.
#[test]
fn newgestalt_register_only_calling_convention_preserves_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let fictional_fn_ptr = 0x0040_0000u32; // outside built-in zone

    for &selector_bytes in &[b"kxN1", b"kxN2", b"kxN3", b"kxN4", b"kxN5"] {
        cpu.write_reg(Register::A0, fictional_fn_ptr);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*selector_bytes));
        call_trap_word(&mut disp, 0xA3AD, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "A7 preserved across NewGestalt({:?})",
            std::str::from_utf8(selector_bytes).unwrap()
        );
    }
}

/// Dispatches _ReplaceGestalt with five distinct fictional unknown
/// selectors via `call_trap_word(0xA5AD)` (all hit the
/// gestaltUndefSelectorErr path per IM:OSUtils 1994 p. 1-35) and
/// witnesses that A7 is preserved across each call. The bare trap
/// word is register-only ABI; the MPW FOURWORDINLINE glue's A1
/// push/pop is balanced and is not part of the trap itself.
#[test]
fn replacegestalt_register_only_calling_convention_preserves_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let fictional_fn_ptr = 0x0040_0000u32;

    for &selector_bytes in &[b"kxR1", b"kxR2", b"kxR3", b"kxR4", b"kxR5"] {
        cpu.write_reg(Register::A0, fictional_fn_ptr);
        cpu.write_reg(Register::D0, u32::from_be_bytes(*selector_bytes));
        call_trap_word(&mut disp, 0xA5AD, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "A7 preserved across ReplaceGestalt({:?})",
            std::str::from_utf8(selector_bytes).unwrap()
        );
    }
}

#[test]
fn gestalt_mutators_keep_their_operation_when_the_a0_return_bit_changes() {
    // Inside Macintosh: Operating System Utilities (1994),
    // pp. 1-31--1-36 defines the three slot-$AD operations. Chapter 8,
    // pp. 8-10--8-14 defines bit 8 independently as the dispatcher's A0
    // preservation choice. UI 3.4 Gestalt.h lines 55--105 declares the
    // ordinary $A3AD/$A5AD words; clearing bit 8 must not turn either
    // operation into a Gestalt query.
    let (mut disp, mut cpu, mut bus) = setup();
    let selector = u32::from_be_bytes(*b"A0op");
    let first_fn = 0x0040_1000;
    let replacement_fn = 0x0040_2000;

    cpu.write_reg(Register::D0, selector);
    cpu.write_reg(Register::A0, first_fn);
    call_trap_word(&mut disp, 0xA2AD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(disp.gestalt_registry.get(&selector), Some(&first_fn));

    cpu.write_reg(Register::D0, selector);
    cpu.write_reg(Register::A0, replacement_fn);
    call_trap_word(&mut disp, 0xA4AD, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(disp.gestalt_registry.get(&selector), Some(&replacement_fn));
    assert_eq!(
        cpu.read_reg(Register::A0),
        replacement_fn,
        "cleared bit 8 makes the dispatcher restore the caller's A0"
    );
}

// ================================================================
// Helper: set up a param block at pb_addr with a name pointer
// ================================================================
fn setup_param_block(
    bus: &mut crate::memory::MacMemoryBus,
    cpu: &mut impl CpuOps,
    pb_addr: u32,
    filename: &[u8],
) -> u32 {
    let name_addr = pb_addr + 0x100; // put name just past pb
    write_pstring(bus, name_addr, filename);
    bus.write_long(pb_addr + 18, name_addr); // ioNamePtr
    cpu.write_reg(Register::A0, pb_addr);
    name_addr
}

fn mount_read_only_test_volume(disp: &mut super::super::TrapDispatcher, name: &str) -> (i16, u32) {
    let volume_ref = disp.mount_vfs_volume(name, 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
    let root_dir_id = disp
        .vfs_volume_for_ref_num(volume_ref)
        .expect("mounted volume")
        .root_dir_id;
    (volume_ref, root_dir_id)
}

fn setup_cat_search(
    bus: &mut crate::memory::MacMemoryBus,
    cpu: &mut impl CpuOps,
    volume_ref: i16,
    requested_count: u32,
) -> (u32, u32, u32, u32) {
    let pb = 0x300000u32;
    let matches = 0x310000u32;
    let info1 = 0x320000u32;
    let info2 = 0x320100u32;
    bus.fill_bytes(pb, 128, 0);
    bus.fill_bytes(matches, 70 * 8, 0xA5);
    bus.fill_bytes(info1, 128, 0);
    bus.fill_bytes(info2, 128, 0);
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 24, matches);
    bus.write_long(pb + 28, requested_count);
    bus.write_long(pb + 40, info1);
    bus.write_long(pb + 44, info2);
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0x18);
    (pb, matches, info1, info2)
}

fn call_cat_search(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> i16 {
    cpu.write_reg(Register::D0, 0x18);
    call(disp, false, 0x60, cpu, bus).unwrap();
    cpu.read_reg(Register::D0) as i16
}

#[test]
fn pbcatsearch_empty_catalog_returns_eof_without_matches() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, _) = mount_read_only_test_volume(&mut disp, "Empty Disk");
    // Exclude the volume root itself by requiring a file attribute.
    let (pb, _, info1, info2) = setup_cat_search(&mut bus, &mut cpu, volume_ref, 4);
    bus.write_long(pb + 36, 4);
    bus.write_byte(info1 + 30, 0);
    bus.write_byte(info2 + 30, 0x10);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 0);
    assert_ne!(bus.read_long(pb + 52), 0);
}

#[test]
fn pbcatsearch_honors_zero_requested_matches_and_rejects_unknown_volume() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("Alpha".to_string(), vec![1]);
    let (pb, matches, _, _) = setup_cat_search(&mut bus, &mut cpu, -1, 0);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), 0);
    assert_eq!(bus.read_long(pb + 32), 0);
    assert_eq!(bus.read_long(pb + 52), 0);
    assert_eq!(bus.read_byte(matches), 0xA5);

    bus.write_word(pb + 22, (-999i16) as u16);
    bus.write_long(pb + 28, 1);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -35);
    assert_eq!(bus.read_word(pb + 16) as i16, -35);
    assert_eq!(bus.read_long(pb + 32), 0);
}

#[test]
fn pbcatsearch_selects_named_volume_and_scopes_results() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, _) = mount_read_only_test_volume(&mut disp, "Archive Disk");
    disp.vfs.insert("Archive Disk/Needle".to_string(), vec![1]);
    disp.set_vfs_entry_metadata("Archive Disk/Needle", *b"TEXT", *b"TEST", 0);
    disp.vfs.insert("Needle".to_string(), vec![2]);
    disp.set_vfs_entry_metadata("Needle", *b"TEXT", *b"BOOT", 0);
    let (pb, matches, info1, info2) = setup_cat_search(&mut bus, &mut cpu, 0, 2);
    let volume_name = 0x320200u32;
    let target_name = 0x320240u32;
    write_pstring(&mut bus, volume_name, b"archive disk");
    write_pstring(&mut bus, target_name, b"needle");
    bus.write_long(pb + 18, volume_name);
    bus.write_long(pb + 36, 2 | 4);
    bus.write_long(info1 + 18, target_name);
    bus.write_byte(info2 + 30, 0x10);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 1);
    assert_eq!(bus.read_word(matches) as i16, volume_ref);
    assert_eq!(bus.read_pstring(matches + 6), b"Needle");
}

#[test]
fn pbcatsearch_paginates_without_duplicates_and_reports_final_eof() {
    let (mut disp, mut cpu, mut bus) = setup();
    for name in ["Alpha", "Beta", "Gamma"] {
        disp.vfs.insert(name.to_string(), vec![1]);
        disp.set_vfs_entry_metadata(name, *b"TEXT", *b"TEST", 0);
    }
    let (pb, matches, _info1, info2) = setup_cat_search(&mut bus, &mut cpu, -1, 2);
    bus.write_long(pb + 36, 4);
    bus.write_byte(info2 + 30, 0x10);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), 0);
    assert_eq!(bus.read_long(pb + 32), 2);
    assert_eq!(bus.read_pstring(matches + 6), b"Alpha");
    assert_eq!(bus.read_pstring(matches + 76), b"Beta");
    let resume = bus.read_long(pb + 56);
    assert!(resume > 0);

    bus.fill_bytes(matches, 140, 0);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 1);
    assert_eq!(bus.read_pstring(matches + 6), b"Gamma");
    assert!(bus.read_long(pb + 56) > resume);

    bus.fill_bytes(matches, 70, 0xA5);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 0);
    assert_eq!(bus.read_byte(matches), 0xA5);
}

#[test]
fn pbcatsearch_applies_file_and_directory_specific_criteria() {
    let (mut disp, mut cpu, mut bus) = setup();
    let folder_id = disp.ensure_vfs_directory("Folder");
    disp.vfs
        .insert("Folder/Document".to_string(), vec![1, 2, 3]);
    disp.vfs_rsrc
        .insert("Folder/Document".to_string(), vec![4, 5]);
    disp.set_vfs_entry_metadata("Folder/Document", *b"TEXT", *b"TEST", 0x0040);
    let metadata = disp.vfs_file_metadata("Folder/Document").unwrap();
    let (pb, matches, info1, info2) = setup_cat_search(&mut bus, &mut cpu, -1, 4);
    let target_name = 0x320200u32;
    write_pstring(&mut bus, target_name, b"document");
    bus.write_long(info1 + 18, target_name);
    bus.write_long(pb + 36, 2 | 4 | 8 | 32 | 128 | 512 | 1024 | 8192);
    bus.write_byte(info2 + 30, 0x10);
    bus.write_long(info1 + 32, u32::from_be_bytes(*b"TEXT"));
    bus.write_long(info2 + 32, u32::MAX);
    bus.write_long(info1 + 36, u32::from_be_bytes(*b"TEST"));
    bus.write_long(info2 + 36, u32::MAX);
    bus.write_word(info1 + 40, 0x0040);
    bus.write_word(info2 + 40, 0xFFFF);
    bus.write_long(info1 + 54, 3);
    bus.write_long(info2 + 54, 3);
    bus.write_long(info1 + 64, 2);
    bus.write_long(info2 + 64, 2);
    bus.write_long(info1 + 72, metadata.created_date);
    bus.write_long(info2 + 72, metadata.created_date);
    bus.write_long(info1 + 76, metadata.modified_date);
    bus.write_long(info2 + 76, metadata.modified_date);
    bus.write_long(info1 + 100, folder_id);
    bus.write_long(info2 + 100, folder_id);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 1);
    assert_eq!(bus.read_pstring(matches + 6), b"Document");

    bus.fill_bytes(matches, 280, 0);
    bus.write_long(pb + 52, 0);
    write_pstring(&mut bus, target_name, b"folder");
    bus.write_long(pb + 36, 2 | 4 | 16);
    bus.write_byte(info1 + 30, 0x10);
    bus.write_byte(info2 + 30, 0x10);
    bus.write_word(info1 + 52, 1);
    bus.write_word(info2 + 52, 1);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 1);
    assert_eq!(bus.read_pstring(matches + 6), b"Folder");

    bus.write_long(pb + 52, 0);
    bus.write_long(pb + 36, 4 | 32);
    bus.write_byte(info1 + 30, 0x10);
    bus.write_byte(info2 + 30, 0x10);
    bus.write_long(info1 + 54, 0);
    bus.write_long(info2 + 54, u32::MAX);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(
        bus.read_long(pb + 32),
        0,
        "file-only length excludes folders"
    );
}

#[test]
fn pbcatsearch_honors_partial_name_locked_attribute_and_negation() {
    let (mut disp, mut cpu, mut bus) = setup();
    for name in ["Locked Report", "Open Report", "Notes"] {
        disp.vfs.insert(name.to_string(), vec![1]);
        disp.set_vfs_entry_metadata(name, *b"TEXT", *b"TEST", 0);
    }
    disp.locked_files.insert("Locked Report".to_string());
    let (pb, matches, info1, info2) = setup_cat_search(&mut bus, &mut cpu, -1, 8);
    let target_name = 0x320200u32;
    write_pstring(&mut bus, target_name, b"report");
    bus.write_long(info1 + 18, target_name);
    bus.write_long(pb + 36, 1 | 4);
    bus.write_byte(info1 + 30, 1);
    bus.write_byte(info2 + 30, 0x11);

    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    assert_eq!(bus.read_long(pb + 32), 1);
    assert_eq!(bus.read_pstring(matches + 6), b"Locked Report");

    bus.fill_bytes(matches, 280, 0);
    bus.write_long(pb + 52, 0);
    bus.write_long(pb + 36, 1 | 4 | 16384);
    assert_eq!(call_cat_search(&mut disp, &mut cpu, &mut bus), -39);
    let names = (0..bus.read_long(pb + 32))
        .map(|index| bus.read_pstring(matches + index * 70 + 6))
        .collect::<Vec<_>>();
    assert!(names.contains(&b"Open Report".to_vec()));
    assert!(names.contains(&b"Notes".to_vec()));
    assert!(!names.contains(&b"Locked Report".to_vec()));
}

// ================================================================
// 15. PBOpen (0x00)
// ================================================================
#[test]
fn pb_open() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("TestFile".to_string(), vec![1, 2, 3, 4, 5]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0, "D0 should be noErr");
    let refnum = bus.read_word(pb + 24);
    assert_eq!(refnum % 94, 2, "HFS refnum must index an FCB");
    let fcb_buffer = bus.read_long(crate::memory::globals::addr::FCB_S_PTR);
    let fcb = fcb_buffer + u32::from(refnum);
    let vcb = bus.read_long(fcb + 20);
    assert_ne!(vcb, 0);
    assert_eq!(bus.read_word(vcb + 78) as i16, -1);
    assert_eq!(bus.read_long(fcb + 8), 5);
    assert_eq!(bus.read_word(fcb + 4) & 0x0300, 0x0100);
    assert!(disp.open_files.contains_key(&refnum));
    assert!(
        disp.write_refnums.contains(&refnum),
        "fsCurPerm should grant write access when it is available"
    );
}

#[test]
fn pb_open_reuses_fcb_after_close() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("TestFile".to_string(), vec![1, 2, 3]);
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
    let refnum = bus.read_word(pb + 24);
    let fcb_buffer = bus.read_long(crate::memory::globals::addr::FCB_S_PTR);
    assert_ne!(bus.read_long(fcb_buffer + u32::from(refnum) + 20), 0);

    call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(fcb_buffer + u32::from(refnum) + 20), 0);

    setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");
    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(pb + 24), refnum);
}

#[test]
fn pbopen_permission_is_visible_through_pbgetfcbinfo_write_flag() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Writable Stack".to_string(), vec![1, 2, 3, 4]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Writable Stack");
    bus.write_byte(pb + 27, 3); // fsRdWrPerm

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

    let refnum = bus.read_word(pb + 24);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(disp.write_refnums.contains(&refnum));

    bus.write_word(pb + 24, refnum);
    bus.write_word(pb + 28, 0);
    cpu.write_reg(Register::D0, 8); // PBGetFCBInfo
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        bus.read_word(pb + 36) & 0x0100,
        0x0100,
        "ioFCBFlags bit 8 reports that the data fork is writable"
    );
}

#[test]
fn pbopen_read_permission_keeps_pbgetfcbinfo_write_flag_clear() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Read Only Stack".to_string(), vec![1, 2, 3, 4]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Read Only Stack");
    bus.write_byte(pb + 27, 1); // fsRdPerm

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

    let refnum = bus.read_word(pb + 24);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(!disp.write_refnums.contains(&refnum));

    bus.write_word(pb + 24, refnum);
    bus.write_word(pb + 28, 0);
    cpu.write_reg(Register::D0, 8); // PBGetFCBInfo
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        bus.read_word(pb + 36) & 0x0100,
        0,
        "ioFCBFlags bit 8 stays clear for a read-only access path"
    );

    let write_buffer = 0x310000;
    bus.write_word(pb + 24, refnum);
    bus.write_long(pb + 32, write_buffer);
    bus.write_long(pb + 36, 1);
    bus.write_byte(write_buffer, 9);
    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -61);
    assert_eq!(bus.read_word(pb + 16) as i16, -61);
    assert_eq!(bus.read_long(pb + 40), 0);
    assert_eq!(disp.vfs.get("Read Only Stack").unwrap(), &[1, 2, 3, 4]);
}

#[test]
fn pbopen_synthetic_driver_refnum_supports_read_write_close() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b".ain");

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    let refnum = bus.read_word(pb + 24);
    assert!(disp.synthetic_drivers.contains_key(&refnum));

    let buffer = 0x310000u32;
    bus.write_word(pb + 24, refnum);
    bus.write_long(pb + 32, buffer);
    bus.write_long(pb + 36, 8);
    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 40), 0);

    bus.write_long(pb + 36, 4);
    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 40), 4);

    call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(!disp.synthetic_drivers.contains_key(&refnum));
}

#[test]
fn pbopen_does_not_advertise_unavailable_appletalk_drivers() {
    for driver_name in [b".mpp".as_slice(), b".AtP".as_slice(), b".xPp".as_slice()] {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, driver_name);
        bus.write_word(pb + 16, 0x7FFF);
        bus.write_word(pb + 24, 0x7FFF);

        call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-26i32) as u32);
        assert_eq!(bus.read_word(pb + 16), (-26i16) as u16);
        assert_eq!(bus.read_word(pb + 24), 0);
        assert!(disp.synthetic_drivers.is_empty());
    }
}

// ================================================================
// 15b. PBOpen — file not found
// ================================================================
#[test]
fn pb_open_not_found() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFile");

    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::D0),
        (-43i32) as u32,
        "D0 should be fnfErr"
    );
    assert_eq!(bus.read_word(pb + 16), (-43i16) as u16);
    assert_eq!(bus.read_word(pb + 24), 0);
}

// ================================================================
// 16. FSRead (0x02)
// ================================================================
#[test]
fn fs_read() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Insert file and open it
    disp.vfs
        .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
    disp.open_files.insert(100, "ReadMe".to_string());
    disp.file_positions.insert(100, 0);

    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100); // ioRefNum
    bus.write_long(pb + 32, read_buf); // ioBuffer
    bus.write_long(pb + 36, 3); // ioReqCount = 3
    bus.write_word(pb + 44, 0); // ioPosMode = fsAtMark
    bus.write_long(pb + 46, 0); // ioPosOffset

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 40), 3, "ioActCount should be 3");
    assert_eq!(bus.read_byte(read_buf), 10);
    assert_eq!(bus.read_byte(read_buf + 1), 20);
    assert_eq!(bus.read_byte(read_buf + 2), 30);
}

#[test]
fn pb_read_async_queues_completion_and_leaves_ioresult_in_progress() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    let completion_addr = 0x320000u32;

    disp.vfs
        .insert("AsyncData".to_string(), vec![10, 20, 30, 40]);
    disp.open_files.insert(100, "AsyncData".to_string());
    disp.file_positions.insert(100, 0);
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 12, completion_addr);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 4);
    bus.write_word(pb + 44, 0);
    bus.write_long(pb + 46, 0);

    call_trap_word(&mut disp, 0xA402, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0, "queueing returns noErr");
    assert_eq!(bus.read_word(pb + 16), 1, "ioResult stays in progress");
    assert_eq!(bus.read_long(pb + 40), 4);
    assert_eq!(bus.read_bytes(read_buf, 4), vec![10, 20, 30, 40]);
    assert_eq!(
        disp.pending_file_completions.pop_front(),
        Some(crate::process_context::PendingFileCompletion {
            parameter_block: pb,
            completion_addr,
            result: 0,
        })
    );
}

#[test]
fn pb_read_sync_clears_completion_and_returns_final_result_directly() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;

    disp.vfs.insert("Empty".to_string(), Vec::new());
    disp.open_files.insert(100, "Empty".to_string());
    disp.file_positions.insert(100, 0);
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 12, 0x320000);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, 0x310000);
    bus.write_long(pb + 36, 1);
    bus.write_word(pb + 44, 0);
    bus.write_long(pb + 46, 0);

    call_trap_word(&mut disp, 0xA002, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
    assert_eq!(bus.read_word(pb + 16) as i16, -39);
    assert_eq!(bus.read_long(pb + 12), 0);
    assert!(disp.pending_file_completions.is_empty());
}

#[test]
fn pb_read_immediate_completes_without_queueing() {
    // Inside Macintosh: Devices (1994), p. 1-16 and UI 3.4 Devices.h
    // lines 985--996: $A202 is PBReadImmed, not the async $A402 form.
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let read_buf = 0x310000u32;

    disp.vfs.insert("Immediate".to_string(), vec![1, 2, 3]);
    disp.open_files.insert(100, "Immediate".to_string());
    disp.file_positions.insert(100, 0);
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 12, 0x320000);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 3);
    bus.write_word(pb + 44, 0);
    bus.write_long(pb + 46, 0);

    call_trap_word(&mut disp, 0xA202, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 12), 0);
    assert_eq!(bus.read_bytes(read_buf, 3), vec![1, 2, 3]);
    assert!(disp.pending_file_completions.is_empty());
}

#[test]
fn fs_read_before_start_returns_poserr_and_keeps_mark() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
    disp.open_files.insert(100, "ReadMe".to_string());
    disp.file_positions.insert(100, 2);

    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 1);
    bus.write_long(pb + 40, 0xDEADBEEF);
    bus.write_word(pb + 44, 1); // fsFromStart
    bus.write_long(pb + 46, (-1i32) as u32);
    bus.write_byte(read_buf, 0xCC);

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
    assert_eq!(bus.read_word(pb + 16) as i16, -40);
    assert_eq!(bus.read_long(pb + 40), 0);
    assert_eq!(bus.read_long(pb + 46), 2);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 2);
    assert_eq!(bus.read_byte(read_buf), 0xCC);
}

#[test]
fn pb_read_newline_mode_returns_delimiter_and_preserves_following_data() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Lines".to_string(), b"first\rsecond\r".to_vec());
    disp.open_files.insert(100, "Lines".to_string());
    disp.file_positions.insert(100, 0);

    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 32);
    bus.write_word(pb + 44, 0x0D80); // Return-delimited newline mode, fsAtMark.
    bus.write_long(pb + 46, 0);

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 40), 6);
    assert_eq!(bus.read_long(pb + 46), 6);
    assert_eq!(bus.read_bytes(read_buf, 6), b"first\r".to_vec());
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 6);

    bus.write_long(pb + 32, read_buf + 16);
    bus.write_long(pb + 36, 32);

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 40), 7);
    assert_eq!(bus.read_long(pb + 46), 13);
    assert_eq!(bus.read_bytes(read_buf + 16, 7), b"second\r".to_vec());
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 13);
}

#[test]
fn pb_read_newline_mode_returns_eof_without_delimiter() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Tail".to_string(), b"tail".to_vec());
    disp.open_files.insert(100, "Tail".to_string());
    disp.file_positions.insert(100, 0);

    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 16);
    bus.write_word(pb + 44, 0x0D80); // Return-delimited newline mode, fsAtMark.
    bus.write_long(pb + 46, 0);

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
    assert_eq!(bus.read_word(pb + 16) as i16, -39);
    assert_eq!(bus.read_long(pb + 40), 4);
    assert_eq!(bus.read_long(pb + 46), 4);
    assert_eq!(bus.read_bytes(read_buf, 4), b"tail".to_vec());
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
}

#[test]
fn pb_read_uses_low_position_bits_when_flags_are_set() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("ReadMe".to_string(), vec![10, 20, 30, 40, 50]);
    disp.open_files.insert(100, "ReadMe".to_string());
    disp.file_positions.insert(100, 0);

    let pb = 0x300000u32;
    let read_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, read_buf);
    bus.write_long(pb + 36, 2);
    bus.write_word(pb + 44, 0x0051); // cache + rdVerify flags, fsFromStart.
    bus.write_long(pb + 46, 2);

    call(&mut disp, false, 0x02, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 40), 2);
    assert_eq!(bus.read_long(pb + 46), 4);
    assert_eq!(bus.read_bytes(read_buf, 2), vec![30, 40]);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
}

// ================================================================
// 17. FSWrite (0x03)
// ================================================================
#[test]
fn fs_write() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("WriteMe".to_string(), Vec::new());
    disp.open_files.insert(100, "WriteMe".to_string());
    disp.write_refnums.insert(100);
    disp.file_positions.insert(100, 0);

    let pb = 0x300000u32;
    let write_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, write_buf);
    bus.write_long(pb + 36, 4); // ioReqCount = 4

    // Write data to the buffer
    bus.write_byte(write_buf, 0xAA);
    bus.write_byte(write_buf + 1, 0xBB);
    bus.write_byte(write_buf + 2, 0xCC);
    bus.write_byte(write_buf + 3, 0xDD);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 40), 4);
    let file_data = disp.vfs.get("WriteMe").unwrap();
    assert_eq!(file_data, &[0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn fs_write_dispatches_signed_sound_driver_refnum_to_audio() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let synth = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, (-4i16) as u16); // .Sound driver
    bus.write_long(pb + 32, synth);
    bus.write_long(pb + 36, 5_024);
    bus.write_byte(crate::memory::globals::addr::SD_VOLUME, 7);

    bus.write_word(synth, 0); // ffMode
    bus.write_long(synth + 2, 0x0000_8000); // half-rate sampling factor
    for offset in 0..5_018u32 {
        bus.write_byte(synth + 6 + offset, (offset & 0xFF) as u8);
    }

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 40), 5_024);
    assert_eq!(disp.sound_manager.channels.len(), 1);
    let mixed = disp.sound_manager.mix_frame(512);
    assert!(!mixed.is_empty());
    assert!(mixed.iter().any(|sample| *sample != 0x80));
}

#[test]
fn fs_write_routes_unknown_negative_refnum_through_device_manager() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, (-7i16) as u16);
    bus.write_long(pb + 32, 0x310000);
    bus.write_long(pb + 36, 4);
    bus.write_long(pb + 40, 0xFFFF_FFFF);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-21i32) as u32); // badUnitErr
    assert_eq!(bus.read_word(pb + 16), (-21i16) as u16);
    assert_eq!(bus.read_long(pb + 40), 0);
    assert!(disp.sound_manager.channels.is_empty());
}

#[test]
fn fs_write_from_start_overwrites() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("WriteMe".to_string(), vec![0x11, 0x22, 0x33, 0x44]);
    disp.open_files.insert(100, "WriteMe".to_string());
    disp.write_refnums.insert(100);
    disp.file_positions.insert(100, 4);

    let pb = 0x300000u32;
    let write_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, write_buf);
    bus.write_long(pb + 36, 2); // ioReqCount = 2
    bus.write_word(pb + 44, 1); // ioPosMode = fsFromStart
    bus.write_long(pb + 46, 1); // ioPosOffset = 1

    bus.write_byte(write_buf, 0xAA);
    bus.write_byte(write_buf + 1, 0xBB);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 40), 2);
    assert_eq!(
        bus.read_long(pb + 46),
        3,
        "ioPosOffset should advance to new mark"
    );
    let file_data = disp.vfs.get("WriteMe").unwrap();
    assert_eq!(file_data, &[0x11, 0xAA, 0xBB, 0x44]);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 3);
}

#[test]
fn fs_write_before_start_returns_poserr_and_keeps_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("WriteMe".to_string(), vec![0x11, 0x22, 0x33, 0x44]);
    disp.open_files.insert(100, "WriteMe".to_string());
    disp.write_refnums.insert(100);
    disp.file_positions.insert(100, 1);

    let pb = 0x300000u32;
    let write_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_long(pb + 32, write_buf);
    bus.write_long(pb + 36, 1);
    bus.write_word(pb + 44, 3); // fsFromMark
    bus.write_long(pb + 46, (-2i32) as u32);
    bus.write_byte(write_buf, 0xAA);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
    assert_eq!(bus.read_word(pb + 16) as i16, -40);
    assert_eq!(bus.read_long(pb + 40), 0);
    assert_eq!(bus.read_long(pb + 46), 1);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 1);
    assert_eq!(disp.vfs.get("WriteMe").unwrap(), &[0x11, 0x22, 0x33, 0x44]);
}

// PBOpenRF stores the rsrc fork bytes in self.vfs under the
// "__rsrc__<name>" key so FSRead/FSWrite share the data-fork code
// path. Writes through that key must mirror back into self.vfs_rsrc —
// otherwise a later OpenRFPerm/PBOpenRF reads the stale snapshot.
#[test]
fn fs_write_to_rsrc_fork_mirrors_to_vfs_rsrc() {
    let (mut disp, mut cpu, mut bus) = setup();

    let rsrc_key = "__rsrc__InstallerTemp".to_string();
    disp.vfs.insert(rsrc_key.clone(), Vec::new());
    disp.vfs_rsrc
        .insert("InstallerTemp".to_string(), Vec::new());
    disp.open_files.insert(101, rsrc_key);
    disp.write_refnums.insert(101);
    disp.file_positions.insert(101, 0);

    let pb = 0x300000u32;
    let write_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 101);
    bus.write_long(pb + 32, write_buf);
    bus.write_long(pb + 36, 4);

    bus.write_bytes(write_buf, &[0xDE, 0xAD, 0xBE, 0xEF]);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        disp.vfs_rsrc.get("InstallerTemp").unwrap(),
        &vec![0xDE, 0xAD, 0xBE, 0xEF],
        "FSWrite to a __rsrc__ open key must mirror back to \
             vfs_rsrc so a later OpenRFPerm sees the new bytes"
    );
}

#[test]
fn fs_write_invalid_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let write_buf = 0x310000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 999); // invalid refnum
    bus.write_long(pb + 32, write_buf);
    bus.write_long(pb + 36, 2);

    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-51i32) as u32);
    assert_eq!(bus.read_word(pb + 16), (-51i16) as u16);
    assert_eq!(bus.read_long(pb + 40), 0);
}

// ================================================================
// 18. FSClose (0x01)
// ================================================================
#[test]
fn fs_close() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.open_files.insert(100, "SomeFile".to_string());
    disp.file_positions.insert(100, 42);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100); // ioRefNum

    call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(!disp.open_files.contains_key(&100));
    assert!(!disp.file_positions.contains_key(&100));
}

#[test]
fn fs_close_resource_refnum_releases_resource_map_and_handles() {
    let (mut disp, mut cpu, mut bus) = setup();
    let refnum = 100;
    let data_ptr = bus.alloc(8);
    bus.write_bytes(data_ptr, &[0xCD; 8]);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);

    disp.set_loaded_resources_for_test(super::super::dispatch::LoadedResources {
        files: HashMap::from([
            (0, super::super::dispatch::ResourceFileMap::default()),
            (
                refnum,
                super::super::dispatch::ResourceFileMap {
                    loaded: HashMap::from([((*b"PICT", 23002), data_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::new(),
                    map_attrs: 0,
                },
            ),
        ]),
        names: HashMap::from([(refnum, "BladeData".to_string())]),
        search_order: vec![0, refnum],
        current_file: refnum,
    });
    disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"PICT", 23002));
    disp.insert_resource_handle_file_for_test(handle, refnum);
    disp.track_handle_ptr(data_ptr, handle);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, refnum);

    call(&mut disp, false, 0x01, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(disp.current_resource_refnum(), 0);
    assert!(!disp.resources.as_ref().unwrap().files.contains_key(&refnum));
    assert_eq!(bus.get_alloc_size(data_ptr), None);
    assert_eq!(bus.get_alloc_size(handle), None);
    assert!(!disp.loaded_handles.contains_key(&handle));
    assert!(!disp.resource_handle_files.contains_key(&handle));
    assert_eq!(disp.handle_for_ptr(data_ptr), None);
}

// ================================================================
// 19. PBGetVol (0x14)
// ================================================================
#[test]
fn pb_get_vol() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf); // ioNamePtr

    call(&mut disp, false, 0x14, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
    // Check Pascal string "MacintoshHD"
    let name_bytes = bus.read_pstring(name_buf);
    assert_eq!(name_bytes, b"MacintoshHD");
}

// ================================================================
// 20. PBGetVInfo (0x07)
// ================================================================
#[test]
fn pb_get_vinfo() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
    assert_eq!(bus.read_word(pb + 40), 100, "ioVNmFls");
    assert_eq!(
        bus.read_word(pb + 46),
        super::BOOT_VOLUME_ALLOCATION_BLOCKS,
        "ioVNmAlBlks"
    );
    assert_eq!(
        bus.read_long(pb + 48),
        super::BOOT_VOLUME_ALLOCATION_BLOCK_SIZE,
        "ioVAlBlkSiz"
    );
    assert_eq!(
        bus.read_word(pb + 62),
        super::BOOT_VOLUME_FREE_BLOCKS,
        "ioVFrBlk"
    );
    let free_bytes = u32::from(bus.read_word(pb + 62)) * bus.read_long(pb + 48);
    assert!(
        free_bytes >= 8 * 1024 * 1024,
        "PBGetVInfo should report enough free space for launch-time scratch checks"
    );
    // Volume name
    let len = bus.read_byte(name_buf) as usize;
    assert_eq!(len, 11);
}

#[test]
fn pb_get_vinfo_trap_variants_respect_basic_and_hfs_parameter_block_boundaries() {
    // IM:IV pp. IV-129..IV-130: PBGetVInfo receives the 64-byte
    // VolumeParam record, while PBHGetVInfo receives HVolumeParam and may
    // populate the four HFS fields that follow it. The async bit changes
    // completion routing, not which parameter-block layout the call uses.
    for (trap_word, is_hfs) in [
        (0xA007, false),
        (0xA407, false),
        (0xA207, true),
        (0xA607, true),
    ] {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        let name_buf = 0x300100u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_buf);
        bus.write_long(pb + 64, 0xDEAD_BEEF);
        bus.write_long(pb + 68, 0xCAFE_BABE);

        call_trap_word(&mut disp, trap_word, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0, "trap ${trap_word:04X}");
        if is_hfs {
            assert_eq!(bus.read_word(pb + 64), 0x4244, "ioVSigWord");
            assert_eq!(bus.read_word(pb + 66), 0, "ioVDrvInfo");
            assert_eq!(bus.read_word(pb + 68), 0, "ioVDRefNum");
            assert_eq!(bus.read_word(pb + 70), 0, "ioVFSID");
        } else {
            assert_eq!(bus.read_long(pb + 64), 0xDEAD_BEEF);
            assert_eq!(bus.read_long(pb + 68), 0xCAFE_BABE);
        }
    }
}

#[test]
fn pb_hget_vinfo_positive_index_one_returns_boot_volume() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_word(pb + 22, 0x1234);
    bus.write_word(pb + 28, 1); // ioVolIndex

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0, "ioResult");
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
    assert_eq!(bus.read_pstring(name_buf), b"MacintoshHD");
}

#[test]
fn pb_hget_vinfo_index_after_last_volume_returns_nsv_err_without_outputs() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_word(pb + 22, 0x1234);
    bus.write_word(pb + 28, 2); // ioVolIndex
    bus.write_long(pb + 30, 0xCAFE_BABE);
    bus.write_pstring(name_buf, b"unchanged");

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-35i32) as u32);
    assert_eq!(bus.read_word(pb + 16), (-35i16) as u16, "ioResult");
    assert_eq!(bus.read_word(pb + 22), 0x1234, "ioVRefNum");
    assert_eq!(bus.read_long(pb + 30), 0xCAFE_BABE, "ioVCrDate");
    assert_eq!(bus.read_pstring(name_buf), b"unchanged");
}

#[test]
fn pb_hget_vinfo_zero_index_selects_named_read_only_volume() {
    let (mut disp, mut cpu, mut bus) = setup();
    let volume_ref = disp.mount_vfs_volume(
        "Legend CD",
        0x0080,
        4,
        1024,
        512,
        512,
        900,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"Legend CD");
    bus.write_word(pb + 22, 0);
    bus.write_word(pb + 28, 0); // ioVolIndex: lookup by name

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 22), volume_ref as u16);
    assert_eq!(bus.read_pstring(name_buf), b"Legend CD");
    assert_eq!(bus.read_word(pb + 38), 0x0080, "ioVAtrb");
}

#[test]
fn pb_hget_vinfo_negative_index_selects_volume_from_full_path() {
    // Inside Macintosh: Files (1992), 2-145: negative ioVolIndex uses
    // ioNamePtr and ioVRefNum in the standard way. A full pathname names
    // its volume before the first colon.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"MacintoshHD:Games:Demo");
    bus.write_word(pb + 22, 0);
    bus.write_word(pb + 28, (-1i16) as u16);

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0, "ioResult");
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
    assert_eq!(bus.read_pstring(name_buf), b"MacintoshHD");
}

#[test]
fn pb_hget_vinfo_keeps_relative_paths_on_the_default_volume() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b":data:title.phd");
    bus.write_word(pb + 22, 0);
    bus.write_word(pb + 28, (-1i16) as u16);

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0, "ioResult");
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
    assert_eq!(bus.read_pstring(name_buf), b"MacintoshHD");
}

#[test]
fn pb_hget_vinfo_negative_index_resolves_working_directory_reference() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, 0);
    bus.write_word(pb + 22, *disp.app_wd_refnum as u16);
    bus.write_word(pb + 28, (-1i16) as u16);

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0, "ioResult");
    assert_eq!(
        bus.read_word(pb + 22),
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
        "ioVRefNum"
    );
}

#[test]
fn pb_hget_vinfo_enumerates_mounted_volumes_in_stable_reference_order() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (first_ref, _) = mount_read_only_test_volume(&mut disp, "First Disk");
    let (second_ref, _) = mount_read_only_test_volume(&mut disp, "Second Disk");
    assert_eq!(first_ref, -2);
    assert_eq!(second_ref, -3);
    assert_eq!(
        mount_read_only_test_volume(&mut disp, "first disk").0,
        first_ref,
        "mounting the same volume name is idempotent"
    );

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);

    for (index, expected_ref, expected_name) in [
        (
            1,
            super::super::dispatch::BOOT_VOLUME_REF_NUM,
            b"MacintoshHD".as_slice(),
        ),
        (2, first_ref, b"First Disk".as_slice()),
        (3, second_ref, b"Second Disk".as_slice()),
    ] {
        bus.write_word(pb + 28, index);
        call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert_eq!(bus.read_word(pb + 22), expected_ref as u16);
        assert_eq!(bus.read_pstring(name_buf), expected_name);
    }

    bus.write_word(pb + 28, 4);
    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "nsvErr");
}

#[test]
fn pb_hget_vinfo_zero_index_selects_reference_and_rejects_unknown_volumes() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, _) = mount_read_only_test_volume(&mut disp, "Reference Disk");
    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_word(pb + 28, 0);

    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 22), volume_ref as u16);
    assert_eq!(bus.read_pstring(name_buf), b"Reference Disk");

    bus.write_pstring(name_buf, b"stale output buffer");
    bus.write_word(pb + 22, volume_ref as u16);
    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_pstring(name_buf), b"Reference Disk");

    bus.write_pstring(name_buf, b"Missing Disk");
    bus.write_word(pb + 22, 0);
    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "unknown name");

    bus.write_pstring(name_buf, b"");
    bus.write_word(pb + 22, (-999i16) as u16);
    call(&mut disp, false, 0x07, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "unknown reference");
}

// ================================================================
// 21. PBGetEOF (0x11)
// ================================================================
#[test]
fn pb_get_eof() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("EOFFile".to_string(), vec![0; 256]);
    disp.open_files.insert(100, "EOFFile".to_string());

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);

    call(&mut disp, false, 0x11, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 28), 256, "ioMisc should be file size");
}

// ================================================================
// 22. PBSetFPos (0x44)
// ================================================================
#[test]
fn pb_set_fpos() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Open a file first
    disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"TestFile");
    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
    let refnum = bus.read_word(pb + 24);

    // Set position to offset 42 from start (posMode=1)
    bus.write_word(pb + 24, refnum);
    bus.write_word(pb + 44, 1); // fsFromStart
    bus.write_long(pb + 46, 42);
    cpu.write_reg(Register::A0, pb);
    call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(*disp.file_positions.get(&refnum).unwrap(), 42);
}

#[test]
fn pb_set_fpos_before_start_returns_poserr_and_keeps_mark() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
    disp.open_files.insert(100, "TestFile".to_string());
    disp.file_positions.insert(100, 4);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_word(pb + 44, 3); // fsFromMark
    bus.write_long(pb + 46, (-5i32) as u32);

    call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -40);
    assert_eq!(bus.read_word(pb + 16) as i16, -40);
    assert_eq!(bus.read_long(pb + 46), 4);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 4);
}

#[test]
fn pb_set_fpos_past_eof_clamps_to_eof_and_returns_eoferr() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("TestFile".to_string(), vec![0u8; 256]);
    disp.open_files.insert(100, "TestFile".to_string());
    disp.file_positions.insert(100, 4);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);
    bus.write_word(pb + 44, 1); // fsFromStart
    bus.write_long(pb + 46, 300);

    call(&mut disp, false, 0x44, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -39);
    assert_eq!(bus.read_word(pb + 16) as i16, -39);
    assert_eq!(bus.read_long(pb + 46), 256);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 256);
}

// ================================================================
// 23. PBFlushFile (0x45) — validates refnum
// Inside Macintosh Volume II, II-114. rfNumErr (-51) for unknown refnum.
// ================================================================
#[test]
fn pb_flush_file_unknown_refnum_returns_rfnumerr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x260900u32;
    for i in 0u32..32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 24, 9999); // ioRefNum — not open
    cpu.write_reg(Register::A0, pb);
    call(&mut disp, false, 0x45, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -51);
    assert_eq!(bus.read_word(pb + 16) as i16, -51);
}

// ================================================================
// 24. PBGetFPos (0x18)
// ================================================================
#[test]
fn pb_get_fpos() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.file_positions.insert(100, 42);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100);

    call(&mut disp, false, 0x18, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(pb + 46), 42, "ioPosOffset should be 42");
    assert_eq!(bus.read_word(pb + 44), 0, "ioPosMode should be 0");
}

// PBGetFPos must return rfNumErr for unknown refnums per IM:II-117.
#[test]
fn pb_get_fpos_unknown_refnum_returns_rfnumerr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x260A00u32;
    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 24, 9999); // ioRefNum — not open
    cpu.write_reg(Register::A0, pb);
    call(&mut disp, false, 0x18, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -51);
    assert_eq!(bus.read_word(pb + 16) as i16, -51);
}

// ================================================================
// 25. PBFlushVol (0x13)
// ================================================================
#[test]
fn pb_flush_vol() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);

    call(&mut disp, false, 0x13, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
}

#[test]
fn pb_flush_vol_unknown_refnum_returns_nsverr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, (-999i16) as u16);

    call(&mut disp, false, 0x13, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -35);
    assert_eq!(bus.read_word(pb + 16) as i16, -35);
}

// ================================================================
// 26. PBCreate (0x08)
// ================================================================
#[test]
fn pb_create() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

    call(&mut disp, false, 0x08, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert!(disp.vfs.contains_key("Pilot 1"));
    assert_eq!(disp.vfs.get("Pilot 1").unwrap().len(), 0);
}

#[test]
fn pb_create_uses_working_directory_refnum_and_ignores_hfs_dirid_bytes() {
    // Files 1992, 2-89: legacy PBCreate takes a ParamBlockRec, whose
    // ioVRefNum may be a WDRefNum. The ioDirID field exists only in the
    // HParamBlockRec used by PBHCreate ($A208), so bytes at offset 48
    // must not redirect a legacy $A008 create.
    let (mut disp, mut cpu, mut bus) = setup();
    let pilots_dir_id = disp.ensure_vfs_directory("Pilots");
    let unrelated_dir_id = disp.ensure_vfs_directory("Unrelated");
    let wd_ref = disp
        .open_working_directory(
            super::super::dispatch::BOOT_VOLUME_REF_NUM,
            pilots_dir_id,
            0,
        )
        .expect("working directory");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Untitled");
    bus.write_word(pb + 22, wd_ref as u16);
    bus.write_long(pb + 48, unrelated_dir_id);

    call_trap_word(&mut disp, 0xA008, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert!(disp.vfs.contains_key("Pilots/Untitled"));
    assert!(!disp.vfs.contains_key("Unrelated/Untitled"));

    bus.write_word(pb + 24, 0);
    bus.write_long(pb + 48, unrelated_dir_id);
    cpu.write_reg(Register::D0, 26);
    call_trap_word(&mut disp, 0xA060, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        disp.open_files.get(&bus.read_word(pb + 24)),
        Some(&"Pilots/Untitled".to_string())
    );
}

#[test]
fn open_df_hfs_bit_alone_selects_the_parameter_block_layout() {
    // Inside Macintosh Volume IV, IV-120: hfsBit (bit 9) tells the File
    // Manager that the parameter block contains the HFS-only ioDirID
    // field. asyncBit (bit 10) changes completion mode, not record shape.
    // Exercise all four spellings with offset 48 deliberately pointing at
    // a valid unrelated directory so reading it for PBOpenDF is observable.
    for (trap_word, uses_hfs_parameter_block) in [
        (0xA060u16, false),
        (0xA260, true),
        (0xA460, false),
        (0xA660, true),
    ] {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("Target".to_string(), vec![1, 2, 3]);
        let unrelated_dir_id = disp.ensure_vfs_directory("Unrelated");

        let pb = 0x300000u32;
        setup_param_block(&mut bus, &mut cpu, pb, b"Target");
        bus.write_long(pb + 12, 0xDEAD_BEEF); // ioCompletion poison
        bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
        bus.write_word(pb + 24, 0x7777); // ioRefNum poison
        bus.write_byte(pb + 27, 3); // fsRdWrPerm
        bus.write_long(pb + 48, unrelated_dir_id);
        cpu.write_reg(Register::D0, 26);
        let a0_before = cpu.read_reg(Register::A0);
        let sp_before = cpu.read_reg(Register::A7);

        call_trap_word(&mut disp, trap_word, &mut cpu, &mut bus).unwrap();

        assert_eq!(disp.current_trap_word, trap_word);
        assert_eq!(cpu.read_reg(Register::A0), a0_before);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_long(pb + 12), 0xDEAD_BEEF);
        if uses_hfs_parameter_block {
            assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "${trap_word:04X}");
            assert_eq!(bus.read_word(pb + 16) as i16, -43, "${trap_word:04X}");
            assert_eq!(bus.read_word(pb + 24), 0, "${trap_word:04X}");
        } else {
            assert_eq!(cpu.read_reg(Register::D0), 0, "${trap_word:04X}");
            assert_eq!(bus.read_word(pb + 16), 0, "${trap_word:04X}");
            let refnum = bus.read_word(pb + 24);
            assert_eq!(
                disp.open_files.get(&refnum),
                Some(&"Target".to_string()),
                "${trap_word:04X}"
            );
            assert!(disp.write_refnums.contains(&refnum), "${trap_word:04X}");
        }
    }
}

// Real Mac files always have both forks; PBCreate must seed an empty
// rsrc fork so the next PBOpenRF returns noErr instead of fnfErr.
// Files 1992, 1-58.
#[test]
fn pb_create_seeds_empty_rsrc_fork() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Installer Temp");

    call_trap_word(&mut disp, 0xA008, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(disp.vfs.contains_key("Installer Temp"));
    assert!(
        disp.vfs_rsrc.contains_key("Installer Temp"),
        "PBCreate must seed an empty resource fork so PBOpenRF \
             on the new file returns noErr"
    );
    assert_eq!(disp.vfs_rsrc.get("Installer Temp").unwrap().len(), 0);
}

#[test]
fn pbh_create_async_setfinfo_then_open_honors_parent_dir_id() {
    let (mut disp, mut cpu, mut bus) = setup();
    let temp_dir_id = disp.ensure_vfs_directory("Temporary Items");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"lcache00.tmp");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, temp_dir_id);

    call_trap_word(&mut disp, 0xA608, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert!(disp.vfs.contains_key("Temporary Items/lcache00.tmp"));
    assert!(!disp.vfs.contains_key("lcache00.tmp"));

    bus.write_long(pb + 32, u32::from_be_bytes(*b"TEXT"));
    bus.write_long(pb + 36, u32::from_be_bytes(*b"ttxt"));
    bus.write_word(pb + 40, 0x0400);
    call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    let metadata = disp
        .vfs_file_metadata("Temporary Items/lcache00.tmp")
        .expect("temp file metadata");
    assert_eq!(metadata.file_type, u32::from_be_bytes(*b"TEXT"));
    assert_eq!(metadata.creator, u32::from_be_bytes(*b"ttxt"));
    assert_eq!(metadata.finder_flags, 0x0400);

    cpu.write_reg(Register::D0, 26);
    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    let refnum = bus.read_word(pb + 24);
    assert!(refnum >= 100);
    assert_eq!(
        disp.open_files.get(&refnum),
        Some(&"Temporary Items/lcache00.tmp".to_string())
    );
}

#[test]
fn pbh_create_preserves_slash_as_a_literal_filename_character() {
    // HFS reserves colon as the pathname separator and permits every
    // other character, including slash, in file and directory names.
    // Files 1992, 2-27 to 2-29.
    let (mut disp, mut cpu, mut bus) = setup();
    let parent_dir_id = disp.ensure_vfs_directory("Game Folder");

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Object IDs/Level Info");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, parent_dir_id);

    call_trap_word(&mut disp, 0xA208, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert!(
        disp.directory_id_for_vfs_path("Game Folder/Object IDs")
            .is_none(),
        "a literal slash must not synthesize a nested directory"
    );

    call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Object IDs/Level Info");

    bus.write_long(pb + 48, parent_dir_id);
    bus.write_word(pb + 28, 0);
    cpu.write_reg(Register::D0, 9);
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Object IDs/Level Info");
}

#[test]
fn pbh_open_data_fork_finds_literal_slash_in_explicit_directory() {
    // Files 1992, pp. 2-27 to 2-29, 2-185 to 2-186: slash is legal
    // in an HFS filename, and ioDirID selects its parent directory.
    let (mut disp, mut cpu, mut bus) = setup();
    let target_dir_id = disp.ensure_vfs_directory("Game Folder");
    let other_dir_id = disp.ensure_vfs_directory("Other Folder");
    let filename = "Object IDs/Level Info";
    let encoded = super::super::TrapDispatcher::encode_hfs_component_for_vfs(filename);
    let target_path = format!("Game Folder/{encoded}");
    disp.vfs.insert(target_path.clone(), vec![1, 2, 3]);
    disp.vfs.insert(format!("Other Folder/{encoded}"), vec![9]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, filename.as_bytes());
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_byte(pb + 27, 1); // fsRdPerm
    bus.write_long(pb + 48, target_dir_id);
    call_trap_word(&mut disp, 0xA200, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(
        disp.open_files.get(&bus.read_word(pb + 24)),
        Some(&target_path)
    );

    bus.write_long(pb + 48, other_dir_id);
    call_trap_word(&mut disp, 0xA200, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(
        disp.open_files.get(&bus.read_word(pb + 24)),
        Some(&format!("Other Folder/{encoded}"))
    );
}

#[test]
fn pb_create_truncate_on_exists_preserves_rsrc_fork() {
    // Per IM Files 1992, 2-89, PBCreate returns dupFNErr when a
    // file with the matching name already exists. Some shareware
    // titles (e.g. Meteor Storm's "MS UserKey" marker) ship that
    // file inside their install folder yet still call HCreate at
    // launch without an intervening HDelete and treat any error
    // as fatal. Systemless models the .sit-extracted folder
    // directly, so the marker is always pre-existing on first
    // launch — we truncate the data fork and report noErr to keep
    // these titles bootable, but PRESERVE the resource fork
    // because some titles bake registration templates into it
    // and read them back via FSpOpenResFile + Get1Resource.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Pilot 1".to_string(), vec![1, 2, 3]);
    disp.vfs_rsrc
        .insert("Pilot 1".to_string(), vec![9, 9, 9, 9]);
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

    call(&mut disp, false, 0x08, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0, "noErr from truncate path");
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(
        disp.vfs.get("Pilot 1").unwrap(),
        &Vec::<u8>::new(),
        "data fork must be empty after truncate"
    );
    assert_eq!(
        disp.vfs_rsrc.get("Pilot 1").unwrap(),
        &vec![9, 9, 9, 9],
        "resource fork must be preserved across truncate"
    );
}

#[test]
fn hfs_dispatch_generated_routes_preserve_exact_d0_selectors() {
    assert_eq!(super::HFS_DISPATCH_OPERATION_ROUTES.len(), 3);
    assert!(super::HFS_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (0x003F, "PBGetVolMountInfoSize"),
        (0x0040, "PBGetVolMountInfo"),
        (0x0041, "PBVolumeMount"),
    ] {
        let route =
            super::hfs_dispatch_operation_route(0xA260, selector).expect("HFSDispatch route");
        assert_eq!(route.routine_name, routine_name);
        assert_eq!(
            route.operation_id,
            format!("selector-operation:_HFSDispatch:0x{selector:04X}:d0-moveq-immediate:8")
        );
    }

    for (trap_word, selector) in [
        (0xA060, 0x003F),
        (0xA660, 0x003F),
        (0xA360, 0x0040),
        (0xA260, 0x003E),
        (0xA260, 0x0042),
        (0xA260, 0x0001_003F),
        (0xA260, u32::MAX),
    ] {
        assert!(super::hfs_dispatch_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn hfs_dispatch_records_known_then_clears_nonidentity_without_changing_behavior() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFF);
    bus.write_long(pb + 32, 0xCAFE_BABE);
    let sp = cpu.read_reg(Register::A7);

    cpu.write_reg(Register::D0, 0x0040);
    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        disp.current_selector_operation,
        Some("selector-operation:_HFSDispatch:0x0040:d0-moveq-immediate:8")
    );
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(cpu.read_reg(Register::A0), pb);
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 32), 0xCAFE_BABE);

    disp.current_selector_operation = Some("stale-identity");
    bus.write_word(pb + 16, 0x3FFF);
    cpu.write_reg(Register::D0, 0x0001_0040);
    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.current_selector_operation, None);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);

    for trap_word in [0xA060, 0xA660] {
        disp.current_selector_operation = Some("stale-identity");
        bus.write_word(pb + 16, 0x3FFF);
        cpu.write_reg(Register::D0, 0x0040);
        call_trap_word(&mut disp, trap_word, &mut cpu, &mut bus).unwrap();
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pb + 16), 0);
    }
}

// ================================================================
// 26b. FSDispatch ($A260) selector 6 — PBDirCreate
// ================================================================
#[test]
fn fsdispatch_pbdircreate_creates_child_directory_and_returns_dirid() {
    // PBDirCreate is PBHCreate for directories: it creates a new
    // directory under ioDirID and returns the new directory ID in ioDirID.
    // Inside Macintosh Volume IV, IV-146.
    let (mut disp, mut cpu, mut bus) = setup();

    let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Sierra");
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, parent_dir_id);
    cpu.write_reg(Register::D0, 6); // HFSDispatch selector: PBDirCreate

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    let created_dir_id = bus.read_long(pb + 48);
    assert_ne!(created_dir_id, parent_dir_id);
    assert_eq!(
        disp.directory_path_for_id(created_dir_id),
        Some("System Folder/Preferences/Sierra")
    );
}

#[test]
fn fsdispatch_pbgetcatinfo_prefers_exact_directory_over_file_fallback() {
    let (mut disp, mut cpu, mut bus) = setup();

    let app_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1");
    let pilots_dir_id = disp.ensure_vfs_directory("EV Override 1.0.1/Pilots");
    disp.vfs_directories.with_mut(|directories| {
        let pilots_directory = directories
            .iter_mut()
            .find(|directory| directory.dir_id == pilots_dir_id)
            .expect("created directory should be canonical");
        pilots_directory.creator = u32::from_be_bytes(*b"TEST");
        pilots_directory.finder_flags = 0x0400;
    });
    disp.vfs
        .insert("EV Override 1.0.1/Pilots/Ben".to_string(), vec![0x42]);

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Pilots");
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, 0); // ioFDirIndex
    bus.write_long(pb + 48, app_dir_id);
    cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Pilots".to_vec());
    assert_eq!(
        bus.read_byte(pb + 30),
        0x10,
        "PBGetCatInfo returned a directory"
    );
    assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"fold"));
    assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"TEST"));
    assert_eq!(bus.read_word(pb + 40), 0x0400);
    assert_eq!(bus.read_long(pb + 48), pilots_dir_id);
    assert_eq!(
        bus.read_word(pb + 52),
        1,
        "ioDrNmFls counts the Ben file in the Pilots directory"
    );
    assert_eq!(bus.read_long(pb + 100), app_dir_id);
}

#[test]
fn fsdispatch_pbgetcatinfo_resolves_full_boot_volume_directory_path() {
    // Files 1992, 2-27 to 2-28: a pathname that starts with a volume name
    // is complete and identifies its target independently of ioDirID.
    let (mut disp, mut cpu, mut bus) = setup();
    let parent_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");
    let sierra_dir_id = disp.ensure_vfs_directory("System Folder/Preferences/Sierra");

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(
        &mut bus,
        &mut cpu,
        pb,
        b"macintoshhd:System Folder:Preferences:Sierra",
    );
    bus.write_word(pb + 16, 0x3FFF);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, 0);
    bus.write_long(pb + 48, 0xFFD5_0000);
    cpu.write_reg(Register::D0, 9);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Sierra");
    assert_eq!(bus.read_byte(pb + 30), 0x10);
    assert_eq!(bus.read_long(pb + 48), sierra_dir_id);
    assert_eq!(bus.read_long(pb + 100), parent_dir_id);
}

#[test]
fn fsdispatch_pbgetcatinfo_empty_name_returns_directory_itself() {
    // Some legacy apps probe the current directory with ioFDirIndex = 0
    // and an empty ioNamePtr. Treat the empty name as the directory
    // selected by ioDirID rather than a missing child.
    let (mut disp, mut cpu, mut bus) = setup();

    let app_dir_id = disp.ensure_vfs_directory("Game Folder");

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"");
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, 0); // ioFDirIndex
    bus.write_long(pb + 48, app_dir_id);
    cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Game Folder".to_vec());
    assert_eq!(
        bus.read_byte(pb + 30),
        0x10,
        "PBGetCatInfo returned a directory"
    );
    assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"fold"));
    assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"MACS"));
    assert_eq!(bus.read_long(pb + 48), app_dir_id);
    assert_eq!(bus.read_long(pb + 100), 2);
}

#[test]
fn fsdispatch_pbgetcatinfo_root_directory_returns_volume_name() {
    // Files 1992, 2-27 and 2-85: the root directory has dirID 2,
    // parent dirID 1, and the same name as its volume.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"poison");
    bus.write_word(pb + 16, 0x3FFF);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, (-1i16) as u16);
    bus.write_long(pb + 48, 2);
    cpu.write_reg(Register::D0, 9);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(
        bus.read_pstring(name_ptr),
        super::super::dispatch::BOOT_VOLUME_NAME.as_bytes()
    );
    assert_eq!(bus.read_byte(pb + 30), 0x10);
    assert_eq!(bus.read_long(pb + 48), 2);
    assert_eq!(bus.read_long(pb + 100), 1);
}

#[test]
fn fsdispatch_pbgetcatinfo_resolves_root_by_parent_id_and_volume_name() {
    // Files 1992, 1-27 and 2-85: the File Manager assigns parent dirID 1
    // to a volume's root so callers can identify it consistently by
    // volume reference number, parent directory ID, and volume name.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(
        &mut bus,
        &mut cpu,
        pb,
        super::super::dispatch::BOOT_VOLUME_NAME.as_bytes(),
    );
    bus.write_word(pb + 16, 0x3FFF);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, 0);
    bus.write_long(pb + 48, 1);
    cpu.write_reg(Register::D0, 9);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(
        bus.read_pstring(name_ptr),
        super::super::dispatch::BOOT_VOLUME_NAME.as_bytes()
    );
    assert_eq!(bus.read_byte(pb + 30), 0x10);
    assert_eq!(bus.read_long(pb + 48), 2);
    assert_eq!(bus.read_long(pb + 100), 1);
}

#[test]
fn fsdispatch_pbgetcatinfo_finds_resource_only_file() {
    let (mut disp, mut cpu, mut bus) = setup();

    let app_dir_id = disp.ensure_vfs_directory("Game Folder");
    disp.vfs_rsrc.insert(
        "Game Folder/Resource Data".to_string(),
        vec![0xCA, 0xFE, 0xBA, 0xBE],
    );
    disp.set_vfs_entry_metadata("Game Folder/Resource Data", *b"rsrc", *b"GAME", 0x0040);

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"Resource Data");
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 28, 0); // ioFDirIndex
    bus.write_long(pb + 48, app_dir_id);
    cpu.write_reg(Register::D0, 9); // HFSDispatch selector: PBGetCatInfo

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Resource Data".to_vec());
    assert_eq!(bus.read_byte(pb + 30), 0, "PBGetCatInfo returned a file");
    assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"rsrc"));
    assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"GAME"));
    assert_eq!(bus.read_word(pb + 40), 0x0040);
    assert!(
        bus.read_long(pb + 48) >= 32,
        "ioDirID should become a file ID"
    );
    assert_eq!(bus.read_long(pb + 54), 0, "data fork length");
    assert_eq!(bus.read_long(pb + 64), 4, "resource fork length");
    assert_eq!(bus.read_long(pb + 100), app_dir_id);
}

#[test]
fn fsdispatch_pbsetcatinfo_updates_file_finder_info_and_dates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let dir_id = disp.ensure_vfs_directory("Installed");
    disp.vfs.insert("Installed/Game".to_string(), Vec::new());
    disp.set_vfs_entry_metadata("Installed/Game", *b"Part", *b"SIT!", 0);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Game");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, dir_id);
    bus.write_long(pb + 32, u32::from_be_bytes(*b"APPL"));
    bus.write_long(pb + 36, u32::from_be_bytes(*b"GAME"));
    bus.write_word(pb + 40, 0x0040);
    bus.write_long(pb + 72, 0x12345678);
    bus.write_long(pb + 76, 0x23456789);
    cpu.write_reg(Register::D0, 10);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    let metadata = disp.vfs_file_metadata("Installed/Game").unwrap();
    assert_eq!(metadata.file_type, u32::from_be_bytes(*b"APPL"));
    assert_eq!(metadata.creator, u32::from_be_bytes(*b"GAME"));
    assert_eq!(metadata.finder_flags, 0x0040);
    assert_eq!(metadata.created_date, 0x12345678);
    assert_eq!(metadata.modified_date, 0x23456789);
}

#[test]
fn fsdispatch_pbsetcatinfo_updates_directory_finder_info() {
    let (mut disp, mut cpu, mut bus) = setup();
    let dir_id = disp.ensure_vfs_directory("Installed");
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, dir_id);
    bus.write_long(pb + 32, u32::from_be_bytes(*b"fold"));
    bus.write_long(pb + 36, u32::from_be_bytes(*b"TEST"));
    bus.write_word(pb + 40, 0x0400);
    cpu.write_reg(Register::D0, 10);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    let directory = disp.directory_entry_for_id(dir_id).unwrap();
    assert_eq!(directory.file_type, u32::from_be_bytes(*b"fold"));
    assert_eq!(directory.creator, u32::from_be_bytes(*b"TEST"));
    assert_eq!(directory.finder_flags, 0x0400);
}

#[test]
fn fsdispatch_pbsetcatinfo_rejects_missing_file() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Missing");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, 2);
    cpu.write_reg(Register::D0, 10);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
    assert_eq!(bus.read_word(pb + 16) as i16, -43);
}

#[test]
fn fsdispatch_pbsetcatinfo_rejects_locked_file_without_mutation() {
    let (mut disp, mut cpu, mut bus) = setup();
    let dir_id = disp.ensure_vfs_directory("Installed");
    disp.vfs.insert("Installed/Game".to_string(), Vec::new());
    disp.set_vfs_entry_metadata("Installed/Game", *b"APPL", *b"GAME", 0);
    disp.locked_files.insert("Installed/Game".to_string());

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Game");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, dir_id);
    bus.write_long(pb + 32, u32::from_be_bytes(*b"TEXT"));
    cpu.write_reg(Register::D0, 10);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -45);
    assert_eq!(bus.read_word(pb + 16) as i16, -45);
    assert_eq!(
        disp.vfs_file_metadata("Installed/Game").unwrap().file_type,
        u32::from_be_bytes(*b"APPL")
    );
}

// ================================================================
// 27. PBDelete (0x09)
// ================================================================
#[test]
fn pb_delete() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Pilot 1".to_string(), vec![1, 2, 3]);
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Pilot 1");

    call(&mut disp, false, 0x09, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert!(!disp.vfs.contains_key("Pilot 1"));
}

#[test]
fn pb_delete_not_found() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"MissingPilot");

    call(&mut disp, false, 0x09, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
    assert_eq!(bus.read_word(pb + 16), (-43i16) as u16);
}

// ================================================================
// 28. PBGetFInfo (0x0C) — file exists
// ================================================================
#[test]
fn pb_get_finfo_found() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("InfoFile".to_string(), vec![1, 2, 3]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"InfoFile");
    bus.write_long(pb + 100, 0xDEAD_BEEF);

    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_long(pb + 100), 0xDEAD_BEEF);
}

#[test]
fn pb_get_finfo_closed_file_preserves_path_for_followup_set_finfo() {
    // Inside Macintosh Volume IV, IV-148–149 recommends calling
    // PBGetFInfo immediately before PBSetFInfo. A closed file does not
    // return an access-path leaf name, so the caller's pathname must
    // survive the get and keep the set aimed at the same file.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.ensure_vfs_directory("PR Resources");
    disp.ensure_vfs_directory("Color Disk 1/Color Playroom");
    disp.vfs
        .insert("PR Resources/PR Settings".to_string(), vec![]);
    disp.vfs.insert(
        "Color Disk 1/Color Playroom/PR Settings".to_string(),
        vec![],
    );
    disp.set_vfs_entry_metadata("PR Resources/PR Settings", *b"pref", *b"PLAY", 0);
    disp.set_vfs_entry_metadata(
        "Color Disk 1/Color Playroom/PR Settings",
        *b"pref",
        *b"DISK",
        0,
    );

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b":PR Resources:PR Settings");
    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        bus.read_pstring(name_ptr),
        b":PR Resources:PR Settings".to_vec()
    );
    assert_eq!(bus.read_word(pb + 24), 0, "closed file has no access path");

    bus.write_long(pb + 36, u32::from_be_bytes(*b"NEW!"));
    call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        disp.vfs_file_metadata("PR Resources/PR Settings")
            .unwrap()
            .creator,
        u32::from_be_bytes(*b"NEW!")
    );
    assert_eq!(
        disp.vfs_file_metadata("Color Disk 1/Color Playroom/PR Settings")
            .unwrap()
            .creator,
        u32::from_be_bytes(*b"DISK")
    );
}

#[test]
fn pb_get_finfo_open_file_returns_leaf_name_and_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.ensure_vfs_directory("PR Resources");
    disp.vfs
        .insert("PR Resources/PR Settings".to_string(), vec![]);
    disp.set_vfs_entry_metadata("PR Resources/PR Settings", *b"pref", *b"PLAY", 0);
    disp.open_files
        .insert(7, "PR Resources/PR Settings".to_string());

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b":PR Resources:PR Settings");
    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_pstring(name_ptr), b"PR Settings".to_vec());
    assert_eq!(bus.read_word(pb + 24), 7);
}

#[test]
fn pb_get_finfo_positive_iofdirindex_enumerates_files_in_working_directory() {
    // Inside Macintosh Volume IV, IV-148: positive ioFDirIndex indexes
    // files on ioVRefNum; when ioVRefNum is a working-directory refnum,
    // it indexes files in that directory.  GetFileInfo indexes files only,
    // unlike GetCatInfo.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Mission/A Launcher".to_string(), vec![]);
    disp.vfs
        .insert("Mission/B Suspended".to_string(), vec![0xAA]);
    disp.set_vfs_entry_metadata("Mission/A Launcher", *b"APPL", *b"WSTL", 0);
    disp.set_vfs_entry_metadata("Mission/B Suspended", *b"SAVE", *b"WSTL", 0);
    disp.ensure_vfs_directory("Mission/AAA Folder");
    disp.vfs
        .insert("Mission/AAA Folder/Inner Save".to_string(), vec![0xBB]);
    disp.set_vfs_entry_metadata("Mission/AAA Folder/Inner Save", *b"SAVE", *b"WSTL", 0);
    disp.set_launched_app_path("Mission/A Launcher");
    let suspended_metadata = disp.vfs_file_metadata("Mission/B Suspended").unwrap();

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output name");
    bus.write_word(pb + 22, *disp.app_wd_refnum as u16);
    bus.write_word(pb + 28, 2); // second file in Mission, not second catalog entry

    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_pstring(name_ptr), b"B Suspended".to_vec());
    assert_eq!(bus.read_long(pb + 32), u32::from_be_bytes(*b"SAVE"));
    assert_eq!(bus.read_long(pb + 36), u32::from_be_bytes(*b"WSTL"));
    assert_eq!(bus.read_long(pb + 48), suspended_metadata.file_id);

    let pb2 = 0x300400u32;
    setup_param_block(&mut bus, &mut cpu, pb2, b"A Launcher");
    bus.write_word(pb2 + 22, *disp.app_wd_refnum as u16);
    bus.write_word(pb2 + 28, 99);

    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
    assert_eq!(bus.read_word(pb2 + 16), (-43i16) as u16);
}

// ================================================================
// 28b. PBGetFInfo (0x0C) — file not found
// ================================================================
#[test]
fn pb_get_finfo_not_found() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFile");

    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
}

// ================================================================
// 28c. PBAllocate ($A010)
// ================================================================
#[test]
fn pballocate_nominal_call_returns_noerr_and_sets_ioactcount() {
    // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
    // PBAllocate reports result via D0/ioResult and writes ioActCount.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_long(pb + 36, 0x1234); // ioReqCount
    bus.write_long(pb + 40, 0xFFFF_FFFF); // ioActCount poison

    call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
    assert_eq!(bus.read_long(pb + 40), 0x1234, "ioActCount mirrors request");
}

#[test]
fn pballocate_writes_ioresult_noerr_when_paramblock_present() {
    // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
    // PBAllocate uses ioResult as the parameter-block mirror of D0.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x7B7B); // ioResult poison
    bus.write_long(pb + 36, 1);

    call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
}

/// Pre-poisons pb.ioResult at pb+16 with a non-noErr, non-OSErr sentinel
/// (0x3FFF), sets ioRefNum to a clearly-bogus 9999, dispatches $A010,
/// asserts the sentinel was overwritten AND D0 == ioResult per the
/// File Manager dispatcher convention (Inside Macintosh Volume II 1985,
/// p. II-114) AND that A7 is preserved across the call.
#[test]
fn pballocate_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison
    bus.write_word(pb + 24, 9999); // ioRefNum bogus
    bus.write_long(pb + 36, 256); // ioReqCount

    let sp_pre = cpu.read_reg(Register::A7);
    call_trap_word(&mut disp, 0xA010, &mut cpu, &mut bus).unwrap();
    let sp_post = cpu.read_reg(Register::A7);

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFF, "ioResult must be overwritten");
    assert_eq!(d0, io_result, "D0 must mirror pb.ioResult");
    assert_eq!(sp_pre, sp_post, "A7 must be preserved");
}

// ================================================================
// 28d. PBAllocContig ($A210)
// ================================================================
#[test]
fn pballoccontig_nominal_call_returns_noerr_and_sets_ioactcount() {
    // Inside Macintosh: Files (1992), pp. 2-130 to 2-131:
    // PBAllocContig reports result via D0/ioResult and writes ioActCount.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_long(pb + 36, 0x1234); // ioReqCount
    bus.write_long(pb + 40, 0xFFFF_FFFF); // ioActCount poison

    call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
    assert_eq!(bus.read_long(pb + 40), 0x1234, "ioActCount mirrors request");
}

#[test]
fn pballoccontig_overwrites_ioresult_field_with_function_result() {
    // Inside Macintosh: Files (1992), p. 2-131 result-code contract:
    // ioResult is the function-result field for PBAllocContig.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x7B7B); // ioResult poison
    bus.write_long(pb + 36, 1);

    call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
}

/// Pre-poisons pb.ioResult at pb+16 with a non-noErr, non-OSErr
/// sentinel (0x3FFF), sets ioRefNum to a clearly-bogus 9999, dispatches
/// $A210, asserts the sentinel was overwritten AND D0 == ioResult per
/// the File Manager dispatcher convention (Inside Macintosh Volume II
/// 1985, p. II-114) AND that A7 is preserved across the call.
#[test]
fn pballoccontig_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison
    bus.write_word(pb + 24, 9999); // ioRefNum bogus
    bus.write_long(pb + 36, 256); // ioReqCount

    let sp_pre = cpu.read_reg(Register::A7);
    call_trap_word(&mut disp, 0xA210, &mut cpu, &mut bus).unwrap();
    let sp_post = cpu.read_reg(Register::A7);

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFF, "ioResult must be overwritten");
    assert_eq!(d0, io_result, "D0 must mirror pb.ioResult");
    assert_eq!(sp_pre, sp_post, "A7 must be preserved");
}

// ================================================================
// 28e. Volume / I/O queue stubs ($A006, $A016, $A017)
// ================================================================
#[test]
fn pbkillio_returns_noerr_when_hle_has_no_async_io_queue() {
    // Inside Macintosh Volume II (1985), p. II-187:
    // PBKillIO reports an OSErr result; HLE succeeds with noErr.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300200u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100); // ioRefNum

    call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
}

#[test]
fn pbkillio_writes_ioresult_noerr_when_paramblock_present() {
    // Inside Macintosh Volume II (1985), p. II-187:
    // ioResult is the returned function result field for PBKillIO.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300240u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x6A6A); // ioResult poison
    bus.write_word(pb + 24, 200); // ioRefNum

    call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
}

#[test]
fn pbkillio_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Pre-poison ioResult with
    // 0x3FFF (not noErr and not a documented OSErr), set ioRefNum to
    // a clearly-bogus value, dispatch _PBKillIO, witness that the
    // sentinel was overwritten AND D0 == ioResult per the Device
    // Manager OS-bit FUNCTION dispatcher convention (IM:II 1985,
    // p. II-114), AND A7 unchanged across the trap.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300280u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
    bus.write_word(pb + 24, 9999u16); // bogus ioRefNum

    let sp_pre = cpu.read_reg(Register::A7);
    call(&mut disp, false, 0x06, &mut cpu, &mut bus).unwrap();
    let sp_post = cpu.read_reg(Register::A7);

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(
        io_result, 0x3FFFi16,
        "ioResult sentinel must be overwritten"
    );
    assert_eq!(d0, io_result, "D0 == ioResult per dispatcher convention");
    assert_eq!(sp_pre, sp_post, "A7 preserved (register-only ABI)");
}

#[test]
fn finitqueue_has_no_parameters_and_preserves_stack_pointer() {
    // Inside Macintosh Volume II (1985), p. II-103:
    // FInitQueue is declared as PROCEDURE FInitQueue with no parameters.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call(&mut disp, false, 0x16, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "HLE returns noErr in D0"
    );
}

#[test]
fn finitqueue_five_call_composition_preserves_stack_and_returns_noerr_each_call() {
    // 5 successive _FInitQueue dispatches inside one StackSpace
    // sandwich. Per-call pop discipline errors accumulate; this
    // pins cumulative drift even when each individual call's
    // drift would be small.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);

    for _ in 0..5 {
        cpu.write_reg(Register::D0, 0xCAFE_F00D);
        call(&mut disp, false, 0x16, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "HLE returns noErr in D0 on each call"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "A7 preserved after 5-call composition"
    );
}

#[test]
fn adddrive_returns_noerr_and_preserves_stack_pointer() {
    // Inside Macintosh: Files (1992), p. 2-236 and Technical Note #108
    // define AddDrive as a register-only call with a noErr return.
    let (mut disp, mut cpu, mut bus) = setup();
    let qel = 0x320400u32;
    cpu.write_reg(Register::A0, qel);
    cpu.write_reg(Register::D0, (7u32 << 16) | 42u32);
    let sp_before = cpu.read_reg(Register::A7);

    call_trap_word(&mut disp, 0xA04E, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
    assert_eq!(cpu.read_reg(Register::A0), qel, "A0 preserved");
}

#[test]
fn adddrive_five_call_composition_preserves_stack_and_returns_noerr_each_call() {
    // 5 successive register-only AddDrive dispatches inside one
    // StackSpace sandwich.
    let (mut disp, mut cpu, mut bus) = setup();
    let qel = 0x320500u32;
    cpu.write_reg(Register::A0, qel);
    let sp_before = cpu.read_reg(Register::A7);

    for _ in 0..5 {
        cpu.write_reg(Register::D0, (11u32 << 16) | 99u32);
        call_trap_word(&mut disp, 0xA04E, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
        assert_eq!(cpu.read_reg(Register::A0), qel, "A0 preserved");
    }

    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

#[test]
fn pboffline_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
    // any documented OSErr), dispatches _PBOffLine with a clearly-
    // bogus ioVRefNum 9999, witnesses that the trap overwrote the
    // sentinel AND that D0 == ioResult (per File Manager dispatcher
    // convention IM:II 1985 p. II-114) AND that A7 is preserved
    // across the call (register-only OS-bit FUNCTION calling
    // convention per IM:Files 1992 pp. 2-141..2-142).
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb = 0x300300u32;
    for i in 0..50u32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, 9999); // ioVRefNum bogus
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call(&mut disp, false, 0x35, &mut cpu, &mut bus).unwrap();

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
    assert_eq!(
        d0, io_result,
        "D0 mirrors ioResult per dispatcher convention"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

#[test]
fn pbeject_returns_noerr_when_hle_has_no_removable_media() {
    // Inside Macintosh: Files (1992), p. 2-141:
    // PBEject reports noErr for successful nominal calls.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300280u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);

    call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr in D0");
}

#[test]
fn pbeject_writes_ioresult_noerr_when_paramblock_present() {
    // Inside Macintosh: Files (1992), p. 2-141:
    // ioResult carries the function result in the parameter block.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x3002C0u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x5151); // ioResult poison
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);

    call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0, "ioResult overwritten");
}

#[test]
fn pbeject_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
    // any documented OSErr), dispatches _PBEject with a clearly-
    // bogus ioVRefNum 9999, witnesses that the trap overwrote the
    // sentinel AND that D0 == ioResult (per File Manager dispatcher
    // convention IM:II 1985 p. II-114) AND that A7 is preserved
    // across the call (register-only OS-bit FUNCTION calling
    // convention per IM:Files 1992 p. 2-141).
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb = 0x300340u32;
    for i in 0..50u32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, 9999); // ioVRefNum bogus
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call(&mut disp, false, 0x17, &mut cpu, &mut bus).unwrap();

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
    assert_eq!(
        d0, io_result,
        "D0 mirrors ioResult per dispatcher convention"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

/// Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor any
/// documented OSErr), dispatches _PBMountVol with a clearly-bogus
/// ioVRefNum=9999 drive number, witnesses that the trap overwrote the
/// sentinel AND that D0 == ioResult (per File Manager dispatcher
/// convention IM:II 1985 p. II-114) AND that A7 is preserved across
/// the call (register-only OS-bit FUNCTION calling convention per
/// IM:Files 1992 p. 2-139).
#[test]
fn pbmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb = 0x300380u32;
    for i in 0..50u32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, 9999); // ioVRefNum bogus drive number
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call_trap_word(&mut disp, 0xA00F, &mut cpu, &mut bus).unwrap();

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
    assert_eq!(
        d0, io_result,
        "D0 mirrors ioResult per dispatcher convention"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

/// Pre-poisons pb.ioResult at pb+16 with 0x3FFF on two separate parameter
/// blocks, dispatches _PBUnmountVol ($A00E) then _PBUnmountVolImmed
/// ($A20E) — which share the same low-byte arm — with a clearly-bogus
/// ioVRefNum 9999, witnesses that each trap overwrote its sentinel AND
/// that D0 == ioResult per File Manager dispatcher convention (IM:II
/// 1985 p. II-114) AND that A7 is preserved across both calls
/// (register-only OS-bit FUNCTION calling convention per IM:Files
/// 1992 p. 2-148).
#[test]
fn pbunmountvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb_a00e = 0x3003C0u32;
    let pb_a20e = 0x300400u32;
    for i in 0..50u32 {
        bus.write_byte(pb_a00e + i, 0);
        bus.write_byte(pb_a20e + i, 0);
    }
    bus.write_word(pb_a00e + 16, 0x3FFF);
    bus.write_word(pb_a00e + 22, 9999);
    bus.write_word(pb_a20e + 16, 0x3FFF);
    bus.write_word(pb_a20e + 22, 9999);

    cpu.write_reg(Register::A0, pb_a00e);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    call_trap_word(&mut disp, 0xA00E, &mut cpu, &mut bus).unwrap();
    let d0_a00e = cpu.read_reg(Register::D0) as i16;
    let io_a00e = bus.read_word(pb_a00e + 16) as i16;
    assert_ne!(io_a00e, 0x3FFFi16, "A00E overwrote ioResult sentinel");
    assert_eq!(d0_a00e, io_a00e, "A00E D0 mirrors ioResult");

    cpu.write_reg(Register::A0, pb_a20e);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    call_trap_word(&mut disp, 0xA20E, &mut cpu, &mut bus).unwrap();
    let d0_a20e = cpu.read_reg(Register::D0) as i16;
    let io_a20e = bus.read_word(pb_a20e + 16) as i16;
    assert_ne!(io_a20e, 0x3FFFi16, "A20E overwrote ioResult sentinel");
    assert_eq!(d0_a20e, io_a20e, "A20E D0 mirrors ioResult");

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "A7 preserved across both A00E and A20E dispatches",
    );
}

/// Pre-poisons pb.ioResult at pb+16 with 0x3FFF, dispatches _PBSetFVers
/// with a bogus ioVRefNum=9999 (no such volume), witnesses that the
/// trap overwrote the sentinel AND that D0 == ioResult per File
/// Manager dispatcher convention (IM:II 1985 p. II-114) AND that A7
/// is preserved across the call (register-only OS-bit FUNCTION
/// calling convention per IM:II 1985 p. II-117). Per IM:IV 1986
/// p. IV-153, PBSetFVers is a documented no-op on hierarchical
/// volumes; Systemless's HFS-only VFS makes the trap always-noErr.
#[test]
fn pbsetfvers_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb = 0x300440u32;
    for i in 0..50u32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_word(pb + 22, 9999); // ioVRefNum bogus
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call_trap_word(&mut disp, 0xA043, &mut cpu, &mut bus).unwrap();

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
    assert_eq!(
        d0, io_result,
        "D0 mirrors ioResult per dispatcher convention"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

/// Pre-poisons pb.ioResult at pb+16 with 0x3FFF, dispatches _PBHRename
/// with bogus old/new filenames guaranteed not to exist in the VFS,
/// witnesses that the trap overwrote the sentinel AND that D0 ==
/// ioResult per File Manager dispatcher convention (IM:II 1985
/// p. II-114) AND that A7 is preserved across the call (register-only
/// OS-bit FUNCTION calling convention per IM:Files 1992 p. 2-118).
#[test]
fn pbhrename_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let pb = 0x300480u32;
    for i in 0..60u32 {
        bus.write_byte(pb + i, 0);
    }
    let old_name_ptr = 0x301000u32;
    let new_name_ptr = 0x301040u32;
    let old_name = b"\x1FNoSuchFile_A20B_PBHRename_OLD";
    let new_name = b"\x1FNoSuchFile_A20B_PBHRename_NEW";
    for (i, &b) in old_name.iter().enumerate() {
        bus.write_byte(old_name_ptr + i as u32, b);
    }
    for (i, &b) in new_name.iter().enumerate() {
        bus.write_byte(new_name_ptr + i as u32, b);
    }
    bus.write_word(pb + 16, 0x3FFF); // ioResult poison
    bus.write_long(pb + 18, old_name_ptr); // ioNamePtr
    bus.write_word(pb + 22, 0); // ioVRefNum default
    bus.write_long(pb + 28, new_name_ptr); // ioMisc -> new name
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    call_trap_word(&mut disp, 0xA20B, &mut cpu, &mut bus).unwrap();

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(io_result, 0x3FFFi16, "trap overwrote ioResult sentinel");
    assert_eq!(
        d0, io_result,
        "D0 mirrors ioResult per dispatcher convention"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before, "A7 preserved");
}

// ================================================================
// 29. PBSetVol (0x15)
// ================================================================
#[test]
fn pb_set_vol() {
    let (mut disp, mut cpu, mut bus) = setup();

    let dir_id = disp.ensure_vfs_directory("Marathon");
    disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
    disp.set_launched_app_path("Marathon/Marathon");

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, *disp.app_wd_refnum as u16);

    call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, dir_id);
    assert_eq!(bus.read_long(addr::CUR_DIR_STORE), dir_id);
    assert_eq!(
        bus.read_word(addr::SF_SAVE_DISK),
        (-super::super::dispatch::BOOT_VOLUME_REF_NUM) as u16
    );
}

#[test]
fn pbsetvol_restores_pbgetvol_working_directory_when_name_is_also_present() {
    // Files 1992, 2-150 to 2-151: PBGetVol returns the working-directory
    // reference established by PBSetVol, and passing that reference back
    // to PBSetVol makes the represented directory the default again.
    let (mut disp, mut cpu, mut bus) = setup();
    let app_dir_id = disp.ensure_vfs_directory("Pinball Demo");
    disp.vfs
        .insert("Pinball Demo/Pinball Demo".to_string(), vec![]);
    disp.set_launched_app_path("Pinball Demo/Pinball Demo");

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);

    call_trap_word(&mut disp, 0xA014, &mut cpu, &mut bus).unwrap();
    let saved_wd_refnum = bus.read_word(pb + 22) as i16;
    assert_ne!(saved_wd_refnum, super::super::dispatch::BOOT_VOLUME_REF_NUM);
    assert_eq!(bus.read_pstring(name_buf), b"MacintoshHD");

    bus.write_long(pb + 18, 0);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    call_trap_word(&mut disp, 0xA015, &mut cpu, &mut bus).unwrap();
    assert_eq!(*disp.default_dir_id, 2);

    bus.write_long(pb + 18, name_buf);
    bus.write_word(pb + 22, saved_wd_refnum as u16);
    call_trap_word(&mut disp, 0xA015, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, app_dir_id);
    assert_eq!(*disp.app_wd_refnum, saved_wd_refnum);
    assert_eq!(bus.read_long(addr::CUR_DIR_STORE), app_dir_id);
}

#[test]
fn pbhsetvol_restores_working_directory_with_saved_volume_name() {
    // PBHSetVol: a working-directory reference supplies the base directory;
    // ioWDDirID is ignored. Inside Macintosh: Files (1992), pp. 2-153–2-154.
    let (mut disp, mut cpu, mut bus) = setup();
    let app_dir_id = disp.ensure_vfs_directory("Game Folder");
    disp.vfs.insert("Game Folder/Game".to_string(), vec![]);
    disp.set_launched_app_path("Game Folder/Game");
    let saved_wd_refnum = *disp.app_wd_refnum;

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"MacintoshHD");
    bus.write_word(pb + 22, saved_wd_refnum as u16);
    bus.write_long(pb + 48, 2); // stale ioWDDirID must not select the root

    call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, app_dir_id);
    assert_eq!(*disp.app_wd_refnum, saved_wd_refnum);
    assert_eq!(bus.read_long(addr::CUR_DIR_STORE), app_dir_id);
}

#[test]
fn pbsetvol_volume_name_selects_named_volume_root() {
    let (mut disp, mut cpu, mut bus) = setup();
    let volume_ref =
        disp.mount_vfs_volume("Legend CD", 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
    let root_dir_id = disp
        .vfs_volume_for_ref_num(volume_ref)
        .expect("mounted volume")
        .root_dir_id;

    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"Legend CD");
    bus.write_word(pb + 22, 0); // name selects the volume

    call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, root_dir_id);
    assert_eq!(disp.resolve_volume_ref_num(*disp.app_wd_refnum), volume_ref);
    assert_eq!(bus.read_word(addr::SF_SAVE_DISK), (-volume_ref) as u16);
}

#[test]
fn pbhgetvol_reports_the_selected_mounted_volume_and_root() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, root_dir_id) = mount_read_only_test_volume(&mut disp, "Selected Disk");
    let pb = 0x300000u32;
    let name_buf = 0x300100u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_long(pb + 18, name_buf);
    bus.write_pstring(name_buf, b"Selected Disk");
    bus.write_word(pb + 22, 0);
    call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);

    bus.write_pstring(name_buf, b"poison");
    call(&mut disp, false, 0x14, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_pstring(name_buf), b"Selected Disk");
    assert_eq!(bus.read_word(pb + 32), volume_ref as u16, "ioWDVRefNum");
    assert_eq!(bus.read_long(pb + 48), root_dir_id, "ioWDDirID");
}

#[test]
fn pbsetvol_rejects_an_unknown_volume_name() {
    let (mut disp, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Missing Disk");
    bus.write_word(pb + 22, 0);

    call(&mut disp, false, 0x15, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "nsvErr");
    assert_eq!(bus.read_word(pb + 16) as i16, -35);
    assert_eq!(*disp.default_dir_id, 2);
}

#[test]
fn pbopenwd_and_pbgetwdinfo_preserve_mounted_volume_identity() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, _) = mount_read_only_test_volume(&mut disp, "Working Disk");
    let data_dir_id = disp.ensure_vfs_directory("Working Disk/Data");
    let pb = 0x300000u32;
    let name_buf = setup_param_block(&mut bus, &mut cpu, pb, b"");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 28, 0x1234_5678);
    bus.write_long(pb + 48, data_dir_id);
    cpu.write_reg(Register::D0, 1);
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    let wd_ref = bus.read_word(pb + 22) as i16;
    assert_ne!(wd_ref, volume_ref);

    bus.write_pstring(name_buf, b"poison");
    bus.write_word(pb + 22, wd_ref as u16);
    bus.write_word(pb + 26, 0);
    cpu.write_reg(Register::D0, 7);
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_pstring(name_buf), b"Working Disk");
    assert_eq!(bus.read_word(pb + 32), volume_ref as u16);
    assert_eq!(bus.read_long(pb + 48), data_dir_id);
    assert_eq!(bus.read_long(pb + 28), 0x1234_5678);
}

#[test]
fn pbhopendf_resolves_a_file_from_a_mounted_volume_root() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, root_dir_id) = mount_read_only_test_volume(&mut disp, "Lookup Disk");
    disp.vfs
        .insert("Lookup Disk/Data File".to_string(), vec![1, 2, 3]);
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Data File");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 48, root_dir_id);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    let ref_num = bus.read_word(pb + 24);
    assert_eq!(
        disp.open_files.get(&ref_num).map(String::as_str),
        Some("Lookup Disk/Data File")
    );
    assert!(disp.write_refnums.contains(&ref_num));
}

#[test]
fn extracted_volume_rejects_write_open_and_fswrite() {
    let (mut disp, mut cpu, mut bus) = setup();
    let volume_ref =
        disp.mount_vfs_volume("Legend CD", 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
    let file_name = "Legend CD/Legend";
    disp.vfs.insert(file_name.to_string(), vec![1, 2, 3]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Legend CD/Legend");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_byte(pb + 27, 3); // fsRdWrPerm
    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "wPrErr");

    bus.write_byte(pb + 27, 1); // fsRdPerm
    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let ref_num = bus.read_word(pb + 24);
    bus.write_word(pb + 24, ref_num);
    bus.write_long(pb + 32, 0x310000);
    bus.write_long(pb + 36, 1);
    bus.write_byte(0x310000, 9);
    call(&mut disp, false, 0x03, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "wPrErr");
    assert_eq!(disp.vfs.get(file_name), Some(&vec![1, 2, 3]));
}

#[test]
fn extracted_volume_rejects_file_and_directory_mutations() {
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, root_dir_id) = mount_read_only_test_volume(&mut disp, "Locked Disk");
    let file_name = "Locked Disk/Document";
    disp.vfs.insert(file_name.to_string(), vec![1, 2, 3]);
    disp.set_vfs_entry_metadata(file_name, *b"TEXT", *b"TEST", 0);
    let pb = 0x300000u32;

    setup_param_block(&mut bus, &mut cpu, pb, b"New File");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 48, root_dir_id);
    call_trap_word(&mut disp, 0xA208, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "PBHCreate");
    assert!(!disp.vfs.contains_key("Locked Disk/New File"));

    setup_param_block(&mut bus, &mut cpu, pb, b"New Folder");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 48, root_dir_id);
    cpu.write_reg(Register::D0, 6);
    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "PBDirCreate");

    setup_param_block(&mut bus, &mut cpu, pb, b"Document");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_long(pb + 48, root_dir_id);
    bus.write_long(pb + 32, u32::from_be_bytes(*b"DATA"));
    call_trap_word(&mut disp, 0xA20D, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "PBHSetFInfo");
    assert_eq!(
        disp.vfs_metadata[file_name].file_type,
        u32::from_be_bytes(*b"TEXT")
    );

    call_trap_word(&mut disp, 0xA209, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "PBHDelete");
    assert_eq!(disp.vfs.get(file_name), Some(&vec![1, 2, 3]));

    bus.write_byte(pb + 27, 1); // fsRdPerm
    call_trap_word(&mut disp, 0xA200, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    bus.write_long(pb + 28, 1);
    call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, -44, "PBSetEOF");
    assert_eq!(disp.vfs.get(file_name), Some(&vec![1, 2, 3]));
}

#[test]
fn ordinary_top_level_vfs_directories_remain_writable() {
    let (mut disp, mut cpu, mut bus) = setup();
    let root_dir_id = disp.ensure_vfs_directory("Archive Folder");
    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Preferences");
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, root_dir_id);
    call_trap_word(&mut disp, 0xA208, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert!(disp.vfs.contains_key("Archive Folder/Preferences"));
}

#[test]
fn pbsetvol_volume_refnum_ignores_stale_iowddirid() {
    // Files 1992, 2-151: PBSetVol uses the basic parameter block and,
    // when passed a volume reference number, sets the default directory
    // to that volume's root. The HFS-only ioWDDirID field belongs to
    // PBHSetVol, not PBSetVol.
    let (mut disp, mut cpu, mut bus) = setup();

    let stale_dir_id = disp.ensure_vfs_directory("Marathon");

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, stale_dir_id);
    bus.write_long(pb + 18, 0); // ioNamePtr = NIL

    call_trap_word(&mut disp, 0xA015, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, 2);
    assert_eq!(
        disp.app_wd_refnum,
        super::super::dispatch::BOOT_VOLUME_REF_NUM
    );
    assert_eq!(bus.read_long(addr::CUR_DIR_STORE), 2);
}

#[test]
fn pbhsetvol_async_sets_default_directory_from_iowddirid_for_volume_refnum_calls() {
    // Inside Macintosh: Files (1992), pp. 2-153 to 2-154:
    // with ioNamePtr = NIL and a volume refnum in ioVRefNum,
    // PBHSetVol uses ioWDDirID as the default directory.
    let (mut disp, mut cpu, mut bus) = setup();

    let dir_id = disp.ensure_vfs_directory("Marathon");

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 48, dir_id);
    bus.write_long(pb + 18, 0); // ioNamePtr = NIL

    call_trap_word(&mut disp, 0xA615, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(*disp.default_dir_id, dir_id);
    assert_ne!(
        disp.app_wd_refnum,
        super::super::dispatch::BOOT_VOLUME_REF_NUM
    );
    assert_eq!(bus.read_long(addr::CUR_DIR_STORE), dir_id);
}

#[test]
fn pbhsetvol_invalid_vrefnum_returns_nsverr() {
    // Inside Macintosh: Files (1992), pp. 2-153 to 2-154:
    // unresolved volume references return nsvErr.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 22, (-999i16) as u16); // ioVRefNum phantom
    bus.write_long(pb + 48, 0); // ioWDDirID = 0 keeps the call on the nsvErr path

    call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -35, "nsvErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, -35, "nsvErr in ioResult");
}

#[test]
fn pbhsetvol_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Pre-poisons pb.ioResult @ pb+16 with 0x3FFF and sets a phantom
    // ioVRefNum (-999); asserts the sentinel is overwritten AND
    // D0 == ioResult per the File Manager basic-PB dispatcher
    // convention AND A7 is preserved across the call.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
    bus.write_word(pb + 22, (-999i16) as u16); // ioVRefNum phantom
    bus.write_long(pb + 48, 0); // ioWDDirID = 0

    let sp_pre = cpu.read_reg(Register::A7);
    call_trap_word(&mut disp, 0xA215, &mut cpu, &mut bus).unwrap();
    let sp_post = cpu.read_reg(Register::A7);

    let io_result = bus.read_word(pb + 16) as i16;
    let d0_result = cpu.read_reg(Register::D0) as i32 as i16;
    assert_ne!(io_result as u16, 0x3FFFu16, "ioResult sentinel overwritten");
    assert_eq!(
        d0_result, io_result,
        "D0 == ioResult per File Manager basic-PB dispatcher convention"
    );
    assert_eq!(sp_pre, sp_post, "A7 preserved across PBHSetVol");
}

// ================================================================
// 30. PBSetFInfo / HSetFInfo (0x0D)
// ================================================================
#[test]
fn pb_set_finfo() {
    // Updated for Files 1992 2-205 contract: existing file →
    // noErr, missing file → fnfErr. Prior to the fix this test
    // relied on PBSetFInfo silently returning noErr for any
    // filename (including ones not in the VFS).
    let (mut disp, mut cpu, mut bus) = setup();
    disp.vfs.insert("AnyFile".to_string(), Vec::new());

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"AnyFile");

    call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
}

#[test]
fn pb_set_finfo_missing_file_is_fnferr() {
    // Regression: previously PBSetFInfo returned noErr for missing
    // files, silently masking missing-save bugs in games that
    // rely on fnfErr (Files 1992, 2-205).
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"NotPresent");

    call(&mut disp, false, 0x0D, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
    assert_eq!(bus.read_word(pb + 16) as i16, -43);
}

// ================================================================
// 31. PBSetEOF / HSetEOF ($A012)
// ================================================================
#[test]
fn pb_set_eof() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("EOFSet".to_string(), vec![0xAA, 0xBB, 0xCC, 0xDD]);
    disp.open_files.insert(100, "EOFSet".to_string());
    disp.write_refnums.insert(100);
    disp.file_positions.insert(100, 4);

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 100); // ioRefNum
    bus.write_long(pb + 28, 2); // ioMisc = new EOF

    call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(disp.vfs.get("EOFSet").unwrap(), &vec![0xAA, 0xBB]);
    assert_eq!(*disp.file_positions.get(&100).unwrap(), 2);
}

#[test]
fn pb_set_eof_invalid_refnum() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 24, 999);
    bus.write_long(pb + 28, 2);

    call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-51i32) as u32);
    assert_eq!(bus.read_word(pb + 16), (-51i16) as u16);
}

// ================================================================
// 32. FSDispatch (0x60) — selector 8 (PBGetFCBInfo), refnum not open
// ================================================================
#[test]
fn fs_dispatch_pbgetfcbinfo_refnum_not_open() {
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
    bus.write_word(pb + 24, 999); // ioRefNum

    cpu.write_reg(Register::D0, 8); // selector = PBGetFCBInfo

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::D0),
        (-38i32) as u32,
        "D0 should be fnOpnErr"
    );
    assert_eq!(bus.read_word(pb + 16), (-38i16) as u16);
}

// ================================================================
// 32b. FSDispatch (0x60) — selector 8 (PBGetFCBInfo), app resource file
// ================================================================
#[test]
fn fs_dispatch_pbgetfcbinfo_refnum_zero_ignores_stale_name_output() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("EV/Escape Velocity".to_string(), vec![0xAB]);
    disp.set_vfs_entry_metadata("EV/Escape Velocity", *b"APPL", *b"EV  ", 0);
    disp.set_launched_app_path("EV/Escape Velocity");
    let expected_parent_dir_id = *disp.default_dir_id;

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(
        &mut bus,
        &mut cpu,
        pb,
        b"stale output buffer that must not be treated as an input filename",
    );
    bus.write_word(pb + 24, 0); // ioRefNum 0 = current app resource file

    cpu.write_reg(Register::D0, 8);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Escape Velocity".to_vec());
    assert_eq!(bus.read_long(pb + 58), expected_parent_dir_id);
    assert_eq!(bus.read_word(pb + 52) as i16, -1);
}

#[test]
fn fs_dispatch_pbgetfcbinfo_data_fork_reports_size_position_and_flags() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Game/Data 1".to_string(), (0u8..12).collect::<Vec<_>>());
    disp.set_vfs_entry_metadata("Game/Data 1", *b"DATA", *b"TST!", 0);
    let metadata = disp.vfs_file_metadata("Game/Data 1").unwrap();
    disp.open_files.insert(123, "Game/Data 1".to_string());
    disp.file_positions.insert(123, 4);

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
    bus.write_word(pb + 24, 123);
    bus.write_word(pb + 28, 0);
    cpu.write_reg(Register::D0, 8);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Data 1".to_vec());
    assert_eq!(bus.read_word(pb + 22) as i16, -1);
    assert_eq!(bus.read_long(pb + 32), metadata.file_id);
    assert_eq!(bus.read_word(pb + 36) & 0x0200, 0);
    assert_eq!(bus.read_long(pb + 40), 12);
    assert_eq!(bus.read_long(pb + 44), 12);
    assert_eq!(bus.read_long(pb + 48), 4);
    assert_eq!(bus.read_word(pb + 52) as i16, -1);
    assert_eq!(bus.read_long(pb + 58), metadata.parent_dir_id);
}

#[test]
fn fs_dispatch_pbgetfcbinfo_resource_fork_reports_resource_size_and_flag() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Game/Data 1".to_string(), vec![0xAA, 0xBB]);
    disp.vfs_rsrc
        .insert("Game/Data 1".to_string(), vec![1, 2, 3, 4, 5]);
    disp.set_vfs_entry_metadata("Game/Data 1", *b"DATA", *b"TST!", 0);
    let metadata = disp.vfs_file_metadata("Game/Data 1").unwrap();
    disp.open_files
        .insert(124, "__rsrc__Game/Data 1".to_string());
    disp.file_positions.insert(124, 3);

    let pb = 0x300000u32;
    let name_ptr = setup_param_block(&mut bus, &mut cpu, pb, b"stale output buffer");
    bus.write_word(pb + 24, 124);
    bus.write_word(pb + 28, 0);
    cpu.write_reg(Register::D0, 8);

    call(&mut disp, false, 0x60, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(bus.read_pstring(name_ptr), b"Data 1".to_vec());
    assert_eq!(bus.read_long(pb + 32), metadata.file_id);
    assert_ne!(bus.read_word(pb + 36) & 0x0200, 0);
    assert_eq!(bus.read_long(pb + 40), 5);
    assert_eq!(bus.read_long(pb + 44), 5);
    assert_eq!(bus.read_long(pb + 48), 3);
    assert_eq!(bus.read_long(pb + 58), metadata.parent_dir_id);
}

#[test]
fn pbhgetfinfo_retries_default_directory_for_stale_dirid() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.ensure_vfs_directory("Marathon");
    disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
    disp.vfs
        .insert("Marathon/Physics Model".to_string(), vec![0xAB]);
    disp.set_launched_app_path("Marathon/Marathon");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Physics Model");
    bus.write_long(pb + 48, 0x01FF01FF);

    call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
}

#[test]
fn initfs_returns_noerr_and_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);

    call(&mut disp, false, 0x6C, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp_pre);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

// PBFlushFile (0x45) — valid open refnum returns noErr.
// Files 1992, 2-114: noErr when the refnum identifies an open access path.
#[test]
fn pb_flush_file_valid_refnum_returns_noerr() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Flush.dat".to_string(), vec![1, 2, 3]);
    let pb = 0x270000u32;
    for i in 0u32..32 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"Flush.dat");
    call(&mut disp, false, 0x00, &mut cpu, &mut bus).unwrap(); // PBOpen → assigns refnum
    let refnum = bus.read_word(pb + 24);
    assert_eq!(refnum % 94, 2, "expected HFS FCB offset");

    for i in 0u32..32 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 24, refnum);
    cpu.write_reg(Register::A0, pb);
    call(&mut disp, false, 0x45, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "valid refnum → noErr");
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
}

// PBRename (0x0B) — renaming an existing VFS file updates all state.
// Files 1992, 2-118: the file entry and any open access paths follow the rename.
#[test]
fn pb_rename_existing_file_succeeds() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("OldName.dat".to_string(), vec![0xAA, 0xBB]);

    let pb = 0x271000u32;
    let old_name_addr = pb + 0x100;
    let new_name_addr = pb + 0x200;
    write_pstring(&mut bus, old_name_addr, b"OldName.dat");
    write_pstring(&mut bus, new_name_addr, b"NewName.dat");

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_long(pb + 18, old_name_addr); // ioNamePtr
    bus.write_long(pb + 28, new_name_addr); // ioMisc — new name
    cpu.write_reg(Register::A0, pb);
    call(&mut disp, false, 0x0B, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "rename → noErr");
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert!(
        !disp.vfs.contains_key("OldName.dat"),
        "old key must be gone"
    );
    assert_eq!(disp.vfs.get("NewName.dat"), Some(&vec![0xAA, 0xBBu8]));
}

#[test]
fn pbhrename_path_new_name_stays_in_source_directory() {
    let (mut disp, mut cpu, mut bus) = setup();

    let dir_id = disp.ensure_vfs_directory("App Folder");
    disp.vfs
        .insert("App Folder/Prefs File__TEMP".to_string(), vec![0xAA, 0xBB]);
    disp.vfs_rsrc
        .insert("App Folder/Prefs File__TEMP".to_string(), vec![0xCC]);

    let pb = 0x271400u32;
    let old_name_addr = pb + 0x100;
    let new_name_addr = pb + 0x200;
    write_pstring(&mut bus, old_name_addr, b":App Folder:Prefs File__TEMP");
    write_pstring(&mut bus, new_name_addr, b":App Folder:Prefs File");

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    bus.write_word(pb + 16, 0x7777);
    bus.write_long(pb + 18, old_name_addr);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_long(pb + 28, new_name_addr);
    bus.write_long(pb + 48, dir_id);

    cpu.write_reg(Register::A0, pb);
    call_trap_word(&mut disp, 0xA20B, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(pb + 16) as i16, 0);
    assert!(!disp.vfs.contains_key("App Folder/Prefs File__TEMP"));
    assert_eq!(
        disp.vfs.get("App Folder/Prefs File"),
        Some(&vec![0xAA, 0xBB])
    );
    assert_eq!(
        disp.vfs_rsrc.get("App Folder/Prefs File"),
        Some(&vec![0xCC])
    );
    assert!(
        !disp.vfs.contains_key("App Folder/App Folder/Prefs File"),
        "PBHRename must rename within the original directory, not append a pathname under it"
    );

    let open_pb = 0x271800u32;
    setup_param_block(&mut bus, &mut cpu, open_pb, b"Prefs File");
    bus.write_word(
        open_pb + 22,
        super::super::dispatch::BOOT_VOLUME_REF_NUM as u16,
    );
    bus.write_long(open_pb + 48, dir_id);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
    assert_eq!(bus.read_word(open_pb + 16) as i16, 0);
    let refnum = bus.read_word(open_pb + 24);
    assert_eq!(
        disp.open_files.get(&refnum),
        Some(&"App Folder/Prefs File".to_string())
    );
}

#[test]
fn pbsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit() {
    // Inside Macintosh: Files (1992), pp. 2-89 and 2-110:
    // PBSetFLock locks a file; PBGetFInfo reports lock state via ioFlAttrib bit 0.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Lockable.dat".to_string(), vec![]);

    let pb = 0x272000u32;
    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
    bus.write_word(pb + 16, 0x7B7B); // ioResult poison
    call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "SetFLock noErr in D0");
    assert_eq!(
        bus.read_word(pb + 16) as i16,
        0,
        "SetFLock noErr in ioResult"
    );

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
    bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBGetFInfo noErr");
    assert_eq!(
        bus.read_byte(pb + 30) & 0x01,
        0x01,
        "ioFlAttrib bit 0 set after PBSetFLock"
    );
}

#[test]
fn pbrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit() {
    // Inside Macintosh: Files (1992), pp. 2-89 and 2-111:
    // PBRstFLock unlocks a file; PBGetFInfo reports lock state via ioFlAttrib bit 0.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("Lockable.dat".to_string(), vec![]);

    let pb = 0x272100u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
    call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
    bus.write_word(pb + 16, 0x7777); // ioResult poison
    call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "RstFLock noErr in D0");
    assert_eq!(
        bus.read_word(pb + 16) as i16,
        0,
        "RstFLock noErr in ioResult"
    );

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"Lockable.dat");
    bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
    call(&mut disp, false, 0x0C, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBGetFInfo noErr");
    assert_eq!(
        bus.read_byte(pb + 30) & 0x01,
        0x00,
        "ioFlAttrib bit 0 clear after PBRstFLock"
    );
}

#[test]
fn pbsetflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
    // Inside Macintosh: Files (1992), p. 2-110:
    // PBSetFLock returns fnfErr (-43) when the target file is missing.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x272200u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"MissingLockable.dat");
    bus.write_word(pb + 16, 0x2222); // ioResult poison
    call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
}

#[test]
fn pbrstflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
    // Inside Macintosh: Files (1992), p. 2-111:
    // PBRstFLock returns fnfErr (-43) when the target file is missing.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x272300u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"MissingLockable.dat");
    bus.write_word(pb + 16, 0x3333); // ioResult poison
    call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
}

#[test]
fn pbsetflock_pbrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Inside Macintosh: Files (1992), pp. 2-110 to 2-111 + Inside
    // Macintosh Volume II (1985), p. II-114.
    //
    // Pre-poisons
    // pb.ioResult @ pb+16 with 0x3FFF, calls _PBSetFLock then
    // _PBRstFLock against bogus filenames not present in the VFS,
    // and asserts the dispatcher convention (D0 == ioResult,
    // ioResult overwritten away from sentinel) plus the register-
    // only ABI (A7 preserved across both calls).
    let (mut disp, mut cpu, mut bus) = setup();

    // PBSetFLock: bogus filename → fnfErr (-43) per IM:Files; both
    // ioResult and D0 receive the same OSErr value.
    let pb = 0x272500u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFileForSetFLock");
    bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison sentinel
    let sp_pre_set = cpu.read_reg(Register::A7);
    call(&mut disp, false, 0x41, &mut cpu, &mut bus).unwrap();
    let sp_post_set = cpu.read_reg(Register::A7);
    let d0_set = cpu.read_reg(Register::D0) as i32;
    let io_set = bus.read_word(pb + 16) as i16 as i32;
    assert_ne!(
        io_set, 0x3FFF,
        "PBSetFLock must overwrite ioResult sentinel"
    );
    assert_eq!(
        d0_set, io_set,
        "PBSetFLock D0 must mirror ioResult per IM:II II-114"
    );
    assert_eq!(sp_pre_set, sp_post_set, "PBSetFLock must not consume stack");

    // PBRstFLock: same shape, fresh param block.
    let pb2 = 0x272600u32;
    setup_param_block(&mut bus, &mut cpu, pb2, b"NoSuchFileForRstFLock");
    bus.write_word(pb2 + 16, 0x3FFF); // ioResult pre-poison sentinel
    let sp_pre_rst = cpu.read_reg(Register::A7);
    call(&mut disp, false, 0x42, &mut cpu, &mut bus).unwrap();
    let sp_post_rst = cpu.read_reg(Register::A7);
    let d0_rst = cpu.read_reg(Register::D0) as i32;
    let io_rst = bus.read_word(pb2 + 16) as i16 as i32;
    assert_ne!(
        io_rst, 0x3FFF,
        "PBRstFLock must overwrite ioResult sentinel"
    );
    assert_eq!(
        d0_rst, io_rst,
        "PBRstFLock D0 must mirror ioResult per IM:II II-114"
    );
    assert_eq!(sp_pre_rst, sp_post_rst, "PBRstFLock must not consume stack");
}

#[test]
fn pbhsetflock_pbhrstflock_write_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Inside Macintosh: Files (1992), pp. 2-196 to 2-198 + Inside
    // Macintosh Volume II (1985), p. II-114.
    //
    // Pre-poisons
    // ph.fileParam.ioResult @ pb+16 with 0x3FFF, calls _PBHSetFLock
    // then _PBHRstFLock against bogus filenames not present in the
    // VFS, and asserts the dispatcher convention (D0 == ioResult,
    // ioResult overwritten away from sentinel) plus the register-
    // only ABI (A7 preserved across both calls). $A241/$A242 share
    // the same low-byte arms (false, 0x41) and (false, 0x42) with
    // $A041/$A042 via the OS-trap dispatcher's trap & 0x00FF mask.
    let (mut disp, mut cpu, mut bus) = setup();

    // PBHSetFLock: bogus filename → fnfErr (-43) per IM:Files; both
    // ioResult and D0 receive the same OSErr value.
    let pb = 0x272700u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"NoSuchFileForHSetFLock");
    bus.write_word(pb + 16, 0x3FFF); // ioResult pre-poison sentinel
    let sp_pre_set = cpu.read_reg(Register::A7);
    call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();
    let sp_post_set = cpu.read_reg(Register::A7);
    let d0_set = cpu.read_reg(Register::D0) as i32;
    let io_set = bus.read_word(pb + 16) as i16 as i32;
    assert_ne!(
        io_set, 0x3FFF,
        "PBHSetFLock must overwrite ioResult sentinel"
    );
    assert_eq!(
        d0_set, io_set,
        "PBHSetFLock D0 must mirror ioResult per IM:II II-114"
    );
    assert_eq!(
        sp_pre_set, sp_post_set,
        "PBHSetFLock must not consume stack"
    );

    // PBHRstFLock: same shape, fresh param block.
    let pb2 = 0x272800u32;
    setup_param_block(&mut bus, &mut cpu, pb2, b"NoSuchFileForHRstFLock");
    bus.write_word(pb2 + 16, 0x3FFF); // ioResult pre-poison sentinel
    let sp_pre_rst = cpu.read_reg(Register::A7);
    call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();
    let sp_post_rst = cpu.read_reg(Register::A7);
    let d0_rst = cpu.read_reg(Register::D0) as i32;
    let io_rst = bus.read_word(pb2 + 16) as i16 as i32;
    assert_ne!(
        io_rst, 0x3FFF,
        "PBHRstFLock must overwrite ioResult sentinel"
    );
    assert_eq!(
        d0_rst, io_rst,
        "PBHRstFLock D0 must mirror ioResult per IM:II II-114"
    );
    assert_eq!(
        sp_pre_rst, sp_post_rst,
        "PBHRstFLock must not consume stack"
    );
}

#[test]
fn pbhsetflock_existing_file_returns_noerr_and_sets_ioflattrib_locked_bit() {
    // Inside Macintosh: Files (1992), pp. 2-196 to 2-197:
    // PBHSetFLock is the HFS variant and trap macro _HSetFLock ($A241).
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("HLockable.dat".to_string(), vec![]);

    let pb = 0x272400u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
    bus.write_word(pb + 16, 0x4444); // ioResult poison
    call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "PBHSetFLock noErr in D0"
    );
    assert_eq!(
        bus.read_word(pb + 16) as i16,
        0,
        "PBHSetFLock noErr in ioResult"
    );

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
    bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
    call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBHGetFInfo noErr");
    assert_eq!(
        bus.read_byte(pb + 30) & 0x01,
        0x01,
        "ioFlAttrib bit 0 set after PBHSetFLock"
    );
}

#[test]
fn pbhrstflock_existing_file_returns_noerr_and_clears_ioflattrib_locked_bit() {
    // Inside Macintosh: Files (1992), pp. 2-197 to 2-198:
    // PBHRstFLock is the HFS variant and trap macro _HRstFLock ($A242).
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("HLockable.dat".to_string(), vec![]);

    let pb = 0x272500u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
    call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
    bus.write_word(pb + 16, 0x5555); // ioResult poison
    call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "PBHRstFLock noErr in D0"
    );
    assert_eq!(
        bus.read_word(pb + 16) as i16,
        0,
        "PBHRstFLock noErr in ioResult"
    );

    for i in 0u32..64 {
        bus.write_byte(pb + i, 0);
    }
    setup_param_block(&mut bus, &mut cpu, pb, b"HLockable.dat");
    bus.write_byte(pb + 30, 0xFF); // ioFlAttrib poison
    call_trap_word(&mut disp, 0xA20C, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "PBHGetFInfo noErr");
    assert_eq!(
        bus.read_byte(pb + 30) & 0x01,
        0x00,
        "ioFlAttrib bit 0 clear after PBHRstFLock"
    );
}

#[test]
fn pbhsetflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
    // Inside Macintosh: Files (1992), pp. 2-196 to 2-197:
    // PBHSetFLock lists fnfErr (-43) for a missing file.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x272600u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"MissingHLock.dat");
    bus.write_word(pb + 16, 0x6666); // ioResult poison
    call_trap_word(&mut disp, 0xA241, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
}

#[test]
fn pbhrstflock_missing_file_returns_fnferr_in_d0_and_ioresult() {
    // Inside Macintosh: Files (1992), pp. 2-197 to 2-198:
    // PBHRstFLock lists fnfErr (-43) for a missing file.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x272700u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"MissingHLock.dat");
    bus.write_word(pb + 16, 0x7777); // ioResult poison
    call_trap_word(&mut disp, 0xA242, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43, "fnfErr in D0");
    assert_eq!(bus.read_word(pb + 16) as i16, -43, "fnfErr in ioResult");
}

#[test]
fn fs_dispatch_pbhopendf_retries_default_directory_for_stale_dirid() {
    let (mut disp, mut cpu, mut bus) = setup();

    disp.ensure_vfs_directory("Marathon");
    disp.vfs.insert("Marathon/Marathon".to_string(), vec![]);
    disp.vfs
        .insert("Marathon/Physics Model".to_string(), vec![0xAB]);
    disp.set_launched_app_path("Marathon/Marathon");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Physics Model");
    bus.write_long(pb + 48, 0x01FF01FF);

    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert!(bus.read_word(pb + 24) >= 100);
}

#[test]
fn fs_dispatch_pbhopendf_write_permission_allows_set_eof() {
    // Files 1992, pp. 2-183 to 2-184: PBHOpenDF accepts fsRdWrPerm in
    // ioPermssn.
    // The granted permission remains attached to the refnum for later
    // File Manager calls such as PBSetEOF (pp. 2-127 to 2-128).
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs
        .insert("Temporary Items/Cache".to_string(), vec![1, 2, 3, 4]);
    let dir_id = disp.ensure_vfs_directory("Temporary Items");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Cache");
    bus.write_byte(pb + 27, 3); // fsRdWrPerm
    bus.write_long(pb + 48, dir_id);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    let refnum = bus.read_word(pb + 24);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(disp.write_refnums.contains(&refnum));

    bus.write_long(pb + 28, 2);
    call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(disp.vfs["Temporary Items/Cache"], vec![1, 2]);
}

#[test]
fn fs_dispatch_pbhopendf_locked_volume_defers_write_error_until_set_eof() {
    // Files 1992, pp. 2-7 to 2-8: a locked volume does not make the
    // open fail. The attempted mutation reports wPrErr.
    let (mut disp, mut cpu, mut bus) = setup();
    let (volume_ref, root_dir_id) = mount_read_only_test_volume(&mut disp, "Locked Disk");
    disp.vfs
        .insert("Locked Disk/Cache".to_string(), vec![1, 2, 3, 4]);

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Cache");
    bus.write_word(pb + 22, volume_ref as u16);
    bus.write_byte(pb + 27, 3); // fsRdWrPerm
    bus.write_long(pb + 48, root_dir_id);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    let refnum = bus.read_word(pb + 24);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(disp.write_refnums.contains(&refnum));

    bus.write_long(pb + 28, 2);
    call(&mut disp, false, 0x12, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -44);
    assert_eq!(bus.read_word(pb + 16) as i16, -44);
    assert_eq!(disp.vfs["Locked Disk/Cache"], vec![1, 2, 3, 4]);
}

#[test]
fn fs_dispatch_pbhopendf_empty_name_returns_bdnamerr_and_clears_refnum() {
    // Files 1992, 2-184 lists bdNamErr for a bad filename; the
    // result-code index clarifies that a zero-length filename is in
    // that class. PBHOpenDF must not silently succeed with a stale
    // ioRefNum when ioNamePtr points at an empty Pascal string.
    let (mut disp, mut cpu, mut bus) = setup();

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"");
    bus.write_word(pb + 16, 0x7777);
    bus.write_word(pb + 24, 1023);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -37);
    assert_eq!(bus.read_word(pb + 16) as i16, -37);
    assert_eq!(bus.read_word(pb + 24), 0);
}

#[test]
fn fsdispatch_pbhopendf_explicit_parent_does_not_open_same_basename_elsewhere() {
    // Files 1992, 2-29: poor man's search path applies only when the
    // directory ID field is 0. With an explicit parent dirID, PBHOpenDF
    // must fail instead of opening an unrelated basename elsewhere.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.vfs.insert("App/App".to_string(), vec![]);
    disp.vfs
        .insert("App/Shared Preferences".to_string(), vec![0x42]);
    let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

    let pb = 0x300000u32;
    setup_param_block(&mut bus, &mut cpu, pb, b"Shared Preferences");
    bus.write_word(pb + 16, 0x7777);
    bus.write_word(pb + 22, super::super::dispatch::BOOT_VOLUME_REF_NUM as u16);
    bus.write_word(pb + 24, 0x1234);
    bus.write_long(pb + 48, pref_dir_id);
    cpu.write_reg(Register::D0, 26);

    call_trap_word(&mut disp, 0xA260, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
    assert_eq!(bus.read_word(pb + 16) as i16, -43);
    assert_eq!(bus.read_word(pb + 24), 0);
}

// OSDispatch ($A88F) selector contracts.
// Temporary Memory: Inside Macintosh Volume VI, 28-38 and 28-45.
// Process Manager: Processes (1994), pp. 2-21 to 2-28 and p. 2-31.
#[test]
fn os_dispatch_generated_routes_preserve_exact_stack_word_values() {
    assert_eq!(super::OS_DISPATCH_OPERATION_ROUTES.len(), 19);
    assert!(super::OS_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (0x0015, "TempMaxMem"),
        (0x0016, "TempTopMem"),
        (0x0018, "TempFreeMem"),
        (0x001D, "TempNewHandle"),
        (0x001E, "TempHLock"),
        (0x001F, "TempHUnlock"),
        (0x0020, "TempDisposeHandle"),
        (0x0033, "AcceptHighLevelEvent"),
        (0x0034, "PostHighLevelEvent"),
        (0x0035, "GetProcessSerialNumberFromPortName"),
        (0x0036, "LaunchDeskAccessory"),
        (0x0038, "GetNextProcess"),
        (0x0039, "GetFrontProcess"),
        (0x003A, "GetProcessInformation"),
        (0x003B, "SetFrontProcess"),
        (0x003C, "WakeUpProcess"),
        (0x003D, "SameProcess"),
        (0x0045, "GetSpecificHighLevelEvent"),
        (0x0046, "GetPortNameFromProcessSerialNumber"),
    ] {
        let route = super::os_dispatch_operation_route(0xA88F, selector)
            .expect("OSDispatch operation route");
        assert_eq!(route.routine_name, routine_name);
    }

    for (trap_word, selector) in [
        (0xA98F, 0x0015),
        (0xA88E, 0x0046),
        (0xA88F, 0x0037), // non-intersection GetCurrentProcess compatibility path
        (0xA88F, 0x3F3C), // MOVE.W immediate-to-stack opcode spelling
        (0xA88F, 0x1500), // byte-swapped TempMaxMem selector
        (0xA88F, 0x0001), // partial selector byte
    ] {
        assert!(super::os_dispatch_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn os_dispatch_records_stack_word_identity_without_changing_temp_free_mem_behavior() {
    let (mut disp, mut cpu, mut bus) = setup();

    for trap_word in [0xA88F, 0xA98F] {
        disp.current_trap_word = trap_word;
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x0015); // conflicting D0 fallback selector
        bus.write_word(TEST_SP, 0x0018); // TempFreeMem
        bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);

        let result = disp
            .dispatch_resource(true, 0x08F, &mut cpu, &mut bus)
            .expect("OSDispatch arm");
        assert!(result.is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_long(TEST_SP + 2), cpu.read_reg(Register::D0));

        let expected = (trap_word == 0xA88F)
            .then_some("selector-operation:_OSDispatch:0x0018:stack-word-immediate:16");
        assert_eq!(disp.current_selector_operation, expected);
    }

    disp.current_trap_word = 0xA88F;
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x0018);
    bus.write_word(TEST_SP, 0xBEEF); // compatibility fallback is not source identity
    let result = disp
        .dispatch_resource(true, 0x08F, &mut cpu, &mut bus)
        .expect("OSDispatch arm");
    assert!(result.is_ok());
    assert_eq!(disp.current_selector_operation, None);
}

#[test]
fn osdispatch_tempfreemem_selector_0018_uses_stack_selector_and_returns_free_bytes() {
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_word(TEST_SP, 0x0018);
    bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
    cpu.write_reg(Register::D0, 0x0001);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "OSDispatch should pop the selector word"
    );
    assert!(
        cpu.read_reg(Register::D0) >= 8 * 1024 * 1024,
        "TempFreeMem should report usable temporary memory in D0"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 2),
        cpu.read_reg(Register::D0),
        "TempFreeMem must write the Pascal function result slot"
    );
}

#[test]
fn osdispatch_tempmaxmem_selector_0015_clears_grow_and_returns_max_bytes() {
    let (mut disp, mut cpu, mut bus) = setup();

    let grow_ptr = 0x2A0000u32;
    bus.write_long(grow_ptr, 0xDEAD_BEEF);
    bus.write_word(TEST_SP, 0x0015);
    bus.write_long(TEST_SP + 2, grow_ptr);
    bus.write_long(TEST_SP + 6, 0xDEAD_BEEF);
    cpu.write_reg(Register::D0, 0x0001);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 6,
        "OSDispatch should pop the selector word and grow pointer"
    );
    assert_eq!(
        bus.read_long(grow_ptr),
        0,
        "TempMaxMem must write 0 to the grow parameter"
    );
    assert!(
        cpu.read_reg(Register::D0) >= 8 * 1024 * 1024,
        "TempMaxMem should report usable temporary memory in D0"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 6),
        cpu.read_reg(Register::D0),
        "TempMaxMem must write the Pascal function result slot"
    );
}

#[test]
fn osdispatch_temptopmem_selector_0016_returns_memtop_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();

    bus.write_long(crate::memory::globals::addr::MEM_TOP, 0x03F0_0000);
    bus.write_word(TEST_SP, 0x0016);
    bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
    cpu.write_reg(Register::D0, 0x0001);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "OSDispatch should pop the selector word"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0x03F0_0000,
        "TempTopMem should return the top of addressable RAM in D0"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 2),
        0x03F0_0000,
        "TempTopMem must write the Pascal function result slot"
    );
}

#[test]
fn osdispatch_tempnewhandle_selector_001d_allocates_and_writes_result_code() {
    let (mut disp, mut cpu, mut bus) = setup();

    let err_ptr = 0x2A0100u32;
    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x001D);
    bus.write_long(TEST_SP + 2, err_ptr);
    bus.write_long(TEST_SP + 6, 64);
    bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    let handle = bus.read_long(TEST_SP + 10);
    let ptr = bus.read_long(handle);
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "TempNewHandle should pop selector and two arguments"
    );
    assert_ne!(handle, 0, "TempNewHandle should return a handle");
    assert_eq!(
        cpu.read_reg(Register::D0),
        handle,
        "TempNewHandle should mirror the handle in D0 for inline callers"
    );
    assert_eq!(
        bus.read_word(err_ptr),
        0,
        "TempNewHandle should write noErr to resultCode"
    );
    assert_eq!(bus.get_alloc_size(ptr), Some(64));
    assert_eq!(disp.handle_for_ptr(ptr), Some(handle));
}

#[test]
fn osdispatch_temphlock_unlock_and_dispose_selectors_manage_temp_handle_state() {
    let (mut disp, mut cpu, mut bus) = setup();

    let data_ptr = bus.alloc(32);
    let handle = bus.alloc(4);
    let err_ptr = 0x2A0200u32;
    bus.write_long(handle, data_ptr);

    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x001E);
    bus.write_long(TEST_SP + 2, err_ptr);
    bus.write_long(TEST_SP + 6, handle);
    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(err_ptr), 0, "TempHLock resultCode");
    assert_eq!(
        disp.handle_state_bits(handle).unwrap_or(0) & 0x80,
        0x80,
        "TempHLock should set the lock bit"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x001F);
    bus.write_long(TEST_SP + 2, err_ptr);
    bus.write_long(TEST_SP + 6, handle);
    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(err_ptr), 0, "TempHUnlock resultCode");
    assert_eq!(
        disp.handle_state_bits(handle).unwrap_or(0) & 0x80,
        0,
        "TempHUnlock should clear the lock bit"
    );

    disp.set_handle_state_bits(handle, 0xC0);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x0020);
    bus.write_long(TEST_SP + 2, err_ptr);
    bus.write_long(TEST_SP + 6, handle);
    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_word(err_ptr), 0, "TempDisposeHandle resultCode");
    assert_eq!(
        bus.get_alloc_size(data_ptr),
        None,
        "TempDisposeHandle should release the data block"
    );
    assert_eq!(
        bus.get_alloc_size(handle),
        None,
        "TempDisposeHandle should release the master pointer"
    );
    assert!(
        !disp.has_handle_state_bits(handle),
        "TempDisposeHandle should clear tracked state bits"
    );
}

#[test]
fn osdispatch_temphlock_selector_001e_reports_nil_handle_error() {
    let (mut disp, mut cpu, mut bus) = setup();

    let err_ptr = 0x2A0300u32;
    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x001E);
    bus.write_long(TEST_SP + 2, err_ptr);
    bus.write_long(TEST_SP + 6, 0);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(
        bus.read_word(err_ptr) as i16,
        -109,
        "TempHLock should report nilHandleErr for a NIL handle"
    );
}

#[test]
fn osdispatch_accepthighlevelevent_selector_0033_empty_queue_returns_nooutstandinghle() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sender_ptr = 0x2A0400u32;
    let msg_refcon_ptr = 0x2A0500u32;
    let msg_buff_ptr = 0x2A0600u32;
    let msg_len_ptr = 0x2A0700u32;
    bus.write_long(sender_ptr, 0xAAAA_AAAA);
    bus.write_long(msg_refcon_ptr, 0xBBBB_BBBB);
    bus.write_long(msg_len_ptr, 128);
    bus.write_word(TEST_SP, 0x0033);
    bus.write_long(TEST_SP + 2, msg_len_ptr);
    bus.write_long(TEST_SP + 6, msg_buff_ptr);
    bus.write_long(TEST_SP + 10, msg_refcon_ptr);
    bus.write_long(TEST_SP + 14, sender_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 18) as i16, -608, "noOutstandingHLE");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 18);
    assert_eq!(
        bus.read_long(sender_ptr),
        0xAAAA_AAAA,
        "sender should be untouched when no high-level event is outstanding"
    );
    assert_eq!(bus.read_long(msg_refcon_ptr), 0xBBBB_BBBB);
    assert_eq!(bus.read_long(msg_len_ptr), 128);
    assert_eq!(bus.read_byte(msg_buff_ptr), 0);
}

#[test]
fn osdispatch_accepthighlevelevent_accepts_empty_open_application_event() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.apple_event_launch_state
        .set_high_level_event_aware(true);
    let (what, message, _, _, _, _, delivered) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);
    assert!(delivered);
    assert_eq!(what, 23);
    assert_eq!(message, u32::from_be_bytes(*b"aevt"));

    let sender_ptr = 0x2A0400u32;
    let msg_refcon_ptr = 0x2A0500u32;
    let msg_buff_ptr = 0x2A0600u32;
    let msg_len_ptr = 0x2A0700u32;
    bus.write_bytes(sender_ptr, &[0xAA; 252]);
    bus.write_long(msg_refcon_ptr, 0xBBBB_BBBB);
    bus.write_long(msg_len_ptr, 128);
    bus.write_byte(msg_buff_ptr, 0xCC);
    bus.write_word(TEST_SP, 0x0033);
    bus.write_long(TEST_SP + 2, msg_len_ptr);
    bus.write_long(TEST_SP + 6, msg_buff_ptr);
    bus.write_long(TEST_SP + 10, msg_refcon_ptr);
    bus.write_long(TEST_SP + 14, sender_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 18), 0);
    assert_eq!(bus.read_long(msg_len_ptr), 0);
    assert_eq!(bus.read_long(msg_refcon_ptr), 0);
    assert_eq!(bus.read_byte(msg_buff_ptr), 0xCC);
    assert_eq!(bus.read_long(sender_ptr), u32::MAX);
    assert_eq!(bus.read_word(sender_ptr + 4), 0);
    assert_eq!(bus.read_byte(sender_ptr + 6), 5);
    assert_eq!(bus.read_bytes(sender_ptr + 7, 5), b"MacOS");
    assert_eq!(bus.read_byte(sender_ptr + 251), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(TEST_SP + 18) as i16, -608);
}

#[test]
fn osdispatch_getspecifichighlevelevent_selector_0045_without_port_returns_false() {
    // MPW 3.5 EPPC.h emits filter, context, err pointer, then a one-byte
    // Boolean result slot around selector $0045. A direct application with
    // no registered high-level-event port follows BasiliskII's noPortErr
    // branch without invoking the filter.
    let (mut disp, mut cpu, mut bus) = setup();

    let filter_ptr = 0x0003_4000u32;
    let context_ptr = 0x2A_7000u32;
    let err_ptr = 0x2A_7100u32;
    bus.write_word(err_ptr, 0x7777);
    bus.write_word(TEST_SP, 0x0045);
    bus.write_long(TEST_SP + 2, filter_ptr);
    bus.write_long(TEST_SP + 6, context_ptr);
    bus.write_long(TEST_SP + 10, err_ptr);
    bus.write_byte(TEST_SP + 14, 0xFF);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(err_ptr) as i16, -903, "noPortErr");
    assert_eq!(bus.read_byte(TEST_SP + 14), 0, "FALSE result");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 14,
        "OSDispatch should pop selector and three pointer arguments"
    );
}

#[test]
fn osdispatch_posthighlevelevent_selector_0034_without_delivery_returns_connectioninvalid() {
    let (mut disp, mut cpu, mut bus) = setup();

    let event_ptr = 0x2A0800u32;
    let receiver_ptr = 0x2A0900u32;
    let msg_buff_ptr = 0x2A0A00u32;
    bus.write_word(event_ptr, 23);
    bus.write_long(event_ptr + 2, 0x7465_7374);
    bus.write_word(TEST_SP, 0x0034);
    bus.write_long(TEST_SP + 2, 0x0000_8000); // receiverIDisPSN
    bus.write_long(TEST_SP + 6, 4);
    bus.write_long(TEST_SP + 10, msg_buff_ptr);
    bus.write_long(TEST_SP + 14, 0x1234_5678);
    bus.write_long(TEST_SP + 18, receiver_ptr);
    bus.write_long(TEST_SP + 22, event_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(
        bus.read_word(TEST_SP + 26) as i16,
        -609,
        "connectionInvalid"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 26);
    assert!(
        disp.event_queue.is_empty(),
        "PostHighLevelEvent should not enqueue an event it cannot later deliver"
    );
}

#[test]
fn osdispatch_getprocessserialnumberfromportname_selector_0035_returns_noporterr() {
    let (mut disp, mut cpu, mut bus) = setup();

    let port_name_ptr = 0x2A0B00u32;
    let psn_ptr = 0x2A0C00u32;
    bus.write_long(psn_ptr, 0xAAAA_AAAA);
    bus.write_long(psn_ptr + 4, 0xBBBB_BBBB);
    bus.write_word(TEST_SP, 0x0035);
    bus.write_long(TEST_SP + 2, psn_ptr);
    bus.write_long(TEST_SP + 6, port_name_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -903, "noPortErr");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(
        bus.read_long(psn_ptr),
        0xAAAA_AAAA,
        "PSN should be untouched when the port name is unknown"
    );
    assert_eq!(bus.read_long(psn_ptr + 4), 0xBBBB_BBBB);
}

#[test]
fn osdispatch_launchdeskaccessory_selector_0036_returns_resnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();

    let fsspec_ptr = 0x2A0D00u32;
    let da_name_ptr = 0x2A0E00u32;
    bus.write_word(TEST_SP, 0x0036);
    bus.write_long(TEST_SP + 2, da_name_ptr);
    bus.write_long(TEST_SP + 6, fsspec_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -192, "resNotFound");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
}

#[test]
fn osdispatch_getcurrentprocess_selector_0037_returns_current_psn() {
    let (mut disp, mut cpu, mut bus) = setup();

    let psn_ptr = 0x2A0000u32;
    bus.write_word(TEST_SP, 0x0037);
    bus.write_long(TEST_SP + 2, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(psn_ptr), 0, "highLongOfPSN");
    assert_eq!(
        bus.read_long(psn_ptr + 4),
        2,
        "lowLongOfPSN = kCurrentProcess"
    );
    assert_eq!(bus.read_word(TEST_SP + 6), 0, "noErr result");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
}

#[test]
fn osdispatch_getnextprocess_selector_0038_from_knoprocess_returns_first_process() {
    let (mut disp, mut cpu, mut bus) = setup();

    let psn_ptr = 0x2A0100u32;
    bus.write_long(psn_ptr, 0);
    bus.write_long(psn_ptr + 4, 0); // kNoProcess
    bus.write_word(TEST_SP, 0x0038);
    bus.write_long(TEST_SP + 2, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(psn_ptr), 0);
    assert_eq!(bus.read_long(psn_ptr + 4), 2, "first process PSN");
    assert_eq!(bus.read_word(TEST_SP + 6), 0, "noErr result");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
}

#[test]
fn osdispatch_getnextprocess_selector_0038_end_of_list_returns_procnotfound_and_knoprocess() {
    let (mut disp, mut cpu, mut bus) = setup();

    let psn_ptr = 0x2A0200u32;
    bus.write_long(psn_ptr, 0);
    bus.write_long(psn_ptr + 4, 2); // current process PSN
    bus.write_word(TEST_SP, 0x0038);
    bus.write_long(TEST_SP + 2, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 6) as i16, -600, "procNotFound");
    assert_eq!(bus.read_long(psn_ptr), 0);
    assert_eq!(bus.read_long(psn_ptr + 4), 0, "kNoProcess at end of list");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
}

#[test]
fn osdispatch_getfrontprocess_selector_0039_returns_foreground_psn() {
    // MPW GetFrontProcess glue pushes a 4-byte init slot ($FFFFFFFF) before
    // psn_ptr. After selector pop: sp+0=init(4B), sp+4=psn_ptr(4B),
    // sp+8=result(2B). A7 advances to sp+8 so MOVE.W (SP)+,D0 restores stack.
    let (mut disp, mut cpu, mut bus) = setup();

    let psn_ptr = 0x2A0300u32;
    bus.write_word(TEST_SP, 0x0039);
    bus.write_long(TEST_SP + 2, 0xFFFFFFFF); // MPW init slot
    bus.write_long(TEST_SP + 6, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_long(psn_ptr), 0);
    assert_eq!(bus.read_long(psn_ptr + 4), 2, "foreground process PSN");
    assert_eq!(bus.read_word(TEST_SP + 10), 0, "noErr result");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
}

#[test]
fn osdispatch_getprocessinformation_invalid_psn_returns_procnotfound() {
    let (mut disp, mut cpu, mut bus) = setup();

    let info_ptr = 0x2A0400u32;
    let psn_ptr = 0x2A0500u32;
    bus.write_long(psn_ptr, 0);
    bus.write_long(psn_ptr + 4, 3); // invalid in single-process HLE
    bus.write_word(TEST_SP, 0x003A);
    bus.write_long(TEST_SP + 2, info_ptr);
    bus.write_long(TEST_SP + 6, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 10) as i16, -600, "procNotFound");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
}

#[test]
fn osdispatch_getprocessinformation_current_process_returns_populated_info() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_vfs_entry_metadata("Games/Current App", *b"APPL", *b"GAME", 0);
    disp.set_launched_app_path("Games/Current App");

    let info_ptr = 0x2A0900u32;
    let psn_ptr = 0x2A0A00u32;
    let name_ptr = 0x2A0B00u32;
    let app_spec_ptr = 0x2A0C00u32;
    let expected_psn_high = 0;
    let expected_psn_low = 2;

    bus.write_long(psn_ptr, expected_psn_high);
    bus.write_long(psn_ptr + 4, expected_psn_low);
    bus.write_word(info_ptr, 60);
    bus.write_long(info_ptr + 4, name_ptr);
    bus.write_long(info_ptr + 56, app_spec_ptr);
    bus.write_word(TEST_SP, 0x003A);
    bus.write_long(TEST_SP + 2, info_ptr);
    bus.write_long(TEST_SP + 6, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 10), 0, "noErr");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    assert_eq!(bus.read_long(info_ptr + 8), expected_psn_high);
    assert_eq!(bus.read_long(info_ptr + 12), expected_psn_low);
    assert_eq!(bus.read_long(info_ptr + 16), u32::from_be_bytes(*b"APPL"));
    assert_eq!(bus.read_long(info_ptr + 20), u32::from_be_bytes(*b"GAME"));
    assert_eq!(bus.read_long(info_ptr + 24), 0, "processMode");
    assert_eq!(bus.read_long(info_ptr + 28), 0x0010_0000, "processLocation");
    assert_eq!(bus.read_long(info_ptr + 32), bus.ram_size(), "processSize");
    assert_eq!(
        bus.read_long(info_ptr + 36),
        bus.ram_size() / 2,
        "processFreeMem"
    );
    assert_eq!(
        bus.read_long(info_ptr + 40),
        0,
        "processLauncher.highLongOfPSN"
    );
    assert_eq!(
        bus.read_long(info_ptr + 44),
        0,
        "processLauncher.lowLongOfPSN"
    );
    assert_eq!(bus.read_long(info_ptr + 48), 0, "processLaunchDate");
    assert_eq!(bus.read_pstring(name_ptr), b"Current App");
    assert_ne!(bus.read_word(app_spec_ptr), 0, "processAppSpec.vRefNum");
}

#[test]
fn osdispatch_getprocessinformation_selector_only_thunk_preserves_return_address() {
    let (mut disp, mut cpu, mut bus) = setup();

    let trap_return_pc = 0x0003_4112u32;
    let thunk_return_pc = 0x0003_2F36u32;
    let saved_caller_a6 = 0x03FF_FFECu32;
    disp.register_segments(HashMap::from([
        (1i16, 0x0003_1A00u32),
        (2i16, 0x0003_8298u32),
    ]));
    cpu.write_reg(Register::PC, trap_return_pc);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x003A);
    bus.write_long(TEST_SP + 2, thunk_return_pc);
    bus.write_long(TEST_SP + 6, saved_caller_a6);
    bus.write_long(TEST_SP + 10, 0x1234_5678);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP), 0, "selector slot becomes noErr");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "selector-only thunks expect MOVE.W (SP)+ to reveal the return PC"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 2),
        thunk_return_pc,
        "OSDispatch must not consume or overwrite the JSR return address"
    );
    assert_eq!(bus.read_long(TEST_SP + 6), saved_caller_a6);
    assert_eq!(cpu.read_reg(Register::D0), 0);
}

#[test]
fn osdispatch_getprocessinformation_reports_initialized_partition_and_free_heap() {
    let (mut disp, mut cpu, mut bus) = setup();

    let info_ptr = 0x2A0D00u32;
    let psn_ptr = 0x2A0E00u32;
    let app_zone = 0x0020_0000u32;
    let heap_end = app_zone + 64;
    let appl_limit = 0x0050_0000u32;
    bus.write_long(crate::memory::globals::addr::MEM_TOP, 32 * 1024 * 1024);
    bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
    bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
    bus.write_long(psn_ptr, 0);
    bus.write_long(psn_ptr + 4, 2);
    bus.write_word(info_ptr, 60);
    bus.write_word(TEST_SP, 0x003A);
    bus.write_long(TEST_SP + 2, info_ptr);
    bus.write_long(TEST_SP + 6, psn_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_word(TEST_SP + 10), 0, "noErr");
    assert_eq!(bus.read_long(info_ptr + 28), app_zone, "processLocation");
    assert_eq!(
        bus.read_long(info_ptr + 32),
        appl_limit - app_zone,
        "processSize"
    );
    assert_eq!(
        bus.read_long(info_ptr + 36),
        appl_limit - heap_end,
        "processFreeMem"
    );
}

#[test]
fn osdispatch_sameprocess_selector_003d_writes_boolean_result() {
    let (mut disp, mut cpu, mut bus) = setup();

    let psn1_ptr = 0x2A0600u32;
    let psn2_ptr = 0x2A0700u32;
    let result_ptr = 0x2A0800u32;

    // Equal PSNs => TRUE
    bus.write_long(psn1_ptr, 0);
    bus.write_long(psn1_ptr + 4, 2);
    bus.write_long(psn2_ptr, 0);
    bus.write_long(psn2_ptr + 4, 2);
    bus.write_byte(result_ptr, 0xFF);
    bus.write_word(TEST_SP, 0x003D);
    bus.write_long(TEST_SP + 2, result_ptr);
    bus.write_long(TEST_SP + 6, psn2_ptr);
    bus.write_long(TEST_SP + 10, psn1_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_byte(result_ptr), 1, "equal PSNs => TRUE");
    assert_eq!(bus.read_word(TEST_SP + 14), 0, "noErr result");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);

    // Different PSNs => FALSE
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(psn2_ptr + 4, 0);
    bus.write_byte(result_ptr, 0xFF);
    bus.write_word(TEST_SP, 0x003D);
    bus.write_long(TEST_SP + 2, result_ptr);
    bus.write_long(TEST_SP + 6, psn2_ptr);
    bus.write_long(TEST_SP + 10, psn1_ptr);

    call(&mut disp, true, 0x08F, &mut cpu, &mut bus).unwrap();

    assert_eq!(bus.read_byte(result_ptr), 0, "different PSNs => FALSE");
    assert_eq!(bus.read_word(TEST_SP + 14), 0, "noErr result");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
}
