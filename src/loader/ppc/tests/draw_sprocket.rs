use super::*;

    #[test]
    fn hle_import_runner_handles_draw_sprocket_find_best_context() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let context_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(context_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            loaded.memory.read_u32_be(context_out_ptr),
            Some(PPC_DSP_CONTEXT)
        );
        assert!(loaded.draw_sprocket.started);
    }

    #[test]
    fn hle_import_runner_records_draw_sprocket_find_best_context_attributes() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(context_out_ptr, vec![0; 4]);
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                frequency: 75 << 16,
                width: PPC_DSP_SCREEN_WIDTH,
                height: PPC_DSP_SCREEN_HEIGHT,
                context_options: 0x40,
                display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                back_buffer_best_depth_mask: 0,
                display_depth: PPC_MAIN_PIXEL_DEPTH,
                back_buffer_depth: 0,
                page_count: 2,
            },
        );
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(context_out_ptr),
            Some(PPC_DSP_CONTEXT)
        );
        assert_eq!(
            loaded.draw_sprocket.context_attributes,
            PpcDspContextAttributes {
                frequency: PPC_DSP_FREQUENCY_60HZ,
                width: PPC_DSP_SCREEN_WIDTH,
                height: PPC_DSP_SCREEN_HEIGHT,
                context_options: PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL,
                display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                back_buffer_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                display_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
                back_buffer_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
                page_count: PPC_DSP_ADVERTISED_PAGE_COUNT,
            }
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_find_best_context_rejects_unsupported_mode() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(context_out_ptr, vec![0xaa; 4]);
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                width: 800,
                height: 600,
                display_best_depth_mask: 1 << 5,
                display_depth: 32,
                ..PpcDspContextAttributes::default()
            },
        );
        let initial_draw_sprocket = loaded.draw_sprocket;
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_FOUND_ERR)
        );
        assert_eq!(loaded.draw_sprocket, initial_draw_sprocket);
        for offset in 0..4 {
            assert_eq!(loaded.memory.read_u8(context_out_ptr + offset), Some(0xaa));
        }
    }

    #[test]
    fn hle_import_runner_traces_draw_sprocket_requested_context_attributes() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        let requested = PpcDspContextAttributes {
            frequency: 75 << 16,
            width: 800,
            height: 600,
            context_options: 0x40,
            display_best_depth_mask: 1 << 5,
            back_buffer_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
            display_depth: 32,
            back_buffer_depth: PPC_MAIN_PIXEL_DEPTH,
            page_count: 3,
        };
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(context_out_ptr, vec![0xaa; 4]);
        write_test_dsp_context_attributes(&mut loaded.memory, attributes_ptr, requested);
        let initial_draw_sprocket = loaded.draw_sprocket;
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_import_trace(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_FOUND_ERR)
        );
        assert_eq!(loaded.draw_sprocket, initial_draw_sprocket);
        assert_eq!(probe.draw_sprocket_trace.len(), 1);
        let trace = &probe.draw_sprocket_trace[0];
        assert_eq!(trace.action, "find_best_context");
        assert_eq!(trace.result, PPC_DSP_CONTEXT_NOT_FOUND_ERR);
        assert_eq!(trace.context, None);
        assert_eq!(trace.requested_frequency, Some(requested.frequency));
        assert_eq!(trace.requested_width, Some(requested.width));
        assert_eq!(trace.requested_height, Some(requested.height));
        assert_eq!(
            trace.requested_context_options,
            Some(requested.context_options)
        );
        assert_eq!(
            trace.requested_display_depth_mask,
            Some(requested.display_best_depth_mask)
        );
        assert_eq!(
            trace.requested_back_buffer_depth_mask,
            Some(requested.back_buffer_best_depth_mask)
        );
        assert_eq!(trace.requested_display_depth, Some(requested.display_depth));
        assert_eq!(
            trace.requested_back_buffer_depth,
            Some(requested.back_buffer_depth)
        );
        assert_eq!(trace.requested_page_count, Some(requested.page_count));
        assert_eq!(trace.width, PPC_DSP_SCREEN_WIDTH);
        assert_eq!(trace.height, PPC_DSP_SCREEN_HEIGHT);
        assert_eq!(trace.display_depth, PPC_MAIN_SCREEN_STORAGE_DEPTH);
        for offset in 0..4 {
            assert_eq!(loaded.memory.read_u8(context_out_ptr + offset), Some(0xaa));
        }
    }

    #[test]
    fn hle_import_runner_draw_sprocket_find_best_context_rejects_depth_requests_excluding_16bpp() {
        for (case, attributes) in [
            (
                "display mask excludes 16-bit",
                PpcDspContextAttributes {
                    display_best_depth_mask: 1 << 5,
                    display_depth: 0,
                    ..PpcDspContextAttributes::default()
                },
            ),
            (
                "display best depth excludes 16-bit",
                PpcDspContextAttributes {
                    display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                    display_depth: 32,
                    ..PpcDspContextAttributes::default()
                },
            ),
            (
                "back-buffer mask excludes 16-bit",
                PpcDspContextAttributes {
                    back_buffer_best_depth_mask: 1 << 5,
                    back_buffer_depth: 0,
                    ..PpcDspContextAttributes::default()
                },
            ),
            (
                "back-buffer best depth excludes 16-bit",
                PpcDspContextAttributes {
                    back_buffer_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                    back_buffer_depth: 32,
                    ..PpcDspContextAttributes::default()
                },
            ),
            (
                "too many pages requested",
                PpcDspContextAttributes {
                    page_count: PPC_DSP_MAX_REQUESTED_PAGE_COUNT + 1,
                    ..PpcDspContextAttributes::default()
                },
            ),
        ] {
            let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
            let mut loaded = load_pef_application(&pef).unwrap();
            let attributes_ptr = PPC_DATA_BASE + 0x1000;
            let context_out_ptr = PPC_DATA_BASE + 0x1100;
            loaded.memory.add_region(
                attributes_ptr,
                vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
            );
            loaded.memory.add_region(context_out_ptr, vec![0xcc; 4]);
            write_test_dsp_context_attributes(&mut loaded.memory, attributes_ptr, attributes);
            let initial_draw_sprocket = loaded.draw_sprocket;
            loaded.cpu.gpr[3] = attributes_ptr;
            loaded.cpu.gpr[4] = context_out_ptr;

            let probe = loaded.run_with_hle_imports(64);

            assert_eq!(probe.handled_import_count, 1, "{case}");
            assert_eq!(probe.unsupported_import_index, None, "{case}");
            assert_eq!(
                loaded.cpu.gpr[3],
                ppc_i16_result(PPC_DSP_CONTEXT_NOT_FOUND_ERR),
                "{case}"
            );
            assert_eq!(loaded.draw_sprocket, initial_draw_sprocket, "{case}");
            for offset in 0..4 {
                assert_eq!(
                    loaded.memory.read_u8(context_out_ptr + offset),
                    Some(0xcc),
                    "{case}"
                );
            }
        }
    }

    #[test]
    fn hle_import_runner_draw_sprocket_accepts_1bit_back_buffer_preference() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(context_out_ptr, vec![0; 4]);
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                display_depth: PPC_MAIN_PIXEL_DEPTH,
                back_buffer_best_depth_mask: 1,
                back_buffer_depth: 1,
                ..PpcDspContextAttributes::default()
            },
        );
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(context_out_ptr),
            Some(PPC_DSP_CONTEXT)
        );
        assert_eq!(
            loaded.draw_sprocket.context_attributes,
            PpcDspContextAttributes::default()
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_find_best_context_param_err_preserves_state() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindBestContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                width: 800,
                height: 600,
                ..PpcDspContextAttributes::default()
            },
        );
        let initial_draw_sprocket = loaded.draw_sprocket;
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.draw_sprocket, initial_draw_sprocket);
        assert_eq!(loaded.memory.read_u8(context_out_ptr), None);
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_can_user_select_context_signature() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpCanUserSelectContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let can_select_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(can_select_out_ptr, vec![0xff; 4]);
        let requested = PpcDspContextAttributes {
            width: 800,
            height: 600,
            ..PpcDspContextAttributes::default()
        };
        write_test_dsp_context_attributes(&mut loaded.memory, attributes_ptr, requested);
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = can_select_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(can_select_out_ptr), Some(0));
        assert_eq!(loaded.memory.read_u8(can_select_out_ptr + 1), Some(0xff));
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 4),
            Some(requested.width)
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_get_first_context_accepts_default_display() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpGetFirstContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let context_out = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(context_out, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = context_out;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(context_out),
            Some(PPC_DSP_CONTEXT)
        );
    }

    #[test]
    fn hle_import_runner_traces_draw_sprocket_can_user_select_output() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpCanUserSelectContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let can_select_out_ptr = PPC_DATA_BASE + 0x1100;
        let requested = PpcDspContextAttributes {
            width: PPC_DSP_SCREEN_WIDTH,
            height: PPC_DSP_SCREEN_HEIGHT,
            ..PpcDspContextAttributes::default()
        };
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(can_select_out_ptr, vec![0xff; 1]);
        write_test_dsp_context_attributes(&mut loaded.memory, attributes_ptr, requested);
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = can_select_out_ptr;

        let probe = loaded.run_with_hle_import_trace(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(can_select_out_ptr), Some(0));
        assert_eq!(probe.draw_sprocket_trace.len(), 1);
        let trace = &probe.draw_sprocket_trace[0];
        assert_eq!(trace.action, "can_user_select_context");
        assert_eq!(trace.result, PPC_NO_ERR);
        assert_eq!(trace.requested_width, Some(requested.width));
        assert_eq!(trace.requested_height, Some(requested.height));
        assert_eq!(trace.can_user_select, Some(false));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_can_user_select_output_is_prevalidated() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpCanUserSelectContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let can_select_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                width: 800,
                height: 600,
                ..PpcDspContextAttributes::default()
            },
        );
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = can_select_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(can_select_out_ptr), None);
        assert_eq!(loaded.memory.read_u32_be(attributes_ptr + 4), Some(800));
        assert_eq!(loaded.memory.read_u32_be(attributes_ptr + 8), Some(600));
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_user_select_context_signature() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpUserSelectContext");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(context_out_ptr, vec![0; 4]);
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                width: PPC_DSP_SCREEN_WIDTH,
                height: PPC_DSP_SCREEN_HEIGHT,
                ..PpcDspContextAttributes::default()
            },
        );
        loaded.cpu.gpr[3] = attributes_ptr;
        loaded.cpu.gpr[4] = 42;
        loaded.cpu.gpr[5] = 0x1234_5678;
        loaded.cpu.gpr[6] = context_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(context_out_ptr),
            Some(PPC_DSP_CONTEXT)
        );
        assert!(loaded.draw_sprocket.started);
        assert_eq!(
            loaded.draw_sprocket.context_attributes.width,
            PPC_DSP_SCREEN_WIDTH
        );
        assert_eq!(
            loaded.draw_sprocket.context_attributes.height,
            PPC_DSP_SCREEN_HEIGHT
        );
        assert_eq!(loaded.draw_sprocket.last_user_select_display_id, Some(42));
        assert_eq!(
            loaded.draw_sprocket.last_user_select_event_proc,
            Some(0x1234_5678)
        );
        assert_eq!(loaded.draw_sprocket.user_select_count, 1);
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_set_blanking_color() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpSetBlankingColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let color_ptr = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(color_ptr, vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
        loaded.cpu.gpr[3] = color_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.draw_sprocket.blanking_color,
            PpcRgbColor {
                red: 0x1234,
                green: 0x5678,
                blue: 0x9abc,
            }
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_get_mouse_writes_global_point() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpGetMouse");
        let mut loaded = load_pef_application(&pef).unwrap();
        let point_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = point_ptr;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_v: -12,
            mouse_h: 345,
            ..PpcInputSnapshot::default()
        });

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(point_ptr), Some((-12_i16) as u16));
        assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(345));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_get_mouse_rejects_null() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpGetMouse");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_finds_context_from_packed_point() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindContextFromPoint");
        let mut loaded = load_pef_application(&pef).unwrap();
        let context_out = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(context_out, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = (240_u32 << 16) | 320;
        loaded.cpu.gpr[4] = context_out;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(context_out),
            Some(PPC_DSP_CONTEXT)
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_rejects_point_outside_context() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpFindContextFromPoint");
        let mut loaded = load_pef_application(&pef).unwrap();
        let context_out = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(context_out, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = ((-1_i16 as u16 as u32) << 16) | 20;
        loaded.cpu.gpr[4] = context_out;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_FOUND_ERR)
        );
        assert_eq!(loaded.memory.read_u32_be(context_out), Some(0xaaaa_aaaa));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_global_to_local_preserves_main_display_point() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GlobalToLocal");
        let mut loaded = load_pef_application(&pef).unwrap();
        let point = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(point, vec![0x00, 0xf0, 0x01, 0x40]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = point;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(point), Some(0x00f0_0140));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_set_blanking_color_rejects_null() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpSetBlankingColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let original = PpcRgbColor {
            red: 0x1111,
            green: 0x2222,
            blue: 0x3333,
        };
        loaded.draw_sprocket.blanking_color = original;
        loaded.cpu.gpr[3] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.draw_sprocket.blanking_color, original);
    }

    #[test]
    fn hle_import_runner_draw_sprocket_set_blanking_color_rejects_truncated_input() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpSetBlankingColor");
        let mut loaded = load_pef_application(&pef).unwrap();
        let color_ptr = PPC_DATA_BASE + 0x1000;
        let original = PpcRgbColor {
            red: 0x1111,
            green: 0x2222,
            blue: 0x3333,
        };
        loaded.draw_sprocket.blanking_color = original;
        loaded
            .memory
            .add_region(color_ptr, vec![0x12, 0x34, 0x56, 0x78]);
        loaded.cpu.gpr[3] = color_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.draw_sprocket.blanking_color, original);
    }

    #[test]
    fn hle_import_runner_creates_draw_sprocket_alt_buffer_with_requested_size() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpAltBuffer_New");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let out_alt_buffer_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(attributes_ptr, vec![0; 28]);
        loaded.memory.add_region(out_alt_buffer_ptr, vec![0; 4]);
        loaded.memory.write_u32_be(attributes_ptr, 32).unwrap();
        loaded.memory.write_u32_be(attributes_ptr + 4, 24).unwrap();
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = attributes_ptr;
        loaded.cpu.gpr[6] = out_alt_buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let alt_buffer = loaded.memory.read_u32_be(out_alt_buffer_ptr).unwrap();
        let world = loaded
            .gworlds
            .iter()
            .find(|world| world.port == alt_buffer)
            .unwrap();
        assert_eq!((world.width, world.height), (32, 24));
        assert_eq!(
            world.depth,
            loaded.draw_sprocket.context_attributes.display_depth
        );
    }

    #[test]
    fn hle_import_runner_gets_draw_sprocket_alt_buffer_port_and_device() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpAltBuffer_GetCGrafPtr");
        let mut loaded = load_pef_application(&pef).unwrap();
        let port_out_ptr = PPC_DATA_BASE + 0x1000;
        let device_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(port_out_ptr, vec![0; 4]);
        loaded.memory.add_region(device_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = PPC_DSP_BACK_GWORLD;
        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL;
        loaded.cpu.gpr[5] = port_out_ptr;
        loaded.cpu.gpr[6] = device_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(port_out_ptr),
            Some(PPC_DSP_BACK_GWORLD)
        );
        assert_eq!(
            loaded.memory.read_u32_be(device_out_ptr),
            Some(PPC_MAIN_GDEVICE)
        );
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_startup() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpStartup");
        let mut loaded = load_pef_application(&pef).unwrap();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert!(loaded.draw_sprocket.started);
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_shutdown() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpShutdown");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket = PpcDrawSprocketState {
            started: true,
            blanking_color: PPC_RGB_WHITE,
            reserved_context: Some(PPC_DSP_CONTEXT),
            active_context: Some(PPC_DSP_CONTEXT),
            context_state: PpcDspContextPlayState::Active,
            context_attributes: PpcDspContextAttributes {
                width: 800,
                height: 600,
                ..PpcDspContextAttributes::default()
            },
            front_buffer_gworld: PPC_DSP_BACK_GWORLD,
            back_buffer_gworld: PPC_MAIN_GWORLD,
            last_fade_context: Some(PPC_DSP_CONTEXT),
            last_fade_kind: Some(PpcDspGammaFadeKind::Out),
            last_fade_percent: Some(0),
            last_fade_zero_color: Some(PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            }),
            fade_count: 2,
            last_user_select_display_id: Some(PPC_DSP_DISPLAY_ID),
            last_user_select_event_proc: Some(0x1234_5678),
            user_select_count: 1,
            last_swap_context: Some(PPC_DSP_CONTEXT),
            swap_count: 3,
            ..PpcDrawSprocketState::default()
        };

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket, PpcDrawSprocketState::default());
    }

    #[test]
    fn hle_import_runner_tracks_draw_sprocket_reserve_set_state_and_release() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Reserve");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                frequency: 0,
                width: 1024,
                height: 768,
                context_options: 0x40,
                display_best_depth_mask: 0,
                back_buffer_best_depth_mask: 0,
                display_depth: 0,
                back_buffer_depth: 0,
                page_count: 4,
            },
        );
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = attributes_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert!(loaded.draw_sprocket.started);
        assert_eq!(loaded.draw_sprocket.reserved_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );
        assert_eq!(
            loaded.draw_sprocket.context_attributes,
            PpcDspContextAttributes {
                frequency: PPC_DSP_FREQUENCY_60HZ,
                width: PPC_DSP_SCREEN_WIDTH,
                height: PPC_DSP_SCREEN_HEIGHT,
                context_options: PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL,
                display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                back_buffer_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
                display_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
                back_buffer_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
                page_count: PPC_DSP_ADVERTISED_PAGE_COUNT,
            }
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SetState");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_ACTIVE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.active_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Active
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_INACTIVE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Release");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.reserved_context, None);
        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_reports_reserved_lifecycle_errors() {
        let already_reserved = ppc_i16_result(PPC_DSP_CONTEXT_ALREADY_RESERVED_ERR);
        let not_reserved = ppc_i16_result(PPC_DSP_CONTEXT_NOT_RESERVED_ERR);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Reserve");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.context_state = PpcDspContextPlayState::Inactive;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], already_reserved);
        assert_eq!(loaded.draw_sprocket.reserved_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Release");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], not_reserved);
        assert_eq!(loaded.draw_sprocket.reserved_context, None);
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SetState");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_ACTIVE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], not_reserved);
        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Inactive
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_tracks_paused_state() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SetState");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.context_state = PpcDspContextPlayState::Active;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_PAUSED;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(
            loaded.draw_sprocket.context_state,
            PpcDspContextPlayState::Paused
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_context_apis_return_not_found_for_invalid_handles() {
        let invalid_context = PPC_DSP_CONTEXT + 4;
        let expected_error = ppc_i16_result(PPC_DSP_CONTEXT_NOT_FOUND_ERR);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Reserve");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = PPC_DATA_BASE + 0x1000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.draw_sprocket.reserved_context, None);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Release");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.cpu.gpr[3] = invalid_context;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.draw_sprocket.reserved_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(loaded.draw_sprocket.active_context, Some(PPC_DSP_CONTEXT));

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SetState");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_ACTIVE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.draw_sprocket.active_context, None);

        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetFrontBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let front_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(front_buffer_out_ptr, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = front_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_ppc_bytes_equal(&mut loaded.memory, front_buffer_out_ptr, 4, 0xaa);

        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetBackBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let back_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(back_buffer_out_ptr, vec![0xbb; 4]);
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL;
        loaded.cpu.gpr[5] = back_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_ppc_bytes_equal(&mut loaded.memory, back_buffer_out_ptr, 4, 0xbb);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetDisplayID");
        let mut loaded = load_pef_application(&pef).unwrap();
        let display_id_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(display_id_out_ptr, vec![0xcc; 4]);
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = display_id_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_ppc_bytes_equal(&mut loaded.memory, display_id_out_ptr, 4, 0xcc);

        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetAttributes");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(
            attributes_out_ptr,
            vec![0xdd; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = attributes_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_ppc_bytes_equal(
            &mut loaded.memory,
            attributes_out_ptr,
            PPC_DSP_CONTEXT_ATTRIBUTES_SIZE,
            0xdd,
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGamma");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = invalid_context;
        loaded.cpu.gpr[4] = 37;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.draw_sprocket.last_fade_context, None);
        assert_eq!(loaded.draw_sprocket.fade_count, 0);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SwapBuffers");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.cpu.gpr[3] = invalid_context;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected_error);
        assert_eq!(loaded.draw_sprocket.active_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(loaded.draw_sprocket.last_swap_context, None);
        assert_eq!(loaded.draw_sprocket.swap_count, 0);
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_front_buffer_and_display_id() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetFrontBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let front_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(front_buffer_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = front_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(front_buffer_out_ptr),
            Some(PPC_MAIN_GWORLD)
        );

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetDisplayID");
        let mut loaded = load_pef_application(&pef).unwrap();
        let display_id_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(display_id_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = display_id_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(display_id_out_ptr),
            Some(PPC_DSP_DISPLAY_ID)
        );
    }

    #[test]
    fn hle_import_runner_draw_sprocket_buffer_outputs_are_all_or_nothing() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetFrontBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let front_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded
            .memory
            .add_region(front_buffer_out_ptr, vec![0xaa; 2]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = front_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(front_buffer_out_ptr), Some(0xaa));
        assert_eq!(loaded.memory.read_u8(front_buffer_out_ptr + 1), Some(0xaa));

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetDisplayID");
        let mut loaded = load_pef_application(&pef).unwrap();
        let display_id_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(display_id_out_ptr, vec![0xbb; 2]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = display_id_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(display_id_out_ptr), Some(0xbb));
        assert_eq!(loaded.memory.read_u8(display_id_out_ptr + 1), Some(0xbb));

        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetBackBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let back_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(back_buffer_out_ptr, vec![0xcc; 2]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL;
        loaded.cpu.gpr[5] = back_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(back_buffer_out_ptr), Some(0xcc));
        assert_eq!(loaded.memory.read_u8(back_buffer_out_ptr + 1), Some(0xcc));
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_back_buffer_as_distinct_page() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetBackBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let back_buffer_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(back_buffer_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL;
        loaded.cpu.gpr[5] = back_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(back_buffer_out_ptr),
            Some(PPC_DSP_BACK_GWORLD)
        );
        assert_ne!(loaded.draw_sprocket.back_buffer_gworld, PPC_MAIN_GWORLD);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL + 1;
        loaded.cpu.gpr[5] = back_buffer_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }

    #[test]
    fn draw_sprocket_one_page_context_draws_into_displayed_buffer() {
        let pef = synthetic_pef_with_import(b"SetPort");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let back_buffer_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.memory.add_region(back_buffer_out_ptr, vec![0; 4]);
        write_test_dsp_context_attributes(
            &mut loaded.memory,
            attributes_ptr,
            PpcDspContextAttributes {
                page_count: 1,
                ..PpcDspContextAttributes::default()
            },
        );
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = attributes_ptr;

        assert_eq!(
            ppc_dsp_context_reserve(
                &loaded.cpu,
                &mut loaded.memory,
                &mut loaded.draw_sprocket,
                &mut loaded.gworlds,
            ),
            PPC_NO_ERR
        );
        assert_eq!(loaded.draw_sprocket.context_attributes.page_count, 1);
        assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_MAIN_GWORLD);

        loaded.cpu.gpr[4] = PPC_DSP_BUFFER_KIND_NORMAL;
        loaded.cpu.gpr[5] = back_buffer_out_ptr;
        assert_eq!(
            ppc_dsp_context_get_back_buffer(&loaded.cpu, &mut loaded.memory, &loaded.draw_sprocket,),
            PPC_NO_ERR
        );
        assert_eq!(
            loaded.memory.read_u32_be(back_buffer_out_ptr),
            Some(PPC_MAIN_GWORLD)
        );
    }

    #[test]
    fn hle_import_runner_tracks_draw_sprocket_swap_buffers() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SwapBuffers");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded
            .memory
            .write_u16_be(PPC_DSP_BACK_SCREEN_BASE, 0x7c00)
            .unwrap();
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert!(loaded.draw_sprocket.started);
        assert_eq!(loaded.draw_sprocket.reserved_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(loaded.draw_sprocket.active_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(
            loaded.draw_sprocket.last_swap_context,
            Some(PPC_DSP_CONTEXT)
        );
        assert_eq!(loaded.draw_sprocket.swap_count, 1);
        assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
        assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
        let current_buffer = loaded.current_front_buffer().unwrap();
        assert_eq!(current_buffer.base_addr, PPC_MAIN_SCREEN_BASE);
        let front_buffer = loaded.presented_front_buffer().unwrap();
        assert_eq!(front_buffer.base_addr, PPC_MAIN_SCREEN_BASE);
        assert_eq!(
            loaded.memory.read_u16_be(front_buffer.base_addr),
            Some(0x7c00)
        );
    }

    #[test]
    fn presented_front_buffer_uses_main_screen_without_active_draw_sprocket() {
        let pef = synthetic_pef_with_import(b"SetPort");
        let loaded = load_pef_application(&pef).unwrap();
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_DSP_BACK_GWORLD);

        let front_buffer = loaded.presented_front_buffer().unwrap();

        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
        assert_eq!(front_buffer.base_addr, PPC_MAIN_SCREEN_BASE);
    }

    #[test]
    fn presented_front_buffer_uses_main_screen_before_first_draw_sprocket_swap() {
        let pef = synthetic_pef_with_import(b"SetPort");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = PPC_DSP_BACK_GWORLD);
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.active_context = None;
        loaded.draw_sprocket.front_buffer_gworld = PPC_MAIN_GWORLD;

        let front_buffer = loaded.presented_front_buffer().unwrap();

        assert_eq!(front_buffer.base_addr, PPC_MAIN_SCREEN_BASE);
    }

    #[test]
    fn hle_import_runner_traces_draw_sprocket_swap_buffers() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SwapBuffers");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.context_state = PpcDspContextPlayState::Active;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_import_trace(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(probe.draw_sprocket_trace.len(), 1);
        let trace = &probe.draw_sprocket_trace[0];
        assert_eq!(trace.import_index, 0);
        assert_eq!(trace.action, "swap_buffers");
        assert_eq!(trace.result, PPC_NO_ERR);
        assert_eq!(trace.context, Some(PPC_DSP_CONTEXT));
        assert_eq!(trace.requested_state, None);
        assert_eq!(trace.requested_frequency, None);
        assert_eq!(trace.requested_width, None);
        assert_eq!(trace.requested_height, None);
        assert_eq!(trace.requested_context_options, None);
        assert_eq!(trace.requested_display_depth_mask, None);
        assert_eq!(trace.requested_back_buffer_depth_mask, None);
        assert_eq!(trace.requested_display_depth, None);
        assert_eq!(trace.requested_back_buffer_depth, None);
        assert_eq!(trace.requested_page_count, None);
        assert_eq!(trace.can_user_select, None);
        assert_eq!(trace.fade_kind, None);
        assert_eq!(trace.fade_percent, None);
        assert_eq!(trace.fade_zero_red, None);
        assert_eq!(trace.fade_zero_green, None);
        assert_eq!(trace.fade_zero_blue, None);
        assert_eq!(trace.reserved_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(trace.active_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(trace.context_state, "active");
        assert_eq!(trace.front_buffer_gworld, PPC_MAIN_GWORLD);
        assert_eq!(trace.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
        assert_eq!(trace.last_swap_context, Some(PPC_DSP_CONTEXT));
        assert_eq!(trace.swap_count, 1);
        assert_eq!(trace.fade_count, 0);
        assert_eq!(trace.frequency, PPC_DSP_FREQUENCY_60HZ);
        assert_eq!(trace.width, PPC_DSP_SCREEN_WIDTH);
        assert_eq!(trace.height, PPC_DSP_SCREEN_HEIGHT);
        assert_eq!(trace.context_options, PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL);
        assert_eq!(trace.display_depth_mask, PPC_DSP_DEPTH_MASK_16);
        assert_eq!(trace.back_buffer_depth_mask, PPC_DSP_DEPTH_MASK_16);
        assert_eq!(trace.display_depth, PPC_MAIN_SCREEN_STORAGE_DEPTH);
        assert_eq!(trace.back_buffer_depth, PPC_MAIN_SCREEN_STORAGE_DEPTH);
        assert_eq!(trace.page_count, PPC_DSP_ADVERTISED_PAGE_COUNT);
    }

    #[test]
    fn hle_import_runner_traces_draw_sprocket_lifecycle_errors() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_Release");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_import_trace(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_RESERVED_ERR)
        );
        assert_eq!(probe.draw_sprocket_trace.len(), 1);
        let trace = &probe.draw_sprocket_trace[0];
        assert_eq!(trace.action, "context_release");
        assert_eq!(trace.result, PPC_DSP_CONTEXT_NOT_RESERVED_ERR);
        assert_eq!(trace.context, Some(PPC_DSP_CONTEXT));
        assert_eq!(trace.reserved_context, None);
        assert_eq!(trace.active_context, None);
        assert_eq!(trace.context_state, "inactive");
        assert_eq!(trace.swap_count, 0);
        assert_eq!(trace.fade_count, 0);
    }

    #[test]
    fn hle_import_runner_draw_sprocket_swap_requires_active_context() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SwapBuffers");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        loaded
            .memory
            .write_u16_be(PPC_DSP_BACK_SCREEN_BASE, 0x7c00)
            .unwrap();
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.draw_sprocket.active_context, None);
        assert_eq!(loaded.draw_sprocket.last_swap_context, None);
        assert_eq!(loaded.draw_sprocket.swap_count, 0);
        assert_eq!(loaded.draw_sprocket.front_buffer_gworld, PPC_MAIN_GWORLD);
        assert_eq!(loaded.draw_sprocket.back_buffer_gworld, PPC_DSP_BACK_GWORLD);
        assert_eq!(
            loaded.memory.read_u16_be(PPC_DSP_BACK_SCREEN_BASE),
            Some(0x7c00)
        );
    }

    #[test]
    fn hle_import_runner_records_draw_sprocket_gamma_fades() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGamma");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 37;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.draw_sprocket.last_fade_context,
            Some(PPC_DSP_CONTEXT)
        );
        assert_eq!(
            loaded.draw_sprocket.last_fade_kind,
            Some(PpcDspGammaFadeKind::Manual)
        );
        assert_eq!(loaded.draw_sprocket.last_fade_percent, Some(37));
        assert_eq!(loaded.draw_sprocket.last_fade_zero_color, None);
        assert_eq!(loaded.draw_sprocket.fade_count, 1);
    }

    #[test]
    fn hle_import_runner_records_draw_sprocket_gamma_fade_out_color() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGammaOut");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        let color_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(color_ptr, vec![0; 6]);
        loaded.memory.write_u16_be(color_ptr, 0x1111).unwrap();
        loaded.memory.write_u16_be(color_ptr + 2, 0x2222).unwrap();
        loaded.memory.write_u16_be(color_ptr + 4, 0x3333).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = color_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.last_fade_context, Some(0));
        assert_eq!(
            loaded.draw_sprocket.last_fade_kind,
            Some(PpcDspGammaFadeKind::Out)
        );
        assert_eq!(loaded.draw_sprocket.last_fade_percent, Some(0));
        assert_eq!(
            loaded.draw_sprocket.last_fade_zero_color,
            Some(PpcRgbColor {
                red: 0x1111,
                green: 0x2222,
                blue: 0x3333
            })
        );
        assert_eq!(loaded.draw_sprocket.fade_count, 1);
    }

    #[test]
    fn hle_import_runner_traces_draw_sprocket_gamma_fade_zero_color() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGammaOut");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.draw_sprocket.reserved_context = Some(PPC_DSP_CONTEXT);
        let color_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(color_ptr, vec![0; 6]);
        loaded.memory.write_u16_be(color_ptr, 0x1111).unwrap();
        loaded.memory.write_u16_be(color_ptr + 2, 0x2222).unwrap();
        loaded.memory.write_u16_be(color_ptr + 4, 0x3333).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = color_ptr;

        let probe = loaded.run_with_hle_import_trace(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(probe.draw_sprocket_trace.len(), 1);
        let trace = &probe.draw_sprocket_trace[0];
        assert_eq!(trace.action, "fade_gamma_out");
        assert_eq!(trace.result, PPC_NO_ERR);
        assert_eq!(trace.fade_kind.as_deref(), Some("out"));
        assert_eq!(trace.fade_percent, Some(0));
        assert_eq!(trace.fade_zero_red, Some(0x1111));
        assert_eq!(trace.fade_zero_green, Some(0x2222));
        assert_eq!(trace.fade_zero_blue, Some(0x3333));
        assert_eq!(trace.fade_count, 1);
    }

    #[test]
    fn hle_import_runner_draw_sprocket_auto_gamma_requires_reserved_context() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGammaOut");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_RESERVED_ERR)
        );
        assert_eq!(loaded.draw_sprocket.last_fade_context, None);
        assert_eq!(loaded.draw_sprocket.fade_count, 0);

        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_FadeGammaIn");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.draw_sprocket.started = true;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_DSP_CONTEXT_NOT_RESERVED_ERR)
        );
        assert_eq!(loaded.draw_sprocket.last_fade_context, None);
        assert_eq!(loaded.draw_sprocket.fade_count, 0);
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_context_attributes() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetAttributes");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0xaa; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = attributes_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr),
            Some(PPC_DSP_FREQUENCY_60HZ)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 4),
            Some(PPC_DSP_SCREEN_WIDTH)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 8),
            Some(PPC_DSP_SCREEN_HEIGHT)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 28),
            Some(PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 32),
            Some(PPC_DSP_DEPTH_MASK_16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 36),
            Some(PPC_DSP_DEPTH_MASK_16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 40),
            Some(PPC_MAIN_SCREEN_STORAGE_DEPTH)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 44),
            Some(PPC_MAIN_SCREEN_STORAGE_DEPTH)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 48),
            Some(PPC_DSP_ADVERTISED_PAGE_COUNT)
        );
        assert_eq!(loaded.memory.read_u8(attributes_ptr + 52), Some(0));
        assert_eq!(loaded.memory.read_u32_be(attributes_ptr + 56), Some(0));
    }

    #[test]
    fn hle_import_runner_draw_sprocket_get_attributes_is_all_or_nothing() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetAttributes");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(attributes_ptr, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = attributes_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        for offset in 0..4 {
            assert_eq!(loaded.memory.read_u8(attributes_ptr + offset), Some(0xaa));
        }
    }

    #[test]
    fn hle_import_runner_returns_stored_draw_sprocket_context_attributes() {
        let pef =
            synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_GetAttributes");
        let mut loaded = load_pef_application(&pef).unwrap();
        let attributes_ptr = PPC_DATA_BASE + 0x1000;
        let context_attributes = PpcDspContextAttributes {
            frequency: 75 << 16,
            width: 800,
            height: 600,
            context_options: PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL | 0x20,
            display_best_depth_mask: 0x20,
            back_buffer_best_depth_mask: 0x10,
            display_depth: 32,
            back_buffer_depth: 16,
            page_count: 1,
        };
        loaded.draw_sprocket.context_attributes = context_attributes;
        loaded.memory.add_region(
            attributes_ptr,
            vec![0xaa; PPC_DSP_CONTEXT_ATTRIBUTES_SIZE as usize],
        );
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = attributes_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr),
            Some(context_attributes.frequency)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 4),
            Some(context_attributes.width)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 8),
            Some(context_attributes.height)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 28),
            Some(context_attributes.context_options)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 32),
            Some(context_attributes.back_buffer_best_depth_mask)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 36),
            Some(context_attributes.display_best_depth_mask)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 40),
            Some(context_attributes.back_buffer_depth)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 44),
            Some(context_attributes.display_depth)
        );
        assert_eq!(
            loaded.memory.read_u32_be(attributes_ptr + 48),
            Some(context_attributes.page_count)
        );
        assert_eq!(loaded.memory.read_u8(attributes_ptr + 52), Some(0));
    }

    #[test]
    fn hle_import_runner_handles_draw_sprocket_vbl_busy_and_alt_buffer_calls() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpContext_SetVBLProc");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0x0200_1000;
        loaded.cpu.gpr[5] = 0x1234_5678;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.draw_sprocket.vbl_proc, Some(0x0200_1000));
        assert_eq!(loaded.draw_sprocket.vbl_refcon, Some(0x1234_5678));

        let busy_ptr = 0x0400_0100;
        loaded.memory.write_u8(busy_ptr, 1).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DSpContextIsBusy;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = busy_ptr;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(busy_ptr), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DSpAltBufferDispose;
        loaded.cpu.gpr[3] = 0x0300_0100;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::DSpContextInvalBackBufferRect;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer;
        loaded.cpu.gpr[3] = PPC_DSP_CONTEXT;
        loaded.cpu.gpr[4] = 0x0300_0100;

        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    }

    #[test]
    fn fire_vbl_tasks_invokes_active_draw_sprocket_vbl_proc() {
        let pef = synthetic_pef_with_library_import(b"DrawSprocketLib", b"DSpStartup");
        let mut loaded = load_pef_application(&pef).unwrap();
        const VECTOR: u32 = PPC_DATA_BASE + 0x4000;
        const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
        loaded.memory.add_region(VECTOR, vec![0; 8]);
        loaded.memory.write_u32_be(VECTOR, ENTRY).unwrap();
        loaded.memory.write_u32_be(VECTOR + 4, PPC_DATA_BASE).unwrap();
        loaded.memory.add_region(ENTRY, vec![0x4e, 0x80, 0x00, 0x20]);
        loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
        loaded.draw_sprocket.context_state = PpcDspContextPlayState::Active;
        loaded.draw_sprocket.vbl_proc = Some(VECTOR);
        loaded.draw_sprocket.vbl_refcon = Some(0x1234_5678);

        let probes = loaded.fire_vbl_tasks_for_ticks(0, 2, 10, 64, false, false);
        assert_eq!(probes.len(), 2);
        assert_eq!(probes[0].invocation.callback, VECTOR);
        assert_eq!(probes[1].invocation.callback, VECTOR);
    }

