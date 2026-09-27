use super::*;
use crate::loader::pef::PefResolvedImport;

// Low-level allocator tests borrow adapter fields independently. Keep the
// cursor canonical by publishing the temporary value when each call ends.
macro_rules! test_heap_cursor {
    ($app:ident) => {
        &mut *$app.process_memory_manager.heap_cursor_mut()
    };
}

macro_rules! test_handles {
    ($app:ident) => {
        &mut *$app.process_memory_manager.handles_mut()
    };
}

macro_rules! test_handle_records {
    ($app:ident) => {
        $app.process_memory_manager.handles()
    };
}

// Read the canonical heap limit through the disjoint manager field.
macro_rules! test_heap_limit {
    ($app:ident) => {
        $app.process_memory_manager.heap_limit($app.stack_base)
    };
}

macro_rules! with_test_screen_clut {
    ($app:ident, |$screen:ident| $body:expr) => {{
        let screen_clut = $app.screen_clut.shared_handle();
        screen_clut.with_mut(|$screen| $body)
    }};
}

macro_rules! with_test_color_manager_clut {
    ($app:ident, |$logical:ident| $body:expr) => {{
        let color_manager_clut = $app.color_manager_clut.shared_handle();
        color_manager_clut.with_mut(|$logical| $body)
    }};
}

macro_rules! with_test_display_cluts {
    ($app:ident, |$screen:ident, $logical:ident| $body:expr) => {{
        let screen_clut = $app.screen_clut.shared_handle();
        let color_manager_clut = $app.color_manager_clut.shared_handle();
        screen_clut.with_mut(|$screen| {
            color_manager_clut.with_mut(|$logical| $body)
        })
    }};
}

macro_rules! with_test_controls {
    ($app:ident, |$controls:ident| $body:expr) => {{
        let controls = $app.controls.shared_handle();
        controls.with_mut(|$controls| $body)
    }};
}
use crate::cpu::{CpuOps, Register};
use crate::managers::resource::serialize_resource_fork;
use crate::memory::MemoryBus;
use crate::trap::test_helpers::{setup_with_port, TEST_SP};
use ppc::PpcMemory;

fn test_q3_object(object: u32, object_type: u32) -> PpcQ3ObjectRecord {
    PpcQ3ObjectRecord {
        object,
        kind: PpcQ3ObjectKind::Generic,
        object_type,
        source: PpcQ3ObjectSource::default(),
        data_ptr: 0,
        data_size: 0,
    }
}

fn run_test_import(loaded: &mut PpcLoadedApp, target: PpcImportDispatcherTarget) {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    drain_test_m68k_guest_calls(loaded);
}

fn initialize_test_cgraf_port(bus: &mut MacMemoryBus, port: u32) {
    bus.write_word(port + 6, 0xc000);
    for offset in [36, 38, 40] {
        bus.write_word(port + offset, 0);
    }
    for offset in [42, 44, 46] {
        bus.write_word(port + offset, u16::MAX);
    }
    bus.write_word(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET, 1);
    bus.write_word(port + PPC_CGRAF_PORT_PN_SIZE_OFFSET + 2, 1);
    bus.write_word(port + PPC_CGRAF_PORT_PN_MODE_OFFSET, PPC_QD_PEN_MODE_PAT_COPY as u16);
    bus.write_word(
        port + PPC_CGRAF_PORT_TX_MODE_OFFSET,
        PPC_QD_TEXT_MODE_SRC_OR as u16,
    );
    bus.write_word(
        port + PPC_CGRAF_PORT_TX_SIZE_OFFSET,
        PPC_QD_TEXT_SIZE_SYSTEM as u16,
    );
}

fn run_test_import_with_process_memory_manager(
    loaded: &mut PpcLoadedApp,
    target: PpcImportDispatcherTarget,
    memory_manager: &SharedProcessMemoryManager,
) {
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = target;
    let probe = loaded.run_with_process_memory_manager(
        64,
        false,
        false,
        &mut memory_manager.borrow_mut(),
    );
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    drain_test_m68k_guest_calls(loaded);
}

fn attach_test_classic_heap(
    native: &mut PpcLoadedApp,
    context: &mut ProcessContext,
    ram_size: usize,
    heap_len: usize,
) -> MacMemoryBus {
    let mut classic_bus = MacMemoryBus::new(ram_size);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, heap_len as u32)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);
    classic_bus
}

fn drain_test_m68k_guest_calls(loaded: &mut PpcLoadedApp) {
    while let Some(pending) = loaded.guest_calls()
        .active_m68k()
        .or_else(|| loaded.guest_calls().activate_m68k())
    {
        let mut cpu = m68k::CpuCore::new();
        cpu.set_cpu_type(REFERENCE_MACHINE_PROFILE.cpu_type());
        cpu.pc = pending.entry;
        cpu.set_a(7, pending.initial_sp);
        for (index, value) in pending.registers.data.into_iter().enumerate() {
            cpu.set_d(index, value);
        }
        for (index, value) in pending.registers.address.into_iter().enumerate() {
            cpu.set_a(index, value);
        }
        let result = cpu.run_batch(&mut loaded.memory, 4096, &[pending.return_pc]);
        assert_eq!(
            result.exit,
            m68k::BatchExit::WatchedPc {
                pc: pending.return_pc
            },
            "entry=${:08X} pc=${:08X} sp=${:08X} return=${:08X}",
            pending.entry,
            cpu.pc,
            cpu.a(7),
            pending.return_pc,
        );
        let result = match pending.result {
            None => None,
            Some(crate::guest_call::M68kResultSource::Data(index)) => {
                Some(cpu.d(usize::from(index)))
            }
            Some(crate::guest_call::M68kResultSource::Address(index)) => {
                Some(cpu.a(usize::from(index)))
            }
            Some(crate::guest_call::M68kResultSource::Memory { address, size }) => {
                Some(match size {
                    1 => u32::from(loaded.memory.read_u8(address).unwrap()),
                    2 => u32::from(loaded.memory.read_u16_be(address).unwrap()),
                    4 => loaded.memory.read_u32_be(address).unwrap(),
                    _ => panic!("unsupported 68k result size {size}"),
                })
            }
            Some(crate::guest_call::M68kResultSource::SpecialCase { selector, .. }) => {
                panic!("special-case result selector {selector} requires the shared runner")
            }
        };
        assert!(loaded
            .toolbox_startup
            .execution
            .calls()
            .complete_m68k_operation_for_powerpc(
            cpu.pc,
            cpu.a(7),
            result,
            &mut loaded.cpu,
            &mut loaded.memory,
            loaded.process_memory_manager.0.borrow_mut().native_mut(),
        ));

        if loaded.cpu.pc == PPC_GUEST_CALL_RETURN_PC
            || (loaded.cpu.pc >= loaded.import_trap_base
                && loaded.cpu.pc
                    < loaded
                        .import_trap_base
                        .saturating_add(loaded.import_count.saturating_mul(4)))
        {
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.unsupported_import_index, None);
        }
    }
}

mod palette_manager;
mod gworld;
mod blit;

mod file_manager;
mod event_manager;

mod text_edit;
mod scrap_manager;
mod list_manager;

mod standard_c_library;
mod toolbox_utilities;

mod thread_manager;

mod process_manager;


mod font_manager;

mod gestalt;

mod display_depth;

mod desk_manager;

mod device_manager;

#[path = "native_exceptions.rs"]
mod native_exceptions;

mod collection_manager;

mod draw_sprocket;

mod input_sprocket;
mod qd3d;
mod menu_manager;
pub(crate) use menu_manager::*;
mod window_manager;
mod mixed_mode;
pub(crate) use mixed_mode::*;
mod quicktime;
mod memory_manager;
mod resource_manager;
mod process_services;
mod trap_manager;
mod code_fragment_manager;
pub(crate) use code_fragment_manager::synthetic_pef_with_enumerable_exports;
mod quickdraw;



mod sound_manager;
fn establish_loaded_reservation(loaded: &mut PpcLoadedApp, address: u32) {
    const LWARX_R12_R4_R5: u32 =
        (31 << 26) | (12 << 21) | (4 << 16) | (5 << 11) | (20 << 1);
    let preserved = (
        loaded.cpu.pc,
        loaded.cpu.gpr[4],
        loaded.cpu.gpr[5],
        loaded.cpu.gpr[12],
    );
    loaded.cpu.gpr[4] = address;
    loaded.cpu.gpr[5] = 0;
    assert_eq!(
        loaded.cpu.step(&mut loaded.memory, LWARX_R12_R4_R5),
        ppc::PpcStepResult::Stepped
    );
    assert_eq!(loaded.cpu.reservation_address(), Some(address));
    (
        loaded.cpu.pc,
        loaded.cpu.gpr[4],
        loaded.cpu.gpr[5],
        loaded.cpu.gpr[12],
    ) = preserved;
}

mod interrupt_callbacks;

mod dialog_parameters;
mod dialog_manager;
mod control_manager;

mod loader_configuration;

pub(crate) fn synthetic_pef() -> Vec<u8> {
    synthetic_pef_with_import(b"TestImport")
}

fn synthetic_pef_with_initializer() -> Vec<u8> {
    let mut pef = synthetic_pef();
    // The synthetic loader section begins at $80. Reuse its main TVector
    // as a valid initializer TVector so loading exercises CFM's startup
    // fragment and InitBlock allocations without executing the code.
    write_i32(&mut pef, 0x80 + 8, 1);
    write_u32(&mut pef, 0x80 + 12, 0);
    pef
}

fn assert_ppc_bytes_equal(memory: &mut PpcSectionMem, start: u32, len: u32, expected: u8) {
    for offset in 0..len {
        assert_eq!(memory.read_u8(start + offset), Some(expected));
    }
}

mod standard_c_library_integration;

pub(crate) fn synthetic_pef_with_import(symbol_name: &[u8]) -> Vec<u8> {
    synthetic_pef_with_library_import(b"InterfaceLib", symbol_name)
}

fn synthetic_pef_with_import_class(symbol_name: &[u8], symbol_class: u8) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"InterfaceLib",
        symbol_name,
        symbol_class,
        &[sm_index_reloc(0x30, 0)],
    ))
}

fn write_test_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
    context_attributes: PpcDspContextAttributes,
) {
    memory
        .write_u32_be(attributes, context_attributes.frequency)
        .unwrap();
    memory
        .write_u32_be(attributes + 4, context_attributes.width)
        .unwrap();
    memory
        .write_u32_be(attributes + 8, context_attributes.height)
        .unwrap();
    memory
        .write_u32_be(attributes + 28, context_attributes.context_options)
        .unwrap();
    memory
        .write_u32_be(
            attributes + 32,
            context_attributes.back_buffer_best_depth_mask,
        )
        .unwrap();
    memory
        .write_u32_be(attributes + 36, context_attributes.display_best_depth_mask)
        .unwrap();
    memory
        .write_u32_be(attributes + 40, context_attributes.back_buffer_depth)
        .unwrap();
    memory
        .write_u32_be(attributes + 44, context_attributes.display_depth)
        .unwrap();
    memory
        .write_u32_be(attributes + 48, context_attributes.page_count)
        .unwrap();
}

fn synthetic_pef_with_library_import(library_name: &[u8], symbol_name: &[u8]) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader(library_name, symbol_name))
}

fn synthetic_pef_with_reloc_chunks(
    library_name: &[u8],
    symbol_name: &[u8],
    chunks: &[u16],
) -> Vec<u8> {
    synthetic_pef_with_loader(synthetic_loader_with_chunks(
        library_name,
        symbol_name,
        chunks,
    ))
}

fn synthetic_pef_with_loader(loader: Vec<u8>) -> Vec<u8> {
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

fn synthetic_pef_with_loader_and_data(loader: Vec<u8>, data: &[u8]) -> Vec<u8> {
    let code = synthetic_code();
    let loader_offset = 0x80usize;
    let code_offset = align_test_offset((loader_offset + loader.len()).max(0x100), 0x10);
    let data_offset = code_offset + code.len();
    let total_len = data_offset + data.len();
    let mut bytes = vec![0u8; total_len.max(loader_offset + loader.len())];

    bytes[0..4].copy_from_slice(b"Joy!");
    bytes[4..8].copy_from_slice(b"peff");
    bytes[8..12].copy_from_slice(b"pwpc");
    write_u32(&mut bytes, 12, 1);
    write_u16(&mut bytes, 32, 3);
    write_u16(&mut bytes, 34, 2);

    write_section(
        &mut bytes,
        0,
        SectionSpec {
            total_size: code.len() as u32,
            unpacked_size: code.len() as u32,
            packed_size: code.len() as u32,
            container_offset: code_offset as u32,
            section_kind: SECTION_KIND_CODE,
        },
    );
    write_section(
        &mut bytes,
        1,
        SectionSpec {
            total_size: data.len() as u32,
            unpacked_size: data.len() as u32,
            packed_size: data.len() as u32,
            container_offset: data_offset as u32,
            section_kind: SECTION_KIND_UNPACKED_DATA,
        },
    );
    write_section(
        &mut bytes,
        2,
        SectionSpec {
            total_size: 0,
            unpacked_size: 0,
            packed_size: loader.len() as u32,
            container_offset: loader_offset as u32,
            section_kind: super::super::pef::SECTION_KIND_LOADER,
        },
    );

    bytes[loader_offset..loader_offset + loader.len()].copy_from_slice(&loader);
    bytes[code_offset..code_offset + code.len()].copy_from_slice(&code);
    bytes[data_offset..data_offset + data.len()].copy_from_slice(&data);
    bytes
}

fn align_test_offset(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn synthetic_code() -> Vec<u8> {
    let mut code = Vec::new();
    let hi = (PPC_IMPORT_TRAP_BASE >> 16) as u16;
    let lo = (PPC_IMPORT_TRAP_BASE & 0xffff) as u16;
    for word in [
        d_form_u(15, 12, 0, hi),       // lis r12, trap@ha
        d_form_u(24, 12, 12, lo),      // ori r12, r12, trap@l
        xfx_form(31, 12, 9, 467),      // mtctr r12
        xl_form(19, 20, 0, 528, true), // bctrl
        d_form_u(14, 0, 0, 0),         // li r0, 0
        xfx_form(31, 0, 8, 467),       // mtlr r0
        BLR,
    ] {
        code.extend_from_slice(&word.to_be_bytes());
    }
    code
}

fn synthetic_loader(library_name: &[u8], symbol_name: &[u8]) -> Vec<u8> {
    let chunks = [run_reloc(0x23, 1)];
    synthetic_loader_with_chunks(library_name, symbol_name, &chunks)
}

fn synthetic_loader_with_chunks(
    library_name: &[u8],
    symbol_name: &[u8],
    chunks: &[u16],
) -> Vec<u8> {
    synthetic_loader_with_symbol_class(library_name, symbol_name, 0x02, chunks)
}

fn synthetic_loader_with_symbol_class(
    library_name: &[u8],
    symbol_name: &[u8],
    symbol_class: u8,
    chunks: &[u16],
) -> Vec<u8> {
    let mut strings = Vec::new();
    let library_name = push_c_string(&mut strings, library_name);
    let symbol_name = push_c_string(&mut strings, symbol_name);
    let reloc_header_offset = 56 + 24 + 4;
    let reloc_instr_offset = reloc_header_offset + 12;
    let strings_offset = reloc_instr_offset + chunks.len() * 2;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_u32(&mut bytes, 4, 0);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 1);
    write_u32(&mut bytes, 28, 1);
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, 36, reloc_instr_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);

    write_u32(&mut bytes, 56, library_name);
    write_u32(&mut bytes, 56 + 12, 1);
    write_symbol(&mut bytes, 56 + 24, symbol_class, symbol_name);

    write_u16(&mut bytes, reloc_header_offset, 1);
    write_u32(&mut bytes, reloc_header_offset + 4, chunks.len() as u32);
    write_u32(&mut bytes, reloc_header_offset + 8, 0);
    for (index, chunk) in chunks.iter().enumerate() {
        write_u16(&mut bytes, reloc_instr_offset + index * 2, *chunk);
    }

    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_loader_with_repeated_imports(import_count: u32) -> Vec<u8> {
    let mut strings = Vec::new();
    let library_name = push_c_string(&mut strings, b"InterfaceLib");
    let symbol_name = push_c_string(&mut strings, b"TickCount");
    let symbol_count = usize::try_from(import_count).unwrap();
    let strings_offset = 56 + 24 + symbol_count * 4;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 1);
    write_u32(&mut bytes, 28, import_count);
    write_u32(&mut bytes, 36, strings_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);
    write_u32(&mut bytes, 56, library_name);
    write_u32(&mut bytes, 56 + 12, import_count);
    for index in 0..symbol_count {
        write_symbol(&mut bytes, 56 + 24 + index * 4, 2, symbol_name);
    }
    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_loader_with_overlapping_library_ranges() -> Vec<u8> {
    let mut strings = Vec::new();
    let first_library = push_c_string(&mut strings, b"InterfaceLib");
    let second_library = push_c_string(&mut strings, b"StdCLib");
    let symbol = push_c_string(&mut strings, b"errno");
    let reloc_header_offset = 56 + 2 * 24 + 4;
    let reloc_instr_offset = reloc_header_offset + 12;
    let strings_offset = reloc_instr_offset + 2;
    let mut bytes = vec![0u8; strings_offset + strings.len()];

    write_i32(&mut bytes, 0, 1);
    write_i32(&mut bytes, 8, -1);
    write_i32(&mut bytes, 16, -1);
    write_u32(&mut bytes, 24, 2);
    write_u32(&mut bytes, 28, 1);
    write_u32(&mut bytes, 32, 1);
    write_u32(&mut bytes, 36, reloc_instr_offset as u32);
    write_u32(&mut bytes, 40, strings_offset as u32);

    write_u32(&mut bytes, 56, first_library);
    write_u32(&mut bytes, 56 + 12, 1);
    write_u32(&mut bytes, 56 + 24, second_library);
    write_u32(&mut bytes, 56 + 24 + 12, 1);
    write_symbol(&mut bytes, 56 + 2 * 24, 2, symbol);

    write_u16(&mut bytes, reloc_header_offset, 1);
    write_u32(&mut bytes, reloc_header_offset + 4, 1);
    write_u32(&mut bytes, reloc_header_offset + 8, 0);
    write_u16(&mut bytes, reloc_instr_offset, sm_index_reloc(0x30, 0));
    bytes[strings_offset..].copy_from_slice(&strings);
    bytes
}

fn synthetic_pef_with_exports() -> Vec<u8> {
    let names = [b"tvector".as_slice(), b"absolute", b"reexport"];
    let mut strings = Vec::new();
    let mut name_offsets = Vec::new();
    for name in names {
        name_offsets.push(strings.len() as u32);
        strings.extend_from_slice(name);
        strings.push(0);
    }

    let strings_offset = 56usize;
    let export_hash_offset = strings_offset + strings.len();
    let key_table_offset = export_hash_offset + 4;
    let symbol_table_offset = key_table_offset + names.len() * 4;
    let loader_len = symbol_table_offset + names.len() * 10;
    let mut loader = vec![0; loader_len];
    write_i32(&mut loader, 0, -1);
    write_i32(&mut loader, 8, -1);
    write_i32(&mut loader, 16, -1);
    write_u32(&mut loader, 40, strings_offset as u32);
    write_u32(&mut loader, 44, export_hash_offset as u32);
    write_u32(&mut loader, 48, 0);
    write_u32(&mut loader, 52, names.len() as u32);
    loader[strings_offset..export_hash_offset].copy_from_slice(&strings);

    // All three entries occupy one valid hash chain. The resolver uses
    // the flattened export table, but the parser still validates chains.
    write_u32(&mut loader, export_hash_offset, 3 << 18);
    for (index, name) in names.iter().enumerate() {
        write_u32(
            &mut loader,
            key_table_offset + index * 4,
            (name.len() as u32) << 16,
        );
    }
    let records = [
        (2u8, name_offsets[0], 0u32, 1i16),
        (1u8, name_offsets[1], 0x1234_5678, -2i16),
        (2u8, name_offsets[2], 0u32, -3i16),
    ];
    for (index, (class, name_offset, value, section_index)) in records.into_iter().enumerate() {
        let base = symbol_table_offset + index * 10;
        write_u32(&mut loader, base, (u32::from(class) << 24) | name_offset);
        write_u32(&mut loader, base + 4, value);
        write_u16(&mut loader, base + 8, section_index as u16);
    }
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

fn write_ppc_pstring(memory: &mut PpcSectionMem, addr: u32, bytes: &[u8]) {
    assert!(bytes.len() <= 255);
    memory.write_u8(addr, bytes.len() as u8).unwrap();
    for (offset, byte) in bytes.iter().enumerate() {
        memory.write_u8(addr + 1 + offset as u32, *byte).unwrap();
    }
}

fn compressed_resource_bytes(decompressed_len: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\xA8\x9Fer");
    out.extend_from_slice(&0x12u16.to_be_bytes());
    out.extend_from_slice(&0x0801u16.to_be_bytes());
    out.extend_from_slice(&decompressed_len.to_be_bytes());
    out.extend_from_slice(&[0x80, 0x03]);
    out.extend_from_slice(&0i16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn noncanonical_single_resource_fork_bytes(
    res_type: [u8; 4],
    res_id: i16,
    data: &[u8],
    attrs: u8,
    map_attrs: u16,
) -> Vec<u8> {
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
    bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
    bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

    let type_list_start = map_start + type_list_offset as usize;
    bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
    bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
    bytes[type_list_start + 8..type_list_start + 10]
        .copy_from_slice(&ref_list_offset.to_be_bytes());

    let ref_list_start = type_list_start + ref_list_offset as usize;
    bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
    bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[ref_list_start + 4] = attrs;
    bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);
    bytes
}

fn test_sound_file_playback(channel: u32) -> PpcSoundFilePlaybackRecord {
    PpcSoundFilePlaybackRecord {
        channel,
        ref_num: PPC_FIRST_FILE_REF_NUM,
        resource_id: 0,
        buffer_size: 0,
        buffer: 0,
        selection: 0,
        completion: 0,
        completion_command: None,
        async_play: false,
        aiff: None,
        decoded_aiff: None,
    }
}

fn write_ppc_fsspec(
    memory: &mut PpcSectionMem,
    addr: u32,
    vref: i16,
    dir_id: u32,
    name: &[u8],
) {
    memory.write_u16_be(addr, vref as u16).unwrap();
    memory.write_u32_be(addr + 2, dir_id).unwrap();
    write_ppc_pstring(memory, addr + 6, name);
}

pub(crate) fn test_v1_one_bit_packbits_pict() -> Vec<u8> {
    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    let mut bytes = Vec::new();
    push_u16(&mut bytes, 0); // patched picSize
    for value in [0i16, 0, 1, 8] {
        push_u16(&mut bytes, value as u16);
    }
    bytes.push(0x11); // versionOp
    bytes.push(0x01); // version 1
    bytes.push(0x98); // PackBitsRect
    push_u16(&mut bytes, 1); // rowBytes < 8: unpacked bitmap data
    for value in [0i16, 0, 1, 8] {
        push_u16(&mut bytes, value as u16);
    }
    for _ in 0..2 {
        for value in [0i16, 0, 1, 8] {
            push_u16(&mut bytes, value as u16);
        }
    }
    push_u16(&mut bytes, 0); // srcCopy
    bytes.push(0b1010_0000);
    bytes.push(0xff); // EndOfPicture
    let size = u16::try_from(bytes.len()).unwrap();
    bytes[0..2].copy_from_slice(&size.to_be_bytes());
    bytes
}

fn test_one_bit_cicon() -> Vec<u8> {
    let mut data = vec![0u8; 101];
    data[4..6].copy_from_slice(&0x8001u16.to_be_bytes());
    data[10..12].copy_from_slice(&1u16.to_be_bytes());
    data[12..14].copy_from_slice(&8u16.to_be_bytes());
    data[32..34].copy_from_slice(&1u16.to_be_bytes());
    data[34..36].copy_from_slice(&1u16.to_be_bytes());
    data[36..38].copy_from_slice(&1u16.to_be_bytes());

    data[54..56].copy_from_slice(&1u16.to_be_bytes());
    data[60..62].copy_from_slice(&1u16.to_be_bytes());
    data[62..64].copy_from_slice(&8u16.to_be_bytes());
    data[68..70].copy_from_slice(&1u16.to_be_bytes());
    data[74..76].copy_from_slice(&1u16.to_be_bytes());
    data[76..78].copy_from_slice(&8u16.to_be_bytes());

    data[82] = 0xf0; // first four pixels are opaque
    data[83] = 0x00; // monochrome fallback
    data[90..92].copy_from_slice(&0u16.to_be_bytes()); // one ColorSpec
    data[92..94].copy_from_slice(&0u16.to_be_bytes()); // pixel value 0
    data[94..96].copy_from_slice(&0xffffu16.to_be_bytes()); // red
    data[96..98].copy_from_slice(&0u16.to_be_bytes());
    data[98..100].copy_from_slice(&0u16.to_be_bytes());
    data[100] = 0x00; // eight one-bit red pixels
    data
}

fn compatibility_binding(
    library_name: &str,
    symbol_name: &str,
    dispatcher_target: PpcImportDispatcherTarget,
) -> PpcImportBinding {
    PpcImportBinding {
        library_index: 0,
        symbol_index: 0,
        library_name: library_name.to_string(),
        symbol_name: symbol_name.to_string(),
        class: 0,
        weak: false,
        address: 0,
        tvector_address: None,
        trap_pc: 0,
        dispatcher_target,
    }
}

mod apple_event_handlers;

mod apple_event_descriptors;

mod hardware_compatibility;

mod math_compatibility;

mod fixmath;

struct SectionSpec {
    total_size: u32,
    unpacked_size: u32,
    packed_size: u32,
    container_offset: u32,
    section_kind: u8,
}

fn write_section(bytes: &mut [u8], index: usize, spec: SectionSpec) {
    let off = 40 + index * 28;
    write_i32(bytes, off, -1);
    write_u32(bytes, off + 8, spec.total_size);
    write_u32(bytes, off + 12, spec.unpacked_size);
    write_u32(bytes, off + 16, spec.packed_size);
    write_u32(bytes, off + 20, spec.container_offset);
    bytes[off + 24] = spec.section_kind;
    bytes[off + 25] = 4;
    bytes[off + 26] = 4;
}

fn push_c_string(strings: &mut Vec<u8>, value: &[u8]) -> u32 {
    let offset = strings.len() as u32;
    strings.extend_from_slice(value);
    strings.push(0);
    offset
}

fn write_symbol(bytes: &mut [u8], offset: usize, class_byte: u8, name_offset: u32) {
    bytes[offset] = class_byte;
    bytes[offset + 1] = ((name_offset >> 16) & 0xff) as u8;
    bytes[offset + 2] = ((name_offset >> 8) & 0xff) as u8;
    bytes[offset + 3] = (name_offset & 0xff) as u8;
}

fn run_reloc(dispatch: u8, run_length: u16) -> u16 {
    (u16::from(dispatch) << 9) | ((run_length - 1) & 0x01ff)
}

fn sm_index_reloc(dispatch: u8, index: u16) -> u16 {
    (u16::from(dispatch) << 9) | (index & 0x01ff)
}

fn delt(offset: u16) -> u16 {
    (0x40u16 << 9) | ((offset - 1) & 0x0fff)
}

fn d_form_u(opcd: u8, rt: u8, ra: u8, value: u16) -> u32 {
    ((opcd as u32) << 26)
        | ((rt as u32 & 0x1f) << 21)
        | ((ra as u32 & 0x1f) << 16)
        | u32::from(value)
}

fn x_form(opcd: u8, rt: u8, ra: u8, rb: u8, xo: u16, rc: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((rt as u32 & 0x1f) << 21)
        | ((ra as u32 & 0x1f) << 16)
        | ((rb as u32 & 0x1f) << 11)
        | ((xo as u32 & 0x3ff) << 1)
        | u32::from(rc)
}

fn a_form_fp(opcd: u8, frt: u8, fra: u8, frb: u8, frc: u8, xo_5: u8, rc: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((frt as u32 & 0x1f) << 21)
        | ((fra as u32 & 0x1f) << 16)
        | ((frb as u32 & 0x1f) << 11)
        | ((frc as u32 & 0x1f) << 6)
        | ((xo_5 as u32 & 0x1f) << 1)
        | u32::from(rc)
}

fn xfx_form(opcd: u8, rt_or_rs: u8, spr_decimal: u16, xo: u16) -> u32 {
    let high_5 = (spr_decimal >> 5) & 0x1f;
    let low_5 = spr_decimal & 0x1f;
    ((opcd as u32 & 0x3f) << 26)
        | ((rt_or_rs as u32 & 0x1f) << 21)
        | ((low_5 as u32) << 16)
        | ((high_5 as u32) << 11)
        | ((xo as u32 & 0x3ff) << 1)
}

fn xl_form(opcd: u8, bo: u8, bi: u8, xo: u16, lk: bool) -> u32 {
    ((opcd as u32 & 0x3f) << 26)
        | ((bo as u32 & 0x1f) << 21)
        | ((bi as u32 & 0x1f) << 16)
        | ((xo as u32 & 0x3ff) << 1)
        | u32::from(lk)
}

fn write_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
