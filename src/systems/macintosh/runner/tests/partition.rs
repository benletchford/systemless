use super::*;
use crate::loader::{ApplicationSizeResource, Code0Header, LoadedApp};
use crate::managers::resource::ResourceFork;
use std::collections::HashMap;

#[test]
fn init_app_propagates_size_high_level_event_capability() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let unaware_size = size_resource_bytes(0, 0x0008_0000, 0x0008_0000);
    let unaware_fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, code0.as_slice()),
        (*b"SIZE", -1, unaware_size.as_slice()),
    ]);
    let unaware_fork = ResourceFork::parse(&unaware_fork_bytes).expect("parse unaware app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let unaware_app = runner.load_app(&unaware_fork).expect("load unaware app");
    runner.init_app(&unaware_app);
    assert!(!runner
        .dispatcher
        .apple_event_launch_state
        .is_high_level_event_aware());

    let aware_size = size_resource_bytes(
        ApplicationSizeResource::HIGH_LEVEL_EVENT_AWARE,
        0x0008_0000,
        0x0008_0000,
    );
    let aware_fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, code0.as_slice()),
        (*b"SIZE", -1, aware_size.as_slice()),
    ]);
    let aware_fork = ResourceFork::parse(&aware_fork_bytes).expect("parse aware app fork");
    let aware_app = runner.load_app(&aware_fork).expect("load aware app");
    runner.init_app(&aware_app);
    assert!(runner
        .dispatcher
        .apple_event_launch_state
        .is_high_level_event_aware());
}

#[test]
fn init_app_seeds_classic_double_click_interval() {
    use crate::memory::globals::addr;

    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_long(addr::DOUBLE_TIME),
        DEFAULT_DOUBLE_TIME_TICKS,
        "a zero DoubleTime makes every application-level double-click test fail"
    );
    assert_eq!(runner.bus.read_long(addr::CARET_TIME), crate::memory::globals::DEFAULT_CARET_TIME_TICKS);

}

#[test]
fn load_app_places_resources_above_large_loaded_image() {
    use crate::memory::globals::addr;

    let code0 = minimal_code0(0x001D_0000, 0x0340, 0, 0);
    let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
    let fork_bytes = make_resource_fork_bytes(&[(*b"BGAS", 128, &marker), (*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    assert!(
        app.loaded_image_end > APP_HEAP_FLOOR,
        "fixture should force the loaded image across the default heap floor"
    );

    let heap_start = app_heap_start_for_loaded_app(&app);
    let (_, marker_ptr) = runner
        .dispatcher
        .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
        .expect("BGAS resource loaded");
    assert!(
        marker_ptr >= heap_start + APP_ZONE_HEADER_SIZE,
        "resource data must be allocated after the relocated zone header"
    );
    assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);

    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), heap_start);
    assert_eq!(
        runner.bus.read_long(addr::HEAP_END),
        heap_start + APP_ZONE_HEADER_SIZE
    );
    assert_eq!(
        runner.bus.read_bytes(marker_ptr, marker.len()),
        marker,
        "launch initialization must not clobber resources for large loaded images"
    );
}

#[test]
fn load_app_records_application_size_resource_id_minus_one() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let size = size_resource_bytes(0x0080, 0x0030_0000, 0x0020_0000);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert_eq!(
        app.size_resource,
        Some(ApplicationSizeResource {
            flags: 0x0080,
            preferred_size: 0x0030_0000,
            minimum_size: 0x0020_0000,
        })
    );
}

#[test]
fn load_app_prefers_valid_size_resource_id_zero() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let original = size_resource_bytes(0x0040, 0x0030_0000, 0x0020_0000);
    let finder_override = size_resource_bytes(0x0080, 0x0050_0000, 0x0040_0000);
    let fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, &code0),
        (*b"SIZE", -1, &original),
        (*b"SIZE", 0, &finder_override),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(16 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert_eq!(
        app.size_resource,
        Some(ApplicationSizeResource {
            flags: 0x0080,
            preferred_size: 0x0050_0000,
            minimum_size: 0x0040_0000,
        })
    );
}

#[test]
fn load_app_falls_back_to_size_resource_id_minus_one_when_id_zero_is_invalid() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let original = size_resource_bytes(0x0040, 0x0030_0000, 0x0020_0000);
    let invalid_override = size_resource_bytes(0x0080, 0x0010_0000, 0x0020_0000);
    let fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, &code0),
        (*b"SIZE", -1, &original),
        (*b"SIZE", 0, &invalid_override),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert_eq!(
        app.size_resource,
        Some(ApplicationSizeResource {
            flags: 0x0040,
            preferred_size: 0x0030_0000,
            minimum_size: 0x0020_0000,
        })
    );
}

#[test]
fn load_app_relocates_large_size_partition_a5_above_application_zone() {
    let minimum_partition = 4_812_800;
    let preferred_partition = 6_348_800;
    let below_a5 = 0x7AF4;
    let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
    let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert!(
        app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE,
        "relocated app image must leave room for the visible app-zone header"
    );
    assert!(
        app.a5_base - APP_HEAP_FLOOR >= minimum_partition - APP_STACK_SAFETY_MARGIN,
        "large SIZE partitions should place A5 high enough for direct A5-zone memory checks"
    );
    assert!(
        app.a5_base - APP_HEAP_FLOOR >= 750 * 1024,
        "Spectre-style startup gates compare A5 - GetZone against a 750K floor"
    );
}

#[test]
fn large_size_partition_does_not_overwrite_synthetic_callbacks() {
    let below_a5 = 0x7AF4;
    let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
    let partition = 63 * 1024 * 1024;
    let size = size_resource_bytes(0x5880, partition, partition);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback = runner.bus.alloc_synthetic(2);
    runner.bus.write_word(callback, 0x4E75);

    let app = runner.load_app(&fork).expect("load app");

    assert_eq!(runner.bus.read_word(callback), 0x4E75);
    assert!(
        app.loaded_image_end + APP_HIGH_MEMORY_RESERVE <= runner.bus.application_memory_limit(),
        "the loaded image must leave room for the application heap and stack"
    );
    assert!(app.initial_sp < callback);
}

#[test]
fn classic_app_stack_stays_addressable_in_twenty_four_bit_mode() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert!(app.initial_sp < CLASSIC_24_BIT_ADDRESS_SPACE_END);
    assert!(app.initial_sp > app.loaded_image_end + APP_HIGH_MEMORY_RESERVE);
}

#[test]
fn load_app_relocates_exact_2mb_size_partition() {
    let below_a5 = 0x68E8;
    let code0 = minimal_code0(0x0D18, below_a5, 0, 0);
    let size = size_resource_bytes(0x5880, 0x0020_0000, 0x0020_0000);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert!(app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR);
    assert!(app.a5_base - APP_HEAP_FLOOR >= 0x0020_0000 - APP_STACK_SAFETY_MARGIN);
}

#[test]
fn load_app_relocates_sub_2mb_size_partition() {
    let preferred_partition = 1_843_200;
    let minimum_partition = 768_000;
    let below_a5 = 29_116;
    let code0 = minimal_code0(3_816, below_a5, 0, 0);
    let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");

    assert!(app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR);
    assert!(
        app.a5_base - APP_HEAP_FLOOR >= preferred_partition - APP_STACK_SAFETY_MARGIN,
        "sub-2 MiB SIZE partitions must still govern the classic A5 layout"
    );
}

#[test]
fn sub_2mb_size_partition_keeps_resident_code_above_low_decompression_range() {
    const DECOMPRESS_START: u32 = 0x0001_000A;
    const OLD_EXECUTING_CODE_END: u32 = 0x0003_35FE;

    // This reproduces the launch geometry of a self-decompressing
    // installer whose SIZE partition is below 2 MiB. Its startup code
    // clears the low destination range before expanding the application;
    // resident CODE must therefore follow the partition-relative A5 world
    // rather than remain inside that range.
    let preferred_partition = 1_022_976;
    let below_a5 = 0x10B0;
    let code0 = minimal_code0(0x164BC, below_a5, 0, 0);
    let size = size_resource_bytes(0x5880, preferred_partition, preferred_partition);
    let mut code1 = vec![0xA5; 128];
    code1[..4].fill(0); // Valid near-model header with no jump-table entries.
    let fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, &code0),
        (*b"CODE", 1, &code1),
        (*b"SIZE", -1, &size),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    let code1_base = app.segment_bases[&1];

    assert!(
        code1_base > OLD_EXECUTING_CODE_END,
        "resident CODE must be placed above the self-decompressor's low destination range"
    );
    runner.bus.fill_zeros(
        DECOMPRESS_START,
        OLD_EXECUTING_CODE_END - DECOMPRESS_START + 1,
    );
    assert_eq!(runner.bus.read_bytes(code1_base, code1.len()), code1);
}

#[test]
fn size_partition_does_not_cap_shared_resource_fork_materialization() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let size = size_resource_bytes(0x5880, 4_194_304, 3_584_000);
    let large_resource = vec![0xA5; 6_481_428];
    let fork_bytes = make_resource_fork_bytes(&[
        (*b"CODE", 0, &code0),
        (*b"SHAP", 128, &large_resource),
        (*b"SIZE", -1, &size),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    // Systemless currently materializes resource-fork data in the shared
    // bus. A classic resource map can instead leave nonpreloaded data on
    // disk until requested, so a guest SIZE limit cannot bound this shared
    // storage until it has a separate arena. More Macintosh Toolbox
    // (1993), pp. 1-8 to 1-9.
    let (_, resource_ptr) = runner
        .dispatcher
        .find_or_load_resource_any(&mut runner.bus, *b"SHAP", 128)
        .expect("large resource registered");
    assert_ne!(
        resource_ptr, 0,
        "large resource data must remain allocatable"
    );
    assert_eq!(runner.bus.read_byte(resource_ptr), 0xA5);
    assert_eq!(
        runner
            .bus
            .read_byte(resource_ptr + large_resource.len() as u32 - 1),
        0xA5
    );
}

#[test]
fn init_app_exposes_low_visible_zone_for_relocated_size_partition() {
    use crate::memory::globals::addr;

    let minimum_partition = 4_812_800;
    let preferred_partition = 6_348_800;
    let below_a5 = 0x7AF4;
    let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
    let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
    let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
    let fork_bytes = make_resource_fork_bytes(&[
        (*b"BGAS", 128, &marker),
        (*b"CODE", 0, &code0),
        (*b"SIZE", -1, &size),
    ]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

    let app = runner.load_app(&fork).expect("load app");
    let image_start = app_image_start_for_loaded_app(&app);
    let (_, marker_ptr) = runner
        .dispatcher
        .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
        .expect("BGAS resource loaded");
    let marker_end = marker_ptr + marker.len() as u32;
    assert!(
        marker_end <= image_start || marker_ptr >= app.loaded_image_end,
        "resource allocation must not overlap the relocated image"
    );

    runner.init_app(&app);

    assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), APP_HEAP_FLOOR);
    assert_eq!(runner.bus.read_long(addr::THE_ZONE), APP_HEAP_FLOOR);
    assert_eq!(
        runner.bus.read_long(addr::HEAP_END),
        APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE
    );
    assert_eq!(
        runner.bus.read_long(APP_HEAP_FLOOR),
        runner.bus.read_long(addr::APPL_LIMIT)
    );
    assert!(
        app.a5_base - runner.bus.read_long(addr::THE_ZONE) >= 750 * 1024,
        "GetZone-visible partition span should satisfy direct startup memory gates"
    );
    assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);
}

#[test]
fn init_app_leaves_application_heap_room_below_appllimit() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    let heap_end = runner.bus.read_long(addr::HEAP_END);
    let appl_limit = runner.bus.read_long(addr::APPL_LIMIT);
    assert_eq!(
        heap_end,
        0x0020_0000 + APP_ZONE_HEADER_SIZE,
        "HeapEnd should expose the initial application-zone extent"
    );
    assert_eq!(
        runner
            .bus
            .read_word(runner.bus.read_long(addr::APP_L_ZONE) + 20),
        64,
        "the launch-time application zone should expose the standard moreMast increment"
    );
    assert!(
        appl_limit.saturating_sub(heap_end) >= 2300 * 1024,
        "direct low-memory startup checks should see growable heap room below ApplLimit"
    );
}

#[test]
fn init_app_honors_size_resource_preferred_partition_for_heap_reporting() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let preferred_partition = 3 * 1024 * 1024;
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: Some(ApplicationSizeResource {
            flags: 0x0080,
            preferred_size: preferred_partition,
            minimum_size: 2 * 1024 * 1024,
        }),
    };

    runner.init_app(&app);

    let expected_limit = 0x0020_0000 + preferred_partition - APP_STACK_SAFETY_MARGIN;
    let expected_free = expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE);
    assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
    assert_eq!(runner.bus.read_long(addr::BUF_PTR), expected_limit);
    assert_eq!(runner.bus.read_long(0x0020_0000), expected_limit);
    assert_eq!(runner.bus.read_long(0x0020_0000 + 12), expected_free);
    assert_eq!(
        crate::memory::app_heap_free_bytes(runner.bus()),
        expected_free
    );
    assert!(
        expected_free < crate::memory::APP_HEAP_COMPAT_FREE_FLOOR,
        "explicit SIZE partitions must bypass the compatibility floor"
    );
}

#[test]
fn init_app_application_partition_override_takes_precedence_over_size_resource() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let size_partition = 3 * 1024 * 1024;
    let override_partition = 4 * 1024 * 1024;
    runner.set_application_partition_size(Some(override_partition));
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: Some(ApplicationSizeResource {
            flags: 0x0080,
            preferred_size: size_partition,
            minimum_size: 2 * 1024 * 1024,
        }),
    };

    runner.init_app(&app);

    let expected_limit = 0x0020_0000 + override_partition - APP_STACK_SAFETY_MARGIN;
    assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
    assert_eq!(
        crate::memory::app_heap_free_bytes(runner.bus()),
        expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE)
    );
}

#[test]
fn init_app_ignores_too_small_application_partition_override() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_application_partition_size(Some(64 * 1024));
    assert_eq!(runner.application_partition_size(), None);
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_long(addr::APPL_LIMIT),
        app.initial_sp - APP_STACK_SAFETY_MARGIN,
        "invalid tiny overrides must fall back to the default launch limit"
    );
}
