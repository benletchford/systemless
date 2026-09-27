use super::*;

#[test]
fn stdclib_labs_uses_the_32_bit_long_abi() {
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "labs"),
        PpcImportDispatcherTarget::StdAbs
    );
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"labs");
    let mut native = load_pef_application(&pef).unwrap();
    for (argument, expected) in [(37_u32, 37_u32), (-37_i32 as u32, 37), (0, 0)] {
        native.cpu.gpr[3] = argument;
        run_test_import(&mut native, PpcImportDispatcherTarget::StdAbs);
        assert_eq!(native.cpu.gpr[3], expected);
    }
}

#[test]
fn stdclib_allocations_are_immediately_process_owned_and_cross_isa_visible() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"malloc");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    native.set_last_mem_error(PPC_PARAM_ERR);

    native.cpu.gpr[3] = 8;
    run_test_import(&mut native, PpcImportDispatcherTarget::StdMalloc);
    let original = native.cpu.gpr[3];
    assert_ne!(original, 0);
    assert!(context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap()
        .ptrs
        .iter()
        .any(|record| record.ptr == original && record.size == 8));
    assert!(!detached
        .native_allocator()
        .unwrap()
        .ptrs
        .iter()
        .any(|record| record.ptr == original));

    let shared = native.memory.shared_view();
    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);
    classic_bus.write_bytes(original, b"payload!");
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, original, 8),
        Some(b"payload!".to_vec())
    );

    native.cpu.gpr[3] = original;
    native.cpu.gpr[4] = u32::MAX;
    run_test_import(&mut native, PpcImportDispatcherTarget::StdRealloc);
    assert_eq!(native.cpu.gpr[3], 0);
    assert_eq!(classic_bus.read_bytes(original, 8), b"payload!");
    assert!(context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap()
        .ptrs
        .iter()
        .any(|record| record.ptr == original));

    native.cpu.gpr[3] = original;
    native.cpu.gpr[4] = 24;
    run_test_import(&mut native, PpcImportDispatcherTarget::StdRealloc);
    let replacement = native.cpu.gpr[3];
    assert_ne!(replacement, 0);
    assert_ne!(replacement, original);
    assert_eq!(classic_bus.read_bytes(replacement, 8), b"payload!");
    let allocator = context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap();
    assert!(allocator
        .ptrs
        .iter()
        .any(|record| record.ptr == replacement && record.size == 24));
    assert!(!allocator.ptrs.iter().any(|record| record.ptr == original));

    native.cpu.gpr[3] = 4;
    native.cpu.gpr[4] = 4;
    run_test_import(&mut native, PpcImportDispatcherTarget::StdCalloc);
    let zeroed = native.cpu.gpr[3];
    assert_ne!(zeroed, 0);
    assert_eq!(classic_bus.read_bytes(zeroed, 16), vec![0; 16]);
    native.memory.write_u8(zeroed, 0x5a).unwrap();
    assert_eq!(classic_bus.read_byte(zeroed), 0x5a);

    native.cpu.gpr[3] = replacement;
    run_test_import(&mut native, PpcImportDispatcherTarget::StdFree);
    assert!(!context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap()
        .ptrs
        .iter()
        .any(|record| record.ptr == replacement));
    assert!(detached.native_allocator().unwrap().ptrs.is_empty());
    assert_eq!(native.last_mem_error(), PPC_PARAM_ERR);
}

#[test]
fn stdio_records_advance_the_process_owned_heap_immediately() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"fopen");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let heap_before = context
        .memory_manager_mut()
        .native_heap_state()
        .unwrap()
        .heap_cursor;
    let path = PPC_DATA_BASE + 0x2000;
    let mode = path + 0x20;
    native.memory.add_region(path, vec![0; 0x100]);
    native.memory.write_bytes(path, b"test.bin\0").unwrap();
    native.memory.write_bytes(mode, b"rb\0").unwrap();
    native.push_test_vfs_file(PpcVfsFileRecord {
        path: "Volume/test.bin".to_string(),
        data: (b"payload".to_vec()).into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    native.cpu.gpr[3] = path;
    native.cpu.gpr[4] = mode;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileOpen),
    );

    let stream = native.cpu.gpr[3];
    assert_ne!(stream, 0);
    let heap_after = context
        .memory_manager_mut()
        .native_heap_state()
        .unwrap()
        .heap_cursor;
    assert!(heap_after > heap_before);
    assert_eq!(native.heap_cursor(), heap_after);
    assert_eq!(
        detached.native_heap_state().unwrap().heap_cursor,
        heap_before
    );
}

#[test]
fn stdclib_heap_and_string_helpers_cover_guest_abi_edges() {
    for (name, target) in [
        ("malloc", PpcImportDispatcherTarget::StdMalloc),
        ("free", PpcImportDispatcherTarget::StdFree),
        ("calloc", PpcImportDispatcherTarget::StdCalloc),
        ("realloc", PpcImportDispatcherTarget::StdRealloc),
        ("strncpy", PpcImportDispatcherTarget::StdStrncpy),
        ("strncat", PpcImportDispatcherTarget::StdStrncat),
        ("atoi", PpcImportDispatcherTarget::StdAtoi),
        ("getenv", PpcImportDispatcherTarget::StdGetenv),
    ] {
        assert_eq!(dispatcher_target_for_import("StdCLib", name), target);
    }

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"malloc")).unwrap();

    let source = PPC_DATA_BASE + 0x2000;
    let destination = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(source, b"abc\0\0\0".to_vec());
    loaded.memory.add_region(destination, vec![0xaa; 16]);
    assert_eq!(
        ppc_std_strncpy(&mut loaded.memory, destination, source, 6),
        destination
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 6),
        Some(b"abc\0\0\0".to_vec())
    );
    loaded.memory.write_bytes(destination, &[0xaa; 6]).unwrap();
    assert_eq!(
        ppc_std_strncpy(&mut loaded.memory, destination, source, 2),
        destination
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 2),
        Some(b"ab".to_vec())
    );
    assert_eq!(
        ppc_std_strncpy(&mut loaded.memory, destination, source, 0),
        destination
    );
    loaded
        .memory
        .write_bytes(destination, b"xy\0\0\0\0")
        .unwrap();
    assert_eq!(
        ppc_std_strncat(&mut loaded.memory, destination, source, 2),
        destination
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 5),
        Some(b"xyab\0".to_vec())
    );
    assert_eq!(
        ppc_std_strncat(&mut loaded.memory, destination, source, 0),
        destination
    );

    let atoi_ptr = PPC_DATA_BASE + 0x2200;
    loaded.memory.add_region(atoi_ptr, vec![0; 32]);
    loaded.memory.write_bytes(atoi_ptr, b"  -123x\0").unwrap();
    assert_eq!(ppc_std_atoi(&mut loaded.memory, atoi_ptr), (-123i32) as u32);
    loaded
        .memory
        .write_bytes(atoi_ptr, b"2147483648\0")
        .unwrap();
    assert_eq!(ppc_std_atoi(&mut loaded.memory, atoi_ptr), i32::MAX as u32);
    loaded.memory.write_bytes(atoi_ptr, b"nope\0").unwrap();
    assert_eq!(ppc_std_atoi(&mut loaded.memory, atoi_ptr), 0);
    assert_eq!(
        PpcImportDispatcherTarget::StdGetenv,
        dispatcher_target_for_import("StdCLib", "getenv")
    );
}

#[test]
fn stdclib_string_search_helpers_cover_guest_abi_edges() {
    for (name, target) in [
        ("memchr", PpcImportDispatcherTarget::StdMemchr),
        ("strchr", PpcImportDispatcherTarget::StdStrchr),
        ("strrchr", PpcImportDispatcherTarget::StdStrrchr),
        ("strspn", PpcImportDispatcherTarget::StdStrspn),
        ("strcspn", PpcImportDispatcherTarget::StdStrcspn),
        ("strpbrk", PpcImportDispatcherTarget::StdStrpbrk),
        ("strstr", PpcImportDispatcherTarget::StdStrstr),
    ] {
        assert_eq!(dispatcher_target_for_import("StdCLib", name), target);
    }

    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"memchr"))
            .unwrap();
    let string = PPC_DATA_BASE + 0x2400;
    let set = string + 0x40;
    let reject = string + 0x50;
    let needle = string + 0x60;
    let empty = string + 0x80;
    loaded.memory.add_region(string, vec![0; 0x100]);
    loaded.memory.write_bytes(string, b"abc123abc\0").unwrap();
    loaded.memory.write_bytes(set, b"abc\0").unwrap();
    loaded.memory.write_bytes(reject, b"13\0").unwrap();
    loaded.memory.write_bytes(needle, b"123a\0").unwrap();

    assert_eq!(
        ppc_std_memchr(&mut loaded.memory, string, b'c', 3),
        string + 2
    );
    assert_eq!(ppc_std_memchr(&mut loaded.memory, string, b'c', 2), 0);
    assert_eq!(ppc_std_memchr(&mut loaded.memory, string, b'a', 0), 0);
    assert_eq!(
        ppc_std_strchr(&mut loaded.memory, string, b'a', false),
        string
    );
    assert_eq!(
        ppc_std_strchr(&mut loaded.memory, string, b'a', true),
        string + 6
    );
    assert_eq!(
        ppc_std_strchr(&mut loaded.memory, string, 0, false),
        string + 9
    );
    assert_eq!(
        ppc_std_strchr(&mut loaded.memory, string, 0, true),
        string + 9
    );
    assert_eq!(ppc_std_strchr(&mut loaded.memory, string, b'z', false), 0);
    assert_eq!(ppc_std_strspn(&mut loaded.memory, string, set, true), 3);
    assert_eq!(ppc_std_strspn(&mut loaded.memory, string, reject, false), 3);
    assert_eq!(ppc_std_strspn(&mut loaded.memory, string, empty, true), 0);
    assert_eq!(ppc_std_strspn(&mut loaded.memory, string, empty, false), 9);
    assert_eq!(
        ppc_std_strpbrk(&mut loaded.memory, string, reject),
        string + 3
    );
    assert_eq!(ppc_std_strpbrk(&mut loaded.memory, string, empty), 0);
    assert_eq!(
        ppc_std_strstr(&mut loaded.memory, string, needle),
        string + 3
    );
    assert_eq!(ppc_std_strstr(&mut loaded.memory, string, empty), string);
    loaded.memory.write_bytes(needle, b"missing\0").unwrap();
    assert_eq!(ppc_std_strstr(&mut loaded.memory, string, needle), 0);

    for (name, argument, count, expected) in [
        (b"memchr".as_slice(), 0x163, 9, string + 2),
        (b"strchr".as_slice(), u32::from(b'a'), 0, string),
        (b"strrchr".as_slice(), u32::from(b'a'), 0, string + 6),
        (b"strspn".as_slice(), set, 0, 3),
        (b"strcspn".as_slice(), reject, 0, 3),
        (b"strpbrk".as_slice(), reject, 0, string + 3),
        (b"strstr".as_slice(), needle, 0, string + 3),
    ] {
        let mut imported =
            load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", name)).unwrap();
        imported.memory.add_region(string, vec![0; 0x100]);
        imported.memory.write_bytes(string, b"abc123abc\0").unwrap();
        imported.memory.write_bytes(set, b"abc\0").unwrap();
        imported.memory.write_bytes(reject, b"13\0").unwrap();
        imported.memory.write_bytes(needle, b"123a\0").unwrap();
        imported.cpu.gpr[3] = string;
        imported.cpu.gpr[4] = argument;
        imported.cpu.gpr[5] = count;
        imported.cpu.pc = imported.entry_pc;
        imported.cpu.lr = PPC_HALT_PC;

        let result = imported.run_with_hle_imports(64);

        assert_eq!(result.handled_import_count, 1, "{}", decode_mac_roman(name));
        assert_eq!(imported.cpu.gpr[3], expected, "{}", decode_mac_roman(name));
    }
}

#[test]
fn stdclib_stdio_uses_host_metadata_and_preserves_the_guest_iob_layout() {
    let mut loaded = load_pef_application(&synthetic_pef_with_loader(
        synthetic_loader_with_symbol_class(b"StdCLib", b"_iob", 1, &[sm_index_reloc(0x30, 0)]),
    ))
    .unwrap();
    assert_eq!(loaded.imports[0].address, PPC_STDIO_IOB_ADDR);
    assert_eq!(loaded.process_file_system.stdio_streams.len(), 3);
    assert!(loaded
        .stdio_streams
        .contains_key(&(PPC_STDIO_IOB_ADDR + PPC_STDIO_FILE_SIZE)));
    assert!(loaded
        .stdio_streams
        .contains_key(&(PPC_STDIO_IOB_ADDR + 2 * PPC_STDIO_FILE_SIZE)));
    assert_eq!(
        ppc_memory_read_bytes(
            &mut loaded.memory,
            PPC_STDIO_IOB_ADDR,
            3 * PPC_STDIO_FILE_SIZE,
        ),
        Some(vec![0; (3 * PPC_STDIO_FILE_SIZE) as usize])
    );
    let mut stdio_streams = loaded
        .process_file_system
        .with_mut(|file_system| std::mem::take(&mut file_system.stdio_streams));

    let mut cpu = PpcCpu::new();
    let mut files = Vec::new();
    let mut vfs_files: ProcessVfsFileRecords = vec![PpcVfsFileRecord {
        path: "Volume/test.bin".to_string(),
        data: (b"abcdef".to_vec()).into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    }]
    .into();
    let mut next_file_ref_num = PPC_FIRST_FILE_REF_NUM;
    let path = PPC_DATA_BASE + 0x2000;
    let mode = PPC_DATA_BASE + 0x2020;
    let destination = PPC_DATA_BASE + 0x2040;
    loaded.memory.add_region(path, vec![0; 0x100]);
    loaded.memory.write_bytes(path, b"test.bin\0").unwrap();
    loaded.memory.write_bytes(mode, b"rb\0").unwrap();

    cpu.gpr[3] = path;
    cpu.gpr[4] = mode;
    let PpcImportAction::Return(stream) = ppc_dispatch_stdio_compatibility(
        PpcStdIoOperation::FileOpen,
        &mut cpu,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut files,
        &mut vfs_files,
        &mut next_file_ref_num,
        &mut stdio_streams,
    ) else {
        panic!("fopen did not return a stream");
    };
    assert_ne!(stream, 0);

    cpu.gpr[3] = destination;
    cpu.gpr[4] = 2;
    cpu.gpr[5] = 2;
    cpu.gpr[6] = stream;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileRead,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(2)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 4),
        Some(b"abcd".to_vec())
    );

    loaded.memory.write_bytes(destination, b"XY").unwrap();
    cpu.gpr[3] = destination;
    cpu.gpr[4] = 1;
    cpu.gpr[5] = 2;
    cpu.gpr[6] = stream;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileWrite,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(0)
    );
    assert_eq!(vfs_files[0].data, b"abcdef");

    loaded.memory.write_bytes(mode, b"r+\0").unwrap();
    cpu.gpr[3] = path;
    cpu.gpr[4] = mode;
    let PpcImportAction::Return(read_write_stream) = ppc_dispatch_stdio_compatibility(
        PpcStdIoOperation::FileOpen,
        &mut cpu,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut files,
        &mut vfs_files,
        &mut next_file_ref_num,
        &mut stdio_streams,
    ) else {
        panic!("fopen did not return the read/write stream");
    };
    loaded.memory.write_bytes(destination, b"XY").unwrap();
    cpu.gpr[3] = destination;
    cpu.gpr[4] = 1;
    cpu.gpr[5] = 2;
    cpu.gpr[6] = read_write_stream;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileWrite,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(2)
    );
    assert_eq!(vfs_files[0].data, b"XYcdef");

    let format = PPC_DATA_BASE + 0x2300;
    loaded.memory.add_region(format, b"%20000d\0".to_vec());
    cpu.gpr[3] = read_write_stream;
    cpu.gpr[4] = format;
    cpu.gpr[5] = 0;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FilePrintf,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(u32::MAX)
    );
    assert_eq!(vfs_files[0].data, b"XYcdef");

    loaded.memory.write_bytes(mode, b"invalid\0").unwrap();
    cpu.gpr[3] = path;
    cpu.gpr[4] = mode;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileOpen,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(0)
    );
    assert_eq!(vfs_files.len(), 1);
    assert_eq!(vfs_files[0].data, b"XYcdef");

    loaded.memory.write_bytes(path, b"missing.bin\0").unwrap();
    loaded.memory.write_bytes(mode, b"r+\0").unwrap();
    cpu.gpr[3] = path;
    cpu.gpr[4] = mode;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileOpen,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(0)
    );
    assert_eq!(vfs_files.len(), 1);

    loaded.memory.write_bytes(mode, b"w\0").unwrap();
    loaded.memory.write_bytes(path, b"test.bin\0").unwrap();
    let saved_heap_cursor = loaded.heap_cursor();
    loaded.set_heap_cursor(test_heap_limit!(loaded).saturating_sub(8));
    cpu.gpr[3] = path;
    cpu.gpr[4] = mode;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileOpen,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(0)
    );
    loaded.set_heap_cursor(saved_heap_cursor);
    assert_eq!(vfs_files[0].data, b"XYcdef");

    cpu.gpr[3] = destination;
    cpu.gpr[4] = 1;
    cpu.gpr[5] = 1;
    cpu.gpr[6] = PPC_STDIO_IOB_ADDR + PPC_STDIO_FILE_SIZE;
    loaded.memory.write_bytes(destination, b"X").unwrap();
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileWrite,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(1)
    );
    cpu.gpr[3] = 0xdead_beef;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileWrite,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(0)
    );
    assert!(stdio_streams
        .get(&(PPC_STDIO_IOB_ADDR + PPC_STDIO_FILE_SIZE))
        .is_some_and(|record| record.error));

    cpu.gpr[3] = stream;
    assert_eq!(
        ppc_dispatch_stdio_compatibility(
            PpcStdIoOperation::FileTell,
            &mut cpu,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut files,
            &mut vfs_files,
            &mut next_file_ref_num,
            &mut stdio_streams,
        ),
        PpcImportAction::Return(4)
    );

    for stream in [
        stream,
        read_write_stream,
        PPC_STDIO_IOB_ADDR + PPC_STDIO_FILE_SIZE,
        PPC_STDIO_IOB_ADDR + 2 * PPC_STDIO_FILE_SIZE,
    ] {
        cpu.gpr[3] = stream;
        assert_eq!(
            ppc_dispatch_stdio_compatibility(
                PpcStdIoOperation::FileClose,
                &mut cpu,
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                &mut files,
                &mut vfs_files,
                &mut next_file_ref_num,
                &mut stdio_streams,
            ),
            PpcImportAction::Return(0)
        );
    }
}

#[test]
fn stdclib_ctype_classification_and_conversion_conforms_to_universal_interfaces() {
    // Universal Interfaces 3.4 ctype.h routes.
    let expected_targets = [
        ("isalnum", PpcImportDispatcherTarget::StdIsalnum),
        ("isalpha", PpcImportDispatcherTarget::StdIsalpha),
        ("isascii", PpcImportDispatcherTarget::StdIsascii),
        ("iscntrl", PpcImportDispatcherTarget::StdIscntrl),
        ("isdigit", PpcImportDispatcherTarget::StdIsdigit),
        ("isgraph", PpcImportDispatcherTarget::StdIsgraph),
        ("islower", PpcImportDispatcherTarget::StdIslower),
        ("isprint", PpcImportDispatcherTarget::StdIsprint),
        ("ispunct", PpcImportDispatcherTarget::StdIspunct),
        ("isspace", PpcImportDispatcherTarget::StdIsspace),
        ("isupper", PpcImportDispatcherTarget::StdIsupper),
        ("isxdigit", PpcImportDispatcherTarget::StdIsxdigit),
        ("toascii", PpcImportDispatcherTarget::StdToascii),
    ];

    for (name, target) in &expected_targets {
        assert_eq!(
            dispatcher_target_for_import("StdCLib", name),
            *target,
            "Import StdCLib::{name} should map to expected dispatcher target"
        );
    }

    // Verify character classification flags across the complete byte domain.
    for byte in 0u8..=255 {
        let entry = ppc_ctype_entry(byte);
        let is_upp = (b'A'..=b'Z').contains(&byte);
        let is_low = (b'a'..=b'z').contains(&byte);
        let is_dig = (b'0'..=b'9').contains(&byte);
        let is_wsp = matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r');
        let is_pun = matches!(byte, 33..=47 | 58..=64 | 91..=96 | 123..=126);
        let is_ctl = matches!(byte, 0..=31 | 127);
        let is_hex = is_dig || (b'a'..=b'f').contains(&byte) || (b'A'..=b'F').contains(&byte);
        let is_bla = byte == b' ';

        assert_eq!(
            (entry & PPC_CTYPE_UPP) != 0,
            is_upp,
            "byte 0x{byte:02x} UPP flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_LOW) != 0,
            is_low,
            "byte 0x{byte:02x} LOW flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_DIG) != 0,
            is_dig,
            "byte 0x{byte:02x} DIG flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_WSP) != 0,
            is_wsp,
            "byte 0x{byte:02x} WSP flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_PUN) != 0,
            is_pun,
            "byte 0x{byte:02x} PUN flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_CTL) != 0,
            is_ctl,
            "byte 0x{byte:02x} CTL flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_HEX) != 0,
            is_hex,
            "byte 0x{byte:02x} HEX flag"
        );
        assert_eq!(
            (entry & PPC_CTYPE_BLA) != 0,
            is_bla,
            "byte 0x{byte:02x} BLA flag"
        );

        if byte >= 128 {
            assert_eq!(entry, 0, "non-ASCII byte 0x{byte:02x} must be 0");
        }
    }

    // Execute every route through a synthetic PEF import.
    for (name, target) in &expected_targets {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"StdCLib",
            name.as_bytes(),
        ))
        .unwrap();

        assert_eq!(loaded.imports[0].dispatcher_target, *target);
        assert_eq!(
            loaded.memory.read_u32_be(PPC_IMPORT_CTYPE_POINTER),
            Some(PPC_IMPORT_CTYPE_TABLE)
        );
        for byte in 0u16..=255 {
            assert_eq!(
                loaded
                    .memory
                    .read_u8(PPC_IMPORT_CTYPE_TABLE + u32::from(byte)),
                Some(ppc_ctype_entry(byte as u8))
            );
        }

        let test_inputs: &[(u32, u32)] = match target {
            PpcImportDispatcherTarget::StdIsalnum => &[
                (u32::from(b'a'), u32::from(PPC_CTYPE_LOW)),
                (u32::from(b'Z'), u32::from(PPC_CTYPE_UPP)),
                (u32::from(b'5'), u32::from(PPC_CTYPE_DIG)),
                (u32::from(b'!'), 0),
                (u32::from(b' '), 0),
                (0, 0),
                (0x80, 0),
                (0xFF, 0),
            ],
            PpcImportDispatcherTarget::StdIsalpha => &[
                (u32::from(b'a'), u32::from(PPC_CTYPE_LOW)),
                (u32::from(b'Z'), u32::from(PPC_CTYPE_UPP)),
                (u32::from(b'5'), 0),
                (u32::from(b'?'), 0),
                (0, 0),
            ],
            PpcImportDispatcherTarget::StdIsascii => &[
                (0, 1),
                (0x41, 1),
                (0x7F, 1),
                (0x80, 0),
                (0xFF, 0),
                (0x100, 0),
                (0x1234_5678, 0),
            ],
            PpcImportDispatcherTarget::StdIscntrl => &[
                (0, u32::from(PPC_CTYPE_CTL)),
                (0x1F, u32::from(PPC_CTYPE_CTL)),
                (0x7F, u32::from(PPC_CTYPE_CTL)),
                (u32::from(b' '), 0),
                (u32::from(b'A'), 0),
            ],
            PpcImportDispatcherTarget::StdIsdigit => &[
                (u32::from(b'0'), u32::from(PPC_CTYPE_DIG)),
                (u32::from(b'9'), u32::from(PPC_CTYPE_DIG)),
                (u32::from(b'/'), 0),
                (u32::from(b':'), 0),
                (u32::from(b'a'), 0),
            ],
            PpcImportDispatcherTarget::StdIsgraph => &[
                (u32::from(b'!'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'~'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'a'), u32::from(PPC_CTYPE_LOW)),
                (u32::from(b'A'), u32::from(PPC_CTYPE_UPP)),
                (u32::from(b'0'), u32::from(PPC_CTYPE_DIG)),
                (u32::from(b' '), 0),
                (0x1F, 0),
                (0x7F, 0),
                (0x80, 0),
            ],
            PpcImportDispatcherTarget::StdIslower => &[
                (u32::from(b'a'), u32::from(PPC_CTYPE_LOW)),
                (u32::from(b'z'), u32::from(PPC_CTYPE_LOW)),
                (u32::from(b'A'), 0),
                (u32::from(b'0'), 0),
                (u32::from(b' '), 0),
            ],
            PpcImportDispatcherTarget::StdIsprint => &[
                (u32::from(b' '), u32::from(PPC_CTYPE_BLA)),
                (u32::from(b'!'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'~'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'a'), u32::from(PPC_CTYPE_LOW)),
                (0x1F, 0),
                (0x7F, 0),
                (0x80, 0),
            ],
            PpcImportDispatcherTarget::StdIspunct => &[
                (u32::from(b'!'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'/'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b':'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'@'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'['), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'`'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'{'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'~'), u32::from(PPC_CTYPE_PUN)),
                (u32::from(b'a'), 0),
                (u32::from(b'0'), 0),
                (u32::from(b' '), 0),
            ],
            PpcImportDispatcherTarget::StdIsspace => &[
                (u32::from(b' '), u32::from(PPC_CTYPE_WSP)),
                (u32::from(b'\t'), u32::from(PPC_CTYPE_WSP)),
                (u32::from(b'\n'), u32::from(PPC_CTYPE_WSP)),
                (0x0B, u32::from(PPC_CTYPE_WSP)),
                (0x0C, u32::from(PPC_CTYPE_WSP)),
                (u32::from(b'\r'), u32::from(PPC_CTYPE_WSP)),
                (u32::from(b'a'), 0),
                (0, 0),
            ],
            PpcImportDispatcherTarget::StdIsupper => &[
                (u32::from(b'A'), u32::from(PPC_CTYPE_UPP)),
                (u32::from(b'Z'), u32::from(PPC_CTYPE_UPP)),
                (u32::from(b'a'), 0),
                (u32::from(b'0'), 0),
                (u32::from(b' '), 0),
            ],
            PpcImportDispatcherTarget::StdIsxdigit => &[
                (u32::from(b'0'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'9'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'a'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'f'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'A'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'F'), u32::from(PPC_CTYPE_HEX)),
                (u32::from(b'g'), 0),
                (u32::from(b'G'), 0),
                (u32::from(b' '), 0),
            ],
            PpcImportDispatcherTarget::StdToascii => &[
                (0x41, 0x41),
                (0xC1, 0x41),
                (0x1234_5678, 0x78),
                (0xFF, 0x7F),
                (u32::MAX, 0x7F),
                (0, 0),
            ],
            _ => unreachable!(),
        };

        for &(input, expected) in test_inputs {
            loaded.cpu.gpr[3] = input;
            run_test_import(&mut loaded, target.clone());
            assert_eq!(
                loaded.cpu.gpr[3], expected,
                "{name}(0x{input:02x}) expected 0x{expected:x}, got 0x{:x}",
                loaded.cpu.gpr[3]
            );
        }
        if !matches!(target, PpcImportDispatcherTarget::StdToascii) {
            loaded.cpu.gpr[3] = u32::MAX;
            run_test_import(&mut loaded, target.clone());
            assert_eq!(loaded.cpu.gpr[3], 0, "{name}(EOF)");
        }
    }
}
