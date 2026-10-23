use super::*;
use crate::systems::macintosh::host_ui::{FileDialogAction, FileDialogReply};

fn reply(loaded: &mut PpcLoadedApp, action: FileDialogAction) {
    let snapshot = loaded.host_file_dialog().expect("host dialog is visible");
    let reply = FileDialogReply {
        id: snapshot.id,
        revision: snapshot.revision,
        action,
    };
    assert!(loaded.reply_host_file_dialog(reply.clone()));
    assert!(!loaded.reply_host_file_dialog(reply));
}

#[test]
fn host_file_dialog_powerpc_open_and_cancel_preserve_reply_abi() {
    for cancel in [false, true] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"StandardGetFile")).unwrap();
        loaded.toolbox_startup.host_ui.enabled = true;
        let result = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(result, vec![0xaa; 88]);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Document".into(),
            data: b"test".to_vec().into(),
            creator: u32::from_be_bytes(*b"TEST"),
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0x4000,
            dirty: false,
        });
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = u32::MAX;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = result;
        assert!(matches!(
            loaded.run_with_hle_imports(64).result,
            PpcRunResult::CycleLimit { .. }
        ));
        let snapshot = loaded.host_file_dialog().unwrap();
        let index = snapshot
            .entries
            .iter()
            .position(|e| e.name == "Document")
            .unwrap();
        reply(
            &mut loaded,
            if cancel {
                FileDialogAction::Cancel
            } else {
                FileDialogAction::Open(index)
            },
        );
        assert!(matches!(
            loaded.run_with_hle_imports(64).result,
            PpcRunResult::Halted { .. }
        ));
        assert_eq!(loaded.memory.read_u8(result), Some(u8::from(!cancel)));
        assert!(loaded.host_file_dialog().is_none());
        if !cancel {
            assert_eq!(
                loaded.memory.read_u32_be(result + 2),
                Some(u32::from_be_bytes(*b"TEXT"))
            );
            assert_eq!(loaded.memory.read_u16_be(result + 78), Some(0x4000));
            assert_eq!(
                ppc_read_pstring_bytes(&mut loaded.memory, result + 12),
                Some(b"Document".to_vec())
            );
        }
    }
}

#[test]
fn host_file_dialog_powerpc_save_navigates_virtual_directories() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"StandardPutFile")).unwrap();
    loaded.toolbox_startup.host_ui.enabled = true;
    let result = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(result, vec![0xaa; 88]);
    loaded.vfs_directories.push(PpcVfsDirectory {
        dir_id: 42,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Saves".into(),
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: false,
    });
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = result;
    assert!(matches!(
        loaded.run_with_hle_imports(64).result,
        PpcRunResult::CycleLimit { .. }
    ));
    let snapshot = loaded.host_file_dialog().unwrap();
    let index = snapshot
        .entries
        .iter()
        .position(|e| e.name == "Saves" && e.directory)
        .unwrap();
    reply(&mut loaded, FileDialogAction::Open(index));
    assert!(matches!(
        loaded.run_with_hle_imports(64).result,
        PpcRunResult::CycleLimit { .. }
    ));
    assert_eq!(loaded.host_file_dialog().unwrap().directory, "Saves");
    reply(&mut loaded, FileDialogAction::Save("Pilot".into()));
    assert!(matches!(
        loaded.run_with_hle_imports(64).result,
        PpcRunResult::Halted { .. }
    ));
    assert_eq!(loaded.memory.read_u8(result), Some(1));
    assert_eq!(loaded.memory.read_u32_be(result + 8), Some(42));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, result + 12),
        Some(b"Pilot".to_vec())
    );
}

#[test]
fn host_file_dialog_powerpc_keeps_modal_visible_during_guest_filter_callbacks() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CustomGetFile")).unwrap();
    loaded.toolbox_startup.host_ui.enabled = true;
    let callback = PPC_DATA_BASE + 0x2000;
    let result = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(callback, vec![0; 8]);
    loaded.memory.add_region(result, vec![0xaa; 88]);
    loaded.memory.write_u32_be(callback, 0x4800_0000).unwrap(); // b .
    loaded
        .memory
        .write_u32_be(callback + 4, 0x4e80_0020)
        .unwrap(); // blr
    loaded.push_test_vfs_file(PpcVfsFileRecord {
        path: "Document".into(),
        data: b"test".to_vec().into(),
        creator: 0,
        file_type: u32::from_be_bytes(*b"TEXT"),
        finder_flags: 0,
        dirty: false,
    });
    loaded.cpu.gpr[3] = callback;
    loaded.cpu.gpr[4] = u32::MAX;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = result;
    assert!(matches!(
        loaded.run_with_hle_imports(64).result,
        PpcRunResult::CycleLimit { .. }
    ));
    let preparing = loaded.host_file_dialog().unwrap();
    assert!(preparing.busy);
    assert!(preparing.entries.is_empty());
    assert!(!loaded.reply_host_file_dialog(FileDialogReply {
        id: preparing.id,
        revision: preparing.revision,
        action: FileDialogAction::Cancel
    }));
    loaded.memory.write_u32_be(callback, 0x3860_0000).unwrap(); // li r3,0: include the file
    assert!(matches!(
        loaded.run_with_hle_imports(1024).result,
        PpcRunResult::CycleLimit { .. }
    ));
    let ready = loaded.host_file_dialog().unwrap();
    assert!(!ready.busy);
    assert_eq!(ready.id, preparing.id);
    assert!(ready.entries.iter().any(|e| e.name == "Document"));
    reply(&mut loaded, FileDialogAction::Cancel);
    assert!(matches!(
        loaded.run_with_hle_imports(64).result,
        PpcRunResult::Halted { .. }
    ));
}
