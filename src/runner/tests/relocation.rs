use super::*;
use crate::cpu::Register;
use crate::loader::{MpwFarSegmentHeader, Retro68RelocationError};
use crate::managers::resource::ResourceFork;

#[test]
fn load_app_patches_mpw_far_jump_table_offsets_from_segment_start() {
    // Inside Macintosh: Processes 1994, p. 7-8: a loaded MPW jump-table
    // entry keeps the routine offset from the beginning of the segment.
    let mut code0 = minimal_code0(40, 0x2000, 8, 32);
    code0[16..24].copy_from_slice(&[
        0x00, 0x01, // segment 1
        0xA9, 0xF0, // far-model unloaded LoadSeg trap
        0x00, 0x00, 0x00, 0x28, // first routine immediately after the far header
    ]);

    let mut code1 = vec![0u8; 0x30];
    code1[0] = 0xFF;
    code1[1] = 0xFF;
    code1[0x28] = 0x4E;
    code1[0x29] = 0x75;

    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    let jt_base = app.a5_base + app.code0_header.jump_table_offset;
    let code1_base = app.segment_bases[&1];

    assert_eq!(runner.bus.read_word(jt_base), 1);
    assert_eq!(runner.bus.read_word(jt_base + 2), 0x4EF9);
    assert_eq!(
        runner.bus.read_long(jt_base + 4),
        code1_base + 0x28,
        "MPW far offsets must not be adjusted by the 40-byte header twice"
    );
}

#[test]
fn load_app_executes_far_jump_table_routine_above_64k() {
    let offset = 0x0001_A114u32;
    let mut code0 = minimal_code0(40, 0x2000, 8, 32);
    code0[16..20].copy_from_slice(&[0x00, 0x01, 0xA9, 0xF0]);
    code0[20..24].copy_from_slice(&offset.to_be_bytes());

    let mut code1 = vec![0; offset as usize + 4];
    code1[..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
    // Distinct routines expose accidental truncation during execution.
    code1[0xA114..0xA118].copy_from_slice(&[0x70, 0x01, 0x4E, 0x75]);
    code1[offset as usize..].copy_from_slice(&[0x70, 0x2A, 0x4E, 0x75]);
    let bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
    let fork = ResourceFork::parse(&bytes).expect("synthetic far-model resource fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&fork).expect("load far-model application");
    let jt = app.a5_base + app.code0_header.jump_table_offset;

    runner.cpu_mut().write_reg(Register::PC, jt + 2);
    runner.step(); // JMP through the patched slot.
    runner.step(); // MOVEQ from the selected routine.
    assert_eq!(runner.cpu().read_reg(Register::D0), 42);
    assert_eq!(app.jump_table[0].offset, offset);
    assert_eq!(runner.bus.read_long(jt + 4), app.segment_bases[&1] + offset);
}

#[test]
fn load_app_applies_mpw_far_a5_and_pc_relocations() {
    let mut code0 = minimal_code0(40, 0x2000, 8, 32);
    code0[16..24].copy_from_slice(&[
        0x00, 0x01, // segment 1
        0xA9, 0xF0, // far-model unloaded LoadSeg trap
        0x00, 0x00, 0x00, 0x28,
    ]);

    let mut code1 = vec![0u8; 0x130];
    code1[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
    code1[20..24].copy_from_slice(&0x50u32.to_be_bytes());
    code1[28..32].copy_from_slice(&0x54u32.to_be_bytes());
    code1[0x28..0x2A].copy_from_slice(&[0x4E, 0xB9]); // JSR.L absolute
    code1[0x2A..0x2E].copy_from_slice(&0x100u32.to_be_bytes());
    code1[0x30..0x32].copy_from_slice(&[0x20, 0x79]); // MOVEA.L absolute
    code1[0x32..0x36].copy_from_slice(&0x40u32.to_be_bytes());
    code1[0x100..0x104].copy_from_slice(&[0x70, 0x01, 0x4E, 0x75]);
    code1[0x128..0x12C].copy_from_slice(&[0x70, 0x2A, 0x4E, 0x75]);
    code1[0x50..0x54].copy_from_slice(&[
        0x19, // A5 relocation at byte offset 0x32
        0x00, 0x00, 0x00,
    ]);
    code1[0x54..0x57].copy_from_slice(&[
        0x15, // PC relocation at byte offset 0x2A
        0x00, 0x00,
    ]);

    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    let code1_base = app.segment_bases[&1];

    assert_eq!(
        runner.bus.read_long(code1_base + 0x2A),
        code1_base + MpwFarSegmentHeader::SIZE as u32 + 0x100,
        "PC relocation stream must add the loaded code address after the far header"
    );
    runner.cpu_mut().write_reg(Register::PC, code1_base + 0x28);
    runner.step(); // JSR using the relocated code-relative address.
    runner.step(); // MOVEQ at the destination, not 40 bytes before it.
    assert_eq!(runner.cpu().read_reg(Register::D0), 42);
    assert_eq!(
        runner.bus.read_long(code1_base + 0x32),
        app.a5_base + 0x40,
        "A5 relocation stream must add the current A5"
    );
    assert_eq!(
        runner
            .bus
            .read_long(code1_base + MpwFarSegmentHeader::CURRENT_A5_OFFSET),
        app.a5_base
    );
    assert_eq!(
        runner
            .bus
            .read_long(code1_base + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET),
        code1_base
    );
}

#[test]
fn load_app_applies_retro68_code_and_data_relocations() {
    let mut code0 = minimal_code0(40, 0x2000, 8, 32);
    code0[16..24].copy_from_slice(&[
        0x00, 0x01, // segment 1
        0xA9, 0xF0, // far-model unloaded LoadSeg trap
        0x00, 0x00, 0x00, 0x28,
    ]);

    let mut code1 = vec![0u8; 0x60];
    code1[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
    for (offset, value) in [0x20u32, 0x30, 0x40, 0x50, 0x60].into_iter().enumerate() {
        let start = MpwFarSegmentHeader::SIZE + offset * 4;
        code1[start..start + 4].copy_from_slice(&value.to_be_bytes());
    }
    let rela1 = [
        0x04, // absolute offset 0, code base
        0x11, // absolute offset 4, data base
        0x12, // absolute offset 8, bss base
        0x13, // absolute offset 12, jump-table base
        0x00, // absolute pass terminator
        0x44, // PC-relative offset 16, code base
        0x00, // PC-relative pass terminator
    ];

    let mut data = Vec::new();
    for value in [0x10u32, 0x20, 0x30, 0x40] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    let rela0 = [
        0x04, // absolute offset 0, code base
        0x11, // absolute offset 4, data base
        0x12, // absolute offset 8, bss base
        0x13, // absolute offset 12, jump-table base
        0x00, // absolute pass terminator
        0x00, // empty PC-relative pass
    ];

    let fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, &code0),
        (*b"CODE", 1, &code1),
        (*b"DATA", 0, &data),
        (*b"RELA", 0, &rela0),
        (*b"RELA", 1, &rela1),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic Retro68 app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load Retro68 app");
    let code1_base = app.segment_bases[&1];
    let code_body = code1_base + MpwFarSegmentHeader::SIZE as u32;
    let data_base = app.a5_base - app.code0_header.below_a5;

    assert_eq!(runner.bus.read_long(code_body), code1_base + 0x20);
    assert_eq!(runner.bus.read_long(code_body + 4), app.a5_base + 0x30);
    assert_eq!(runner.bus.read_long(code_body + 8), app.a5_base + 0x40);
    assert_eq!(runner.bus.read_long(code_body + 12), app.a5_base + 0x50);
    assert_eq!(
        runner.bus.read_long(code_body + 16),
        0x28,
        "the first PC-relative record must immediately follow the first terminator"
    );
    assert_eq!(runner.bus.read_long(data_base), 0x10);
    assert_eq!(runner.bus.read_long(data_base + 4), app.a5_base + 0x20);
    assert_eq!(runner.bus.read_long(data_base + 8), app.a5_base + 0x30);
    assert_eq!(runner.bus.read_long(data_base + 12), app.a5_base + 0x40);
    assert_eq!(
        runner
            .bus
            .read_long(code1_base + MpwFarSegmentHeader::CURRENT_A5_OFFSET),
        app.a5_base
    );
    assert_eq!(
        runner
            .bus
            .read_long(code1_base + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET),
        code1_base
    );
}

#[test]
fn invalid_retro68_relocations_do_not_partially_modify_memory() {
    let mut runner = FixtureRunner::new(1024 * 1024, FixtureRunnerConfig::default());
    let target = 0x1000;
    runner.bus.write_long(target, 0x20);

    let result = apply_retro68_rela_relocations(
        &mut runner.bus,
        target,
        4,
        &[
            0x04, // valid absolute relocation at offset 0
            0x00, // absolute pass terminator
            0x08, // invalid PC-relative relocation at offset 1
            0x00,
        ],
        [0x100, 0, 0, 0],
    );

    assert_eq!(
        result,
        Err(Retro68RelocationError::TargetOutOfBounds {
            offset: 1,
            target_size: 4,
        })
    );
    assert_eq!(runner.bus.read_long(target), 0x20);
}
