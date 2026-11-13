use super::*;
use crate::managers::resource::ResourceFork;
use crate::memory::globals::addr;

#[test]
fn vfs_summary_hashes_observe_same_size_fork_edits() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.dispatcher.vfs.insert("Save".into(), b"hello".to_vec());
    runner.dispatcher.vfs_rsrc.insert("Save".into(), b"hello".to_vec());
    let before = runner.vfs_file_summary("Save").unwrap();
    assert_eq!(before.data_hash, 0xa430_d846_80aa_bd0b);
    assert_eq!(before.resource_hash, before.data_hash);
    runner.dispatcher.vfs.with_entry_mut("Save", |bytes| bytes[0] = b'j');
    runner.dispatcher.vfs_rsrc.with_entry_mut("Save", |bytes| bytes[0] = b'j');
    let after = runner.vfs_file_summary("Save").unwrap();
    assert_eq!(after.data_len, before.data_len);
    assert_ne!(after.data_hash, before.data_hash);
    assert_eq!(after.resource_hash, after.data_hash);
    assert_eq!(runner.vfs_file_summary("Save").unwrap(), after);
}

#[test]
fn vfs_file_snapshot_round_trips_both_forks_and_metadata() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher
        .vfs
        .insert("Pilots/Test Pilot".to_string(), vec![1, 2, 3]);
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Pilots/Test Pilot".to_string(), vec![4, 5, 6, 7]);
    runner
        .dispatcher
        .vfs
        .insert("__rsrc__Pilots/Test Pilot".to_string(), vec![0xEE]);
    runner
        .dispatcher
        .vfs
        .insert("Game Data/Shapes".to_string(), vec![0xAA; 1024]);
    runner
        .dispatcher
        .set_vfs_entry_metadata("Pilots/Test Pilot", *b"PIL ", *b"EVO!", 0x4000);
    runner
        .dispatcher
        .set_vfs_entry_metadata("Game Data/Shapes", *b"shap", *b"26.2", 0);

    let summaries = runner.vfs_file_summaries();
    assert_eq!(summaries.len(), 2);
    assert!(summaries
        .iter()
        .any(|summary| summary.path == "Game Data/Shapes"));

    let summaries = runner.vfs_file_summaries_where(|path| path.starts_with("Pilots/"));
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].path, "Pilots/Test Pilot");
    assert_eq!(summaries[0].data_len, 3);
    assert_eq!(summaries[0].resource_len, 4);
    assert_eq!(summaries[0].file_type, u32::from_be_bytes(*b"PIL "));
    assert_eq!(summaries[0].creator, u32::from_be_bytes(*b"EVO!"));

    let snapshot = runner
        .vfs_file_snapshot("Pilots/Test Pilot")
        .expect("snapshot");
    assert_eq!(snapshot.data_fork, vec![1, 2, 3]);
    assert_eq!(snapshot.resource_fork, vec![4, 5, 6, 7]);
    assert_eq!(snapshot.finder_flags, 0x4000);

    let mut restored = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    restored.import_vfs_file(&snapshot);
    assert_eq!(
        restored.vfs_file_snapshot("Pilots/Test Pilot"),
        Some(snapshot)
    );

    assert!(restored.remove_vfs_file("Pilots/Test Pilot"));
    assert_eq!(restored.vfs_file_snapshot("Pilots/Test Pilot"), None);
}

#[test]
fn import_vfs_file_relative_to_launched_app_mounts_under_app_parent() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher
        .set_launched_app_path("EV Override/EV Override");
    let plugin = VfsFileSnapshot {
        path: "Warblade".to_string(),
        data_fork: vec![0, 128, 255, 4],
        resource_fork: vec![1, 2, 3, 4],
        file_type: u32::from_be_bytes(*b"Op.f"),
        creator: u32::from_be_bytes(*b"Es.O"),
        finder_flags: 0x4000,
        created_date: 123,
        modified_date: 456,
    };

    runner
        .import_vfs_file_relative_to_launched_app("EV Plug-Ins", &plugin)
        .expect("relative plugin import");

    let mounted = runner
        .vfs_file_snapshot("EV Override/EV Plug-Ins/Warblade")
        .expect("mounted plugin snapshot");
    assert_eq!(mounted.data_fork, plugin.data_fork);
    assert_eq!(mounted.resource_fork, plugin.resource_fork);
    assert_eq!(mounted.created_date, plugin.created_date);
    assert_eq!(mounted.modified_date, plugin.modified_date);
    assert_eq!(mounted.file_type, plugin.file_type);
    assert_eq!(mounted.creator, plugin.creator);
    assert_eq!(mounted.finder_flags, plugin.finder_flags);

    let parent_dir_id = runner
        .dispatcher
        .vfs_metadata
        .get("EV Override/EV Plug-Ins/Warblade")
        .expect("plugin metadata")
        .parent_dir_id;
    let entries = runner.dispatcher.list_vfs_catalog_entries(parent_dir_id);
    assert!(entries
        .iter()
        .any(|entry| !entry.is_directory && entry.name == "Warblade"));
}

#[test]
fn map_vfs_file_preserves_source_forks_metadata_and_rejects_collisions() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let source = VfsFileSnapshot {
        path: "INDYDEMO.000".to_string(),
        data_fork: vec![1, 2, 3],
        resource_fork: vec![4, 5],
        file_type: u32::from_be_bytes(*b"DATA"),
        creator: u32::from_be_bytes(*b"Iny4"),
        finder_flags: 0x4000,
        created_date: 123,
        modified_date: 456,
    };
    runner.import_vfs_file(&source);

    runner
        .map_vfs_file("INDYDEMO.000", "Atlantis Demo/INDYDEMO.000")
        .expect("file mapping");

    let mapped = runner
        .vfs_file_snapshot("Atlantis Demo/INDYDEMO.000")
        .expect("mapped file");
    assert_eq!(mapped.data_fork, source.data_fork);
    assert_eq!(mapped.resource_fork, source.resource_fork);
    assert_eq!(mapped.file_type, source.file_type);
    assert_eq!(mapped.creator, source.creator);
    assert_eq!(mapped.finder_flags, source.finder_flags);
    assert_eq!(mapped.created_date, source.created_date);
    assert_eq!(mapped.modified_date, source.modified_date);
    assert!(runner.vfs_file_snapshot("INDYDEMO.000").is_some());
    assert!(runner
        .map_vfs_file("INDYDEMO.000", "Atlantis Demo/INDYDEMO.000")
        .unwrap_err()
        .contains("destination already exists"));
    assert!(runner
        .map_vfs_file("MISSING", "Atlantis Demo/MISSING")
        .unwrap_err()
        .contains("source does not exist"));
}

#[test]
fn event_yield_services_pending_launch_application_from_vfs() {
    let current_code0 = minimal_code0(0, 0x2000, 0, 0);
    let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
    let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
    let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
    let current_fork = ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner
        .dispatcher
        .vfs
        .insert("Apps/Main App".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Apps/Main App".to_string(), current_fork_bytes);
    runner
        .dispatcher
        .vfs
        .insert("Apps/Register Helper".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
    runner.dispatcher.ensure_vfs_catalog();
    runner.dispatcher.set_launched_app_path("Apps/Main App");

    let app = runner.load_app(&current_fork).expect("load current app");
    runner.init_app(&app);
    let tick_count_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
    runner.bus.write_long(tick_count_entry, 0x0020_1000);
    assert!(runner.dispatcher.has_native_trap_patch(&runner.bus, 0xA975));
    runner.bus.write_long(addr::TICKS, 1234);
    runner.bus.write_long(addr::TIME, 0x1020_3040);
    runner.bus.write_long(addr::RND_SEED, 0x89AB_CDEF);
    runner.set_guest_tick_for_test(1234);
    runner
        .dispatcher
        .queue_pending_launch_application("Apps/Register Helper", true);

    assert!(
        !runner.service_pending_launch_application(false, false),
        "launchContinue target must wait for an Event Manager yield"
    );
    let switched = runner.service_pending_launch_application(true, false);

    assert!(
        switched,
        "event yield should service the queued helper launch"
    );
    assert!(
        !runner.is_halted(),
        "queued helper launch should not halt the runner"
    );
    assert_eq!(
        runner.dispatcher.launched_app_path(),
        Some("Apps/Register Helper")
    );
    assert_eq!(
        runner.bus.read_long(addr::TICKS),
        1234,
        "Process Manager launch must preserve system TickCount"
    );
    assert_eq!(runner.bus.read_long(addr::TIME), 0x1020_3040);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
    assert!(
        !runner.dispatcher.has_native_trap_patch(&runner.bus, 0xA975),
        "application trap patches must be torn down at a foreground launch"
    );
    let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
    let cur_ap_name = String::from_utf8(
        (0..cur_ap_len)
            .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
            .collect(),
    )
    .expect("CurApName is ASCII");
    assert_eq!(cur_ap_name, "Register Helper");
    assert!(
        runner.dispatcher.vfs.contains_key("Apps/Main App"),
        "archive VFS entries must survive the foreground app switch"
    );
    assert!(
        runner
            .dispatcher
            .vfs_rsrc
            .contains_key("Apps/Register Helper"),
        "launched app resource fork must remain available after the switch"
    );
}

#[test]
fn immediate_pending_launch_application_switches_from_vfs_without_event_yield() {
    let current_code0 = minimal_code0(0, 0x2000, 0, 0);
    let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
    let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
    let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
    let current_fork = ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner
        .dispatcher
        .vfs
        .insert("Apps/Main App".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Apps/Main App".to_string(), current_fork_bytes);
    runner
        .dispatcher
        .vfs
        .insert("Apps/Register Helper".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
    runner.dispatcher.ensure_vfs_catalog();
    runner.dispatcher.set_launched_app_path("Apps/Main App");

    let app = runner.load_app(&current_fork).expect("load current app");
    runner.init_app(&app);
    runner.bus.write_long(addr::TICKS, 4321);
    runner.bus.write_long(addr::TIME, 0x5060_7080);
    runner.bus.write_long(addr::RND_SEED, 0x7654_3210);
    runner.set_guest_tick_for_test(4321);
    runner
        .dispatcher
        .queue_pending_launch_application("Apps/Register Helper", false);

    let switched = runner.service_pending_launch_application(false, false);

    assert!(
        switched,
        "immediate pending launch should not require an Event Manager yield"
    );
    assert!(
        !runner.is_halted(),
        "immediate queued helper launch should not halt the runner"
    );
    assert_eq!(
        runner.dispatcher.launched_app_path(),
        Some("Apps/Register Helper")
    );
    assert_eq!(
        runner.bus.read_long(addr::TICKS),
        4321,
        "foreground app switch must preserve system TickCount"
    );
    assert_eq!(runner.bus.read_long(addr::TIME), 0x5060_7080);
    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
    let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
    let cur_ap_name = String::from_utf8(
        (0..cur_ap_len)
            .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
            .collect(),
    )
    .expect("CurApName is ASCII");
    assert_eq!(cur_ap_name, "Register Helper");
}

#[test]
fn mounted_cd_volume_is_enumerated_locked_and_rejects_file_manager_writes() {
    use crate::cpu::{CpuOps, Register};
    use crate::memory::MemoryBus;
    use crate::trap::test_helpers::MockCpu;

    let mut builder = hfsplus::testutil::HfsPlusImageBuilder::new();
    builder.add_file("Data File", b"disc contents", 0o100644);
    let mut runner = crate::game::new_runner();
    let disc = crate::game::mount_locked_volume_image(&mut runner, &builder.build())
        .expect("disc image should mount");
    let mut cpu = MockCpu::new();
    let pb = 0x30_0000u32;
    let name = pb + 0x100;
    let call = |runner: &mut FixtureRunner, cpu: &mut MockCpu, trap: u16| {
        cpu.write_reg(Register::A0, pb);
        runner
            .dispatcher
            .dispatch(trap, cpu, &mut runner.bus)
            .unwrap();
        cpu.read_reg(Register::D0) as i32
    };

    // Indexed PBHGetVInfo walks the VCB queue: the startup volume, then the
    // disc, then nsvErr. Inside Macintosh: Files (1992), p. 2-145.
    let mut volumes = Vec::new();
    for index in 1..=3u16 {
        runner.bus.fill_bytes(pb, 0x100, 0);
        runner.bus.write_long(pb + 18, name);
        runner.bus.write_word(pb + 28, index);
        let result = call(&mut runner, &mut cpu, 0xA207);
        if result != 0 {
            assert_eq!(result, -35, "nsvErr ends enumeration");
            break;
        }
        volumes.push((
            String::from_utf8(runner.bus.read_pstring(name)).unwrap(),
            runner.bus.read_word(pb + 22) as i16,
            runner.bus.read_word(pb + 38) & 0x0080,
        ));
    }
    assert_eq!(
        volumes,
        [
            ("MacintoshHD".to_string(), -1, 0),
            ("HFS+ Disk Image".to_string(), disc.ref_num, 0x0080),
        ]
    );
    assert_ne!(disc.ref_num, -1);

    // A full pathname reaches the file on the second volume; read access
    // succeeds and write access fails with the hardware-lock wPrErr.
    let open = |runner: &mut FixtureRunner, cpu: &mut MockCpu, permission: u8| {
        runner.bus.fill_bytes(pb, 0x100, 0);
        runner.bus.write_long(pb + 18, name);
        runner.bus.write_pstring(name, b"HFS+ Disk Image:Data File");
        runner.bus.write_byte(pb + 27, permission);
        call(runner, cpu, 0xA200)
    };
    assert_eq!(open(&mut runner, &mut cpu, 3), -44, "fsRdWrPerm is wPrErr");
    assert_eq!(open(&mut runner, &mut cpu, 1), 0, "fsRdPerm opens");
    let ref_num = runner.bus.read_word(pb + 24);
    assert_eq!(
        runner
            .dispatcher
            .open_files
            .get(&ref_num)
            .map(String::as_str),
        Some("HFS+ Disk Image/Data File")
    );

    // Creating a file is refused on the disc but allowed on the startup disk.
    for (path, expected) in [
        (&b"HFS+ Disk Image:Saved Game"[..], -44),
        (&b"MacintoshHD:Saved Game"[..], 0),
    ] {
        runner.bus.fill_bytes(pb, 0x100, 0);
        runner.bus.write_long(pb + 18, name);
        runner.bus.write_pstring(name, path);
        assert_eq!(
            call(&mut runner, &mut cpu, 0xA208),
            expected,
            "PBHCreate {}",
            String::from_utf8_lossy(path)
        );
    }
    assert!(!runner
        .dispatcher
        .vfs
        .contains_key("HFS+ Disk Image/Saved Game"));
    assert!(runner.dispatcher.vfs.contains_key("Saved Game"));
    assert_eq!(
        runner.dispatcher.vfs.get("HFS+ Disk Image/Data File"),
        Some(&b"disc contents".to_vec())
    );
}
