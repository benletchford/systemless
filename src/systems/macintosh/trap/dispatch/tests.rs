    use super::*;
    use crate::cpu::{CpuOps, Register};
    use crate::trap::menu::test_tracked_menu_state;
    use crate::trap::test_helpers::{setup, setup_with_trap_tables};
    use std::collections::VecDeque;

    #[test]
    fn generated_raw_trap_routes_cover_every_a_line_word_exactly() {
        for low_word in 0u16..0x1000 {
            let word = 0xA000 | low_word;
            let route = raw_trap_route(word);
            assert_eq!(route.raw_word, word);
            if (word & 0x0800) != 0 {
                let slot = word & 0x03FF;
                assert!(route.is_toolbox);
                assert_eq!(route.table_slot, slot);
                assert_eq!(route.table_index, OS_TRAP_TABLE_SLOTS + slot);
                assert_eq!(
                    route.table_address,
                    TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4
                );
                assert_eq!(route.canonical_word, 0xA800 | slot);
                assert_eq!(route.os_flags, 0);
                assert!(!route.os_returns_a0);
                assert_eq!(route.toolbox_auto_pop, (word & 0x0400) != 0);
            } else {
                let slot = word & 0x00FF;
                assert!(!route.is_toolbox);
                assert_eq!(route.table_slot, slot);
                assert_eq!(route.table_index, slot);
                assert_eq!(
                    route.table_address,
                    OS_TRAP_TABLE_BASE + u32::from(slot) * 4
                );
                assert_eq!(route.canonical_word, 0xA000 | slot);
                assert_eq!(route.os_flags, word & 0x0700);
                assert_eq!(route.os_returns_a0, (word & 0x0100) != 0);
                assert!(!route.toolbox_auto_pop);
            }
        }
    }

    #[test]
    fn raw_routes_classify_only_source_backed_os_routine_variants() {
        use OsRoutineVariant::{
            CurrentHeap, CurrentHeapClear, DriverInstall, DriverInstallReserveMemory,
            FileAsynchronous, FileHfsAsynchronous, FileHfsSynchronous, FileSynchronous,
            GestaltQuery, GestaltRegister, GestaltReplace, LowerText, ParameterBlockAsynchronous,
            ParameterBlockImmediate, ParameterBlockSynchronous, PowerIdleState, PowerIdleUpdate,
            PowerSerial, SleepQueueInstall, SleepQueueRemove, StripText, StripUpperText,
            SystemHeap, SystemHeapClear, TextCompareExact, TextCompareFoldCase,
            TextCompareFoldCaseAndMarks, TextCompareStripMarks, TimeTaskExtended, TimeTaskOriginal,
            TrapAddressLegacy, TrapAddressNewOs, TrapAddressNewTool, Unclassified,
            UpperStringPreserveMarks, UpperStringStripMarks, UpperText,
        };

        // Inside Macintosh: Memory (1992), pp. 2-31 and 2-35; Universal
        // Interfaces 3.4 MacMemory.h lines 436--485 and 550--599.
        for slot in [0x1Eu16, 0x22] {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected) in [
                    (0x0000, CurrentHeap),
                    (0x0200, CurrentHeapClear),
                    (0x0400, SystemHeap),
                    (0x0600, SystemHeapClear),
                ] {
                    assert_eq!(
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant,
                        expected
                    );
                }
            }
        }

        // Inside Macintosh: Operating System Utilities (1994), pp. 8-27--8-31
        // and 8-32--8-33; UI 3.4 Patches.h lines 80--231.
        for slot in [0x46u16, 0x47] {
            for return_a0 in [0x0000u16, 0x0100] {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    TrapAddressLegacy
                );
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    TrapAddressNewOs
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    TrapAddressNewTool
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    Unclassified,
                    "bit 10 without the new-system bit is undeclared"
                );
            }
        }

        // Inside Macintosh: Operating System Utilities (1994),
        // pp. 1-31--1-36; UI 3.4 Gestalt.h lines 55--105.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA0AD | return_a0).os_routine_variant,
                GestaltQuery
            );
            assert_eq!(
                raw_trap_route(0xA2AD | return_a0).os_routine_variant,
                GestaltRegister
            );
            assert_eq!(
                raw_trap_route(0xA4AD | return_a0).os_routine_variant,
                GestaltReplace
            );
            assert_eq!(
                raw_trap_route(0xA6AD | return_a0).os_routine_variant,
                Unclassified,
                "combined Gestalt Manager modifier bits are undeclared"
            );
        }

        // Inside Macintosh: Devices (1994), pp. 1-83--1-85; UI 3.4
        // Devices.h lines 1109--1141 declares DriverInstall $A03D and
        // DriverInstallReserveMem $A43D.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA03D | return_a0).os_routine_variant,
                DriverInstall
            );
            assert_eq!(
                raw_trap_route(0xA43D | return_a0).os_routine_variant,
                DriverInstallReserveMemory
            );
            assert_eq!(
                raw_trap_route(0xA23D | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA63D | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Inside Macintosh: Devices (1994), pp. 6-18, 6-26, and 6-33;
        // UI 3.4 Power.h lines 447--461 and 705--731.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA28A | return_a0).os_routine_variant,
                SleepQueueInstall
            );
            assert_eq!(
                raw_trap_route(0xA48A | return_a0).os_routine_variant,
                SleepQueueRemove
            );
            assert_eq!(
                raw_trap_route(0xA68A | return_a0).os_routine_variant,
                Unclassified,
                "combined sleep-queue modifier bits have no reviewed semantics"
            );
        }

        // Inside Macintosh: Devices (1994), pp. 6-29--6-30 and 6-33--6-35;
        // UI 3.4 Power.h lines 650--701 and 733--791.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA285 | return_a0).os_routine_variant,
                PowerIdleUpdate
            );
            assert_eq!(
                raw_trap_route(0xA485 | return_a0).os_routine_variant,
                PowerIdleState
            );
            assert_eq!(
                raw_trap_route(0xA685 | return_a0).os_routine_variant,
                PowerSerial
            );
            assert_eq!(
                raw_trap_route(0xA085 | return_a0).os_routine_variant,
                Unclassified,
                "the bare slot has no reviewed Power Manager routine identity"
            );
        }

        // Inside Macintosh: Processes (1994), pp. 3-18--3-20; UI 3.4
        // Timer.h lines 74--100 declare InsTime $A058 and InsXTime $A458.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA058 | return_a0).os_routine_variant,
                TimeTaskOriginal
            );
            assert_eq!(
                raw_trap_route(0xA458 | return_a0).os_routine_variant,
                TimeTaskExtended
            );
            assert_eq!(
                raw_trap_route(0xA258 | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA658 | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Inside Macintosh: Text (1993), pp. 5-64--5-65.
        for return_a0 in [0x0000u16, 0x0100] {
            assert_eq!(
                raw_trap_route(0xA054 | return_a0).os_routine_variant,
                UpperStringPreserveMarks
            );
            assert_eq!(
                raw_trap_route(0xA254 | return_a0).os_routine_variant,
                UpperStringStripMarks
            );
            assert_eq!(
                raw_trap_route(0xA454 | return_a0).os_routine_variant,
                Unclassified
            );
            assert_eq!(
                raw_trap_route(0xA654 | return_a0).os_routine_variant,
                Unclassified
            );
        }

        // Devices 1994, p. 1-16; UI 3.4 Devices.h lines 905--1044 and
        // 1282--1415 declare exact Sync, Immed, and Async words for $01--$06.
        for slot in 0x01u16..=0x06 {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected) in [
                    (0x0000, ParameterBlockSynchronous),
                    (0x0200, ParameterBlockImmediate),
                    (0x0400, ParameterBlockAsynchronous),
                ] {
                    assert_eq!(
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant,
                        expected
                    );
                }
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    Unclassified,
                    "combined ASYNC+IMMED slot ${slot:02X} is undeclared"
                );
            }
        }

        // Inside Macintosh: Text (1993), pp. 5-51--5-52 and 5-60--5-61.
        for slot in [0x3Cu16, 0x50] {
            for return_a0 in [0x0000u16, 0x0100] {
                for (routine_bits, expected, sensitivity) in [
                    (0x0000, TextCompareFoldCaseAndMarks, (false, false)),
                    (0x0200, TextCompareFoldCase, (false, true)),
                    (0x0400, TextCompareStripMarks, (true, false)),
                    (0x0600, TextCompareExact, (true, true)),
                ] {
                    let variant =
                        raw_trap_route(0xA000 | slot | return_a0 | routine_bits).os_routine_variant;
                    assert_eq!(variant, expected);
                    assert_eq!(variant.text_comparison_sensitivity(), Some(sensitivity));
                }
            }
        }

        // IM:Memory documents SYS, but not bit 9, for these routines. UI 3.4
        // MacMemory.h declares the current/system pairs at lines 517--533,
        // 631--695, 862--1010, 1184--1202, and 1331--1362.
        for slot in [
            0x1Cu16, 0x1D, 0x27, 0x28, 0x40, 0x4C, 0x4D, 0x61, 0x62, 0x66,
        ] {
            for return_a0 in [0x0000u16, 0x0100] {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    CurrentHeap
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    SystemHeap
                );
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    Unclassified
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    Unclassified
                );
            }
        }

        // Inside Macintosh VI, pp. 14-62--14-63 and Appendix C table C-2;
        // Universal Interfaces 3.4 TextUtils.h lines 404--455.
        for return_a0 in [0x0000u16, 0x0100] {
            for (routine_bits, expected) in [
                (0x0000, LowerText),
                (0x0200, StripText),
                (0x0400, UpperText),
                (0x0600, StripUpperText),
            ] {
                assert_eq!(
                    raw_trap_route(0xA056 | return_a0 | routine_bits).os_routine_variant,
                    expected
                );
            }
        }

        // Files 1992, pp. 2-6 and 2-238--2-239 plus its assembly summary;
        // UI 3.4 Files.h lines 1315--3343 declare these exact words.
        let basic_file_slots = [
            0x07u16, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x18,
            0x41, 0x42, 0x43, 0x44, 0x45,
        ];
        let hfs_file_slots = [
            0x07u16, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x10, 0x14, 0x15, 0x41, 0x42,
        ];
        for return_a0 in [0x0000u16, 0x0100] {
            for slot in basic_file_slots {
                assert_eq!(
                    raw_trap_route(0xA000 | slot | return_a0).os_routine_variant,
                    FileSynchronous
                );
                assert_eq!(
                    raw_trap_route(0xA400 | slot | return_a0).os_routine_variant,
                    FileAsynchronous
                );
            }
            for slot in hfs_file_slots {
                assert_eq!(
                    raw_trap_route(0xA200 | slot | return_a0).os_routine_variant,
                    FileHfsSynchronous
                );
                assert_eq!(
                    raw_trap_route(0xA600 | slot | return_a0).os_routine_variant,
                    FileHfsAsynchronous
                );
            }
        }

        let classified = (0xA000u16..=0xAFFF)
            .filter(|&word| raw_trap_route(word).os_routine_variant != Unclassified)
            .count();
        assert_eq!(classified, 280);
        assert_eq!(
            raw_trap_route(0xA271).os_routine_variant,
            Unclassified,
            "an unrelated OS bit-9 form must not acquire invented semantics"
        );
        assert_eq!(raw_trap_route(0xAE56).os_routine_variant, Unclassified);
        assert_eq!(
            raw_trap_route(0xA200).os_routine_variant,
            Unclassified,
            "PBHOpen/PBOpenImmed is an intentionally unresolved declaration collision"
        );
        assert_eq!(raw_trap_route(0xA613).os_routine_variant, Unclassified);
    }

    #[test]
    fn generated_profile_routes_cover_all_raw_words_and_live_table_cells() {
        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, profile)
                .expect("trap table construction requires writable cells and system storage");
            let unimplemented = dispatcher.default_trap_gateway(&bus, 0xAA6E).unwrap();

            for low_word in 0u16..0x1000 {
                let word = 0xA000 | low_word;
                let route = profile.route(word);
                let table_entry = route.raw.table_address;
                assert_eq!(
                    table_entry,
                    TrapDispatcher::raw_trap_table_entry(word),
                    "raw word ${word:04X}"
                );
                let raw_target = bus.read_long(table_entry);
                assert_eq!(
                    route.has_permanent_come_from,
                    bus.read_long(raw_target) == COME_FROM_PATCH_SIGNATURE,
                    "raw word ${word:04X}"
                );
                let logical = dispatcher.trap_table_address(&bus, word).unwrap();
                assert_eq!(
                    route.default_is_unimplemented,
                    logical == unimplemented,
                    "raw word ${word:04X}"
                );
                assert_eq!(
                    logical,
                    dispatcher
                        .trap_table_address(&bus, route.raw.canonical_word)
                        .unwrap(),
                    "variant ${word:04X} must select its canonical slot"
                );
            }
        }
    }

    #[test]
    fn generated_default_routes_cover_every_canonical_operation_once() {
        let mut seen = HashSet::new();
        for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
            let canonical_word = if table_index < OS_TRAP_TABLE_SLOTS {
                0xA000 | table_index
            } else {
                0xA800 | (table_index - OS_TRAP_TABLE_SLOTS)
            };
            let route = default_trap_route(canonical_word);
            assert_eq!(route.operation_id, canonical_word);
            assert!(seen.insert(route.operation_id));
            assert_eq!(
                route,
                default_trap_route(
                    canonical_word | if table_index < 0x100 { 0x0700 } else { 0x0400 }
                )
            );
        }
        assert_eq!(seen.len(), 1280);
    }

    #[test]
    fn power_manager_generated_routes_preserve_exact_low_word_values() {
        assert_eq!(POWER_MANAGER_OPERATION_ROUTES.len(), 34);
        assert!(POWER_MANAGER_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0000, "PMSelectorCount"),
            (0x0001, "PMFeatures"),
            (0x0002, "GetSleepTimeout"),
            (0x0003, "SetSleepTimeout"),
            (0x0004, "GetHardDiskTimeout"),
            (0x0005, "SetHardDiskTimeout"),
            (0x0006, "HardDiskPowered"),
            (0x0007, "SpinDownHardDisk"),
            (0x0008, "IsSpindownDisabled"),
            (0x0009, "SetSpindownDisable"),
            (0x000A, "HardDiskQInstall"),
            (0x000B, "HardDiskQRemove"),
            (0x000C, "GetScaledBatteryInfo"),
            (0x000D, "AutoSleepControl"),
            (0x000E, "GetIntModemInfo"),
            (0x000F, "SetIntModemState"),
            (0x0010, "MaximumProcessorSpeed"),
            (0x0011, "CurrentProcessorSpeed"),
            (0x0012, "FullProcessorSpeed"),
            (0x0013, "SetProcessorSpeed"),
            (0x0014, "GetSCSIDiskModeAddress"),
            (0x0015, "SetSCSIDiskModeAddress"),
            (0x0016, "GetWakeupTimer"),
            (0x0017, "SetWakeupTimer"),
            (0x0018, "IsProcessorCyclingEnabled"),
            (0x0019, "EnableProcessorCycling"),
            (0x001A, "BatteryCount"),
            (0x001B, "GetBatteryVoltage"),
            (0x001C, "GetBatteryTimes"),
            (0x001D, "GetDimmingTimeout"),
            (0x001E, "SetDimmingTimeout"),
            (0x001F, "DimmingControl"),
            (0x0020, "IsDimmingControlDisabled"),
            (0x0021, "IsAutoSlpControlDisabled"),
        ] {
            let route =
                power_manager_operation_route(0xA09E, selector).expect("PowerMgrDispatch route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA19E, 0x0003),
            (0xA09E, 0x0022),
            (0xA09E, 0x0036),
            (0xA09E, 0x7000),
            (0xA09E, 0x303C),
        ] {
            assert!(power_manager_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn power_manager_records_identity_while_remaining_fail_closed() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x1234_0003);

        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(dispatcher.current_trap_adapter, TrapAdapterId::Nonterminal);
        assert_eq!(
            dispatcher.current_selector_operation,
            Some("selector-operation:_PowerMgrDispatch:0x0003:d0-low-word-immediate:16")
        );

        cpu.write_reg(Register::D0, 0x0004);
        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(
            dispatcher.current_selector_operation,
            Some("selector-operation:_PowerMgrDispatch:0x0004:d0-moveq-immediate:8")
        );

        cpu.write_reg(Register::D0, 0x0022);
        let result = dispatcher.dispatch(0xA09E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA09E))));
        assert_eq!(dispatcher.current_selector_operation, None);

        cpu.write_reg(Register::D0, 0x0003);
        let result = dispatcher.dispatch(0xA19E, &mut cpu, &mut bus);
        assert!(matches!(result, Err(Error::UnimplementedTrap(0xA19E))));
        assert_eq!(dispatcher.current_selector_operation, None);
    }

    #[test]
    fn every_profile_saved_default_reaches_its_declared_adapter() {
        // A saved Trap Manager pointer remains callable after replacement and
        // reaches the original system routine. Inside Macintosh: Operating
        // System Utilities (1994), pp. 8-23--8-30. Isolate every invocation
        // because arbitrary default operations may legitimately mutate global
        // manager state even when their poison arguments produce an error.
        const CALLER_SP: u32 = 0x001F_FF00;
        const RETURN_PC: u32 = 0x001F_0002;
        const PATCH: u32 = 0x0028_0000;

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
                let is_toolbox = table_index >= OS_TRAP_TABLE_SLOTS;
                let slot = if is_toolbox {
                    table_index - OS_TRAP_TABLE_SLOTS
                } else {
                    table_index
                };
                let canonical_word = if is_toolbox {
                    0xA800 | slot
                } else {
                    0xA000 | slot
                };
                let (mut dispatcher, mut cpu, mut bus) = setup();
                dispatcher
                    .materialize_trap_tables(&mut bus, profile)
                    .expect("trap table construction requires writable cells and system storage");
                let saved_default = dispatcher.trap_table_address(&bus, canonical_word).unwrap();
                let profile_route = profile.route(canonical_word);
                let invoked_word = bus.read_word(saved_default);
                let invoked_operation = profile_route.default_gateway_word;
                let declared = *default_trap_route(invoked_operation);
                dispatcher
                    .install_trap_address(&mut bus, canonical_word, PATCH)
                    .expect("patch must install into the materialized table");
                assert_eq!(
                    dispatcher.native_trap_handler(&bus, canonical_word),
                    Some(PATCH)
                );

                let entry_sp = CALLER_SP - 4;
                bus.write_long(entry_sp, RETURN_PC);
                cpu.write_reg(Register::D0, 0xD0D0_0000);
                cpu.write_reg(Register::D1, 0xD1D1_0000);
                cpu.write_reg(Register::D2, 0xD2D2_0000);
                cpu.write_reg(Register::A0, 0);
                cpu.write_reg(Register::A1, 0);
                cpu.write_reg(Register::A2, 0);
                cpu.write_reg(Register::A7, entry_sp);
                cpu.write_reg(Register::PC, saved_default + 2);

                let result = dispatcher.dispatch(invoked_word, &mut cpu, &mut bus);

                assert_eq!(
                    dispatcher.current_trap_operation, invoked_operation,
                    "{profile:?} operation ${canonical_word:04X}"
                );
                assert!(
                    declared.allows(dispatcher.current_trap_adapter),
                    "{profile:?} adapter ${canonical_word:04X}: {:?}",
                    dispatcher.current_trap_adapter
                );
                if dispatcher.current_trap_adapter == TrapAdapterId::Nonterminal {
                    assert!(
                        matches!(result, Err(Error::UnimplementedTrap(word)) if word == invoked_word),
                        "{profile:?} declared nonterminal ${canonical_word:04X}: {result:?}"
                    );
                } else {
                    assert!(
                        !matches!(result, Err(Error::UnimplementedTrap(_))),
                        "{profile:?} declared adapter fell through ${canonical_word:04X}"
                    );
                }
                assert!(
                    dispatcher.pending_native_trap_calls.is_empty(),
                    "saved default must bypass current patch ${canonical_word:04X}"
                );
            }
        }
    }

    fn call_trap_manager_getter<C: CpuOps>(
        dispatcher: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        getter: u16,
        trap_word: u16,
    ) -> u32 {
        cpu.write_reg(Register::D0, 0xFFFF_0000 | u32::from(trap_word));
        let saved_gateway = dispatcher
            .default_trap_gateway(bus, getter)
            .expect("materialized Trap Manager getter gateway");
        cpu.write_reg(Register::PC, saved_gateway + 2);
        dispatcher
            .dispatch(getter, cpu, bus)
            .unwrap_or_else(|error| panic!("getter ${getter:04X} for ${trap_word:04X}: {error:?}"));
        cpu.read_reg(Register::A0)
    }

    fn call_trap_manager_setter<C: CpuOps>(
        dispatcher: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        setter: u16,
        trap_word: u16,
        handler: u32,
    ) {
        cpu.write_reg(Register::D0, 0xFFFF_0000 | u32::from(trap_word));
        cpu.write_reg(Register::A0, handler);
        let saved_gateway = dispatcher
            .default_trap_gateway(bus, setter)
            .expect("materialized Trap Manager setter gateway");
        cpu.write_reg(Register::PC, saved_gateway + 2);
        dispatcher
            .dispatch(setter, cpu, bus)
            .unwrap_or_else(|error| panic!("setter ${setter:04X} for ${trap_word:04X}: {error:?}"));
    }

    #[test]
    fn generated_profile_slots_exhaustively_roundtrip_classic_patch_lifecycle() {
        // The typed and legacy Trap Manager operations must observe the same
        // process table used by A-line dispatch. Saved logical pointers remain
        // callable after a patch, nested replacement restores in LIFO order,
        // and a raw table write bypasses any permanent come-from head. Inside
        // Macintosh: Operating System Utilities (1994), pp. 8-23--8-33.
        const RETURN_PC: u32 = 0x001F_0002;
        const SP: u32 = 0x001F_FF00;
        const FIRST_PATCH_BASE: u32 = 0x0028_0000;
        const SECOND_PATCH_BASE: u32 = 0x0029_0000;
        const RAW_PATCH_BASE: u32 = 0x002A_0000;

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, profile)
                .expect("trap table construction requires writable cells and system storage");

            for table_index in 0..(OS_TRAP_TABLE_SLOTS + TOOLBOX_TRAP_TABLE_SLOTS) {
                let is_toolbox = table_index >= OS_TRAP_TABLE_SLOTS;
                let slot = if is_toolbox {
                    table_index - OS_TRAP_TABLE_SLOTS
                } else {
                    table_index
                };
                let trap_word = if is_toolbox {
                    0xA800 | slot
                } else {
                    0xA000 | slot
                };
                let getter = if is_toolbox { 0xA746 } else { 0xA346 };
                let setter = if is_toolbox { 0xA647 } else { 0xA247 };
                let route = profile.route(trap_word);
                let raw_entry = route.raw.table_address;
                let initial_raw = bus.read_long(raw_entry);
                let default = dispatcher.trap_table_address(&bus, trap_word).unwrap();
                let first_patch = FIRST_PATCH_BASE + u32::from(table_index) * 4;
                let second_patch = SECOND_PATCH_BASE + u32::from(table_index) * 4;
                let raw_patch = RAW_PATCH_BASE + u32::from(table_index) * 4;

                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} typed default getter ${trap_word:04X}"
                );

                let legacy_uses_os = matches!(slot, 0x000..=0x04F | 0x054 | 0x057);
                if legacy_uses_os != is_toolbox {
                    assert_eq!(
                        call_trap_manager_getter(
                            &mut dispatcher,
                            &mut cpu,
                            &mut bus,
                            0xA146,
                            trap_word,
                        ),
                        default,
                        "{profile:?} legacy default getter ${trap_word:04X}"
                    );
                }

                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    first_patch,
                );
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    first_patch,
                    "{profile:?} first patch ${trap_word:04X}"
                );
                if route.has_permanent_come_from {
                    assert_eq!(
                        bus.read_long(raw_entry),
                        initial_raw,
                        "{profile:?} protected raw head ${trap_word:04X}"
                    );
                } else {
                    assert_eq!(
                        bus.read_long(raw_entry),
                        first_patch,
                        "{profile:?} direct raw patch ${trap_word:04X}"
                    );
                }

                let saved_first = call_trap_manager_getter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    getter,
                    trap_word,
                );
                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    second_patch,
                );
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    second_patch,
                    "{profile:?} nested patch ${trap_word:04X}"
                );
                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    saved_first,
                );

                let variants = if is_toolbox { 2 } else { 8 };
                for variant in 0..variants {
                    let raw_word = if is_toolbox {
                        trap_word | (variant << 10)
                    } else {
                        trap_word | (variant << 8)
                    };
                    cpu.write_reg(Register::PC, RETURN_PC);
                    cpu.write_reg(Register::A7, SP);
                    cpu.write_reg(Register::D1, 0xD1D1_BEEF);
                    if is_toolbox && variant != 0 {
                        bus.write_long(SP, RETURN_PC);
                    }

                    dispatcher.dispatch(raw_word, &mut cpu, &mut bus).unwrap();
                    let argument_sp = if is_toolbox && variant != 0 {
                        SP + 4
                    } else {
                        SP
                    };
                    let handler_sp = argument_sp - 4;

                    assert_eq!(
                        cpu.read_reg(Register::PC),
                        first_patch,
                        "{profile:?} patched variant ${raw_word:04X}"
                    );
                    assert_eq!(
                        cpu.read_reg(Register::A7),
                        handler_sp,
                        "{profile:?} handler SP ${raw_word:04X}"
                    );
                    assert_eq!(bus.read_long(handler_sp), RETURN_PC);
                    if !is_toolbox {
                        assert_eq!(
                            cpu.read_reg(Register::D1),
                            0xD1D1_0000 | u32::from(raw_word),
                            "{profile:?} full OS word ${raw_word:04X}"
                        );
                    }

                    cpu.write_reg(Register::PC, RETURN_PC);
                    cpu.write_reg(Register::A7, argument_sp);
                    dispatcher.retire_returned_native_trap_call(&mut cpu);
                    assert!(
                        dispatcher.pending_native_trap_calls.is_empty(),
                        "{profile:?} retired ${raw_word:04X}"
                    );
                }

                // The saved default remains an executable OS trap-plus-RTS or
                // Toolbox auto-pop A-line while its table slot is patched.
                // A profile may intentionally give multiple cells the same
                // procedure address, so inspect its declared gateway identity
                // rather than assuming every cell embeds its own slot word.
                if is_toolbox {
                    assert_eq!(bus.read_word(default), route.default_gateway_word | 0x0400);
                } else {
                    assert_eq!(bus.read_word(default), route.default_gateway_word);
                    assert_eq!(bus.read_word(default + 2), 0x4E75);
                }

                call_trap_manager_setter(
                    &mut dispatcher,
                    &mut cpu,
                    &mut bus,
                    setter,
                    trap_word,
                    default,
                );
                assert_eq!(bus.read_long(raw_entry), initial_raw);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} restored default ${trap_word:04X}"
                );

                if legacy_uses_os != is_toolbox {
                    call_trap_manager_setter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        0xA047,
                        trap_word,
                        first_patch,
                    );
                    assert_eq!(
                        call_trap_manager_getter(
                            &mut dispatcher,
                            &mut cpu,
                            &mut bus,
                            getter,
                            trap_word,
                        ),
                        first_patch,
                        "{profile:?} legacy setter ${trap_word:04X}"
                    );
                    call_trap_manager_setter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        0xA047,
                        trap_word,
                        default,
                    );
                    assert_eq!(bus.read_long(raw_entry), initial_raw);
                }

                bus.write_long(raw_entry, raw_patch);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    raw_patch,
                    "{profile:?} raw table patch ${trap_word:04X}"
                );
                cpu.write_reg(Register::PC, RETURN_PC);
                cpu.write_reg(Register::A7, SP);
                dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
                assert_eq!(cpu.read_reg(Register::PC), raw_patch);
                cpu.write_reg(Register::PC, RETURN_PC);
                cpu.write_reg(Register::A7, SP);
                dispatcher.retire_returned_native_trap_call(&mut cpu);
                assert!(dispatcher.pending_native_trap_calls.is_empty());

                bus.write_long(raw_entry, initial_raw);
                assert_eq!(
                    call_trap_manager_getter(
                        &mut dispatcher,
                        &mut cpu,
                        &mut bus,
                        getter,
                        trap_word,
                    ),
                    default,
                    "{profile:?} raw restore ${trap_word:04X}"
                );
            }
        }
    }

    #[test]
    fn standalone_trap_initialization_preserves_live_patches_and_restarts_after_teardown() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let word = 0xA078; // SwapMMUMode has a permanent head on the classic profile.
        let entry = OS_TRAP_TABLE_BASE + 0x78 * 4;
        cpu.write_reg(Register::D0, 0x78);
        dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
        let default = cpu.read_reg(Register::A0);
        let initial_head = bus.read_long(entry);
        assert_ne!(default, 0);
        assert_ne!(initial_head, default);
        assert_eq!(
            bus.read_long(initial_head),
            super::super::manager::COME_FROM_PATCH_SIGNATURE
        );
        assert_eq!(
            dispatcher.trap_table_profile,
            Some(TrapTableProfile::M68k68040)
        );

        let patch = 0x0021_0000;
        bus.write_long(entry, patch);
        let vectors = [bus.read_long(0x28), bus.read_long(0x2C)];
        bus.write_long(0x2C, patch);
        dispatcher.initialize_trap_tables(&mut bus).unwrap();
        assert_eq!(bus.read_long(entry), patch);
        assert_eq!(bus.read_long(0x2C), patch);
        cpu.write_reg(Register::PC, 0x0020_0002);
        dispatcher.dispatch(word, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::PC), patch);
        let sp = cpu.read_reg(Register::A7);
        dispatcher.initialize_trap_tables(&mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(dispatcher.pending_native_trap_calls[&word].len(), 1);

        dispatcher.teardown_trap_table_process_context();
        dispatcher.initialize_trap_tables(&mut bus).unwrap();
        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_ne!(bus.read_long(entry), initial_head);
        assert_eq!(dispatcher.trap_table_address(&bus, word), Some(default));
        assert_ne!([bus.read_long(0x28), bus.read_long(0x2C)], vectors);
        assert!(dispatcher.aline_vector_is_default(&bus));
        assert!(dispatcher.fline_vector_is_default(&bus));
    }

    #[test]
    fn replacement_dispatcher_reuses_memory_owned_defaults_with_fresh_process_heads() {
        let (mut first, _, mut bus) = setup();
        first
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .unwrap();
        let tick = first.trap_table_address(&bus, 0xA975).unwrap();
        let protected_cell = raw_trap_route(0xA823).table_address;
        let head = bus.read_long(protected_cell);
        let vectors = [bus.read_long(0x28), bus.read_long(0x2c)];
        let mut second = TrapDispatcher::new();
        second
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .unwrap();
        assert_eq!(second.trap_table_address(&bus, 0xA975), Some(tick));
        assert_eq!(bus.read_word(tick), 0xAD75);
        assert_ne!(bus.read_long(protected_cell), head);
        assert_ne!([bus.read_long(0x28), bus.read_long(0x2c)], vectors);
        bus.write_word(tick, 0xffff);
        assert_eq!(bus.read_word(tick), 0xAD75);
    }

    #[test]
    fn profile_materialization_refuses_before_mutating_active_process() {
        for failure in 0..6 {
            let (mut dispatcher, _, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
                .unwrap();
            let tick_cell = raw_trap_route(0xA975).table_address;
            bus.write_long(tick_cell, 0x1234_5678);
            dispatcher.current_trap_caller = Some(0x0020_1000);
            dispatcher.pending_native_trap_calls.insert(
                0xA975,
                vec![NativeTrapCallState {
                    return_pc: 0x0020_2000,
                    argument_sp: 0x003f_ff00,
                    os_dispatch_frame: None,
                    preserved_d_regs: [1; 5],
                    preserved_a_regs: [2; 5],
                }],
            );
            match failure {
                0 => {
                    while bus.synthetic_code_allocation_start(4).is_some() {
                        bus.alloc_synthetic(4);
                    }
                }
                1 => bus.protect_readonly_code(OS_TRAP_TABLE_BASE, 4),
                2 => bus.protect_readonly_code(TOOLBOX_TRAP_TABLE_BASE, 4),
                3 => bus.protect_readonly_code(0x28, 4),
                4 | 5 => {
                    let address = if failure == 4 {
                        bus.synthetic_code_allocation_start(4).unwrap()
                    } else {
                        OS_TRAP_TABLE_BASE
                    };
                    let mut foreign = crate::memory::GuestAddressSpace::new();
                    foreign.add_readonly_region(address, vec![0x55; 4]);
                    bus.attach_guest_address_space(foreign.shared_view());
                }
                _ => unreachable!(),
            }
            let low_memory = bus.read_bytes(0, 0x2000);
            let (base, len) = bus.synthetic_reservation_range().unwrap();
            let code = bus.read_bytes(base, len as usize);
            let next = bus.synthetic_code_allocation_start(4);
            let defaults = dispatcher.trap_exception_vector_defaults;
            let tick_default = bus.system_trap_gateway(0xA975);
            assert!(matches!(
                dispatcher.materialize_trap_tables(&mut bus, TrapTableProfile::PowerPc604),
                Err(Error::TrapTableInitialization)
            ));
            assert_eq!(bus.read_bytes(0, 0x2000), low_memory);
            assert_eq!(bus.read_bytes(base, len as usize), code);
            assert_eq!(bus.synthetic_code_allocation_start(4), next);
            assert_eq!(
                dispatcher.trap_table_profile,
                Some(TrapTableProfile::M68k68040)
            );
            assert_eq!(dispatcher.trap_exception_vector_defaults, defaults);
            assert_eq!(bus.system_trap_gateway(0xA975), tick_default);
            assert_eq!(dispatcher.current_trap_caller, Some(0x0020_1000));
            assert_eq!(bus.read_long(tick_cell), 0x1234_5678);
            let frames = &dispatcher.pending_native_trap_calls[&0xA975];
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].return_pc, 0x0020_2000);
            assert_eq!(frames[0].argument_sp, 0x003f_ff00);
            assert_eq!(frames[0].preserved_d_regs, [1; 5]);
            assert_eq!(frames[0].preserved_a_regs, [2; 5]);
            if failure >= 4 {
                bus.detach_guest_address_space();
                dispatcher
                    .materialize_trap_tables(&mut bus, TrapTableProfile::PowerPc604)
                    .unwrap();
                assert_eq!(
                    dispatcher.trap_table_profile,
                    Some(TrapTableProfile::PowerPc604)
                );
                assert_eq!(dispatcher.current_trap_caller, None);
                assert!(dispatcher.pending_native_trap_calls.is_empty());
                assert_ne!(bus.read_long(tick_cell), 0x1234_5678);
            }
        }
    }

    #[test]
    fn standalone_trap_initialization_refuses_unavailable_memory_atomically_and_retries() {
        for failure in [0, 1, 2, 3, 5, 6] {
            let (mut dispatcher, mut cpu, _) = setup();
            let mut bus = MacMemoryBus::new(match failure {
                0 => 0x1000,
                _ => 4 * 1024 * 1024,
            });
            match failure {
                1 => {
                    assert_ne!(bus.alloc_synthetic(64 * 1024), 0);
                }
                2 => bus.protect_readonly_code(TOOLBOX_TRAP_TABLE_BASE, 4),
                5 => bus.protect_readonly_code(0x28, 4),
                6 => bus.protect_readonly_code(OS_TRAP_TABLE_BASE, 4),
                3 => {
                    let address = bus.synthetic_code_allocation_start(4).unwrap();
                    let mut foreign = crate::memory::GuestAddressSpace::new();
                    foreign.add_readonly_region(address, vec![0x55; 4]);
                    bus.attach_guest_address_space(foreign.shared_view());
                }
                _ => {}
            }
            let low_memory = bus.read_bytes(0, 0x2000.min(bus.ram_size() as usize));
            let synthetic = bus
                .synthetic_reservation_range()
                .map(|(base, len)| (base, bus.read_bytes(base, len as usize)));
            cpu.write_reg(Register::D0, 0x78);
            cpu.write_reg(Register::A0, 0x1234_5678);
            cpu.write_reg(Register::PC, 0x0020_0002);
            let sp = cpu.read_reg(Register::A7);
            assert!(matches!(
                dispatcher.initialize_trap_tables(&mut bus),
                Err(Error::TrapTableInitialization)
            ));
            assert!(matches!(
                dispatcher.dispatch(0xA346, &mut cpu, &mut bus),
                Err(Error::TrapTableInitialization)
            ));
            for number in [0x46, 0x47] {
                assert!(matches!(
                    dispatcher.dispatch_memory(false, number, &mut cpu, &mut bus),
                    Some(Err(Error::TrapTableInitialization))
                ));
            }
            assert_eq!(bus.read_bytes(0, low_memory.len()), low_memory);
            if let Some((base, bytes)) = synthetic {
                assert_eq!(bus.read_bytes(base, bytes.len()), bytes);
            }
            assert_eq!(cpu.read_reg(Register::D0), 0x78);
            assert_eq!(cpu.read_reg(Register::A0), 0x1234_5678);
            assert_eq!(cpu.read_reg(Register::PC), 0x0020_0002);
            assert_eq!(cpu.read_reg(Register::A7), sp);
            assert_eq!(dispatcher.trap_count, 0);
            assert_eq!(dispatcher.trap_table_profile, None);
            assert!(!dispatcher.aline_vector_is_default(&bus));
            assert!(!dispatcher.fline_vector_is_default(&bus));
            assert!(bus.system_trap_gateways_are_empty());
            if failure == 3 {
                bus.detach_guest_address_space();
            } else {
                bus = MacMemoryBus::new(4 * 1024 * 1024);
            }
            dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
            assert_ne!(cpu.read_reg(Register::A0), 0);
            assert_eq!(
                dispatcher.trap_table_profile,
                Some(TrapTableProfile::M68k68040)
            );
        }
    }

    #[test]
    fn standalone_trap_initialization_keeps_gateways_callable_in_24_bit_mode() {
        let (mut dispatcher, mut cpu, _) = setup();
        let mut bus = MacMemoryBus::new(32 * 1024 * 1024);
        bus.set_addressing_32_bit(false);
        dispatcher.initialize_trap_tables(&mut bus).unwrap();

        cpu.write_reg(Register::D0, 0xA8AA);
        dispatcher.dispatch(0xA746, &mut cpu, &mut bus).unwrap();
        let gateway = cpu.read_reg(Register::A0);
        assert_ne!(gateway, 0);
        assert_eq!(bus.read_word(gateway), 0xACAA);
    }

    #[test]
    fn classic_getter_does_not_reconstruct_a_default_for_a_cyclic_guest_chain() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let entry = OS_TRAP_TABLE_BASE + 0x78 * 4;
        let head = bus.read_long(entry);
        let default = dispatcher.trap_table_address(&bus, 0xA078).unwrap();
        assert!(bus.try_write_protected_code_long(head + 4, head));
        cpu.write_reg(Register::D0, 0x78);
        dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), 0);
        assert_eq!(bus.read_long(entry), head);
        assert_eq!(bus.read_long(head + 4), head);
        assert!(bus.try_write_protected_code_long(head + 4, default));
        cpu.write_reg(Register::D0, 0x78);
        dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), default);
    }

    #[test]
    fn malformed_trap_entry_refuses_dispatch_but_preserves_saved_default_gateway_calls() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let entry = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        let default = dispatcher.trap_table_address(&bus, 0xA975).unwrap();
        let head =
            crate::trap::gateways::TrapSystemGateways::create_come_from_head(&mut bus, default);
        assert!(bus.try_write_protected_code_long(head + 4, head));
        bus.write_long(entry, head);
        let sp = cpu.read_reg(Register::A7);
        let sentinel = 0xABCD_EF01;
        bus.write_long(sp, sentinel);
        cpu.write_reg(Register::PC, 0x0020_0002);
        assert!(matches!(
            dispatcher.dispatch(0xA975, &mut cpu, &mut bus),
            Err(Error::TrapTableLookup(0xA975))
        ));
        assert_eq!(
            bus.read_long(sp),
            sentinel,
            "no TickCount result was delivered"
        );
        assert!(dispatcher.pending_native_trap_calls.is_empty());

        // A saved system address deliberately bypasses the current patch
        // head, even if the application has since corrupted that head.
        // Inside Macintosh: Operating System Utilities (1994), pp. 8-23--8-30.
        let return_pc = 0x0020_0100;
        bus.write_long(sp, return_pc);
        bus.write_long(sp + 4, sentinel);
        bus.write_long(crate::memory::globals::addr::TICKS, 1234);
        cpu.write_reg(Register::PC, default + 2);
        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), 1234);
        assert_eq!(bus.read_long(head + 4), head);
    }

    #[test]
    fn come_from_chain_resolution_reaches_the_last_exit_and_rejects_cycles() {
        let first = 0x0010_0000;
        let second = 0x0010_0100;
        let target = 0x0020_0000;
        let read_chain = |address| match address {
            address if address == first || address == second => Some(COME_FROM_PATCH_SIGNATURE),
            address if address == first + 4 => Some(second),
            address if address == second + 4 => Some(target),
            _ => None,
        };

        assert_eq!(
            resolve_trap_table_target(first, read_chain),
            Some(TrapTableTarget::Protected {
                last_head: second,
                logical_successor: target,
            })
        );
        assert_eq!(
            resolve_trap_table_target(target, read_chain),
            Some(TrapTableTarget::Direct(target))
        );
        assert_eq!(
            resolve_trap_table_target(first, |address| match address {
                address if address == first || address == second => {
                    Some(COME_FROM_PATCH_SIGNATURE)
                }
                address if address == first + 4 => Some(second),
                address if address == second + 4 => Some(first),
                _ => None,
            }),
            None
        );
    }

    #[test]
    fn materialized_trap_tables_contain_all_callable_profile_entries() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");

        for slot in 0..OS_TRAP_TABLE_SLOTS {
            let trap_word = 0xA000 | slot;
            let gateway_word = TrapTableProfile::M68k68040
                .route(trap_word)
                .default_gateway_word;
            let entry = bus.read_long(OS_TRAP_TABLE_BASE + u32::from(slot) * 4);
            assert_ne!(entry, 0, "OS trap slot ${slot:02X}");
            if M68K_68040_COME_FROM_TRAPS.contains(&trap_word) {
                assert_eq!(bus.read_long(entry), 0x6006_4EF9);
                assert_eq!(bus.read_word(entry + 8), 0x60F8);
                assert_eq!(bus.read_word(bus.read_long(entry + 4)), gateway_word);
            } else {
                assert_eq!(bus.read_word(entry), gateway_word);
                assert_eq!(bus.read_word(entry + 2), 0x4E75);
            }
        }
        for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
            let trap_word = 0xA800 | slot;
            let gateway_word = TrapTableProfile::M68k68040
                .route(trap_word)
                .default_gateway_word;
            let entry = bus.read_long(TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4);
            assert_ne!(entry, 0, "Toolbox trap slot ${slot:03X}");
            if M68K_68040_COME_FROM_TRAPS.contains(&trap_word) {
                assert_eq!(bus.read_long(entry), 0x6006_4EF9);
                assert_eq!(bus.read_word(entry + 8), 0x60F8);
                assert_eq!(
                    bus.read_word(bus.read_long(entry + 4)),
                    gateway_word | 0x0400
                );
            } else {
                assert_eq!(bus.read_word(entry), gateway_word | 0x0400);
            }
        }
    }

    #[test]
    fn machine_profiles_generate_distinct_protected_exception_vector_defaults() {
        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, profile)
                .expect("trap table construction requires writable cells and system storage");
            let defaults = dispatcher.trap_exception_vector_defaults.unwrap();

            assert_ne!(defaults[0], defaults[1]);
            assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], defaults);
            for &gateway in &defaults {
                assert_ne!(gateway, 0);
                assert_eq!(bus.read_word(gateway), 0x4E73); // RTE
                bus.write_word(gateway, 0x4E71);
                assert_eq!(bus.read_word(gateway), 0x4E73, "gateway is protected");

                for slot in 0..OS_TRAP_TABLE_SLOTS {
                    assert_ne!(
                        gateway,
                        bus.read_long(OS_TRAP_TABLE_BASE + u32::from(slot) * 4)
                    );
                }
                for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                    assert_ne!(
                        gateway,
                        bus.read_long(TOOLBOX_TRAP_TABLE_BASE + u32::from(slot) * 4)
                    );
                }
            }
        }
    }

    #[test]
    fn machine_profiles_materialize_their_observed_come_from_sets() {
        for (profile, expected) in [
            (TrapTableProfile::M68k68040, M68K_68040_COME_FROM_TRAPS),
            (TrapTableProfile::PowerPc604, POWERPC_604_COME_FROM_TRAPS),
        ] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, profile)
                .expect("trap table construction requires writable cells and system storage");
            let mut observed = Vec::new();
            for slot in 0..OS_TRAP_TABLE_SLOTS {
                let word = 0xA000 | slot;
                let raw = bus.read_long(TrapDispatcher::raw_trap_table_entry(word));
                if bus.read_long(raw) == 0x6006_4EF9 {
                    observed.push(word);
                }
            }
            for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                let word = 0xA800 | slot;
                let raw = bus.read_long(TrapDispatcher::raw_trap_table_entry(word));
                if bus.read_long(raw) == 0x6006_4EF9 {
                    observed.push(word);
                }
            }
            assert_eq!(observed, expected);
        }
    }

    #[test]
    fn process_switch_restores_raw_trap_topology_and_native_call_frames() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        let protected_word = 0xA823;
        let direct_word = 0xA975;
        let protected_entry = TrapDispatcher::raw_trap_table_entry(protected_word);
        let direct_entry = TrapDispatcher::raw_trap_table_entry(direct_word);
        let first_protected_patch = 0x0021_0000;
        let first_direct_patch = 0x0021_1000;
        let second_protected_patch = 0x0022_0000;
        let second_direct_patch = 0x0022_1000;

        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let first_head = bus.read_long(protected_entry);
        let first_defaults = dispatcher.trap_exception_vector_defaults.unwrap();
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], first_defaults);
        let first_aline_patch = 0x0020_F000;
        bus.write_long(0x28, first_aline_patch);
        dispatcher
            .install_trap_address(&mut bus, protected_word, first_protected_patch)
            .expect("first protected patch must install");
        bus.write_long(direct_entry, first_direct_patch);
        dispatcher.pending_native_trap_calls.insert(
            0xA039,
            vec![NativeTrapCallState {
                return_pc: 0x0023_0000,
                argument_sp: 0x003F_FF00,
                os_dispatch_frame: None,
                preserved_d_regs: [1; 5],
                preserved_a_regs: [2; 5],
            }],
        );
        dispatcher.current_trap_caller = Some(0x0023_1000);

        let fresh_second = dispatcher
            .create_trap_table_process_context(&mut bus, TrapTableProfile::PowerPc604)
            .unwrap();
        let first_context = dispatcher
            .switch_trap_table_process_context(&mut bus, fresh_second)
            .expect("first process context must be saved");
        let second_head = bus.read_long(protected_entry);
        let second_defaults = dispatcher.trap_exception_vector_defaults.unwrap();
        assert_ne!(second_head, first_head);
        assert_ne!(second_defaults, first_defaults);
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], second_defaults);
        assert_eq!(
            dispatcher.trap_table_profile,
            Some(TrapTableProfile::PowerPc604)
        );
        assert!(!dispatcher.has_native_trap_patch(&bus, protected_word));
        assert!(!dispatcher.has_native_trap_patch(&bus, direct_word));
        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(dispatcher.current_trap_caller, None);

        dispatcher
            .install_trap_address(&mut bus, protected_word, second_protected_patch)
            .expect("second protected patch must install");
        bus.write_long(direct_entry, second_direct_patch);
        dispatcher.pending_native_trap_calls.insert(
            0xA975,
            vec![NativeTrapCallState {
                return_pc: 0x0024_0000,
                argument_sp: 0x003F_FE00,
                os_dispatch_frame: None,
                preserved_d_regs: [3; 5],
                preserved_a_regs: [4; 5],
            }],
        );
        dispatcher.current_trap_caller = Some(0x0024_1000);

        let second_context = dispatcher
            .switch_trap_table_process_context(&mut bus, first_context)
            .expect("second process context must be saved");
        assert_eq!(
            dispatcher.trap_table_profile,
            Some(TrapTableProfile::M68k68040)
        );
        assert_eq!(bus.read_long(protected_entry), first_head);
        assert_eq!(
            dispatcher.trap_table_address(&bus, protected_word),
            Some(first_protected_patch)
        );
        assert_eq!(bus.read_long(direct_entry), first_direct_patch);
        assert_eq!(bus.read_long(0x28), first_aline_patch);
        assert_eq!(bus.read_long(0x2C), first_defaults[1]);
        assert_eq!(
            dispatcher.trap_exception_vector_defaults,
            Some(first_defaults)
        );
        assert!(dispatcher.pending_native_trap_calls.contains_key(&0xA039));
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA975));
        assert_eq!(dispatcher.current_trap_caller, Some(0x0023_1000));

        let _first_context = dispatcher
            .switch_trap_table_process_context(&mut bus, second_context)
            .expect("restored first context must be saved again");
        assert_eq!(bus.read_long(protected_entry), second_head);
        assert_eq!(
            dispatcher.trap_table_address(&bus, protected_word),
            Some(second_protected_patch)
        );
        assert_eq!(bus.read_long(direct_entry), second_direct_patch);
        assert_eq!([bus.read_long(0x28), bus.read_long(0x2C)], second_defaults);
        assert_eq!(
            dispatcher.trap_exception_vector_defaults,
            Some(second_defaults)
        );
        assert!(dispatcher.pending_native_trap_calls.contains_key(&0xA975));
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA039));
        assert_eq!(dispatcher.current_trap_caller, Some(0x0024_1000));

        dispatcher.teardown_trap_table_process_context();
        assert_eq!(dispatcher.trap_table_profile, None);
        assert_eq!(dispatcher.trap_exception_vector_defaults, None);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(dispatcher.current_trap_caller, None);
    }

    #[test]
    fn machine_profiles_classify_only_aa6e_as_unimplemented() {
        let declared = default_trap_route(0xAA6E);
        assert!(declared.allows(TrapAdapterId::Unimplemented));
        assert!(!declared.allows(TrapAdapterId::Toolbox));

        for profile in [TrapTableProfile::M68k68040, TrapTableProfile::PowerPc604] {
            let (mut dispatcher, _cpu, mut bus) = setup();
            dispatcher
                .materialize_trap_tables(&mut bus, profile)
                .expect("trap table construction requires writable cells and system storage");
            let unimplemented = dispatcher.trap_table_address(&bus, 0xAA6E).unwrap();
            let mut matching_slots = Vec::new();

            for slot in 0..OS_TRAP_TABLE_SLOTS {
                let word = 0xA000 | slot;
                if dispatcher.trap_table_address(&bus, word) == Some(unimplemented) {
                    matching_slots.push(word);
                }
            }
            for slot in 0..TOOLBOX_TRAP_TABLE_SLOTS {
                let word = 0xA800 | slot;
                if dispatcher.trap_table_address(&bus, word) == Some(unimplemented) {
                    matching_slots.push(word);
                }
            }

            assert_eq!(matching_slots, [0xAA6E]);
            assert_eq!(bus.read_word(unimplemented), 0xAE6E);
            assert_ne!(
                dispatcher.trap_table_address(&bus, 0xAA57),
                Some(unimplemented)
            );
        }
    }

    #[test]
    fn aa6e_uses_the_terminal_unimplemented_adapter() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::DS_ERR_CODE, 0xBEEF);

        let result = dispatcher.dispatch(0xAA6E, &mut cpu, &mut bus);

        assert!(matches!(result, Err(crate::Error::Halted)));
        assert_eq!(dispatcher.current_trap_operation, 0xAA6E);
        assert_eq!(
            dispatcher.current_trap_adapter,
            TrapAdapterId::Unimplemented
        );
        assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
    }

    #[test]
    fn machine_profiles_materialize_only_their_reviewed_default_pointer_aliases() {
        let aliases = [(0xA87D, 0xAA02), (0xAA08, 0xAA26)];

        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        for (target, alias) in aliases {
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                dispatcher.trap_table_address(&bus, alias),
                "68040 defaults ${target:04X}/${alias:04X}"
            );
            assert_eq!(
                TrapTableProfile::M68k68040
                    .route(alias)
                    .default_gateway_word,
                target
            );
        }

        let second = dispatcher
            .create_trap_table_process_context(&mut bus, TrapTableProfile::PowerPc604)
            .unwrap();
        let _first = dispatcher
            .switch_trap_table_process_context(&mut bus, second)
            .expect("68040 context must be saved");
        for (target, alias) in aliases {
            assert_ne!(
                dispatcher.trap_table_address(&bus, target),
                dispatcher.trap_table_address(&bus, alias),
                "604 defaults ${target:04X}/${alias:04X}"
            );
            assert_eq!(
                TrapTableProfile::PowerPc604
                    .route(alias)
                    .default_gateway_word,
                alias
            );
        }
    }

    #[test]
    fn a_profile_default_alias_keeps_independent_patch_and_restore_state() {
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");

        for (target, alias, patch) in [(0xA87D, 0xAA02, 0x0021_0000), (0xAA08, 0xAA26, 0x0021_1000)]
        {
            let shared_default = dispatcher.trap_table_address(&bus, alias).unwrap();
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                Some(shared_default)
            );

            dispatcher
                .install_trap_address(&mut bus, alias, patch)
                .expect("alias patch must install");
            assert_eq!(dispatcher.native_trap_handler(&bus, alias), Some(patch));
            assert_eq!(
                dispatcher.trap_table_address(&bus, target),
                Some(shared_default),
                "patching alias ${alias:04X} must not patch ${target:04X}"
            );

            dispatcher
                .install_trap_address(&mut bus, alias, shared_default)
                .expect("alias default must restore");
            assert_eq!(dispatcher.native_trap_handler(&bus, alias), None);
            assert_eq!(
                dispatcher.trap_table_address(&bus, alias),
                Some(shared_default)
            );
        }
    }

    #[test]
    fn saved_closecport_alias_gateway_executes_the_shared_default_procedure() {
        // Universal Interfaces 3.4 names $AA02 as CloseCPort, while Inside
        // Macintosh Volume V, V-72/V-291 records $A87D. The selected 68040
        // profile exposes one default address for those slots. Calling the
        // address saved from $AA02 must therefore execute the shared $A87D
        // procedure and retain its one-CGrafPtr Pascal stack contract.
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let gateway = dispatcher.trap_table_address(&bus, 0xAA02).unwrap();
        let return_pc = 0x0020_0000;
        let sp = 0x003F_FF00;

        assert_eq!(dispatcher.trap_table_address(&bus, 0xA87D), Some(gateway));
        assert_eq!(bus.read_word(gateway), 0xAC7D);
        bus.write_long(sp, return_pc);
        bus.write_long(sp + 4, 0); // NIL CGrafPtr
        cpu.write_reg(Register::PC, gateway + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher
            .dispatch(bus.read_word(gateway), &mut cpu, &mut bus)
            .unwrap();

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn trap_manager_mutates_hidden_successor_without_replacing_raw_head() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let head = bus.read_long(raw_entry);
        let original = bus.read_long(head + 4);
        let first = 0x0021_0000;
        let nested = 0x0021_1000;

        bus.write_long(head + 4, 0xDEAD_BEEF);
        assert_eq!(bus.read_long(head + 4), original);

        for handler in [first, nested, first, original] {
            cpu.write_reg(Register::D0, u32::from(trap_word));
            cpu.write_reg(Register::A0, handler);
            dispatcher.dispatch(0xA247, &mut cpu, &mut bus).unwrap();
            assert_eq!(bus.read_long(raw_entry), head);
            assert_eq!(bus.read_long(head + 4), handler);

            cpu.write_reg(Register::D0, u32::from(trap_word));
            dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
            assert_eq!(cpu.read_reg(Register::A0), handler);
        }
        assert!(!dispatcher.has_native_trap_patch(&bus, trap_word));
    }

    #[test]
    fn trap_manager_mutates_the_last_exit_in_a_multi_head_chain() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let first = bus.read_long(raw_entry);
        let original = bus.read_long(first + 4);
        let second = bus.alloc_synthetic(10);
        bus.write_readonly_code_word(second, 0x6006);
        bus.write_readonly_code_word(second + 2, 0x4EF9);
        TrapDispatcher::write_readonly_code_long(&mut bus, second + 4, original);
        bus.write_readonly_code_word(second + 8, 0x60F8);
        bus.protect_readonly_code(second, 10);
        TrapDispatcher::write_readonly_code_long(&mut bus, first + 4, second);

        cpu.write_reg(Register::D0, u32::from(trap_word));
        dispatcher.dispatch(0xA346, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), original);

        let replacement = 0x0021_0000;
        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, replacement);
        dispatcher.dispatch(0xA247, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(raw_entry), first);
        assert_eq!(bus.read_long(first + 4), second);
        assert_eq!(bus.read_long(second + 4), replacement);
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(replacement)
        );
    }

    #[test]
    fn direct_raw_write_can_bypass_a_permanent_come_from_head() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let trap_word = 0xAAFB;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let old_head = bus.read_long(raw_entry);
        let direct = 0x0021_0000;
        let replacement = 0x0021_1000;

        bus.write_long(raw_entry, direct);
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(direct)
        );

        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, replacement);
        dispatcher.dispatch(0xA647, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(raw_entry), replacement);
        assert_eq!(
            bus.read_long(old_head + 4),
            dispatcher.default_trap_gateway(&bus, trap_word).unwrap()
        );
    }

    #[test]
    fn trap_setter_preserves_an_arbitrary_00f0_pointer_exactly() {
        // Inside Macintosh: Operating System Utilities (1994), pp. 8-29--8-31:
        // Set/NSet installs the supplied address; no guest address range is a
        // host-only restoration token.
        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let trap_word = 0xA004;
        let handler = 0x00F0_A004;

        dispatcher
            .install_trap_address(&mut bus, trap_word, handler)
            .expect("arbitrary handler pointer must install");

        assert_eq!(
            dispatcher.trap_table_address(&bus, trap_word),
            Some(handler)
        );
        assert_eq!(
            dispatcher.native_trap_handler(&bus, trap_word),
            Some(handler)
        );
    }

    #[test]
    fn nset_rejects_a_come_from_head_as_the_new_handler() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let trap_word = 0xA078;
        let raw_entry = TrapDispatcher::raw_trap_table_entry(trap_word);
        let head = bus.read_long(raw_entry);

        cpu.write_reg(Register::D0, u32::from(trap_word));
        cpu.write_reg(Register::A0, head);
        let result = dispatcher.dispatch(0xA247, &mut cpu, &mut bus);

        assert!(matches!(result, Err(crate::Error::Halted)));
        assert_eq!(bus.read_word(crate::memory::globals::addr::DS_ERR_CODE), 12);
        assert_eq!(bus.read_long(raw_entry), head);
        assert!(!dispatcher.has_native_trap_patch(&bus, trap_word));
    }

    #[test]
    fn direct_raw_table_write_is_authoritative_for_dispatch() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let handler = 0x0021_0000;
        let return_pc = 0x0020_0002;
        let sp = 0x003F_FF00;
        bus.write_long(TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4, handler);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xA975, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(bus.read_long(sp - 4), return_pc);
        assert!(dispatcher.has_native_trap_patch(&bus, 0xA975));
    }

    #[test]
    fn trap_manager_apis_and_raw_table_long_stay_coherent() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher
            .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
            .expect("trap table construction requires writable cells and system storage");
        let entry_address = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        let default = bus.read_long(entry_address);
        let handler = 0x0021_0000;

        cpu.write_reg(Register::D0, 0xA975);
        cpu.write_reg(Register::A0, handler);
        dispatcher.dispatch(0xA047, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(entry_address), handler);

        cpu.write_reg(Register::D0, 0xA975);
        dispatcher.dispatch(0xA146, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::A0), handler);

        cpu.write_reg(Register::D0, 0xA975);
        cpu.write_reg(Register::A0, default);
        dispatcher.dispatch(0xA047, &mut cpu, &mut bus).unwrap();
        assert_eq!(bus.read_long(entry_address), default);
        assert!(!dispatcher.has_native_trap_patch(&bus, 0xA975));
    }

    fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
        make_single_resource_fork_bytes_with_attrs(res_type, res_id, data, 0)
    }

    fn make_single_resource_fork_bytes_with_attrs(
        res_type: [u8; 4],
        res_id: i16,
        data: &[u8],
        attrs: u8,
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
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
        bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 8..type_list_start + 10]
            .copy_from_slice(&ref_list_offset.to_be_bytes());

        let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
        bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
        bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_list_start + 4] = attrs;
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
    }

    fn minimal_test_nfnt() -> Vec<u8> {
        let mut bytes = vec![0u8; 38];
        bytes[2..4].copy_from_slice(&32u16.to_be_bytes());
        bytes[4..6].copy_from_slice(&32u16.to_be_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_be_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_be_bytes());
        bytes[16..18].copy_from_slice(&9u16.to_be_bytes());
        bytes[18..20].copy_from_slice(&1u16.to_be_bytes());
        bytes[24..26].copy_from_slice(&1u16.to_be_bytes());
        bytes[26] = 0xC0;
        bytes[28..30].copy_from_slice(&0u16.to_be_bytes());
        bytes[30..32].copy_from_slice(&1u16.to_be_bytes());
        bytes[32..34].copy_from_slice(&2u16.to_be_bytes());
        bytes[34..36].copy_from_slice(&1u16.to_be_bytes());
        bytes[36..38].copy_from_slice(&1u16.to_be_bytes());
        bytes
    }

    fn test_fond(font_resource_id: i16, size: i16) -> Vec<u8> {
        let mut bytes = vec![0u8; 60];
        bytes[52..54].copy_from_slice(&0u16.to_be_bytes()); // one association minus one
        bytes[54..56].copy_from_slice(&(size as u16).to_be_bytes());
        bytes[56..58].copy_from_slice(&0u16.to_be_bytes()); // plain style
        bytes[58..60].copy_from_slice(&(font_resource_id as u16).to_be_bytes());
        bytes
    }

    #[test]
    fn native_trap_dispatch_returns_past_the_a_line_opcode() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let trap_pc = 0x0020_0000u32;
        let handler_addr = 0x0021_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher
            .install_trap_address(&mut bus, 0xA9F0, handler_addr)
            .unwrap();
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler_addr);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(bus.read_long(sp - 4), trap_pc + 2);
    }

    #[test]
    fn os_hle_dispatch_enforces_every_structural_variant_frame() {
        // The OS Trap Dispatcher places the actual word in D1, restores
        // D1/D2/A1/A2 and conditionally A0, leaves the stack unchanged, and
        // performs TST.W D0. Inside Macintosh: Operating System Utilities
        // (1994), pp. 8-11--8-13.
        for variant in 0u16..8 {
            let (mut dispatcher, mut cpu, mut bus) = setup();
            let trap_word = 0xA01E | (variant << 8); // NewPtr flag/A0 forms
            let original_d1 = 0xD1D1_BEEF;
            let original_d2 = 0xD2D2_BEEF;
            let original_a0 = 0xA0A0_BEEF;
            let original_a1 = 0xA1A1_BEEF;
            let original_a2 = 0xA2A2_BEEF;
            let original_sp = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::D0, 4);
            cpu.write_reg(Register::D1, original_d1);
            cpu.write_reg(Register::D2, original_d2);
            cpu.write_reg(Register::A0, original_a0);
            cpu.write_reg(Register::A1, original_a1);
            cpu.write_reg(Register::A2, original_a2);
            cpu.set_ccr(0x1F);

            dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();

            assert_eq!(dispatcher.current_trap_word, trap_word);
            assert_eq!(cpu.read_reg(Register::D1), original_d1);
            assert_eq!(cpu.read_reg(Register::D2), original_d2);
            assert_eq!(cpu.read_reg(Register::A1), original_a1);
            assert_eq!(cpu.read_reg(Register::A2), original_a2);
            assert_eq!(cpu.read_reg(Register::A7), original_sp);
            if (trap_word & 0x0100) == 0 {
                assert_eq!(cpu.read_reg(Register::A0), original_a0);
            } else {
                assert_ne!(cpu.read_reg(Register::A0), original_a0);
                assert_ne!(cpu.read_reg(Register::A0), 0);
            }
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(cpu.get_ccr(), 0x14, "variant ${trap_word:04X}");
        }
    }

    #[test]
    fn native_os_patch_receives_and_retires_every_structural_variant_frame() {
        let handler_addr = 0x0021_0000u32;
        let return_pc = 0x0020_0002u32;
        let sp = 0x003F_FF00u32;
        for variant in 0u16..8 {
            let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
            let trap_word = 0xA039 | (variant << 8); // ReadDateTime variants
            let original_d1 = 0xD1D1_BEEF;
            let original_d2 = 0xD2D2_BEEF;
            let original_a0 = 0xA0A0_BEEF;
            let original_a1 = 0xA1A1_BEEF;
            let original_a2 = 0xA2A2_BEEF;
            dispatcher
                .install_trap_address(&mut bus, 0xA039, handler_addr)
                .unwrap();
            cpu.write_reg(Register::PC, return_pc);
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D1, original_d1);
            cpu.write_reg(Register::D2, original_d2);
            cpu.write_reg(Register::A0, original_a0);
            cpu.write_reg(Register::A1, original_a1);
            cpu.write_reg(Register::A2, original_a2);

            dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();

            assert_eq!(cpu.read_reg(Register::PC), handler_addr);
            assert_eq!(cpu.read_reg(Register::A7), sp - 4);
            assert_eq!(bus.read_long(sp - 4), return_pc);
            assert_eq!(
                cpu.read_reg(Register::D1),
                0xD1D1_0000 | u32::from(trap_word)
            );
            assert_eq!(
                dispatcher
                    .pending_native_trap_calls
                    .get(&0xA039)
                    .and_then(|calls| calls.last())
                    .and_then(|call| call.os_dispatch_frame)
                    .map(|frame| frame.trap_word),
                Some(trap_word)
            );

            cpu.write_reg(Register::D0, 0xCAFE_8000);
            cpu.write_reg(Register::D1, 0x1111_1111);
            cpu.write_reg(Register::D2, 0x2222_2222);
            cpu.write_reg(Register::A0, 0xAAAA_AAAA);
            cpu.write_reg(Register::A1, 0x1111_AAAA);
            cpu.write_reg(Register::A2, 0x2222_AAAA);
            cpu.write_reg(Register::PC, return_pc);
            cpu.write_reg(Register::A7, sp);
            cpu.set_ccr(0x1F);
            dispatcher.retire_returned_native_trap_call(&mut cpu);

            assert!(dispatcher.pending_native_trap_calls.is_empty());
            assert_eq!(cpu.read_reg(Register::D0), 0xCAFE_8000);
            assert_eq!(cpu.read_reg(Register::D1), original_d1);
            assert_eq!(cpu.read_reg(Register::D2), original_d2);
            assert_eq!(cpu.read_reg(Register::A1), original_a1);
            assert_eq!(cpu.read_reg(Register::A2), original_a2);
            assert_eq!(
                cpu.read_reg(Register::A0),
                if (trap_word & 0x0100) == 0 {
                    original_a0
                } else {
                    0xAAAA_AAAA
                }
            );
            assert_eq!(cpu.get_ccr(), 0x18, "variant ${trap_word:04X}");
        }
    }

    #[test]
    fn saved_os_gateway_tail_uses_the_original_variant_dispatch_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let gateway = bus.get_or_create_system_trap_gateway(0xA01E);
        let handler = 0x0021_0000u32;
        let return_pc = 0x0020_0002u32;
        let sp = 0x003F_FF00u32;
        let trap_word = 0xA71E; // NewPtrSysClear, returning A0
        let original_d1 = 0xD1D1_BEEF;
        let original_d2 = 0xD2D2_BEEF;
        let original_a1 = 0xA1A1_BEEF;
        let original_a2 = 0xA2A2_BEEF;
        dispatcher
            .install_trap_address(&mut bus, 0xA01E, handler)
            .unwrap();
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 4);
        cpu.write_reg(Register::D1, original_d1);
        cpu.write_reg(Register::D2, original_d2);
        cpu.write_reg(Register::A0, 0xA0A0_BEEF);
        cpu.write_reg(Register::A1, original_a1);
        cpu.write_reg(Register::A2, original_a2);

        dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
        cpu.write_reg(Register::D1, 0x1111_1111);
        cpu.write_reg(Register::D2, 0x2222_2222);
        cpu.write_reg(Register::A0, 0xAAAA_AAAA);
        cpu.write_reg(Register::A1, 0x1111_AAAA);
        cpu.write_reg(Register::A2, 0x2222_AAAA);
        cpu.write_reg(Register::PC, gateway + 2);

        dispatcher.dispatch(0xA01E, &mut cpu, &mut bus).unwrap();

        let result_ptr = cpu.read_reg(Register::A0);
        assert_eq!(dispatcher.current_trap_word, trap_word);
        assert_ne!(result_ptr, 0);
        assert_ne!(result_ptr, 0xAAAA_AAAA);
        assert_eq!(bus.read_long(result_ptr), 0);
        assert_eq!(cpu.read_reg(Register::D1), original_d1);
        assert_eq!(cpu.read_reg(Register::D2), original_d2);
        assert_eq!(cpu.read_reg(Register::A1), original_a1);
        assert_eq!(cpu.read_reg(Register::A2), original_a2);
        assert_eq!(cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(cpu.get_ccr(), 0x04);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn saved_os_gateway_subroutine_keeps_the_outer_dispatch_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let gateway = bus.get_or_create_system_trap_gateway(0xA039);
        let handler = 0x0021_0000u32;
        let patch_continuation = 0x0021_0100u32;
        let return_pc = 0x0020_0002u32;
        let output = 0x0022_0000u32;
        let sp = 0x003F_FF00u32;
        let original_d1 = 0xD1D1_BEEF;
        let original_d2 = 0xD2D2_BEEF;
        let original_a0 = 0xA0A0_BEEF;
        let original_a1 = 0xA1A1_BEEF;
        let original_a2 = 0xA2A2_BEEF;
        dispatcher
            .install_trap_address(&mut bus, 0xA039, handler)
            .unwrap();
        bus.write_long(crate::memory::globals::addr::TIME, 0x1234_5678);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D1, original_d1);
        cpu.write_reg(Register::D2, original_d2);
        cpu.write_reg(Register::A0, original_a0);
        cpu.write_reg(Register::A1, original_a1);
        cpu.write_reg(Register::A2, original_a2);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();
        let nested_sp = sp - 8;
        bus.write_long(nested_sp, patch_continuation);
        cpu.write_reg(Register::D1, 0x1111_1111);
        cpu.write_reg(Register::D2, 0x2222_2222);
        cpu.write_reg(Register::A0, output);
        cpu.write_reg(Register::A1, 0x1111_AAAA);
        cpu.write_reg(Register::A2, 0x2222_AAAA);
        cpu.write_reg(Register::A7, nested_sp);
        cpu.write_reg(Register::PC, gateway + 2);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();

        assert_eq!(bus.read_long(output), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_1111);
        assert_eq!(cpu.read_reg(Register::D2), 0x2222_2222);
        assert_eq!(cpu.read_reg(Register::A0), output);
        assert_eq!(cpu.read_reg(Register::A1), 0x1111_AAAA);
        assert_eq!(cpu.read_reg(Register::A2), 0x2222_AAAA);
        assert_eq!(cpu.read_reg(Register::A7), nested_sp);
        assert_eq!(
            dispatcher
                .pending_native_trap_calls
                .get(&0xA039)
                .map(Vec::len),
            Some(1)
        );

        // The saved routine's RTS would return to the patch, whose final RTS
        // then closes the original dispatcher frame.
        cpu.write_reg(Register::D0, 0xCAFE_8000);
        cpu.write_reg(Register::D1, 0x3333_3333);
        cpu.write_reg(Register::D2, 0x4444_4444);
        cpu.write_reg(Register::A0, 0xBBBB_BBBB);
        cpu.write_reg(Register::A1, 0x3333_AAAA);
        cpu.write_reg(Register::A2, 0x4444_AAAA);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.set_ccr(0x1F);
        dispatcher.retire_returned_native_trap_call(&mut cpu);

        assert!(dispatcher.pending_native_trap_calls.is_empty());
        assert_eq!(cpu.read_reg(Register::D0), 0xCAFE_8000);
        assert_eq!(cpu.read_reg(Register::D1), original_d1);
        assert_eq!(cpu.read_reg(Register::D2), original_d2);
        assert_eq!(cpu.read_reg(Register::A0), original_a0);
        assert_eq!(cpu.read_reg(Register::A1), original_a1);
        assert_eq!(cpu.read_reg(Register::A2), original_a2);
        assert_eq!(cpu.get_ccr(), 0x18);
    }

    #[test]
    fn native_auto_pop_trap_enters_patch_with_the_glue_callers_return_frame() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let trap_pc = 0x0020_0000u32;
        let handler_addr = 0x0021_0000u32;
        let caller_pc = 0x0022_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher
            .install_trap_address(&mut bus, 0xA975, handler_addr)
            .unwrap();
        bus.write_long(sp, caller_pc);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), handler_addr);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_long(sp), caller_pc);
        let calls = dispatcher.pending_native_trap_calls.get(&0xA975).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].return_pc, caller_pc);
        assert_eq!(calls[0].argument_sp, sp + 4);
    }

    #[test]
    fn nested_same_native_trap_calls_retain_lifo_call_state() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let handler_addr = 0x0021_0000u32;
        let outer_return = 0x0020_0002u32;
        let inner_return = 0x0021_0102u32;
        let outer_sp = 0x003F_FF00u32;
        let inner_sp = 0x003F_FE00u32;
        dispatcher
            .install_trap_address(&mut bus, 0xA9F0, handler_addr)
            .unwrap();

        cpu.write_reg(Register::PC, outer_return);
        cpu.write_reg(Register::A7, outer_sp);
        dispatcher.dispatch(0xA9F0, &mut cpu, &mut bus).unwrap();

        cpu.write_reg(Register::PC, inner_return);
        cpu.write_reg(Register::A7, inner_sp);
        bus.write_long(inner_sp, inner_return);
        dispatcher.dispatch(0xADF0, &mut cpu, &mut bus).unwrap();

        let calls = dispatcher.pending_native_trap_calls.get(&0xA9F0).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].return_pc, outer_return);
        assert_eq!(calls[1].return_pc, inner_return);
        assert_eq!(calls[1].argument_sp, inner_sp + 4);
        assert_eq!(
            dispatcher
                .take_latest_native_trap_call(0xA9F0)
                .unwrap()
                .return_pc,
            inner_return
        );
        assert_eq!(
            dispatcher
                .take_latest_native_trap_call(0xA9F0)
                .unwrap()
                .return_pc,
            outer_return
        );
        assert!(!dispatcher.pending_native_trap_calls.contains_key(&0xA9F0));
    }

    #[test]
    fn saved_tool_trap_gateway_bypasses_a_later_patch() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let gateway = dispatcher.get_or_create_tool_trap_trampoline(&mut bus, 0xA975);
        let caller_pc = 0x0021_0000u32;
        let sp = 0x003F_FF00u32;
        dispatcher.set_tick_count_for_test(&mut bus, 0x1234_5678);
        dispatcher
            .install_trap_address(&mut bus, 0xA975, 0x0030_0000)
            .unwrap();
        bus.write_long(sp, caller_pc);
        cpu.write_reg(Register::PC, 0x0020_0002);
        cpu.write_reg(Register::A7, sp);

        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();
        assert_eq!(cpu.read_reg(Register::PC), 0x0030_0000);
        assert_eq!(
            dispatcher
                .pending_native_trap_calls
                .get(&0xA975)
                .unwrap()
                .len(),
            1
        );

        cpu.write_reg(Register::PC, gateway + 2);
        dispatcher.dispatch(0xAD75, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), caller_pc);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), 0x1234_5678);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn saved_os_trap_gateway_bypasses_a_later_patch() {
        let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
        let gateway = bus.get_or_create_system_trap_gateway(0xA039);
        let sp = 0x003F_FF00u32;
        let output = 0x0020_0000u32;
        bus.write_long(crate::memory::globals::addr::TIME, 0x1234_5678);
        bus.write_long(sp, 0x0021_0000);
        dispatcher
            .install_trap_address(&mut bus, 0xA039, 0x0030_0000)
            .unwrap();
        cpu.write_reg(Register::PC, gateway + 2);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::A0, output);

        dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();

        assert_eq!(cpu.read_reg(Register::PC), gateway + 2);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(output), 0x1234_5678);
        assert!(dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn fond_associations_register_nfnt_independent_of_resource_load_order() {
        let nfnt = minimal_test_nfnt();

        let mut fond_first = TrapDispatcher::new();
        fond_first.remember_resource_backing_data(7, *b"FOND", 30010, test_fond(42, 15));
        fond_first.remember_resource_backing_data(7, *b"NFNT", 42, nfnt.clone());
        let face = crate::quickdraw::fonts::get_font_face(30010, 15)
            .expect("FOND loaded before NFNT should register the associated face");
        assert_eq!((face.font_id, face.size), (30010, 15));

        let mut nfnt_first = TrapDispatcher::new();
        nfnt_first.remember_resource_backing_data(8, *b"NFNT", 43, nfnt);
        nfnt_first.remember_resource_backing_data(8, *b"FOND", 30011, test_fond(43, 16));
        let face = crate::quickdraw::fonts::get_font_face(30011, 16)
            .expect("FOND loaded after NFNT should register the associated face");
        assert_eq!((face.font_id, face.size), (30011, 16));
    }

    #[test]
    fn hle_tick_cost_accumulates_and_resets() {
        let mut disp = TrapDispatcher::new();

        disp.add_hle_tick_cost(123);
        disp.add_hle_tick_cost(456);

        assert_eq!(disp.take_hle_tick_cost(), 579);
        assert_eq!(disp.take_hle_tick_cost(), 0);
    }

    #[test]
    fn hle_work_cost_helpers_scale_with_resource_and_pixel_work() {
        let small_resource = TrapDispatcher::resource_load_tick_cost(128);
        let large_resource = TrapDispatcher::resource_load_tick_cost(4096);
        let small_blit = TrapDispatcher::quickdraw_blit_tick_cost(16, 16, 8, 8, false);
        let large_blit = TrapDispatcher::quickdraw_blit_tick_cost(320, 200, 8, 8, false);
        let transformed_blit = TrapDispatcher::quickdraw_blit_tick_cost(320, 200, 8, 8, true);
        let picture = TrapDispatcher::draw_picture_tick_cost(320, 200, 32_768);

        assert!(large_resource > small_resource);
        assert!(large_blit > small_blit);
        assert!(transformed_blit > large_blit);
        assert!(picture > large_blit);
    }

    fn install_menu_tracking(disp: &mut TrapDispatcher) {
        disp.menu_tracking
            .set(Some(test_tracked_menu_state(0, (0, 0, 0, 0), 0)));
    }

    fn install_dialog_tracking(disp: &mut TrapDispatcher) {
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr: 0,
            bounds: (0, 0, 0, 0),
            title: String::new(),
            proc_id: 0,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: 0,
            item_hit_ptr: 0,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
    }

    fn install_control_tracking(disp: &mut TrapDispatcher) {
        disp.control_tracking = Some(ControlTrackingState {
            ctrl_handle: 0,
            ctrl_ptr: 0,
            popup_tracking: true,
            active_menu: 0,
            highlighted_item: 0,
            saved_pixels: Default::default(),
            dropdown_rect: (0, 0, 0, 0),
            popup_content_top: 0,
            popup_scroll_direction: None,
            simple_part: 0,
            simple_screen_rect: (0, 0, 0, 0),
            simple_highlighted: false,
            saved_hilite: 0,
            stack_ptr: 0,
            scrollbar_action_proc: 0,
            scrollbar_part: 0,
            scrollbar_last_action_tick: 0,
            scrollbar_idle_refires: 0,
            scrollbar_callback_pending: false,
        });
    }

    fn install_window_tracking(disp: &mut TrapDispatcher) {
        disp.window_tracking = Some(WindowTrackingState {
            window_ptr: 0,
            stack_ptr: 0,
            start_mouse: (0, 0),
            original_port_origin: (0, 0),
            bounds_rect: (0, 0, 0, 0),
            original_outline_rect: (0, 0, 0, 0),
            outline_rect: (0, 0, 0, 0),
            outline_saved_pixels: Vec::new(),
            command_down: false,
        });
    }

    fn install_go_away_tracking(disp: &mut TrapDispatcher) {
        disp.go_away_tracking = Some(GoAwayTrackingState {
            window_ptr: 0,
            stack_ptr: 0,
            hit_rect: (0, 0, 18, 18),
            highlight_rect: (3, 8, 14, 19),
            highlighted: true,
        });
    }

    fn install_region_tracking(disp: &mut TrapDispatcher) {
        disp.region_tracking = Some(RegionTrackingState {
            stack_ptr: 0,
            start_mouse: (0, 0),
            port_bounds_origin: (0, 0),
            limit_rect: None,
            slop_rect: (0, 0, 1, 1),
            axis: 0,
            original_outline_rect: (0, 0, 1, 1),
            outline_rect: None,
            outline_saved_pixels: Vec::new(),
            outline_pattern: [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
        });
    }

    fn centered_playfield_rect() -> ScreenCopyBitsRect {
        ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 400,
            src_right: 640,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        }
    }

    #[test]
    fn fullscreen_input_transform_requires_fullscreen_and_hidden_cursor() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.last_screen_copybits_rect = Some(centered_playfield_rect());

        disp.fullscreen_locked = false;
        disp.cursor_state.set_level_for_test(-1);
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.fullscreen_locked = true;
        disp.cursor_state.set_level_for_test(0);
        assert_eq!(disp.fullscreen_input_transform(), None);

        disp.cursor_state.set_level_for_test(-1);
        assert_eq!(
            disp.fullscreen_input_transform(),
            Some(centered_playfield_rect())
        );
    }

    #[test]
    fn fullscreen_input_transform_rejects_identity_fullscreen_blit() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.fullscreen_locked = true;
        disp.cursor_state.set_level_for_test(-1);
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 600,
            src_right: 800,
            dst_top: 0,
            dst_left: 0,
            dst_bottom: 600,
            dst_right: 800,
        });

        assert_eq!(disp.fullscreen_input_transform(), None);
    }

    #[test]
    fn fullscreen_input_transform_rejects_invalid_copybits_rect() {
        let mut disp = TrapDispatcher::new();
        disp.screen_mode = (0, 1000, 800, 600, 8);
        disp.fullscreen_locked = true;
        disp.cursor_state.set_level_for_test(-1);
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 0,
            src_right: 640,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        });

        assert_eq!(disp.fullscreen_input_transform(), None);
    }

    #[test]
    fn find_vfs_file_in_directory_falls_back_from_colon_path_to_basename() {
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("Disk/App Folder/Settings".to_string(), vec![1, 2, 3]);
        let dir_id = disp.ensure_vfs_directory("Disk/App Folder");

        assert_eq!(
            disp.find_vfs_file_in_directory(dir_id, ":Resources:Settings"),
            Some("Disk/App Folder/Settings".to_string())
        );
    }

    #[test]
    fn hfs_path_encoding_preserves_literal_slashes_and_percent_sequences() {
        let encoded = TrapDispatcher::normalize_hfs_path("Folder:100%/Done");

        assert_eq!(encoded, format!("Folder/100%{VFS_HFS_LITERAL_SLASH}Done"));
        assert_eq!(
            TrapDispatcher::hfs_name_from_vfs_component(&format!(
                "100%{VFS_HFS_LITERAL_SLASH}Done"
            )),
            "100%/Done"
        );
    }

    #[test]
    fn hfs_path_encoding_strips_the_synthetic_unix_volume() {
        assert_eq!(
            TrapDispatcher::normalize_hfs_path("Unix:assertions.txt"),
            "assertions.txt"
        );
        assert_eq!(
            TrapDispatcher::normalize_hfs_path("Unix:Folder:100%/Done"),
            format!("Folder/100%{VFS_HFS_LITERAL_SLASH}Done")
        );
    }

    #[test]
    fn hfs_lookup_path_strips_only_complete_boot_volume_prefixes() {
        assert_eq!(
            TrapDispatcher::normalize_hfs_lookup_path(
                "macintoshhd:System Folder:Preferences:Sierra"
            ),
            "System Folder/Preferences/Sierra"
        );
        assert_eq!(
            TrapDispatcher::normalize_hfs_lookup_path(":MacintoshHD:Preferences"),
            "MacintoshHD/Preferences"
        );
        assert_eq!(
            TrapDispatcher::normalize_hfs_lookup_path("Games:Pinball:Scores"),
            "Games/Pinball/Scores"
        );
    }

    #[test]
    fn find_vfs_file_in_directory_does_not_escape_explicit_parent_for_basename() {
        // Files 1992, 2-29: the poor man's search path is used only when
        // dirID is 0; an explicit parent dirID must not fall through to an
        // unrelated file with the same basename elsewhere on the volume.
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("App/Shared Preferences".to_string(), vec![1, 2, 3]);
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

        assert_eq!(
            disp.find_vfs_file_in_directory(pref_dir_id, "Shared Preferences"),
            None
        );
    }

    #[test]
    fn find_vfs_directory_in_directory_does_not_escape_explicit_parent() {
        // Files 1992, 2-29: a nonzero parent directory ID suppresses the
        // poor man's search path. A same-named directory elsewhere on the
        // volume must not satisfy the lookup.
        let mut disp = TrapDispatcher::new();
        let simfarm_dir_id = disp.ensure_vfs_directory("Maxis/SimFarm");

        assert_eq!(
            disp.find_vfs_directory_in_directory(simfarm_dir_id, "SimFarm"),
            None
        );
    }

    #[test]
    fn find_vfs_rsrc_file_in_directory_falls_back_from_colon_path_to_basename() {
        let mut disp = TrapDispatcher::new();
        disp.vfs_rsrc
            .insert("Disk/App Folder/Companion.rsrc".to_string(), vec![1, 2, 3]);
        let dir_id = disp.ensure_vfs_directory("Disk/App Folder");

        assert_eq!(
            disp.find_vfs_rsrc_file_in_directory(dir_id, ":Resources:Companion.rsrc"),
            Some("Disk/App Folder/Companion.rsrc".to_string())
        );
    }

    #[test]
    fn find_vfs_rsrc_file_in_directory_does_not_escape_explicit_parent_for_basename() {
        // Same explicit-parent rule as data forks: a concrete dirID bounds
        // the lookup, so a resource fork with the same basename elsewhere
        // must not satisfy the request.
        let mut disp = TrapDispatcher::new();
        disp.vfs_rsrc
            .insert("App/Settings.rsrc".to_string(), vec![1, 2, 3]);
        let pref_dir_id = disp.ensure_vfs_directory("System Folder/Preferences");

        assert_eq!(
            disp.find_vfs_rsrc_file_in_directory(pref_dir_id, "Settings.rsrc"),
            None
        );
    }

    #[test]
    fn remove_vfs_path_removes_data_resource_and_metadata_entries() {
        let mut disp = TrapDispatcher::new();
        disp.vfs.insert("Game/Plug-In".to_string(), vec![1, 2, 3]);
        disp.vfs_rsrc
            .insert("Game/Plug-In".to_string(), vec![4, 5, 6]);
        disp.set_vfs_entry_metadata("Game/Plug-In", *b"DATA", *b"TEST", 0x4000);

        assert!(disp.remove_vfs_path("Game/Plug-In"));
        assert!(!disp.vfs.contains_key("Game/Plug-In"));
        assert!(!disp.vfs_rsrc.contains_key("Game/Plug-In"));
        assert!(!disp.vfs_metadata.contains_key("Game/Plug-In"));
    }

    #[test]
    fn remove_vfs_path_removes_directory_subtree_without_touching_siblings() {
        let mut disp = TrapDispatcher::new();
        disp.ensure_vfs_directory("Game/Plug-Ins/MAGMA");
        disp.ensure_vfs_directory("Game/Plug-Ins/Keep");
        disp.vfs
            .insert("Game/Plug-Ins/MAGMA/Data".to_string(), vec![1]);
        disp.vfs_rsrc
            .insert("Game/Plug-Ins/MAGMA/Data".to_string(), vec![2]);
        disp.vfs
            .insert("Game/Plug-Ins/Keep/Data".to_string(), vec![3]);
        disp.set_vfs_entry_metadata("Game/Plug-Ins/MAGMA/Data", *b"DATA", *b"MAGM", 0);
        disp.set_vfs_entry_metadata("Game/Plug-Ins/Keep/Data", *b"DATA", *b"KEEP", 0);

        assert!(disp.remove_vfs_path("Game/Plug-Ins/MAGMA"));
        assert!(!disp.vfs.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp.vfs_rsrc.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp.vfs_metadata.contains_key("Game/Plug-Ins/MAGMA/Data"));
        assert!(!disp
            .vfs_directories
            .iter()
            .any(|directory| directory.path == "Game/Plug-Ins/MAGMA"));
        assert!(disp.vfs.contains_key("Game/Plug-Ins/Keep/Data"));
        assert!(disp.vfs_metadata.contains_key("Game/Plug-Ins/Keep/Data"));
        assert!(disp
            .vfs_directories
            .iter()
            .any(|directory| directory.path == "Game/Plug-Ins/Keep"));
    }

    #[test]
    fn remove_vfs_path_relative_to_launched_app_uses_app_parent() {
        let mut disp = TrapDispatcher::new();
        disp.vfs
            .insert("Game Folder/Plug-Ins/MAGMA".to_string(), vec![1]);
        disp.vfs_rsrc
            .insert("Game Folder/Plug-Ins/MAGMA".to_string(), vec![2]);
        disp.set_launched_app_path("Game Folder/Game App");

        assert!(disp.remove_vfs_path_relative_to_launched_app("Plug-Ins/MAGMA"));
        assert!(!disp.vfs.contains_key("Game Folder/Plug-Ins/MAGMA"));
        assert!(!disp.vfs_rsrc.contains_key("Game Folder/Plug-Ins/MAGMA"));
    }

    #[test]
    fn load_resources_exposes_application_map_and_fork_to_native_runtime_code() {
        let serialized = make_single_resource_fork_bytes_with_attrs(*b"TEST", 7, b"payload", 0x04);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let reference_offset = fork.get(*b"TEST", 7).unwrap().reference_offset as u32;
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();
        disp.set_launched_app_path("Game Folder/Game App");

        disp.load_resources(&fork, &mut bus);

        assert_eq!(
            bus.read_word(crate::memory::globals::addr::RES_LOAD),
            0x0100
        );
        let map_handle = bus.read_long(0x0A50);
        let map_ptr = bus.read_long(map_handle);
        let resource_handle = bus.read_long(map_ptr + reference_offset + 8);
        let resource_ptr = bus.read_long(resource_handle);

        assert_ne!(map_handle, 0, "TopMapHndl must address the application map");
        assert_eq!(
            bus.read_long(map_ptr + 16),
            0,
            "map chain terminates at NIL"
        );
        assert_eq!(
            bus.read_word(map_ptr + 20),
            0,
            "map belongs to the app file"
        );
        assert_eq!(bus.read_bytes(resource_ptr, 7), b"payload");
        assert_eq!(
            disp.vfs.get("__rsrc__Game Folder/Game App"),
            Some(&serialized),
            "native PBRead must see the open application resource fork"
        );
    }

    #[test]
    fn merge_resources_into_existing_file_adds_missing_entries_without_replacing() {
        let app_rsrc = make_single_resource_fork_bytes(*b"TEST", 1, b"app");
        let companion_rsrc = make_single_resource_fork_bytes(*b"TEST", 2, b"side");
        let duplicate_rsrc = make_single_resource_fork_bytes(*b"TEST", 1, b"other");
        let app_fork = ResourceFork::parse(&app_rsrc).unwrap();
        let companion_fork = ResourceFork::parse(&companion_rsrc).unwrap();
        let duplicate_fork = ResourceFork::parse(&duplicate_rsrc).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();

        disp.load_resources(&app_fork, &mut bus);
        assert_eq!(
            disp.merge_resources_into_existing_file(&companion_fork, &mut bus, 0),
            1
        );
        assert_eq!(
            disp.merge_resources_into_existing_file(&duplicate_fork, &mut bus, 0),
            1
        );

        let (_, app_ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 1)
            .unwrap();
        let (_, companion_ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 2)
            .unwrap();
        assert_eq!(bus.read_bytes(app_ptr, 3), b"app");
        assert_eq!(bus.read_bytes(companion_ptr, 4), b"side");
        assert_eq!(disp.count_resources(*b"TEST", true), 2);
    }

    #[test]
    fn opening_resource_fork_defers_non_preload_data_until_requested() {
        let payload = vec![0xA5; 1024 * 1024];
        let serialized = make_single_resource_fork_bytes(*b"TEST", 7, &payload);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();
        let heap_before = bus.heap_bump_ptr();

        disp.load_resources(&fork, &mut bus);

        let heap_after_open = bus.heap_bump_ptr();
        assert!(
            heap_after_open - heap_before < payload.len() as u32,
            "opening the resource map must not copy ordinary resource data into the guest heap"
        );
        assert_eq!(
            disp.resources.as_ref().unwrap().files[&0].loaded[&(*b"TEST", 7)],
            0
        );

        let (_, ptr) = disp
            .find_or_load_resource_any(&mut bus, *b"TEST", 7)
            .expect("GetResource-style lookup should materialize deferred data");
        assert_eq!(bus.read_bytes(ptr, payload.len()), payload);
        assert!(bus.heap_bump_ptr() - heap_after_open >= payload.len() as u32);
    }

    #[test]
    fn opening_resource_fork_materializes_respreload_data() {
        let serialized =
            make_single_resource_fork_bytes_with_attrs(*b"TEST", 7, b"preloaded", 0x04);
        let fork = ResourceFork::parse(&serialized).unwrap();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let mut disp = TrapDispatcher::new();

        disp.load_resources(&fork, &mut bus);

        let ptr = disp.resources.as_ref().unwrap().files[&0].loaded[&(*b"TEST", 7)];
        assert_ne!(ptr, 0);
        assert_eq!(bus.read_bytes(ptr, 9), b"preloaded");
        assert!(disp.resident_resources.contains(&(0, *b"TEST", 7)));
    }

    // Lock the `is_tracking_refire` contract — returns true exactly when
    // (a) tracking is active AND (b) the trap word is one of the
    // refire-relevant traps (auto-pop variants included). The method is
    // the canonical predicate; both dispatch.rs and runner.rs call it.

    #[test]
    fn is_tracking_refire_false_when_no_tracking_active() {
        let disp = TrapDispatcher::new();
        // Refire-relevant traps with no tracking → false.
        assert!(!disp.is_tracking_refire(0xA93D)); // MenuSelect
        assert!(!disp.is_tracking_refire(0xA80B)); // MenuKey
        assert!(!disp.is_tracking_refire(0xA991)); // ModalDialog
        assert!(!disp.is_tracking_refire(0xA985)); // Alert
        assert!(!disp.is_tracking_refire(0xA986)); // StopAlert
        assert!(!disp.is_tracking_refire(0xA987)); // NoteAlert
        assert!(!disp.is_tracking_refire(0xA988)); // CautionAlert
        assert!(!disp.is_tracking_refire(0xA968)); // TrackControl
        assert!(!disp.is_tracking_refire(0xA91E)); // TrackGoAway
        assert!(!disp.is_tracking_refire(0xA925)); // DragWindow
        assert!(!disp.is_tracking_refire(0xA92B)); // GrowWindow
        assert!(!disp.is_tracking_refire(0xA905)); // DragGrayRgn
        assert!(!disp.is_tracking_refire(0xA926)); // DragTheRgn

        // Auto-pop variants too.
        assert!(!disp.is_tracking_refire(0xAD3D));
        assert!(!disp.is_tracking_refire(0xAC0B));
        assert!(!disp.is_tracking_refire(0xAD91));
        assert!(!disp.is_tracking_refire(0xAD68));
        assert!(!disp.is_tracking_refire(0xAD2B));
    }

    #[test]
    fn menu_tracking_uses_operation_resumption_instead_of_refiring() {
        let mut disp = TrapDispatcher::new();
        install_menu_tracking(&mut disp);
        assert!(!disp.is_tracking_refire(0xA93D));
        assert!(!disp.is_tracking_refire(0xA80B));
        // Auto-pop variants share the same predicate.
        assert!(!disp.is_tracking_refire(0xAD3D));
        assert!(!disp.is_tracking_refire(0xAC0B));
    }

    #[test]
    fn is_tracking_refire_true_for_dialog_trap_when_dialog_tracking() {
        let mut disp = TrapDispatcher::new();
        install_dialog_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA991));
        assert!(disp.is_tracking_refire(0xAD91));
        assert!(disp.is_tracking_refire(0xA985));
        assert!(disp.is_tracking_refire(0xA986));
        assert!(disp.is_tracking_refire(0xA987));
        assert!(disp.is_tracking_refire(0xA988));
    }

    #[test]
    fn is_tracking_refire_true_for_trackcontrol_when_control_tracking() {
        let mut disp = TrapDispatcher::new();
        install_control_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA968));
        assert!(disp.is_tracking_refire(0xAD68));
    }

    #[test]
    fn is_tracking_refire_true_for_dragwindow_when_window_tracking() {
        let mut disp = TrapDispatcher::new();
        install_window_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA925));
        assert!(disp.is_tracking_refire(0xAD25));
    }

    #[test]
    fn is_tracking_refire_true_for_trackgoaway_when_close_box_tracking() {
        let mut disp = TrapDispatcher::new();
        install_go_away_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA91E));
        assert!(disp.is_tracking_refire(0xAD1E));
    }

    #[test]
    fn is_tracking_refire_true_for_drag_region_family_when_region_tracking() {
        let mut disp = TrapDispatcher::new();
        install_region_tracking(&mut disp);
        assert!(disp.is_tracking_refire(0xA905));
        assert!(disp.is_tracking_refire(0xAD05));
        assert!(disp.is_tracking_refire(0xA926));
        assert!(disp.is_tracking_refire(0xAD26));
    }

    // Lock the `current_trap_caller` contract — preserved when an auto-pop
    // trap halts (so the runner's halt log can surface the JSR caller PC),
    // cleared on success.

    #[test]
    fn current_trap_caller_preserved_on_halt() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::{setup, TEST_SP};

        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let caller_pc = 0xCAFE_BABEu32;
        // Auto-pop pops the JSR return address from the top of stack.
        bus.write_long(sp, caller_pc);
        // SysError reads errorCode (INTEGER, 16-bit) from new SP after
        // auto-pop has advanced past the return address.
        bus.write_word(sp + 4, 0x002A);

        // SysError ($A9C9) with auto-pop bit set ($A9C9 | 0x0400 = $ADC9).
        let result = disp.dispatch(0xADC9, &mut cpu, &mut bus);

        assert!(
            matches!(result, Err(crate::Error::Halted)),
            "SysError must halt the runner, got {:?}",
            result
        );
        assert_eq!(
            disp.current_trap_caller,
            Some(caller_pc),
            "current_trap_caller must be retained across a halt so \
             the runner halt log can surface caller=$XXXXXXXX"
        );
    }

    #[test]
    fn current_trap_caller_falls_back_to_direct_halt_site() {
        use crate::trap::test_helpers::setup;

        let (mut disp, mut cpu, mut bus) = setup();
        let trap_pc = 0x1234_5678u32;
        cpu.write_reg(Register::PC, trap_pc);

        let result = disp.dispatch(0xA05B, &mut cpu, &mut bus);

        assert!(
            matches!(result, Err(crate::Error::Halted)),
            "PowerOff must halt the runner, got {:?}",
            result
        );
        assert_eq!(
            disp.current_trap_caller,
            Some(trap_pc.wrapping_sub(2)),
            "direct halt traps must surface the trap site when no auto-pop \
             caller is available"
        );
    }

    #[test]
    fn current_trap_caller_cleared_on_success() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::{setup, TEST_SP};

        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let caller_pc = 0xDEAD_BEEFu32;
        bus.write_long(sp, caller_pc);

        // TickCount ($A975) auto-pop variant ($AD75). No-arg trap that
        // writes a 32-bit tick count to the (post-auto-pop) top of stack
        // and returns Ok.
        let result = disp.dispatch(0xAD75, &mut cpu, &mut bus);

        assert!(result.is_ok(), "TickCount must succeed: {:?}", result);
        assert_eq!(
            disp.current_trap_caller, None,
            "current_trap_caller must be cleared after a successful \
             auto-pop dispatch so the next trap doesn't inherit a stale value"
        );
    }

    #[test]
    fn tool_trap_trampoline_canonicalizes_bare_and_canonical_getmasktable_words() {
        use crate::memory::MemoryBus;
        use crate::trap::test_helpers::setup;

        let (mut disp, _cpu, mut bus) = setup();

        let addr_bare = disp.get_or_create_tool_trap_trampoline(&mut bus, 0x836);
        let addr_canonical = disp.get_or_create_tool_trap_trampoline(&mut bus, 0xA836);

        assert_eq!(
            addr_bare, addr_canonical,
            "canonicalized tool-trap words should share one trampoline"
        );
        assert_eq!(
            bus.read_word(addr_bare),
            0xAC36,
            "GetMaskTable trampoline must store the canonical auto-pop trap word"
        );
    }

    #[test]
    fn is_tracking_refire_false_for_unrelated_traps_during_tracking() {
        // Even with tracking active, only the specific refire traps must
        // trigger push-back. Any other trap dispatched during tracking
        // (TickCount, GetNewWindow, SysError, the game's own jump-table
        // A-line stubs, …) MUST return false.
        let mut disp = TrapDispatcher::new();
        install_menu_tracking(&mut disp);
        install_dialog_tracking(&mut disp);
        assert!(!disp.is_tracking_refire(0xA975)); // TickCount
        assert!(!disp.is_tracking_refire(0xA9BD)); // GetNewWindow
        assert!(!disp.is_tracking_refire(0xA9C9)); // SysError
        assert!(!disp.is_tracking_refire(0xA89F)); // Random unrelated trap
                                                   // Cross-trap negative cases: dialog refire word with only menu
                                                   // tracking, and vice versa.
        let mut menu_only = TrapDispatcher::new();
        install_menu_tracking(&mut menu_only);
        assert!(!menu_only.is_tracking_refire(0xA991));
        assert!(!menu_only.is_tracking_refire(0xA985));
        let mut dialog_only = TrapDispatcher::new();
        install_dialog_tracking(&mut dialog_only);
        assert!(!dialog_only.is_tracking_refire(0xA93D));
        assert!(!dialog_only.is_tracking_refire(0xA80B));
    }

    /// Pin the system-STR synthesizer table. Adding or removing a known
    /// ID is a deliberate change that must update this test — the
    /// table is the source of truth for which `'STR '` resources
    /// systemless synthesizes when no loaded fork provides them, and
    /// silently dropping a row would regress games that depend on it
    /// (see Meteor Storm's owner-name probe in commit 62da1616 and the
    /// meteor_storm_launch_chain memory note).
    #[test]
    fn system_str_default_body_pins_known_ids() {
        // Owner Name (Sharing Setup) — Networking 1994, 2-799.
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16096),
            Some(&b"\x0EMacintosh User\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"[..])
        );
        // Macintosh Name (Sharing Setup, AppleTalk identity).
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16413),
            Some(&b"\x09Macintosh\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"[..])
        );
        // Owner Password (encrypted blob — empty Pascal string).
        assert_eq!(
            TrapDispatcher::system_str_default_body(-16097),
            Some(&b"\x00"[..])
        );

        // Pascal-string contract: every body must contain the complete
        // string. Sharing Setup stores the visible names in fixed 32-byte
        // System-file resources, so their trailing bytes are significant to
        // applications that copy the whole resource handle.
        for &id in &[-16096i16, -16097, -16413] {
            let body = TrapDispatcher::system_str_default_body(id).expect("known id");
            assert!(!body.is_empty(), "id={} body must be non-empty", id);
            let len = body[0] as usize;
            assert!(
                len + 1 <= body.len(),
                "id={} length byte ({}) exceeds body length ({})",
                id,
                len,
                body.len()
            );
        }
        assert_eq!(TrapDispatcher::system_str_default_body(-16096).unwrap().len(), 32);
        assert_eq!(TrapDispatcher::system_str_default_body(-16413).unwrap().len(), 32);

        // Negative space: anything outside the table returns None so
        // unrelated GetResource('STR ', N) probes still observe the
        // documented resNotFound behaviour.
        for &id in &[
            0i16, 1, 100, -1, -100, -16095, -16098, -16412, -16414, 16096,
        ] {
            assert!(
                TrapDispatcher::system_str_default_body(id).is_none(),
                "id={} must NOT be in the synthesizer table",
                id
            );
        }
    }

    #[test]
    fn active_modal_dialog_is_visible_to_frontends_before_snapshot_retention() {
        let mut disp = TrapDispatcher::new();
        let bus = MacMemoryBus::new(4 * 1024 * 1024);
        let bounds = (93, 236, 225, 564);
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr: 0x0010_0000,
            bounds,
            proc_id: 1,
            ..DialogTrackingState::default()
        });

        assert_eq!(disp.visible_dialog_bounds(), Some(bounds));
        assert_eq!(disp.visible_dialog_structure_bounds(&bus), Some(bounds));
    }

    #[test]
    fn app_managed_front_dialog_is_visible_without_modal_tracking_or_snapshot() {
        let mut disp = TrapDispatcher::new();
        let mut bus = MacMemoryBus::new(4 * 1024 * 1024);
        let dialog_ptr = 0x0010_0000;
        let bounds = (93, 236, 225, 564);
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.window_proc_ids.insert(dialog_ptr, 1);
        bus.write_byte(dialog_ptr + 110, 1);
        bus.write_long(dialog_ptr + 2, 0x0010_1000);
        bus.write_word(dialog_ptr + 6, 0);
        bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
        bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, (bounds.2 - bounds.0) as u16);
        bus.write_word(dialog_ptr + 22, (bounds.3 - bounds.1) as u16);

        assert_eq!(disp.visible_dialog_bounds(), Some(bounds));
        assert_eq!(
            disp.visible_dialog_structure_bounds(&bus),
            Some(bounds),
            "synthetic records without Window Manager regions fall back to content bounds"
        );
    }

    #[test]
    fn attached_event_queue_remains_shared_through_panic() {
        let mut dispatcher = TrapDispatcher::new();
        let mut context = ProcessContext::default();
        dispatcher.attach_unconverted_process_services(&mut context);
        context.shared_event_queue().push_back(QueuedEvent {
            what: 1,
            message: 0x1111,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.event_queue.push_back(QueuedEvent {
                what: 2,
                message: 0x2222,
                when: 0,
                where_v: 30,
                where_h: 40,
                modifiers: 0,
            });
            dispatcher.event_queue.invalidate_menu_bar();
            panic!("simulated panic inside guest execution");
        }));

        assert!(panic_result.is_err());
        assert_eq!(context.event_queue().len(), 2);
        assert_eq!(context.event_queue().get(0).unwrap().message, 0x1111);
        assert_eq!(context.event_queue().get(1).unwrap().message, 0x2222);
        assert!(context.event_queue().menu_bar_is_invalid());
        assert_eq!(dispatcher.event_queue.len(), 2);
        assert!(dispatcher.event_queue.menu_bar_is_invalid());
    }

    #[test]
    fn migrated_constructor_uses_exact_process_tick_and_execution_owners() {
        let mut context = ProcessContext::default();
        let handles = context.migrated_handles();
        let expected_ticks = handles.ticks.shared_handle();
        let expected_execution = handles.execution.shared_handle();

        let mut dispatcher = TrapDispatcher::new_with_migrated_handles(handles);

        assert!(dispatcher.tick_state.ptr_eq(&expected_ticks));
        assert!(dispatcher.guest_calls.ptr_eq(&expected_execution));
        assert!(dispatcher.menu_tracking.is_view_of(&dispatcher.guest_calls));

        dispatcher.attach_unconverted_process_services(&mut context);
        assert!(dispatcher.tick_state.ptr_eq(&expected_ticks));
        assert!(dispatcher.guest_calls.ptr_eq(&expected_execution));
        assert!(dispatcher.menu_tracking.is_view_of(&dispatcher.guest_calls));
    }

    #[test]
    fn standalone_constructors_create_independent_coherent_service_owners() {
        let first = TrapDispatcher::new();
        let second = TrapDispatcher::new();

        assert!(!first.tick_state.ptr_eq(&second.tick_state));
        assert!(!first.guest_calls.ptr_eq(&second.guest_calls));
        assert!(first.menu_tracking.is_view_of(&first.guest_calls));
        assert!(second.menu_tracking.is_view_of(&second.guest_calls));
    }

    #[test]
    fn fresh_dispatcher_uses_filesystem_resource_manager_owner() {
        let mut dispatcher = TrapDispatcher::new();
        let owner: &ProcessResourceManagerState = &dispatcher.process_file_system.resource_manager;

        assert!(std::ptr::eq(&*dispatcher, owner));

        let key = (7, *b"TEST", 128);
        dispatcher.insert_resource_backing_data_for_test(key, b"fresh".to_vec());
        assert_eq!(
            dispatcher
                .process_file_system
                .resource_manager
                .resource_backing_data
                .get(&key),
            Some(&b"fresh".to_vec())
        );
    }

    #[test]
    fn attached_dispatchers_share_filesystem_resource_manager_owner() {
        let mut context = ProcessContext::default();
        let mut first = TrapDispatcher::new();
        let mut second = TrapDispatcher::new();

        first.attach_unconverted_process_services(&mut context);
        second.attach_unconverted_process_services(&mut context);

        assert!(first
            .process_file_system
            .resource_manager
            .ptr_eq(&second.process_file_system.resource_manager));
        assert!(std::ptr::eq(
            &*first,
            &*first.process_file_system.resource_manager
        ));
        assert!(std::ptr::eq(
            &*second,
            &*second.process_file_system.resource_manager
        ));

        let key = (7, *b"TEST", 128);
        first.insert_resource_backing_data_for_test(key, b"attached".to_vec());
        assert_eq!(
            second.resource_backing_data.get(&key),
            Some(&b"attached".to_vec())
        );
    }

    #[test]
    fn detached_filesystem_resource_manager_clone_is_independent() {
        let mut dispatcher = TrapDispatcher::new();
        let key = (7, *b"TEST", 128);
        dispatcher.insert_resource_backing_data_for_test(key, b"original".to_vec());

        let detached = dispatcher.process_file_system.clone();
        assert!(!dispatcher.process_file_system.ptr_eq(&detached));
        assert_eq!(
            detached.resource_backing_data.get(&key),
            Some(&b"original".to_vec())
        );

        detached.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_backing_data
                .get_mut(&key)
                .expect("cloned resource backing data")
                .extend_from_slice(b"-detached");
        });
        assert_eq!(
            dispatcher.resource_backing_data.get(&key),
            Some(&b"original".to_vec())
        );
        assert_eq!(
            detached.resource_backing_data.get(&key),
            Some(&b"original-detached".to_vec())
        );
    }

    #[test]
    fn attached_dispatchers_share_file_completion_queue_immediately() {
        let completion = PendingFileCompletion {
            parameter_block: 0x1000,
            completion_addr: 0x2000,
            result: -39,
        };
        let mut context = ProcessContext::default();
        let mut first = TrapDispatcher::new();
        let mut second = TrapDispatcher::new();

        first.attach_unconverted_process_services(&mut context);
        second.attach_unconverted_process_services(&mut context);
        assert!(first
            .pending_file_completions
            .ptr_eq(&second.pending_file_completions));

        first.pending_file_completions.push_back(completion);
        assert_eq!(
            second.pending_file_completions.pop_front(),
            Some(completion)
        );
        assert!(first.pending_file_completions.is_empty());
    }

    #[test]
    fn attached_menu_tracking_mutates_process_context_immediately() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x1234,
        )));
        let mut dispatcher = TrapDispatcher::new_with_migrated_handles(context.migrated_handles());
        dispatcher.attach_unconverted_process_services(&mut context);

        assert_eq!(
            dispatcher.menu_tracking.as_ref().map(|t| t.menu_handle),
            Some(0x1234)
        );
        dispatcher
            .menu_tracking
            .with_tracking_mut(|tracking| tracking.highlighted_item = 5)
            .unwrap();

        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x1234, 5))
        );
        assert_eq!(
            dispatcher.menu_tracking.as_ref().unwrap().highlighted_item,
            5
        );
    }

    #[test]
    fn attached_menu_tracking_remains_shared_through_panic() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x5678,
        )));
        let mut dispatcher = TrapDispatcher::new_with_migrated_handles(context.migrated_handles());
        dispatcher.attach_unconverted_process_services(&mut context);

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher
                .menu_tracking
                .with_tracking_mut(|tracking| tracking.highlighted_item = 9)
                .unwrap();
            panic!("simulated panic inside menu trap execution");
        }));

        assert!(panic_result.is_err());
        assert_eq!(
            context
                .menu_tracking()
                .map(|t| (t.menu_handle, t.highlighted_item)),
            Some((0x5678, 9))
        );
        assert_eq!(
            dispatcher.menu_tracking.as_ref().unwrap().highlighted_item,
            9
        );
    }

    #[test]
    fn attached_process_state_remains_canonical_through_panic() {
        let mut context = ProcessContext::default();
        context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
            0x9abc,
        )));
        let mut dispatcher = TrapDispatcher::new_with_migrated_handles(context.migrated_handles());
        dispatcher.attach_unconverted_process_services(&mut context);
        let memory_manager = context.memory_manager_handle().clone();

        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.with_process_state(|disp| {
                disp.event_queue.push_back(QueuedEvent {
                    what: 1,
                    message: 0x3333,
                    when: 0,
                    where_v: 1,
                    where_h: 2,
                    modifiers: 0,
                });
                disp.menu_tracking
                    .with_tracking_mut(|tracking| tracking.highlighted_item = 7)
                    .unwrap();
                disp.track_handle_ptr(0x4444, 0x5555);
                disp.set_handle_state_bits(0x5555, 0xc0);
                panic!("simulated panic inside a complete guest execution slice");
            });
        }));

        assert!(panic_result.is_err());
        assert_eq!(
            context.event_queue().front().map(|event| event.message),
            Some(0x3333)
        );
        assert_eq!(memory_manager.borrow().handle_for_ptr(0x4444), Some(0x5555));
        assert_eq!(memory_manager.borrow().handle_state(0x5555), 0xc0);
        assert_eq!(dispatcher.handle_for_ptr(0x4444), Some(0x5555));
        assert_eq!(dispatcher.handle_state_bits(0x5555), Some(0xc0));
        memory_manager.borrow_mut().track_handle_ptr(0x6666, 0x7777);
        assert_eq!(dispatcher.handle_for_ptr(0x6666), Some(0x7777));
        assert_eq!(
            context
                .menu_tracking()
                .map(|tracking| (tracking.menu_handle, tracking.highlighted_item)),
            Some((0x9abc, 7))
        );
        assert_eq!(dispatcher.event_queue.len(), 1);
        assert_eq!(
            dispatcher.menu_tracking.as_ref().unwrap().highlighted_item,
            7
        );
        assert!(dispatcher
            .process_memory_manager
            .as_ref()
            .is_some_and(|attached| attached.ptr_eq(&memory_manager)));
    }

    #[test]
    fn detached_dispatchers_keep_independent_memory_manager_metadata() {
        let mut first = TrapDispatcher::new();
        let mut second = TrapDispatcher::new();
        let mut first_context = ProcessContext::default();
        let mut second_context = ProcessContext::default();
        first.attach_unconverted_process_services(&mut first_context);
        second.attach_unconverted_process_services(&mut second_context);

        first.track_handle_ptr(0x2200, 0x1100);
        first.set_handle_state_bits(0x1100, 0x80);

        assert_eq!(second.handle_for_ptr(0x2200), None);
        assert_eq!(second.handle_state_bits(0x1100), None);
        assert!(!first
            .process_memory_manager
            .as_ref()
            .unwrap()
            .ptr_eq(second.process_memory_manager.as_ref().unwrap()));
    }

    #[test]
    fn attaching_dispatcher_moves_standalone_metadata_into_process_owner() {
        let mut dispatcher = TrapDispatcher::new();
        let standalone = dispatcher.process_memory_manager();
        dispatcher.track_handle_ptr(0x2200, 0x1100);
        dispatcher.set_handle_state_bits(0x1100, 0x80);

        let mut context = ProcessContext::default();
        dispatcher.attach_unconverted_process_services(&mut context);
        let attached = dispatcher.process_memory_manager();

        assert!(!attached.ptr_eq(&standalone));
        assert_eq!(attached.borrow().handle_for_ptr(0x2200), Some(0x1100));
        assert_eq!(attached.borrow().state_for_handle(0x1100), Some(0x80));
        assert_eq!(standalone.borrow().handle_for_ptr(0x2200), None);
        assert_eq!(standalone.borrow().state_for_handle(0x1100), None);
    }

    #[test]
    fn attaching_dispatcher_transfers_a_standalone_native_allocator() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let mut dispatcher = TrapDispatcher::new();
        let standalone = dispatcher.process_memory_manager();
        standalone.borrow_mut().publish_native_allocator(
            crate::process_context::ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[crate::process_context::ProcessPtrRecord {
                ptr: HEAP_BASE,
                size: 24,
            }],
            &[],
            &[],
        );

        let mut context = ProcessContext::default();
        dispatcher.attach_unconverted_process_services(&mut context);
        let attached = dispatcher.process_memory_manager();

        assert!(!attached.ptr_eq(&standalone));
        assert!(!standalone.borrow().has_native_allocator());
        assert_eq!(
            attached.borrow().native_ptr_records(),
            [crate::process_context::ProcessPtrRecord {
                ptr: HEAP_BASE,
                size: 24,
            }]
        );
    }

    #[test]
    fn attaching_dispatcher_transfers_its_standalone_classic_allocator() {
        let mut dispatcher = TrapDispatcher::new();
        let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
        let ptr = dispatcher.new_process_classic_ptr(&mut bus, 24);
        assert_ne!(ptr, 0);

        let mut context = ProcessContext::default();
        dispatcher.attach_unconverted_process_services(&mut context);
        context.attach_classic_memory_bus(&mut bus);

        assert_eq!(
            context.memory_manager_mut().process_ptr_size(&bus, ptr),
            Some(24)
        );
        dispatcher.dispose_process_ptr(&mut bus, ptr);
        let reused = context.memory_manager_mut().new_classic_ptr(&mut bus, 16);
        assert_eq!(reused, ptr);
    }

    #[test]
    fn attached_dispatcher_relocates_native_handle_without_slice_pointer() {
        const HEAP_BASE: u32 = 0x0300_0000;
        let handle = HEAP_BASE;
        let old_ptr = HEAP_BASE + 0x10;
        let heap_cursor = HEAP_BASE + 0x40;
        let mut native = crate::memory::GuestAddressSpace::new();
        native.add_region(HEAP_BASE, vec![0; 0x1000]);
        ppc::PpcMemory::write_u32_be(&mut native, handle, old_ptr).unwrap();

        let mut context = ProcessContext::default();
        {
            let mut manager = context.memory_manager_mut();
            manager.publish_native_allocator(
                crate::process_context::ProcessNativeHeapState {
                    heap_base: HEAP_BASE,
                    heap_cursor,
                    heap_limit: HEAP_BASE + 0x1000,
                    last_mem_error: 0,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
            manager.register_native_handle_records([(
                crate::process_context::ProcessHandleRecord {
                    handle,
                    ptr: old_ptr,
                    size: 8,
                    capacity: 16,
                },
                0,
            )]);
        }

        let mut bus = MacMemoryBus::new(0x2000);
        let shared = native.shared_view();
        bus.attach_guest_address_space(shared);
        let mut dispatcher = TrapDispatcher::new();
        dispatcher.attach_unconverted_process_services(&mut context);
        let replacement = vec![0x5a; 48];

        assert!(dispatcher.replace_process_native_handle_bytes(
            &mut bus,
            handle,
            old_ptr,
            &replacement,
        ));
        assert_eq!(bus.read_long(handle), heap_cursor);
        assert_eq!(bus.read_bytes(heap_cursor, replacement.len()), replacement);
        assert_eq!(dispatcher.handle_for_ptr(heap_cursor), Some(handle));
        assert_eq!(dispatcher.handle_for_ptr(old_ptr), None);
        assert_eq!(
            context.memory_manager_mut().native_allocation(handle),
            Some(crate::process_context::ProcessHandleRecord {
                handle,
                ptr: heap_cursor,
                size: 48,
                capacity: 48,
            })
        );
    }
