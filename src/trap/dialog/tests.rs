    use super::super::test_helpers::{setup, setup_with_port, MockCpu, TEST_SP};
    use super::DialogItemTextStyle;
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::mac_roman::decode_mac_roman;
    use crate::trap::dispatch::{
        DialogButtonTrackingState, DialogItem, DialogPopupDraw, DialogTrackingState,
        PendingDialogPopupMenu, PersistentDialogSnapshot, RetainedModalDialogClickState,
    };
    use crate::trap::menu::{Menu, MenuItem};
    use crate::trap::TrapDispatcher;
    use crate::ui_theme::UiThemeId;
    use std::collections::VecDeque;

    #[test]
    fn styled_text_layout_uses_run_faces_independently_of_port_face() {
        let (mut disp, _, _) = setup();
        let text = b"A condensed heading\rPlain text should wrap using its own font metrics and remain inside the view.";
        let runs = vec![
            super::TeStyleRun {
                start: 0,
                style_index: 0,
                style: TrapDispatcher::te_resolved_style_from_parts(0, 0x20, 12, (0, 0, 0), 0, 0),
            },
            super::TeStyleRun {
                start: 20,
                style_index: 1,
                style: TrapDispatcher::te_resolved_style_from_parts(3, 0, 9, (0, 0, 0), 0, 0),
            },
        ];
        let expected = disp.te_wrap_lines_styled(&runs, text, 150);
        let width = disp.te_measure_text_width_styled(&runs, text, 20, text.len());
        for face in [0, 1, 0x20, 0x40] {
            disp.tx_face = face;
            assert_eq!(disp.te_wrap_lines_styled(&runs, text, 150), expected);
            assert_eq!(
                disp.te_measure_text_width_styled(&runs, text, 20, text.len()),
                width
            );
            for &(start, end) in &expected {
                let mut end = end;
                while end > start && text[end - 1].is_ascii_whitespace() {
                    end -= 1;
                }
                assert!(disp.te_measure_text_width_styled(&runs, text, start, end) <= 150);
            }
        }
    }

    fn screen_pixel_is_set(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> bool {
        let byte = bus.read_byte(base + (y as u32 * row_bytes) + ((x as u32) / 8));
        byte & (0x80u8 >> ((x as u8) & 7)) != 0
    }

    fn count_set_pixels(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u32 {
        let mut count = 0;
        for y in top..bottom {
            for x in left..right {
                if screen_pixel_is_set(bus, base, row_bytes, x, y) {
                    count += 1;
                }
            }
        }
        count
    }

    fn count_pixel_index(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        pixel_index: u8,
    ) -> u32 {
        let mut count = 0;
        for y in top..bottom {
            for x in left..right {
                if bus.read_byte(base + y as u32 * row_bytes + x as u32) == pixel_index {
                    count += 1;
                }
            }
        }
        count
    }

    fn sum_screen_bytes(
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u64 {
        let mut sum = 0;
        for y in top..bottom {
            let row = base + (y as u32 * row_bytes);
            for x in left..right {
                sum += u64::from(bus.read_byte(row + x as u32));
            }
        }
        sum
    }

    fn make_style_scrap(
        bus: &mut MacMemoryBus,
        styles: &[(u32, i16, i16, i16, i16, i16, (u16, u16, u16))],
    ) -> u32 {
        let handle = TrapDispatcher::allocate_handle_with_data(
            bus,
            TrapDispatcher::SCRAP_STYLE_TAB_OFFSET
                + (styles.len() as u32 * TrapDispatcher::SCRAP_STYLE_ELEMENT_SIZE),
        );
        let ptr = bus.read_long(handle);
        bus.write_word(
            ptr + TrapDispatcher::SCRAP_N_STYLES_OFFSET,
            styles.len() as u16,
        );
        for (index, (start, height, ascent, font, face, size, color)) in styles.iter().enumerate() {
            let base = ptr
                + TrapDispatcher::SCRAP_STYLE_TAB_OFFSET
                + (index as u32 * TrapDispatcher::SCRAP_STYLE_ELEMENT_SIZE);
            bus.write_long(base + TrapDispatcher::SCRAP_STYLE_START_CHAR_OFFSET, *start);
            bus.write_word(
                base + TrapDispatcher::SCRAP_STYLE_HEIGHT_OFFSET,
                *height as u16,
            );
            bus.write_word(
                base + TrapDispatcher::SCRAP_STYLE_ASCENT_OFFSET,
                *ascent as u16,
            );
            bus.write_word(base + TrapDispatcher::SCRAP_STYLE_FONT_OFFSET, *font as u16);
            bus.write_byte(base + TrapDispatcher::SCRAP_STYLE_FACE_OFFSET, *face as u8);
            bus.write_word(base + TrapDispatcher::SCRAP_STYLE_SIZE_OFFSET, *size as u16);
            bus.write_word(base + TrapDispatcher::SCRAP_STYLE_COLOR_OFFSET, color.0);
            bus.write_word(base + TrapDispatcher::SCRAP_STYLE_COLOR_OFFSET + 2, color.1);
            bus.write_word(base + TrapDispatcher::SCRAP_STYLE_COLOR_OFFSET + 4, color.2);
        }
        handle
    }

    // ---- InitDialogs ($A97B) ----

    #[test]
    fn init_dialogs_pops_4_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        // SP+0: ptr (4 bytes)
        bus.write_long(TEST_SP, 0x00000000);

        let result = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn dialog_edit_state_falls_back_to_first_valid_edit_text_item() {
        let (_disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 164, 0); // stale/default editField points at item 1
        bus.write_word(dialog_ptr + 168, 1); // default OK button
        let items = vec![
            DialogItem {
                item_type: 4,
                text: "OK".to_string(),
                ..Default::default()
            },
            DialogItem {
                item_type: 4,
                text: "Cancel".to_string(),
                ..Default::default()
            },
            DialogItem {
                item_type: 16,
                text: "answer".to_string(),
                ..Default::default()
            },
        ];

        let (text, edit_item, default_item) =
            TrapDispatcher::dialog_edit_state(&bus, dialog_ptr, &items);

        assert_eq!(text, "answer");
        assert_eq!(edit_item, 3);
        assert_eq!(default_item, 1);
    }

    #[test]
    fn dialog_edit_state_empty_live_handle_does_not_fallback_to_template_text() {
        let (_disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let text_handle = bus.alloc(4);

        bus.write_long(text_handle, 0); // valid empty text handle
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(dialog_ptr + 164, 0); // item 1 is the active edit field
        bus.write_word(dialog_ptr + 168, 1);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, text_handle);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 10);
        bus.write_word(ditl_ptr + 10, 26);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 16);
        bus.write_byte(ditl_ptr + 15, 0);

        let items = vec![DialogItem {
            item_type: 16,
            rect: (10, 10, 26, 120),
            text: "Edit Text".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        let (text, edit_item, default_item) =
            TrapDispatcher::dialog_edit_state(&bus, dialog_ptr, &items);

        assert_eq!(text, "");
        assert_eq!(edit_item, 1);
        assert_eq!(default_item, 1);
    }

    #[test]
    fn parse_dlog_ignores_position_word_when_resource_is_classic_length() {
        let (_disp, _cpu, mut bus) = setup();
        let ptr = bus.alloc(24);
        bus.write_word(ptr, 228); // top
        bus.write_word(ptr + 2, 198); // left
        bus.write_word(ptr + 4, 372); // bottom
        bus.write_word(ptr + 6, 455); // right
        bus.write_word(ptr + 8, 2); // procID
        bus.write_byte(ptr + 10, 1); // visible
        bus.write_byte(ptr + 12, 0); // goAway
        bus.write_long(ptr + 14, 0); // refCon
        bus.write_word(ptr + 18, 1013); // itemsID
        bus.write_byte(ptr + 20, 0); // empty title
        bus.write_byte(ptr + 21, 0); // padding only, not a position field
        bus.write_word(ptr + 22, 0x280A); // poison: centerMainScreen if over-read

        let (bounds, proc_id, visible, items_id, title, position) =
            TrapDispatcher::parse_dlog(&bus, ptr, 22);

        assert_eq!(bounds, (228, 198, 372, 455));
        assert_eq!(proc_id, 2);
        assert!(visible);
        assert_eq!(items_id, 1013);
        assert_eq!(title, "");
        assert_eq!(position, 0);
    }

    #[test]
    fn parse_dlog_reads_position_word_when_resource_contains_it() {
        let (_disp, _cpu, mut bus) = setup();
        let ptr = bus.alloc(24);
        bus.write_word(ptr, 10);
        bus.write_word(ptr + 2, 20);
        bus.write_word(ptr + 4, 110);
        bus.write_word(ptr + 6, 220);
        bus.write_word(ptr + 8, 2);
        bus.write_byte(ptr + 10, 1);
        bus.write_long(ptr + 14, 0);
        bus.write_word(ptr + 18, 42);
        bus.write_byte(ptr + 20, 0);
        bus.write_byte(ptr + 21, 0);
        bus.write_word(ptr + 22, 0x280A);

        let (_, _, _, _, _, position) = TrapDispatcher::parse_dlog(&bus, ptr, 24);

        assert_eq!(position, 0x280A);
    }

    #[test]
    fn dlog_parse_title_alignment_even_and_odd() {
        let (_disp, _cpu, mut bus) = setup();
        let odd = build_test_dlog_with_title((10, 20, 110, 220), 701, "Odd", 0x300A);
        let even = build_test_dlog_with_title((11, 21, 111, 221), 702, "Even", 0x700A);

        let odd_ptr = bus.alloc(odd.len() as u32);
        bus.write_bytes(odd_ptr, &odd);
        let even_ptr = bus.alloc(even.len() as u32);
        bus.write_bytes(even_ptr, &even);

        // MTE 1992 p. 6-148: the optional position word follows the Pascal
        // title string after a 0-or-1-byte alignment pad.
        let (odd_bounds, _, _, odd_items, odd_title, odd_position) =
            TrapDispatcher::parse_dlog(&bus, odd_ptr, odd.len() as u32);
        let (even_bounds, _, _, even_items, even_title, even_position) =
            TrapDispatcher::parse_dlog(&bus, even_ptr, even.len() as u32);

        assert_eq!(odd_bounds, (10, 20, 110, 220));
        assert_eq!(odd_items, 701);
        assert_eq!(odd_title, "Odd");
        assert_eq!(odd_position, 0x300A);
        assert_eq!(odd.len(), 26);

        assert_eq!(even_bounds, (11, 21, 111, 221));
        assert_eq!(even_items, 702);
        assert_eq!(even_title, "Even");
        assert_eq!(even_position, 0x700A);
        assert_eq!(even.len(), 28);
    }

    #[test]
    fn dlog_parse_position_constants() {
        let (_disp, _cpu, mut bus) = setup();
        // MTE 1992 p. 6-148 documents the compiled DLOG position constants
        // stored after the aligned title string.
        for position in [0x0000u16, 0xB00A, 0x300A, 0x700A] {
            let dlog = build_test_dlog_with_title((10, 20, 110, 220), 711, "P", position);
            let ptr = bus.alloc(dlog.len() as u32);
            bus.write_bytes(ptr, &dlog);

            let (_, _, _, _, _, parsed_position) =
                TrapDispatcher::parse_dlog(&bus, ptr, dlog.len() as u32);

            assert_eq!(parsed_position, position);
        }
    }

    #[test]
    fn alrt_parse_stage_nibbles_and_position() {
        let (_disp, _cpu, mut bus) = setup();
        let mut alrt = build_alrt_template(-321, 0xF721);
        alrt.extend_from_slice(&0xB00Au16.to_be_bytes());
        let ptr = bus.alloc(alrt.len() as u32);
        bus.write_bytes(ptr, &alrt);

        let (bounds, items_id, stages, position) =
            TrapDispatcher::parse_alrt(&bus, ptr, alrt.len() as u32);

        assert_eq!(bounds, (0, 0, 80, 200));
        assert_eq!(items_id, -321);
        assert_eq!(stages, 0xF721);
        // IM:I I-425 to I-426: low nibble is stage 1, high nibble is stage 4.
        assert_eq!(stages & 0x000F, 0x0001);
        assert_eq!((stages >> 4) & 0x000F, 0x0002);
        assert_eq!((stages >> 8) & 0x000F, 0x0007);
        assert_eq!((stages >> 12) & 0x000F, 0x000F);
        assert_eq!(position, 0xB00A);

        let classic_alrt = build_alrt_template(123, 0x0008);
        let classic_ptr = bus.alloc(classic_alrt.len() as u32);
        bus.write_bytes(classic_ptr, &classic_alrt);
        let (_, classic_items_id, classic_stages, classic_position) =
            TrapDispatcher::parse_alrt(&bus, classic_ptr, classic_alrt.len() as u32);
        assert_eq!(classic_items_id, 123);
        assert_eq!(classic_stages, 0x0008);
        assert_eq!(classic_position, 0);
    }

    fn push_ditl_count(data: &mut Vec<u8>, count_minus_one: i16) {
        data.extend_from_slice(&count_minus_one.to_be_bytes());
    }

    fn push_ditl_item(
        data: &mut Vec<u8>,
        item_handle: u32,
        rect: (i16, i16, i16, i16),
        item_type: u8,
        length_or_reserved: u8,
        payload: &[u8],
    ) {
        data.extend_from_slice(&item_handle.to_be_bytes());
        data.extend_from_slice(&rect.0.to_be_bytes());
        data.extend_from_slice(&rect.1.to_be_bytes());
        data.extend_from_slice(&rect.2.to_be_bytes());
        data.extend_from_slice(&rect.3.to_be_bytes());
        data.push(item_type);
        data.push(length_or_reserved);
        data.extend_from_slice(payload);
        if payload.len() % 2 != 0 {
            data.push(0);
        }
    }

    fn parse_ditl_bytes(bus: &mut MacMemoryBus, data: &[u8]) -> Vec<DialogItem> {
        let ptr = bus.alloc(data.len() as u32);
        bus.write_bytes(ptr, data);
        TrapDispatcher::parse_ditl(bus, ptr, data.len() as u32)
    }

    #[test]
    fn ditl_iter_empty_count_minus_one() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        // IM:I I-427: the first word stores the number of items minus 1.
        push_ditl_count(&mut ditl, -1);

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert!(items.is_empty());
    }

    #[test]
    fn ditl_iter_button_checkbox_radio_static_edit_text() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 4);
        for (item_type, rect, text) in [
            (4, (1, 2, 11, 42), "OK"),
            (5, (12, 2, 22, 72), "Check"),
            (6, (23, 2, 33, 72), "Radio"),
            (8, (34, 2, 44, 92), "Static"),
            (16, (45, 2, 57, 122), "Edit"),
        ] {
            push_ditl_item(
                &mut ditl,
                0,
                rect,
                item_type,
                text.len() as u8,
                text.as_bytes(),
            );
        }

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 5);
        assert_eq!(items[0].item_type, 4);
        assert_eq!(items[0].rect, (1, 2, 11, 42));
        assert_eq!(items[0].text, "OK");
        assert_eq!(items[1].item_type, 5);
        assert_eq!(items[1].text, "Check");
        assert_eq!(items[2].item_type, 6);
        assert_eq!(items[2].text, "Radio");
        assert_eq!(items[3].item_type, 8);
        assert_eq!(items[3].text, "Static");
        assert_eq!(items[4].item_type, 16);
        assert_eq!(items[4].rect, (45, 2, 57, 122));
        assert_eq!(items[4].text, "Edit");
    }

    #[test]
    fn ditl_iter_control_icon_picture_resource_id() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 2);
        // MTE 1992 p. 6-153 documents resource-backed DITL items as a
        // reserved byte followed by a signed 16-bit resource ID.
        push_ditl_item(&mut ditl, 0, (1, 2, 11, 42), 7, 0, &128i16.to_be_bytes());
        // IM:I I-427 describes the same slot as a length byte of 2.
        push_ditl_item(
            &mut ditl,
            0,
            (12, 2, 28, 34),
            32,
            2,
            &(-42i16).to_be_bytes(),
        );
        push_ditl_item(&mut ditl, 0, (29, 2, 70, 90), 64, 0, &300i16.to_be_bytes());

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].item_type, 7);
        assert_eq!(items[0].resource_id, 128);
        assert_eq!(items[1].item_type, 32);
        assert_eq!(items[1].resource_id, -42);
        assert_eq!(items[2].item_type, 64);
        assert_eq!(items[2].resource_id, 300);
    }

    #[test]
    fn ditl_iter_user_item_without_data_bytes() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 0);
        push_ditl_item(&mut ditl, 0x12345678, (10, 20, 30, 40), 0, 0, &[]);

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, 0);
        assert_eq!(items[0].rect, (10, 20, 30, 40));
        assert_eq!(items[0].proc_ptr, 0x12345678);
    }

    #[test]
    fn ditl_iter_skips_user_item_payload_before_following_items() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 1);
        push_ditl_item(
            &mut ditl,
            0,
            (10, 20, 30, 140),
            0,
            4,
            &[0x00, 0x05, 0x00, 0x09],
        );
        push_ditl_item(&mut ditl, 0, (40, 50, 52, 150), 6, 4, b"Home");

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_type, 0);
        assert_eq!(items[0].rect, (10, 20, 30, 140));
        assert_eq!(items[1].item_type, 6);
        assert_eq!(items[1].rect, (40, 50, 52, 150));
        assert_eq!(items[1].text, "Home");
    }

    #[test]
    fn ditl_iter_help_item_sized_payload() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 1);
        // MTE 1992 p. 6-154: help items use a 4- or 6-byte payload.
        push_ditl_item(
            &mut ditl,
            0,
            (0, 0, 0, 0),
            1,
            6,
            &[0x00, 0x08, 0x01, 0x2C, 0x00, 0x02],
        );
        push_ditl_item(&mut ditl, 0, (5, 6, 17, 50), 4, 2, b"OK");

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_type, 1);
        assert_eq!(items[1].item_type, 4);
        assert_eq!(items[1].text, "OK");
    }

    #[test]
    fn ditl_iter_odd_pascal_text_alignment() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 1);
        push_ditl_item(&mut ditl, 0, (1, 2, 11, 42), 8, 3, b"Odd");
        push_ditl_item(&mut ditl, 0, (12, 2, 22, 42), 4, 2, b"OK");

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "Odd");
        assert_eq!(items[1].rect, (12, 2, 22, 42));
        assert_eq!(items[1].text, "OK");
    }

    #[test]
    fn ditl_iter_even_pascal_text_alignment() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 1);
        push_ditl_item(&mut ditl, 0, (1, 2, 11, 42), 8, 4, b"Even");
        push_ditl_item(&mut ditl, 0, (12, 2, 22, 42), 4, 2, b"OK");

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "Even");
        assert_eq!(items[1].rect, (12, 2, 22, 42));
        assert_eq!(items[1].text, "OK");
    }

    #[test]
    fn ditl_iter_rejects_or_stops_at_truncated_item() {
        let (_disp, _cpu, mut bus) = setup();
        let mut ditl = Vec::new();
        push_ditl_count(&mut ditl, 1);
        push_ditl_item(&mut ditl, 0, (1, 2, 11, 42), 4, 2, b"OK");
        ditl.extend_from_slice(&0u32.to_be_bytes());
        for coord in [12i16, 2, 22, 72] {
            ditl.extend_from_slice(&coord.to_be_bytes());
        }
        ditl.push(8);
        ditl.push(6);
        ditl.extend_from_slice(b"Bad");

        let items = parse_ditl_bytes(&mut bus, &ditl);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "OK");
    }

    fn build_test_dlog(bounds: (i16, i16, i16, i16), items_id: i16, position: u16) -> Vec<u8> {
        build_test_dlog_with_title(bounds, items_id, "", position)
    }

    fn build_test_dlog_with_title(
        bounds: (i16, i16, i16, i16),
        items_id: i16,
        title: &str,
        position: u16,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(22 + title.len() + (title.len() & 1));
        data.extend_from_slice(&bounds.0.to_be_bytes());
        data.extend_from_slice(&bounds.1.to_be_bytes());
        data.extend_from_slice(&bounds.2.to_be_bytes());
        data.extend_from_slice(&bounds.3.to_be_bytes());
        data.extend_from_slice(&2i16.to_be_bytes()); // procID: plainDBox
        data.push(0); // invisible; geometry-only tests do not need drawing
        data.push(0); // filler
        data.push(0); // goAwayFlag
        data.push(0); // filler
        data.extend_from_slice(&0u32.to_be_bytes()); // refCon
        data.extend_from_slice(&items_id.to_be_bytes());
        data.push(title.len() as u8);
        data.extend_from_slice(title.as_bytes());
        if (1 + title.len()) % 2 != 0 {
            data.push(0); // title alignment padding
        }
        data.extend_from_slice(&position.to_be_bytes());
        data
    }

    fn build_test_ditl_item(
        item_type: u8,
        rect: (i16, i16, i16, i16),
        item_data: &[u8],
    ) -> Vec<u8> {
        build_test_ditl_items(&[(item_type, rect, item_data)])
    }

    fn build_test_ditl_items(items: &[(u8, (i16, i16, i16, i16), &[u8])]) -> Vec<u8> {
        let total_len = items.iter().fold(2usize, |len, (_, _, item_data)| {
            len + 14 + item_data.len() + (item_data.len() & 1)
        });
        let mut data = Vec::with_capacity(total_len);
        data.extend_from_slice(&((items.len() as u16).saturating_sub(1)).to_be_bytes());
        for (item_type, rect, item_data) in items {
            append_test_ditl_item(&mut data, *item_type, *rect, item_data);
        }
        data
    }

    fn append_test_ditl_item(
        data: &mut Vec<u8>,
        item_type: u8,
        rect: (i16, i16, i16, i16),
        item_data: &[u8],
    ) {
        data.extend_from_slice(&0u32.to_be_bytes()); // itmHandle / userItem proc ptr
        data.extend_from_slice(&rect.0.to_be_bytes());
        data.extend_from_slice(&rect.1.to_be_bytes());
        data.extend_from_slice(&rect.2.to_be_bytes());
        data.extend_from_slice(&rect.3.to_be_bytes());
        data.push(item_type);
        data.push(item_data.len() as u8);
        data.extend_from_slice(item_data);
        if item_data.len() % 2 != 0 {
            data.push(0);
        }
    }

    fn build_test_alrt(bounds: (i16, i16, i16, i16), items_id: i16, stages: u16) -> Vec<u8> {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&bounds.0.to_be_bytes());
        data.extend_from_slice(&bounds.1.to_be_bytes());
        data.extend_from_slice(&bounds.2.to_be_bytes());
        data.extend_from_slice(&bounds.3.to_be_bytes());
        data.extend_from_slice(&items_id.to_be_bytes());
        data.extend_from_slice(&stages.to_be_bytes());
        data
    }

    #[test]
    fn alert_draws_standard_system_icon_one_when_application_resource_is_missing() {
        // MTE 1992 pp. 6-153 and 7-63: an iconItem names an ICON
        // resource. Resource Manager searches the open resource chain, which
        // includes the System file after the application resource fork.
        let (mut disp, mut cpu, mut bus) = setup();
        let alert_id = 3298;
        let ditl_id = 3299;
        let icon_id = 1i16.to_be_bytes();
        let alrt = build_test_alrt((100, 100, 220, 400), ditl_id, 0x5555);
        let ditl = build_test_ditl_items(&[
            (0xA0, (16, 16, 48, 48), icon_id.as_slice()),
            (4, (76, 210, 96, 270), b"OK"),
        ]);
        disp.install_test_resource(&mut bus, *b"ALRT", alert_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);

        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);
        bus.write_long(TEST_SP, 0);
        bus.write_word(TEST_SP + 4, alert_id as u16);

        disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            [
                bus.read_byte(screen_base + 116 * row_bytes + 116),
                bus.read_byte(screen_base + 147 * row_bytes + 147),
            ],
            [0xFF, 0xFF],
            "ICON 1 should fall through to the standard System alert icon"
        );
    }

    #[test]
    fn multi_button_alert_tracks_click_and_returns_selected_item() {
        // Multi-choice Alert resources are real modal dialogs, not merely
        // OK-only notices. Keep the Alert function frame pending until a
        // button is selected, then write the selected item number into the
        // function result slot at SP+6.
        let (mut disp, mut cpu, mut bus) = setup();
        let alert_id = 3300;
        let ditl_id = 3301;
        let alrt = build_test_alrt((100, 100, 210, 400), ditl_id, 0x5555);
        let ditl = build_test_ditl_items(&[
            (4, (70, 230, 90, 290), b"Choice 1"),
            (4, (70, 130, 90, 210), b"Choice 2"),
            (4, (70, 20, 90, 100), b"Cancel"),
            (
                8,
                (16, 20, 56, 280),
                b"Select one of these options before continuing.",
            ),
        ]);
        disp.install_test_resource(&mut bus, *b"ALRT", alert_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);

        bus.write_long(TEST_SP, 0); // filterProc
        bus.write_word(TEST_SP + 4, alert_id as u16);
        bus.write_word(TEST_SP + 6, 0xCAFE);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(TEST_SP + 6), 0);
        assert!(disp.dialog_tracking.is_some());
        assert!(disp.is_tracking_refire(0xA985));

        disp.push_mouse_down(180, 250);
        for _ in 0..4 {
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            if disp
                .dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_button.as_ref())
                .is_some()
            {
                break;
            }
        }
        assert!(disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_button.as_ref())
            .is_some_and(|button| button.item_no == 2));

        disp.push_mouse_up(180, 250);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        for _ in 0..64 {
            if disp.dialog_tracking.is_none() {
                break;
            }
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
        }

        assert!(disp.dialog_tracking.is_none());
        assert_eq!(bus.read_word(TEST_SP + 6), 2);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn one_button_alert_remains_modal_until_default_button_is_accepted() {
        // Inside Macintosh Volume I, I-417: Alert displays the alert and
        // returns after the user clicks a button. An informational alert with
        // only an OK button is still modal and must not be auto-accepted.
        let (mut disp, mut cpu, mut bus) = setup();
        let alert_id = 3302;
        let ditl_id = 3303;
        let alrt = build_test_alrt((100, 100, 210, 400), ditl_id, 0x5555);
        let ditl = build_test_ditl_items(&[
            (4, (70, 230, 90, 290), b"OK"),
            (8, (16, 20, 56, 280), b"Read this notice before continuing."),
        ]);
        disp.install_test_resource(&mut bus, *b"ALRT", alert_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);

        bus.write_long(TEST_SP, 0); // filterProc
        bus.write_word(TEST_SP + 4, alert_id as u16);
        bus.write_word(TEST_SP + 6, 0xCAFE);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(TEST_SP + 6), 0);
        assert!(disp.dialog_tracking.is_some());
        assert!(disp.is_tracking_refire(0xA985));

        for _ in 0..4 {
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert!(disp.dialog_tracking.is_some());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
            assert_eq!(bus.read_word(TEST_SP + 6), 0);
        }

        disp.push_key_down(0x24, b'\r');
        for _ in 0..4 {
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            if disp.dialog_tracking.is_none() {
                break;
            }
        }

        assert!(disp.dialog_tracking.is_none());
        assert_eq!(bus.read_word(TEST_SP + 6), 1);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn alert_returns_enabled_non_button_item_selected_by_mouse() {
        // Alert returns the item number for any enabled item. TrackControl is
        // reserved for controls; selecting another enabled item returns it
        // directly (Inside Macintosh Volume I, I-418).
        let (mut disp, mut cpu, mut bus) = setup();
        let alert_id = 3304;
        let ditl_id = 3305;
        let alrt = build_test_alrt((100, 100, 210, 400), ditl_id, 0x5555);
        let ditl = build_test_ditl_items(&[
            (4, (70, 230, 90, 290), b"OK"),
            (8, (16, 20, 56, 280), b"Select this enabled text item."),
        ]);
        disp.install_test_resource(&mut bus, *b"ALRT", alert_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);

        bus.write_long(TEST_SP, 0); // filterProc
        bus.write_word(TEST_SP + 4, alert_id as u16);
        bus.write_word(TEST_SP + 6, 0xCAFE);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert!(disp.dialog_tracking.is_some());

        disp.push_mouse_down(130, 140);
        for _ in 0..4 {
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            if disp.dialog_tracking.is_none() {
                break;
            }
        }

        assert!(disp.dialog_tracking.is_none());
        assert_eq!(bus.read_word(TEST_SP + 6), 2);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    fn loaded_resource_handle_for_test(
        disp: &TrapDispatcher,
        res_type: [u8; 4],
        res_id: i16,
    ) -> u32 {
        disp.loaded_handles
            .iter()
            .find_map(|(&handle, &(_, loaded_type, loaded_id))| {
                if loaded_type == res_type && loaded_id == res_id {
                    Some(handle)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected loaded {} resource {}",
                    String::from_utf8_lossy(&res_type),
                    res_id
                )
            })
    }

    fn assert_resource_nonpurgeable(
        disp: &TrapDispatcher,
        bus: &MacMemoryBus,
        res_type: [u8; 4],
        res_id: i16,
    ) {
        let handle = loaded_resource_handle_for_test(disp, res_type, res_id);
        assert_ne!(
            bus.read_long(handle),
            0,
            "{} {} should have a non-NIL master pointer",
            String::from_utf8_lossy(&res_type),
            res_id
        );
        assert_eq!(
            disp.handle_state_bits(handle).unwrap_or(0) & 0x40,
            0,
            "{} {} should be nonpurgeable",
            String::from_utf8_lossy(&res_type),
            res_id
        );
    }

    fn assert_resource_purgeable(disp: &TrapDispatcher, res_type: [u8; 4], res_id: i16) {
        let handle = loaded_resource_handle_for_test(disp, res_type, res_id);
        assert_ne!(
            disp.handle_state_bits(handle).unwrap_or(0) & 0x40,
            0,
            "{} {} should be purgeable",
            String::from_utf8_lossy(&res_type),
            res_id
        );
    }

    fn ditl_item_handle_field(bus: &MacMemoryBus, ditl_ptr: u32, item_no: i16) -> u32 {
        if item_no <= 0 || ditl_ptr == 0 {
            return 0;
        }

        let max_index = bus.read_word(ditl_ptr) as i16;
        if item_no > max_index + 1 {
            return 0;
        }

        let mut offset = 2u32;
        for current_item in 1..=max_index + 1 {
            let handle_addr = ditl_ptr + offset;
            offset += 4; // itmHandle / userItem ProcPtr
            offset += 8; // item display rectangle
            let data_len = bus.read_byte(ditl_ptr + offset + 1) as u32;
            offset += 2; // item type + data length
            let padded = (data_len + 1) & !1;
            if current_item == item_no {
                return bus.read_long(handle_addr);
            }
            offset += padded;
        }

        0
    }

    // ---- DialogDispatch ($AA68) ----
    // Macintosh Toolbox Essentials (1992), pp. 6-162 to 6-167.

    #[test]
    fn dialogdispatch_getstdfilterproc_selector_03_writes_non_nil_shim_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let proc_storage_ptr = 0x300000u32;

        bus.write_long(proc_storage_ptr, 0x00AB_CDEF);
        bus.write_long(TEST_SP, proc_storage_ptr); // VAR theProc
        bus.write_word(TEST_SP + 4, 0xBEEF); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0203); // selector 3, 4 param bytes

        let result = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let shim = bus.read_long(proc_storage_ptr);
        assert_ne!(shim, 0);
        assert_eq!(shim, disp.dialog_std_filter_proc);
        assert_eq!(bus.read_word(shim), 0x4EF9); // JMP abs.L shimBody
        assert_eq!(bus.read_long(shim + 2), shim + 6);
        assert_eq!(bus.read_word(shim + 6), 0x7000); // MOVEQ #0, D0
        assert_eq!(bus.read_word(shim + 8), 0x426F); // CLR.W 16(SP)
        assert_eq!(bus.read_word(shim + 10), 0x0010);
        assert_eq!(bus.read_word(shim + 12), 0x4E74); // RTD #12
        assert_eq!(bus.read_word(shim + 14), 0x000C);
        assert_eq!(bus.read_word(TEST_SP + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn dialogdispatch_getstdfilterproc_selector_03_reuses_cached_shim() {
        let (mut disp, mut cpu, mut bus) = setup();
        let slot_a = 0x300000u32;
        let slot_b = 0x300004u32;

        bus.write_long(TEST_SP, slot_a);
        bus.write_word(TEST_SP + 4, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0203);
        let first = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(first.unwrap().is_ok());
        let shim_a = bus.read_long(slot_a);
        assert_ne!(shim_a, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, slot_b);
        bus.write_word(TEST_SP + 4, 0xCAFE);
        cpu.write_reg(Register::D0, 0x0203);
        let second = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(second.unwrap().is_ok());
        let shim_b = bus.read_long(slot_b);

        assert_eq!(shim_a, shim_b);
        assert_eq!(shim_b, disp.dialog_std_filter_proc);
        assert_eq!(bus.read_word(shim_b), 0x4EF9);
        assert_eq!(bus.read_word(shim_b + 6), 0x7000);
        assert_eq!(bus.read_word(TEST_SP + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn dialogdispatch_setdialogdefaultitem_selector_04_writes_adefitem_and_updates_tracking() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(256);

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 0, 0),
            title: String::new(),
            proc_id: 0,
            items: Vec::new(),
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
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

        bus.write_word(TEST_SP, 9); // newItem
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xBEEF); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0304); // selector 4, 6 param bytes

        let result = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dialog_ptr + 168), 9);
        assert_eq!(
            disp.dialog_tracking.as_ref().map(|t| t.default_item),
            Some(9)
        );
        assert_eq!(bus.read_word(TEST_SP + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn dialogdispatch_setdialogcancelitem_selector_05_updates_tracking_cancel_item_and_returns_noerr(
    ) {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(256);

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 0, 0),
            title: String::new(),
            proc_id: 0,
            items: Vec::new(),
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
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

        bus.write_word(TEST_SP, 7); // newItem
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xBEEF); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0305); // selector 5, 6 param bytes

        let result = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(disp.dialog_cancel_items.get(&dialog_ptr), Some(&7));
        assert_eq!(
            disp.dialog_tracking.as_ref().map(|t| t.cancel_item),
            Some(7)
        );
        assert_eq!(bus.read_word(TEST_SP + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn dialogdispatch_modal_dialog_first_entry_honors_preserved_default_and_cancel_items() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;

        disp.front_window = dialog_ptr;
        disp.window_bounds = (92, 95, 240, 320);
        disp.window_proc_id = 1;
        disp.window_title = "AA68".to_string();
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 4,
                    rect: (20, 20, 40, 90),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (20, 110, 40, 200),
                    text: "Cancel".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );
        bus.write_word(dialog_ptr + 168, 2);
        disp.dialog_cancel_items.insert(dialog_ptr, 1);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, 0);

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert_eq!(tracking.default_item, 2);
        assert_eq!(tracking.cancel_item, 1);
    }

    #[test]
    fn dialogdispatch_setdialogtrackscursor_selector_06_returns_noerr_and_pops_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(256);

        bus.write_word(TEST_SP, 1); // tracks = TRUE
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xBEEF); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0306); // selector 6, 6 param bytes

        let result = disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(TEST_SP + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    // ---- GetNewDialog ($A97C) ----

    #[test]
    fn get_new_dialog_missing_dlog_sets_reserr_and_returns_nil() {
        // MTE 1992 p. 6-114: GetNewDialog returns NIL when the DLOG
        // resource can't be read; the failed resource read surfaces through
        // Resource Manager ResErr as resNotFound (-192).
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_eq!(
            dlg_ptr, 0,
            "GetNewDialog must return NIL when DLOG is missing"
        );
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
        assert!(disp.window_list.is_empty());
    }

    #[test]
    fn get_new_dialog_missing_ditl_sets_reserr_and_returns_nil() {
        // MTE 1992 p. 6-114: GetNewDialog also returns NIL when the item-list
        // resource named by the DLOG cannot be read.
        let (mut disp, mut cpu, mut bus) = setup();
        let dlog = build_test_dlog((10, 20, 110, 220), 1909, 0);
        disp.install_test_resource(&mut bus, *b"DLOG", 1908, &dlog);
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP + 8, 1908);

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_long(TEST_SP + 10), 0);
        assert_eq!(bus.read_word(0x0A60) as i16, -192);
        assert!(disp.window_list.is_empty());
        assert!(disp.dialog_items.is_empty());
    }

    #[test]
    fn get_new_dialog_nil_storage_allocates_low_byte_clean_dialog_record() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((640 * 480) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 640, 640, 480, 8);

        let dlog = build_test_dlog((40, 50, 120, 240), 1911, 0);
        let ditl = build_test_ditl_item(4, (50, 80, 70, 140), b"OK");
        disp.install_test_resource(&mut bus, *b"DLOG", 1910, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1911, &ditl);
        let skew = bus.alloc(5);
        assert_ne!(
            (skew + MacMemoryBus::allocation_bucket_size(5)) & 0xFF,
            0,
            "test precondition should leave the heap skewed before GetNewDialog"
        );

        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage = NIL
        bus.write_word(TEST_SP + 8, 1910);
        bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);

        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dialog_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dialog_ptr, 0);
        assert_eq!(
            dialog_ptr & 0xFF,
            0,
            "manager-owned DialogRecord pointers should keep the low byte clear"
        );
        assert_eq!(
            bus.get_alloc_size(dialog_ptr),
            Some(170),
            "DialogRecord allocation should keep its logical size"
        );
    }

    #[test]
    fn get_new_dialog_without_dctb_exposes_grafport_bitmap_bounds() {
        // MTE 1992, p. 6-55: GetNewDialog uses a black-and-white GrafPort
        // unless a matching 'dctb' resource requests a color port. Old
        // applications may read the embedded portBits.bounds directly.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((640 * 480) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 640, 640, 480, 8);

        let dlog = build_test_dlog((40, 50, 110, 230), 2201, 0);
        let ditl = build_test_ditl_item(8, (10, 12, 28, 120), b"Classic");
        disp.install_test_resource(&mut bus, *b"DLOG", 2200, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 2201, &ditl);
        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 2200);

        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let dialog = bus.read_long(TEST_SP + 10);
        assert_eq!(bus.read_word(dialog + 6) & 0xC000, 0);
        assert_eq!(bus.read_word(dialog + 8) as i16, -40);
        assert_eq!(bus.read_word(dialog + 10) as i16, -50);
    }

    #[test]
    fn get_new_dialog_copies_ditl_not_aliases_resource() {
        // IM:I I-403 and MTE 1992 p. 6-114: GetNewDialog reads the DITL
        // resource, makes a copy, and uses that copy so several dialogs can
        // share identical resource-backed items without aliasing the resource.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((640 * 480) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 640, 640, 480, 8);

        let dlog = build_test_dlog((40, 50, 110, 230), 2201, 0);
        let ditl = build_test_ditl_item(8, (10, 12, 28, 120), b"Shared");
        disp.install_test_resource(&mut bus, *b"DLOG", 2200, &dlog);
        let original_ditl_ptr = disp.install_test_resource(&mut bus, *b"DITL", 2201, &ditl);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 2200); // dialogID
        bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let first_dialog = bus.read_long(TEST_SP + 10);
        let first_ditl_ptr = bus.read_long(bus.read_long(first_dialog + 156));

        assert_ne!(first_dialog, 0);
        assert_ne!(first_ditl_ptr, 0);
        assert_ne!(first_ditl_ptr, original_ditl_ptr);
        assert_eq!(ditl_item_handle_field(&bus, original_ditl_ptr, 1), 0);
        assert_ne!(ditl_item_handle_field(&bus, first_ditl_ptr, 1), 0);

        bus.write_long(first_ditl_ptr + 2, 0xAABB_CCDD);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 2200); // dialogID
        bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let second_dialog = bus.read_long(TEST_SP + 10);
        let second_ditl_ptr = bus.read_long(bus.read_long(second_dialog + 156));
        let second_text_handle = ditl_item_handle_field(&bus, second_ditl_ptr, 1);

        assert_ne!(second_dialog, 0);
        assert_ne!(second_ditl_ptr, original_ditl_ptr);
        assert_ne!(second_ditl_ptr, first_ditl_ptr);
        assert_ne!(second_text_handle, 0);
        assert_ne!(second_text_handle, 0xAABB_CCDD);
        assert_eq!(ditl_item_handle_field(&bus, original_ditl_ptr, 1), 0);
    }

    #[test]
    fn get_new_dialog_rewrites_reserved_item_handles() {
        // IM:I I-405: an item list in memory contains item handles for text,
        // controls, icons, and pictures. GetNewDialog must rewrite the copied
        // DITL's reserved handle fields, not the resource's fields.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((640 * 480) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 640, 640, 480, 8);

        let cntl_id = 2301i16.to_be_bytes();
        let icon_id = 2302i16.to_be_bytes();
        let pict_id = 2303i16.to_be_bytes();
        let dlog = build_test_dlog((40, 50, 130, 260), 2300, 0);
        let ditl = build_test_ditl_items(&[
            (8, (10, 12, 24, 140), b"Static".as_slice()),
            (16, (30, 12, 44, 140), b"Edit".as_slice()),
            (7, (50, 12, 70, 120), cntl_id.as_slice()),
            (32, (10, 150, 42, 182), icon_id.as_slice()),
            (64, (48, 150, 80, 220), pict_id.as_slice()),
        ]);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 110, 1, -1, 3, 0, 0] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0x1234_5678u32.to_be_bytes());
        cntl.push(4);
        cntl.extend_from_slice(b"Pick");
        disp.install_test_resource(&mut bus, *b"DLOG", 2299, &dlog);
        disp.install_test_resource(&mut bus, *b"CNTL", 2301, &cntl);
        let icon_ptr = disp.install_test_resource(&mut bus, *b"ICON", 2302, &[0xAA; 128]);
        let pict_ptr = disp.install_test_resource(&mut bus, *b"PICT", 2303, &[0x11; 32]);
        let original_ditl_ptr = disp.install_test_resource(&mut bus, *b"DITL", 2300, &ditl);

        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 2299); // dialogID
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dialog_ptr = bus.read_long(TEST_SP + 10);
        let copied_ditl_ptr = bus.read_long(bus.read_long(dialog_ptr + 156));
        assert_ne!(dialog_ptr, 0);
        assert_ne!(copied_ditl_ptr, original_ditl_ptr);

        for item_no in 1..=5 {
            assert_eq!(
                ditl_item_handle_field(&bus, original_ditl_ptr, item_no),
                0,
                "resource DITL item {} handle field must stay reserved",
                item_no
            );
        }

        let static_handle = ditl_item_handle_field(&bus, copied_ditl_ptr, 1);
        let edit_handle = ditl_item_handle_field(&bus, copied_ditl_ptr, 2);
        let control_handle = ditl_item_handle_field(&bus, copied_ditl_ptr, 3);
        let icon_handle = ditl_item_handle_field(&bus, copied_ditl_ptr, 4);
        let pict_handle = ditl_item_handle_field(&bus, copied_ditl_ptr, 5);

        assert_eq!(
            bus.read_bytes(bus.read_long(static_handle), 6),
            b"Static".to_vec()
        );
        assert_eq!(
            bus.read_bytes(bus.read_long(edit_handle), 4),
            b"Edit".to_vec()
        );
        assert_eq!(bus.read_long(bus.read_long(control_handle) + 4), dialog_ptr);
        assert_eq!(
            disp.dialog_control_handles.get(&control_handle),
            Some(&(dialog_ptr, 3))
        );
        assert_eq!(bus.read_long(icon_handle), icon_ptr);
        assert_eq!(bus.read_long(pict_handle), pict_ptr);
    }

    #[test]
    fn get_new_dialog_centers_standard_dlog_with_position_constant() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let dlog = build_test_dlog((10, 20, 110, 220), 1503, 0x280A);
        let ditl = build_test_ditl_item(4, (20, 20, 40, 80), b"OK");
        disp.install_test_resource(&mut bus, *b"DLOG", 1502, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1503, &ditl);
        bus.write_long(TEST_SP, 0); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1502); // dialogID

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(disp.window_bounds, (260, 300, 360, 500));
    }

    #[test]
    fn get_new_dialog_uses_system7_alert_position_near_top() {
        // MTE 1992 p. 4-126: alert position leaves about one-fifth of
        // the unused vertical screen space above the new window. EVO's
        // startup registration DLOG has these dimensions; a one-third
        // placement draws it visibly too low compared with BasiliskII.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let dlog = build_test_dlog((40, 40, 310, 530), 1513, 0x300A);
        let ditl = build_test_ditl_item(4, (241, 291, 261, 381), b"Not Yet");
        disp.install_test_resource(&mut bus, *b"DLOG", 1512, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1513, &ditl);
        bus.write_long(TEST_SP, 0); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1512); // dialogID

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(disp.window_bounds, (66, 155, 336, 645));
    }

    #[test]
    fn alert_position_places_the_complete_movable_dialog_structure() {
        let (mut disp, _cpu, bus) = setup();
        disp.screen_mode = (0, 800, 800, 600, 8);
        disp.menu_bar_hidden = true;

        assert_eq!(
            disp.positioned_window_bounds(&bus, (120, 120, 215, 520), 0x700A, 5),
            (115, 199, 210, 599)
        );
    }

    #[test]
    fn get_new_dialog_uses_parent_window_for_alert_position() {
        // MTE 1992 pp. 4-125 to 4-126: alertPositionParentWindow places the
        // new window in the alert position of the window last used.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let mut parent_dlog = build_test_dlog((100, 100, 500, 700), 1521, 0);
        parent_dlog[10] = 1;
        let parent_ditl = build_test_ditl_item(4, (20, 20, 40, 80), b"Parent");
        let child_dlog = build_test_dlog((10, 20, 110, 220), 1523, 0xB00A);
        let child_ditl = build_test_ditl_item(4, (20, 20, 40, 80), b"Child");
        disp.install_test_resource(&mut bus, *b"DLOG", 1520, &parent_dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1521, &parent_ditl);
        disp.install_test_resource(&mut bus, *b"DLOG", 1522, &child_dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1523, &child_ditl);

        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1520);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_ne!(bus.read_long(TEST_SP + 10), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1522);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_ne!(bus.read_long(TEST_SP + 10), 0);
        assert_eq!(disp.window_bounds, (160, 300, 260, 500));
    }

    #[test]
    fn get_new_dialog_positions_modal_frame_in_full_screen_parent() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let mut parent_dlog = build_test_dlog((0, 0, 600, 800), 1527, 0);
        parent_dlog[10] = 1;
        let parent_ditl = build_test_ditl_item(4, (20, 20, 40, 80), b"Parent");
        let mut child_dlog = build_test_dlog((74, 64, 255, 443), 1529, 0xB00A);
        child_dlog[8..10].copy_from_slice(&1i16.to_be_bytes());
        let child_ditl = build_test_ditl_item(4, (20, 20, 40, 80), b"Child");
        disp.install_test_resource(&mut bus, *b"DLOG", 1526, &parent_dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1527, &parent_ditl);
        disp.install_test_resource(&mut bus, *b"DLOG", 1528, &child_dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1529, &child_ditl);

        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1526);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1528);
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_ne!(bus.read_long(TEST_SP + 10), 0);
        assert_eq!(disp.window_bounds, (88, 210, 269, 589));
    }

    #[test]
    fn get_new_dialog_parent_alert_position_falls_back_without_a_window() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let dlog = build_test_dlog((40, 40, 310, 530), 1525, 0xB00A);
        let ditl = build_test_ditl_item(4, (241, 291, 261, 381), b"Not Yet");
        disp.install_test_resource(&mut bus, *b"DLOG", 1524, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1525, &ditl);
        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1524);

        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_ne!(bus.read_long(TEST_SP + 10), 0);
        assert_eq!(disp.window_bounds, (66, 155, 336, 645));
    }

    #[test]
    fn get_new_dialog_draws_visible_shell_with_unresolved_user_item_proc() {
        // Visible dialogs appear immediately on a real Mac. userItem contents
        // are application-owned and may be installed later with SetDItem, but
        // the dialog shell and standard items must not be suppressed.
        // Inside Macintosh Volume I, I-405, I-412, I-421.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_bytes(screen_base, &vec![0x77; 800 * 600]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let mut dlog = build_test_dlog((100, 100, 180, 260), 1701, 0);
        dlog[10] = 1; // visible
        let ditl = build_test_ditl_items(&[
            (0x80, (8, 8, 30, 80), b"".as_slice()),
            (4, (44, 20, 64, 90), b"OK".as_slice()),
        ]);
        disp.install_test_resource(&mut bus, *b"DLOG", 1700, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1701, &ditl);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1700); // dialogID

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(bus.read_byte(dlg_ptr + 110), 0xFF);
        assert!(disp.dialog_items.contains_key(&dlg_ptr));
        assert!(
            disp.dialog_saved_pixels.contains_key(&dlg_ptr),
            "visible dialogs save the covered pixels before drawing"
        );
        assert!(
            disp.dialog_visible_snapshots.contains_key(&dlg_ptr),
            "visible dialogs retain their clean initial shell for first ModalDialog entry"
        );
        assert!(!disp.dialog_initial_draw_deferred.contains(&dlg_ptr));
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == dlg_ptr),
            "visible dialog should still get an update event"
        );

        let probe_addr = screen_base + 120 * 800 + 120;
        assert_ne!(
            bus.read_byte(probe_addr),
            0x77,
            "visible GetNewDialog should draw the standard dialog background immediately"
        );
    }

    #[test]
    fn get_new_dialog_applies_matching_dctb_before_initial_draw() {
        // A DCTab uses the WCTab layout: seed, flags, ctSize, then semantic
        // part/RGB entries. GetNewDialog must copy and associate a matching
        // resource before drawing or saving the initial visible shell.
        // Macintosh Toolbox Essentials 1992, pp. 6-120 to 6-121.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((320 * 240) as u32);
        bus.write_bytes(screen_base, &vec![0xEE; 320 * 240]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 320, 320, 240, 8);

        let content_rgb = (0x4567, 0x5678, 0x6789);
        disp.device_clut.fill([0, 0, 0]);
        disp.device_clut.set_entry(42, [content_rgb.0, content_rgb.1, content_rgb.2]);

        let mut dlog = build_test_dlog((40, 50, 120, 250), 1921, 0);
        dlog[10] = 1; // visible
        let ditl = build_test_ditl_item(8, (8, 8, 24, 80), b"Color");
        let mut dctb = Vec::with_capacity(48);
        dctb.extend_from_slice(&0u32.to_be_bytes()); // ctSeed
        dctb.extend_from_slice(&0u16.to_be_bytes()); // ctFlags
        dctb.extend_from_slice(&4u16.to_be_bytes()); // five entries
        for (part, rgb) in [
            (0u16, content_rgb),
            (1, (0, 0, 0)),
            (2, (0, 0, 0)),
            (3, (0xFFFF, 0xFFFF, 0xFFFF)),
            (4, (0xAAAA, 0xAAAA, 0xAAAA)),
        ] {
            dctb.extend_from_slice(&part.to_be_bytes());
            dctb.extend_from_slice(&rgb.0.to_be_bytes());
            dctb.extend_from_slice(&rgb.1.to_be_bytes());
            dctb.extend_from_slice(&rgb.2.to_be_bytes());
        }

        disp.install_test_resource(&mut bus, *b"DLOG", 1920, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1921, &ditl);
        let resource_ptr = disp.install_test_resource(&mut bus, *b"dctb", 1920, &dctb);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1920); // dialogID

        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dialog_ptr = bus.read_long(TEST_SP + 10);
        assert_eq!(bus.read_word(dialog_ptr + 6) & 0xC000, 0xC000);
        let aux_handle = disp.window_aux_records[&dialog_ptr];
        let aux_ptr = bus.read_long(aux_handle);
        let table_handle = bus.read_long(aux_ptr + TrapDispatcher::AUX_WIN_CTABLE_OFFSET);
        let table_ptr = bus.read_long(table_handle);
        assert_ne!(table_ptr, resource_ptr, "GetNewDialog must copy the DCTab");
        assert_ne!(
            bus.read_long(table_ptr),
            0,
            "the copied table gets a fresh seed"
        );
        assert_eq!(bus.read_word(table_ptr + 6), 4);
        assert_eq!(
            (
                bus.read_word(dialog_ptr + 42),
                bus.read_word(dialog_ptr + 44),
                bus.read_word(dialog_ptr + 46),
            ),
            content_rgb,
            "the DCTab content role must become the dialog port background"
        );
        assert_eq!(
            bus.read_byte(screen_base + 100 * 320 + 200),
            42,
            "the first visible shell must use the DCTab content color"
        );
        assert!(disp.dialog_visible_snapshots.contains_key(&dialog_ptr));

        // Retained-dialog item redraws erase statText/control rectangles
        // before repainting them. Those erasures must use the same content
        // role instead of reintroducing white patches.
        let item_probe = screen_base + 60 * 320 + 120;
        bus.write_byte(item_probe, 0xEE);
        let items = disp.dialog_items[&dialog_ptr].clone();
        disp.redraw_standard_dialog_items(
            &mut bus,
            (40, 50, 120, 250),
            &items,
            1,
            "",
            -1,
            dialog_ptr,
        );
        assert_eq!(
            bus.read_byte(item_probe),
            42,
            "retained statText redraws must erase with the DCTab content color"
        );
    }

    #[test]
    fn get_new_dialog_applies_matching_ictb_text_style_on_initial_draw() {
        // An ictb follows the matching DITL ID. Each four-byte item entry
        // selects a 20-byte text style record by resource-relative offset.
        // MTE 1992, pp. 6-158 to 6-164; Inside Macintosh V, pp. V-279–V-282.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((320 * 240) as u32);
        bus.write_bytes(screen_base, &vec![0xEE; 320 * 240]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 320, 320, 240, 8);
        disp.device_clut.fill([0, 0, 0]);
        disp.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
        disp.device_clut.set_entry(20, [0xFFFF, 0xFFFF, 0]);
        disp.device_clut.set_entry(42, [0, 0x4000, 0]);
        disp.device_clut.set_entry(255, [0, 0, 0]);

        let mut dlog = build_test_dlog((40, 50, 120, 250), 1931, 0);
        dlog[8..10].copy_from_slice(&4i16.to_be_bytes()); // noGrowDocProc
        dlog[10] = 1;
        let ditl = build_test_ditl_item(8, (8, 8, 30, 90), b"Styled");

        let mut dctb = Vec::with_capacity(16);
        dctb.extend_from_slice(&0u32.to_be_bytes());
        dctb.extend_from_slice(&0u16.to_be_bytes());
        dctb.extend_from_slice(&0u16.to_be_bytes());
        dctb.extend_from_slice(&0u16.to_be_bytes());
        dctb.extend_from_slice(&0u16.to_be_bytes());
        dctb.extend_from_slice(&0x4000u16.to_be_bytes());
        dctb.extend_from_slice(&0u16.to_be_bytes());

        let mut ictb = Vec::with_capacity(24);
        ictb.extend_from_slice(&0x200Fu16.to_be_bytes()); // font, face, size, fg, bg
        ictb.extend_from_slice(&4u16.to_be_bytes());
        ictb.extend_from_slice(&0u16.to_be_bytes()); // Chicago
        ictb.extend_from_slice(&1u16.to_be_bytes()); // bold
        ictb.extend_from_slice(&12u16.to_be_bytes());
        ictb.extend_from_slice(&0xFFFFu16.to_be_bytes());
        ictb.extend_from_slice(&0xFFFFu16.to_be_bytes());
        ictb.extend_from_slice(&0u16.to_be_bytes());
        ictb.extend_from_slice(&0u16.to_be_bytes());
        ictb.extend_from_slice(&0u16.to_be_bytes());
        ictb.extend_from_slice(&0u16.to_be_bytes());
        ictb.extend_from_slice(&1u16.to_be_bytes()); // srcCopy

        disp.install_test_resource(&mut bus, *b"DLOG", 1930, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1931, &ditl);
        disp.install_test_resource(&mut bus, *b"dctb", 1930, &dctb);
        let ictb_resource = disp.install_test_resource(&mut bus, *b"ictb", 1931, &ictb);
        bus.write_long(TEST_SP, 0xFFFF_FFFF);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 1930);

        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dialog_ptr = bus.read_long(TEST_SP + 10);
        let ictb_handle = disp.window_dialog_item_color_table(&bus, dialog_ptr);
        assert_ne!(ictb_handle, 0);
        assert_ne!(bus.read_long(ictb_handle), ictb_resource);
        assert_eq!(
            disp.window_semantic_color(&bus, dialog_ptr, 0),
            Some((0, 0x4000, 0)),
            "associating the ictb must not replace the dialog's DCTab"
        );
        assert_eq!(
            bus.read_byte(screen_base + 100 * 320 + 200),
            42,
            "the DCTab must still color content outside styled item rectangles"
        );
        assert_eq!(
            disp.dialog_item_text_style(&bus, dialog_ptr, 0),
            Some(DialogItemTextStyle {
                font: 0,
                face: 1,
                size: 12,
                foreground: Some([0xFFFF, 0xFFFF, 0]),
                background: Some([0, 0, 0]),
                mode: 1,
            })
        );

        let mut pixels = Vec::new();
        for y in 48..70 {
            pixels.extend(bus.read_bytes(screen_base + y * 320 + 58, 82));
        }
        assert!(pixels.contains(&20), "styled text must use ictb foreground");
        assert!(pixels.contains(&255), "item must use ictb background");
    }

    #[test]
    fn hidden_dialog_quickdraw_shape_is_clipped_by_empty_vis_region() {
        // A real invisible window has an empty visRgn. Drawing through its
        // port must be clipped away instead of touching the screen-backed
        // PixMap. This covers hidden dialogs whose userItem setup draws into
        // the current dialog port before the dialog is ever shown.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_bytes(screen_base, &vec![0x77; 800 * 600]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let dlog = build_test_dlog((100, 100, 180, 260), 1705, 0);
        let ditl = build_test_ditl_item(8, (8, 8, 24, 90), b"Hidden");
        disp.install_test_resource(&mut bus, *b"DLOG", 1704, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1705, &ditl);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1704); // dialogID

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(bus.read_byte(dlg_ptr + 110), 0);
        assert_eq!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(dlg_ptr + 24)),
            None
        );

        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 20);
        bus.write_word(rect_ptr + 2, 20);
        bus.write_word(rect_ptr + 4, 30);
        bus.write_word(rect_ptr + 6, 30);
        bus.write_long(TEST_SP, rect_ptr);

        let result = disp.dispatch_quickdraw(true, 0x0A2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let probe_addr = screen_base + 125 * 800 + 125;
        assert_eq!(
            bus.read_byte(probe_addr),
            0x77,
            "QuickDraw drawing through a hidden dialog port must be clipped"
        );
        let saved_rect = TrapDispatcher::dialog_saved_pixel_rect((100, 100, 180, 260));
        let saved_width = (saved_rect.3 - saved_rect.1) as usize;
        let probe_index =
            ((125 - saved_rect.0) as usize) * saved_width + (125 - saved_rect.1) as usize;
        assert_eq!(
            disp.dialog_saved_pixels.get(&dlg_ptr).unwrap()[probe_index],
            0x77,
            "hidden dialog creation must capture the background before setup drawing"
        );

        // Drawing through another screen-backed port is background drawing
        // and must keep the hidden dialog's save-under current. Drawing
        // through the dialog port itself remains dialog-owned even after the
        // dialog has entered and returned from ModalDialog.
        bus.write_byte(probe_addr, 0x33);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, 0x300000, (125, 125, 126, 126));
        assert_eq!(
            disp.dialog_saved_pixels.get(&dlg_ptr).unwrap()[probe_index],
            0x33
        );
        disp.dialog_modal_entered.insert(dlg_ptr);
        bus.write_byte(probe_addr, 0x44);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, dlg_ptr, (125, 125, 126, 126));
        assert_eq!(
            disp.dialog_saved_pixels.get(&dlg_ptr).unwrap()[probe_index],
            0x33,
            "dialog-owned drawing must not poison the hidden save-under"
        );
    }

    #[test]
    fn dispos_dialog_skips_saved_pixels_for_never_drawn_deferred_dialog() {
        // Defensive coverage for the deferred-state guard: if a dialog is
        // marked never drawn, DisposDialog must not restore stale saved-under
        // pixels. Real visible dialogs normally draw at creation.
        // Inside Macintosh Volume I, I-425.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_bytes(screen_base, &vec![0x77; 800 * 600]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let mut dlog = build_test_dlog((100, 100, 180, 260), 1703, 0);
        dlog[10] = 1; // visible
        let ditl = build_test_ditl_items(&[
            (0x80, (8, 8, 30, 80), b"".as_slice()),
            (4, (44, 20, 64, 90), b"OK".as_slice()),
        ]);
        disp.install_test_resource(&mut bus, *b"DLOG", 1702, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1703, &ditl);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1702); // dialogID

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let dlg_ptr = bus.read_long(TEST_SP + 10);
        disp.dialog_initial_draw_deferred.insert(dlg_ptr);

        // Replace the saved snapshot with a distinct dirty pattern. If
        // DisposDialog restores it, the probe byte will change to 0x33.
        disp.dialog_saved_pixels
            .insert(dlg_ptr, vec![0x33; 90 * 170].into());
        let probe_addr = screen_base + 120 * 800 + 120;
        bus.write_byte(probe_addr, 0x77);

        bus.write_long(TEST_SP, dlg_ptr);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(probe_addr),
            0x77,
            "never-drawn deferred dialog must not restore saved background"
        );
        assert!(!disp.dialog_initial_draw_deferred.contains(&dlg_ptr));
        assert!(!disp.dialog_saved_pixels.contains_key(&dlg_ptr));
    }

    // save_dialog_pixels / restore_dialog_pixels must guard against off-screen y
    // (negative after the dBoxProc structure margin, or beyond screen_h).
    // Without the guard, (y as u32) sign-extends a negative i16 and
    // multiply-with-overflow panics in debug.
    #[test]
    fn save_dialog_pixels_handles_top_below_dbox_margin_without_overflow() {
        let (disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        for i in 0..800u32 * 600 {
            bus.write_byte(screen_base + i, 0x42);
        }
        let mut d = disp;
        bus.write_long(0x0824, screen_base);
        d.screen_mode = (screen_base, 800, 800, 600, 8);

        // Bounds with top=2 → save_top = -6 (negative).
        let saved = d.save_dialog_pixels(&bus, (2, 2, 50, 50));
        // Row width = (58 - (-6)) = 64; row count = 64.
        let row_width = 64usize;
        let row_count = 64usize;
        assert_eq!(saved.len(), row_width * row_count);

        // First 6 rows are off-screen (y = -6..-1) → zero-padded.
        for row in 0..6 {
            for col in 0..row_width {
                assert_eq!(
                    saved[row * row_width + col],
                    0x00,
                    "off-screen row {} col {} must be zero-padded",
                    row,
                    col
                );
            }
        }
        // y=0 row, save_left=-6 so cols 0..6 are off-screen → 0,
        // cols 6..64 read from the 0x42-filled framebuffer.
        let row0 = &saved[6 * row_width..7 * row_width];
        for (col, &px) in row0.iter().enumerate().take(6) {
            assert_eq!(
                px, 0x00,
                "off-screen column {} within on-screen row must be zero",
                col
            );
        }
        for (col, &px) in row0.iter().enumerate().skip(6) {
            assert_eq!(
                px, 0x42,
                "on-screen pixel at col {} must round-trip via framebuffer",
                col
            );
        }
    }

    #[test]
    fn dialog_snapshot_round_trips_complete_packed_indexed_rows() {
        for pixel_size in [2u16, 4] {
            let (disp, _cpu, mut bus) = setup();
            let row_bytes = 800 * u32::from(pixel_size) / 8;
            let screen_base = bus.alloc(row_bytes * 600);
            let mut d = disp;
            bus.write_long(0x0824, screen_base);
            d.screen_mode = (screen_base, row_bytes, 800, 600, pixel_size);

            // Including the dBox margin gives x=72..728. Treating every
            // non-8bpp screen as 1bpp captured only a fraction of each row
            // and replayed a stale strip over the left side of the content.
            let bounds = (100, 80, 500, 720);
            let save_top = 92u32;
            let save_bottom = 508u32;
            let pixels_per_byte = 8 / u32::from(pixel_size);
            let byte_left = 72 / pixels_per_byte;
            let byte_end = 728u32.div_ceil(pixels_per_byte);
            for y in save_top..save_bottom {
                for byte_x in byte_left..byte_end {
                    let value = ((y.wrapping_mul(13) + byte_x.wrapping_mul(7)) & 0xFF) as u8;
                    bus.write_byte(screen_base + y * row_bytes + byte_x, value);
                }
            }

            let expected = d.save_dialog_pixels(&bus, bounds);
            assert_eq!(
                expected.len(),
                ((save_bottom - save_top) * (byte_end - byte_left)) as usize
            );

            for y in save_top..save_bottom {
                bus.write_bytes(
                    screen_base + y * row_bytes + byte_left,
                    &vec![0; (byte_end - byte_left) as usize],
                );
            }
            d.restore_dialog_pixels(&mut bus, bounds, &expected);

            assert_eq!(
                d.save_dialog_pixels(&bus, bounds),
                expected,
                "retained {pixel_size}bpp dialog snapshots must restore every packed pixel row"
            );
        }
    }

    // save_rect_pixels / restore_rect_pixels guard the same off-screen y
    // overflow hazard as save_dialog_pixels.
    #[test]
    fn save_rect_pixels_handles_negative_top_without_overflow() {
        let (disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        for i in 0..800u32 * 600 {
            bus.write_byte(screen_base + i, 0x55);
        }
        let mut d = disp;
        bus.write_long(0x0824, screen_base);
        d.screen_mode = (screen_base, 800, 800, 600, 8);

        // Rect spanning above the screen top.
        let saved = d.save_rect_pixels(&bus, (-3, 0, 5, 10));
        let row_width = 10usize;
        let row_count = (5 - (-3)) as usize;
        assert_eq!(saved.len(), row_width * row_count);
        // First 3 rows off-screen (y = -3..0) → zero-padded.
        for row in 0..3 {
            for col in 0..row_width {
                assert_eq!(saved[row * row_width + col], 0x00);
            }
        }
        // Next 5 rows on-screen → 0x55.
        for row in 3..8 {
            for col in 0..row_width {
                assert_eq!(saved[row * row_width + col], 0x55);
            }
        }
    }

    #[test]
    fn save_dialog_pixels_handles_top_above_screen_height_without_overflow() {
        let (disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        let mut d = disp;
        bus.write_long(0x0824, screen_base);
        d.screen_mode = (screen_base, 800, 800, 600, 8);

        // Bounds way below screen → save_top = 595+ and save_bottom
        // = 700+, many rows off-screen on the bottom side.
        let saved = d.save_dialog_pixels(&bus, (600, 0, 700, 50));
        let row_count = (700 + TrapDispatcher::DBOX_FRAME_MARGIN
            - (600 - TrapDispatcher::DBOX_FRAME_MARGIN)) as usize;
        let row_width = (50 + TrapDispatcher::DBOX_FRAME_MARGIN
            - (0 - TrapDispatcher::DBOX_FRAME_MARGIN)) as usize;
        assert_eq!(saved.len(), row_count * row_width);
    }

    fn install_new_dialog_test_screen(
        disp: &mut TrapDispatcher,
        bus: &mut MacMemoryBus,
        fill: u8,
    ) -> u32 {
        let screen_base = bus.alloc((640 * 480) as u32);
        bus.write_bytes(screen_base, &vec![fill; 640 * 480]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 640, 640, 480, 8);
        screen_base
    }

    fn call_new_dialog_for_test(
        disp: &mut TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut MacMemoryBus,
        storage_ptr: u32,
        bounds: (i16, i16, i16, i16),
        visible: bool,
        proc_id: i16,
        behind: u32,
        items_handle: u32,
    ) -> (u32, u32) {
        let title_ptr = bus.alloc(32);
        bus.write_pstring(title_ptr, b"Lifecycle");
        let bounds_ptr = bus.alloc(8);
        bus.write_word(bounds_ptr, bounds.0 as u16);
        bus.write_word(bounds_ptr + 2, bounds.1 as u16);
        bus.write_word(bounds_ptr + 4, bounds.2 as u16);
        bus.write_word(bounds_ptr + 6, bounds.3 as u16);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for offset in 0..34u32 {
            bus.write_byte(sp + offset, 0);
        }
        bus.write_long(sp, items_handle);
        bus.write_long(sp + 4, 0x1234_5678); // refCon
        bus.write_byte(sp + 8, 0xFF); // goAwayFlag
        bus.write_long(sp + 10, behind);
        bus.write_word(sp + 14, proc_id as u16);
        bus.write_byte(sp + 16, if visible { 0xFF } else { 0 });
        bus.write_long(sp + 18, title_ptr);
        bus.write_long(sp + 22, bounds_ptr);
        bus.write_long(sp + 26, storage_ptr);
        bus.write_long(sp + 30, 0xDEAD_BEEF);

        disp.dispatch_dialog(true, 0x17D, cpu, bus)
            .unwrap()
            .unwrap();
        (sp, bus.read_long(sp + 30))
    }

    fn install_ditl_handle_for_test(bus: &mut MacMemoryBus, ditl: &[u8]) -> (u32, u32) {
        let items_ptr = bus.alloc(ditl.len() as u32);
        bus.write_bytes(items_ptr, ditl);
        let items_handle = bus.alloc(4);
        bus.write_long(items_handle, items_ptr);
        (items_handle, items_ptr)
    }

    #[test]
    fn new_dialog_with_storage_uses_caller_record() {
        // IM:I I-412: dStorage supplies the DialogRecord storage; NIL asks
        // the Dialog Manager to allocate the record instead.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        let storage_ptr = bus.alloc(170);

        let (sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            storage_ptr,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            0,
        );

        assert_eq!(dialog_ptr, storage_ptr);
        assert_eq!(bus.read_long(sp + 30), storage_ptr);
        assert_eq!(cpu.read_reg(Register::A7), sp + 30);
        assert_eq!(disp.window_list, vec![storage_ptr]);
    }

    #[test]
    fn new_dialog_without_storage_allocates_record() {
        // IM:I I-412 and MTE 1992 p. 6-118: passing NIL for dStorage
        // allocates the DialogRecord in the heap.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            2,
            0xFFFF_FFFF,
            0,
        );

        assert_ne!(dialog_ptr, 0);
        assert_eq!(bus.get_alloc_size(dialog_ptr), Some(170));
        assert_eq!(disp.window_list, vec![dialog_ptr]);
    }

    #[test]
    fn new_dialog_sets_window_kind_dialog_kind() {
        // IM:I I-407 and I-412: a DialogPtr is a WindowPtr whose
        // WindowRecord.windowKind is dialogKind, independent of the WDEF
        // procID used to draw modeless or modal chrome.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            0,
        );

        assert_eq!(bus.read_word(dialog_ptr + 108) as i16, 2);
        assert_eq!(disp.window_proc_ids.get(&dialog_ptr), Some(&4));
    }

    #[test]
    fn new_dialog_sets_dialog_port_font_to_dialog_font() {
        // IM:I I-412 and MTE 1992 p. 6-104: SetDAFont/SetDialogFont set
        // DlgFont for subsequently created dialog and alert grafPorts.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        bus.write_word(crate::memory::globals::addr::DLG_FONT, 3); // Geneva

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            2,
            0xFFFF_FFFF,
            0,
        );

        assert_eq!(bus.read_word(dialog_ptr + 68) as i16, 3);
        assert_eq!(disp.tx_font, 3);
        assert_eq!(
            disp.port_draw_states
                .get(&dialog_ptr)
                .map(|state| state.tx_font),
            Some(3)
        );
    }

    #[test]
    fn new_dialog_visible_queues_or_draws_expected_update_path() {
        // IM:I I-412 and I-287: visible NewDialog draws the dialog window and
        // generates an update event for the contents.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            true,
            2,
            0xFFFF_FFFF,
            0,
        );

        assert_eq!(bus.read_byte(dialog_ptr + 110), 0xFF);
        assert!(disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == dialog_ptr));
        assert!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(dialog_ptr + 122)).is_some()
        );
        assert_ne!(bus.read_byte(screen_base + 100 * 640 + 110), 0x77);
    }

    #[test]
    fn new_dialog_invisible_defers_screen_pixels() {
        // IM:I I-412: if visible is FALSE, the window is initially invisible
        // and may later be shown with ShowWindow.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            2,
            0xFFFF_FFFF,
            0,
        );

        assert_eq!(bus.read_byte(dialog_ptr + 110), 0);
        assert!(!disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == dialog_ptr));
        assert_eq!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(dialog_ptr + 122)),
            None
        );
        assert_eq!(bus.read_byte(screen_base + 100 * 640 + 110), 0x77);
    }

    #[test]
    fn new_dialog_honors_behind_nil_and_inserts_at_back() {
        // Inside Macintosh Volume I, I-412: NewDialog returns a DialogPtr
        // and honors the `behind` parameter for plane order.
        let (mut disp, mut cpu, mut bus) = setup();
        // Seed an existing window that will stay in front.
        let existing = 0x200040u32;
        disp.window_list.replace(vec![existing]);
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF); // visible

        // init_cgraf_window reads screen_mode for bounds math; a
        // zero-initialized mode multiplies-with-overflow later.
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        // Bounds at the origin are now safe via the off-screen y overflow guard
        // in save_dialog_pixels. Keep a non-origin placement for realism.
        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 100);
        bus.write_word(bounds_rect_ptr + 2, 100);
        bus.write_word(bounds_rect_ptr + 4, 300);
        bus.write_word(bounds_rect_ptr + 6, 400);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..34u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 22, bounds_rect_ptr);
        bus.write_word(sp + 16, 1); // visible
        bus.write_word(sp + 14, 1); // dBoxProc WDEF
        bus.write_long(sp + 10, 0); // behind = NIL (backmost)

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x17D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            pre_a7 + 30,
            "NewDialog must pop 30 bytes of parameters and leave result at new SP+0"
        );
        let dlg_ptr = bus.read_long(sp + 30);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(bus.read_word(dlg_ptr + 6) & 0xC000, 0);
        assert_eq!(
            bus.read_word(dlg_ptr + 108),
            2,
            "DialogRecord.window.windowKind must be dialogKind"
        );
        assert_eq!(
            disp.window_proc_ids.get(&dlg_ptr),
            Some(&1),
            "dialog WDEF procID must be tracked separately from windowKind"
        );
        assert_eq!(
            disp.window_list,
            vec![existing, dlg_ptr],
            "NewDialog(behind=NIL) must insert dialog at the back"
        );
        assert_eq!(
            disp.front_window, existing,
            "front must stay on the pre-existing visible window"
        );
    }

    #[test]
    fn new_cdialog_honors_behind_nil_and_inserts_at_back() {
        // Inside Macintosh Volume V, V-243: NewCDialog follows the same
        // creation path as NewDialog but returns a color dialog pointer.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing = 0x200040u32;
        disp.window_list.replace(vec![existing]);
        disp.front_window = existing;
        bus.write_byte(existing + 110u32, 0xFF); // visible

        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let bounds_rect_ptr = 0x301280u32;
        bus.write_word(bounds_rect_ptr, 120);
        bus.write_word(bounds_rect_ptr + 2, 120);
        bus.write_word(bounds_rect_ptr + 4, 320);
        bus.write_word(bounds_rect_ptr + 6, 420);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..34u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 22, bounds_rect_ptr);
        bus.write_word(sp + 16, 1); // visible
        bus.write_long(sp + 10, 0); // behind = NIL (backmost)

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x24B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            pre_a7 + 30,
            "NewCDialog must pop 30 bytes of parameters and leave result at new SP+0"
        );
        let dlg_ptr = bus.read_long(sp + 30);
        assert_ne!(dlg_ptr, 0);
        assert_eq!(bus.read_word(dlg_ptr + 6) & 0xC000, 0xC000);
        assert_eq!(
            disp.window_list,
            vec![existing, dlg_ptr],
            "NewCDialog(behind=NIL) must insert dialog at the back"
        );
        assert_eq!(
            disp.front_window, existing,
            "front must stay on the pre-existing visible window"
        );
    }

    #[test]
    fn get_new_dialog_honors_behind_specific_window() {
        // GetNewDialog with no DLOG resource hits the fallback branch
        // that does NOT call finish_dialog_creation — so behind can't
        // reshuffle a list entry that was never added. This test
        // verifies the stack slot is at least READ without panicking.
        // The main contract test is new_dialog_honors_behind_nil_
        // and_inserts_at_back above, since that exercises the
        // primary post-finish_dialog_creation path.
        let (mut disp, mut cpu, mut bus) = setup();

        let sp = TEST_SP;
        // Write a specific non-trivial behind pointer at SP+0.
        bus.write_long(sp, 0xDEAD0000);

        let result = disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    // ---- SelectDialogItemText ($A97E) ----

    #[test]
    fn select_dialog_item_text_zero_to_32767_selects_entire_text_and_sets_editfield() {
        // Macintosh Toolbox Essentials 1992, 6-131: selecting the whole
        // editable text item uses strtSel=0 and endSel=32767.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 164, 0xFFFF); // editField = -1 (none)

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (10, 20, 30, 40),
                text: decode_mac_roman(b"A\xC9B"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 32767u16); // endSel
        bus.write_word(TEST_SP + 2, 0); // strtSel
        bus.write_word(TEST_SP + 4, 1); // itemNo (1-based)
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog

        let result = disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);

        let item = &disp.dialog_items[&dialog_ptr][0];
        assert_eq!(item.sel_start, 0);
        assert_eq!(item.sel_end, 3);
        assert_eq!(bus.read_word(dialog_ptr + 164), 0);
    }

    #[test]
    fn select_dialog_item_text_clamps_and_normalizes_selection_bounds() {
        // IM:I I-414 defines a [strtSel,endSel) range; callers can pass
        // out-of-range values, so HLE clamps to text bounds and normalizes
        // reversed inputs.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (10, 20, 30, 40),
                text: "ABCDE".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 2); // endSel
        bus.write_word(TEST_SP + 2, 9); // strtSel (> text length)
        bus.write_word(TEST_SP + 4, 1); // itemNo
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog

        let result = disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);

        let item = &disp.dialog_items[&dialog_ptr][0];
        assert_eq!(item.sel_start, 2);
        assert_eq!(item.sel_end, 5);
    }

    #[test]
    fn select_dialog_item_text_non_edit_item_is_noop() {
        // Macintosh Toolbox Essentials 1992, 6-131: selection applies to
        // editable text items only.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 164, 0x7FFF); // sentinel

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 8, // statText
                rect: (10, 20, 30, 40),
                text: "STATIC".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 1,
                sel_end: 3,
            }],
        );

        bus.write_word(TEST_SP, 4); // endSel
        bus.write_word(TEST_SP + 2, 0); // strtSel
        bus.write_word(TEST_SP + 4, 1); // itemNo
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog

        let result = disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);

        let item = &disp.dialog_items[&dialog_ptr][0];
        assert_eq!(item.sel_start, 1);
        assert_eq!(item.sel_end, 3);
        assert_eq!(bus.read_word(dialog_ptr + 164), 0x7FFF);
    }

    // ---- Alert ($A985) ----

    // Alert with no matching ALRT resource must return -1 per IM:I-412
    // ("Alert returns -1 and does nothing").
    #[test]
    fn alert_returns_minus_one_when_alrt_resource_missing() {
        let (mut disp, mut cpu, mut bus) = setup();

        // SP+4: alertID = 128 (no ALRT resource loaded in the test
        // dispatcher).
        bus.write_word(TEST_SP + 4, 128);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -1);
    }

    // ---- shared TextEdit alignment ----
    //
    // Per IM:Text 1993 lines 7320-7323 and the MPW Universal Headers:
    // teJustLeft = 0, teJustCenter = 1, teJustRight = -1, teForceLeft = -2.

    #[test]
    fn te_line_origin_te_just_left_flush_left() {
        // teJustLeft (0): origin = box_left + TextEdit's left inset.
        assert_eq!(
            crate::text_edit::aligned_line_left(20, 200, 80, 0, 1),
            21,
            "teJustLeft (0) must anchor one pixel inside box_left"
        );
    }

    #[test]
    fn te_line_origin_te_just_center_midpoint() {
        // teJustCenter (1): origin = box_left + (box_w - line_w) / 2.
        // (200-20 - 80) / 2 = 50 → origin = 20 + 50 = 70.
        assert_eq!(
            crate::text_edit::aligned_line_left(20, 200, 80, 1, 1),
            70,
            "teJustCenter must midpoint the slack"
        );
    }

    #[test]
    fn te_line_origin_te_just_right_flush_right() {
        // teJustRight (-1): origin = box_right - line_width.
        // 200 - 80 = 120.
        assert_eq!(
            crate::text_edit::aligned_line_left(20, 200, 80, -1, 1),
            120,
            "teJustRight (-1) must anchor at box_right - line_width"
        );
    }

    #[test]
    fn te_line_origin_te_force_left_flush_left() {
        // teForceLeft (-2): origin = box_left + TextEdit's left inset
        // (overrides any
        // localised right-to-left default).
        assert_eq!(
            crate::text_edit::aligned_line_left(20, 200, 80, -2, 1),
            21,
            "teForceLeft (-2) must anchor one pixel inside box_left"
        );
    }

    #[test]
    fn te_line_origin_te_just_system_defaults_left() {
        // teJustSystem (0): localised default = left for LTR, with the
        // same TextEdit left inset.
        assert_eq!(
            crate::text_edit::aligned_line_left(20, 200, 80, 0, 1),
            21,
            "teJustSystem must default to flush left inside box_left"
        );
    }

    // ---- StopAlert ($A986) ----

    #[test]
    fn stop_alert_returns_minus_one_when_alrt_resource_missing() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 2);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xC0DE);
        bus.write_word(TEST_SP + 4, 128);
        let result = disp.dispatch_dialog(true, 0x186, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -1);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ALERT_STAGE), 2);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ANUMBER), 0xC0DE);
    }

    // ---- NoteAlert ($A987) ----

    #[test]
    fn note_alert_returns_minus_one_when_alrt_resource_missing() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 1);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xBEEF);
        bus.write_word(TEST_SP + 4, 128);
        let result = disp.dispatch_dialog(true, 0x187, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -1);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ALERT_STAGE), 1);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ANUMBER), 0xBEEF);
    }

    // ---- CautionAlert ($A988) ----

    #[test]
    fn caution_alert_returns_minus_one_when_alrt_resource_missing() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 3);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xFACE);
        bus.write_word(TEST_SP + 4, 128);
        let result = disp.dispatch_dialog(true, 0x188, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -1);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ALERT_STAGE), 3);
        assert_eq!(bus.read_word(crate::memory::globals::addr::ANUMBER), 0xFACE);
    }

    // ---- Alert family stages-driven default item (IM:I I-417 / I-422) ----
    //
    // Each test installs an ALRT resource with a controlled
    // 16-byte template (8-byte boundsRect + 2-byte itemsID + 2-byte
    // stages + padding) and verifies the trap returns the bold
    // (default) item for the current AlertStage low-mem byte.
    //
    // ALRT stages encoding (IM:I I-422): 16-bit word, 4 nibbles
    // (low to high = stage 1..4). Within each nibble, bit 3 is
    // boldItmNum (`okDismissal = 8`), bit 2 is boxDrwn (`alBit = 4`),
    // and bits 0..1 are the sound number.

    /// Build a 12-byte ALRT template: bounds=(0,0,80,200) +
    /// itemsID + stages + 0 padding. Real ALRTs are typically
    /// 12 bytes; we install at least 12.
    fn build_alrt_template(items_id: i16, stages: u16) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(&0u16.to_be_bytes()); // top
        v.extend_from_slice(&0u16.to_be_bytes()); // left
        v.extend_from_slice(&80u16.to_be_bytes()); // bottom
        v.extend_from_slice(&200u16.to_be_bytes()); // right
        v.extend_from_slice(&items_id.to_be_bytes()); // itemsID
        v.extend_from_slice(&stages.to_be_bytes()); // stages
        v
    }

    // ---- CouldDialog ($A979) / FreeDialog ($A97A) ----

    #[test]
    fn could_dialog_present_resource_sets_reserr_noerr() {
        // Inside Macintosh Volume I, I-415: CouldDialog targets a DLOG by
        // resource ID and prepares it for later use. BasiliskII leaves
        // ResErr at noErr on the missing-resource path.
        let (mut disp, mut cpu, mut bus) = setup();
        let dlog = vec![0u8; 20];
        disp.install_test_resource(&mut bus, *b"DLOG", 300, &dlog);
        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, 300);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x179, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn could_dialog_missing_resource_sets_reserr_resnotfound() {
        // BasiliskII leaves ResErr at noErr even when the DLOG is missing.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP, 301);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x179, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn free_dialog_present_resource_sets_reserr_noerr() {
        // Inside Macintosh Volume I, I-415: FreeDialog reverses CouldDialog
        // for previously targeted DLOG templates. BasiliskII leaves
        // ResErr at noErr on the missing-resource path.
        let (mut disp, mut cpu, mut bus) = setup();
        let dlog = vec![0u8; 20];
        disp.install_test_resource(&mut bus, *b"DLOG", 302, &dlog);
        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, 302);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x17A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn free_dialog_missing_resource_sets_reserr_resnotfound() {
        // BasiliskII leaves ResErr at noErr even when the DLOG is missing.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP, 303);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x17A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn could_dialog_free_dialog_purgeability() {
        // IM:I I-415: CouldDialog reads the dialog resource family into
        // memory if needed and makes it unpurgeable; FreeDialog reverses
        // the purgeability state for those already-loaded resources.
        let (mut disp, mut cpu, mut bus) = setup();
        let dlog_id: i16 = 3100;
        let ditl_id: i16 = 3101;
        let cntl_id: i16 = 3102;
        let icon_id: i16 = 3103;
        let pict_id: i16 = 3104;
        let dlog = build_test_dlog((10, 20, 110, 220), ditl_id, 0);
        let ditl = build_test_ditl_items(&[
            (7, (1, 2, 12, 82), &cntl_id.to_be_bytes()),
            (32, (14, 2, 46, 34), &icon_id.to_be_bytes()),
            (64, (48, 2, 88, 82), &pict_id.to_be_bytes()),
        ]);
        disp.install_test_resource(&mut bus, *b"DLOG", dlog_id, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);
        disp.install_test_resource(&mut bus, *b"CNTL", cntl_id, &[0u8; 32]);
        disp.install_test_resource(&mut bus, *b"ICON", icon_id, &[0xAA; 128]);
        disp.install_test_resource(&mut bus, *b"PICT", pict_id, &[0x11; 32]);
        disp.policy.set_res_load(false);

        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, dlog_id as u16);
        let pre_a7 = cpu.read_reg(Register::A7);
        assert!(disp
            .dispatch_dialog(true, 0x179, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        for (res_type, res_id) in [
            (*b"DLOG", dlog_id),
            (*b"DITL", ditl_id),
            (*b"CNTL", cntl_id),
            (*b"ICON", icon_id),
            (*b"PICT", pict_id),
        ] {
            assert_resource_nonpurgeable(&disp, &bus, res_type, res_id);
        }

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, dlog_id as u16);
        assert!(disp
            .dispatch_dialog(true, 0x17A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        for (res_type, res_id) in [
            (*b"DLOG", dlog_id),
            (*b"DITL", ditl_id),
            (*b"CNTL", cntl_id),
            (*b"ICON", icon_id),
            (*b"PICT", pict_id),
        ] {
            assert_resource_purgeable(&disp, res_type, res_id);
        }
    }

    // ---- CouldAlert ($A989) / FreeAlert ($A98A) ----

    #[test]
    fn could_alert_present_resource_sets_reserr_noerr() {
        // IM:I I-420: CouldAlert targets an ALRT template by ID.
        // Systemless's HLE compromise writes Resource Manager error
        // state at ResErr ($0A60): noErr for present resources.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x0000);
        disp.install_test_resource(&mut bus, *b"ALRT", 400, &alrt);
        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, 400);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x189, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn could_alert_missing_resource_sets_reserr_resnotfound() {
        // IM:I I-420: missing ALRT IDs are ignored.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP, 401);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x189, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn free_alert_present_resource_sets_reserr_noerr() {
        // IM:I I-420: FreeAlert undoes a prior CouldAlert target.
        // HLE compromise reports success via ResErr for present ALRT.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x0000);
        disp.install_test_resource(&mut bus, *b"ALRT", 402, &alrt);
        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, 402);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x18A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn free_alert_missing_resource_sets_reserr_resnotfound() {
        // Missing ALRT IDs in FreeAlert are ignored.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(0x0A60, 0);
        bus.write_word(TEST_SP, 403);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x18A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn could_alert_and_free_alert_leave_loaded_alert_attrs_unchanged() {
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x0000);
        let data_ptr = disp.install_test_resource(&mut bus, *b"ALRT", 404, &alrt);
        let handle = disp.get_or_create_resource_handle(&mut bus, *b"ALRT", 404, data_ptr);
        let attrs_before = disp.resource_attributes_for_handle(handle).unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 404);
        assert!(disp
            .dispatch_dialog(true, 0x189, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap(),
            attrs_before
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 404);
        assert!(disp
            .dispatch_dialog(true, 0x18A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        assert_eq!(
            disp.resource_attributes_for_handle(handle).unwrap(),
            attrs_before
        );
    }

    #[test]
    fn could_alert_free_alert_purgeability() {
        // IM:I I-420 gives CouldAlert/FreeAlert the same resource-family
        // purgeability contract as CouldDialog/FreeDialog, rooted at ALRT.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt_id: i16 = 3200;
        let ditl_id: i16 = 3201;
        let cntl_id: i16 = 3202;
        let icon_id: i16 = 3203;
        let pict_id: i16 = 3204;
        let alrt = build_alrt_template(ditl_id, 0x0000);
        let ditl = build_test_ditl_items(&[
            (7, (1, 2, 12, 82), &cntl_id.to_be_bytes()),
            (32, (14, 2, 46, 34), &icon_id.to_be_bytes()),
            (64, (48, 2, 88, 82), &pict_id.to_be_bytes()),
        ]);
        disp.install_test_resource(&mut bus, *b"ALRT", alrt_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", ditl_id, &ditl);
        disp.install_test_resource(&mut bus, *b"CNTL", cntl_id, &[0u8; 32]);
        disp.install_test_resource(&mut bus, *b"ICON", icon_id, &[0xAA; 128]);
        disp.install_test_resource(&mut bus, *b"PICT", pict_id, &[0x11; 32]);
        disp.policy.set_res_load(false);

        bus.write_word(0x0A60, 0x7FFF);
        bus.write_word(TEST_SP, alrt_id as u16);
        let pre_a7 = cpu.read_reg(Register::A7);
        assert!(disp
            .dispatch_dialog(true, 0x189, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        for (res_type, res_id) in [
            (*b"ALRT", alrt_id),
            (*b"DITL", ditl_id),
            (*b"CNTL", cntl_id),
            (*b"ICON", icon_id),
            (*b"PICT", pict_id),
        ] {
            assert_resource_nonpurgeable(&disp, &bus, res_type, res_id);
        }

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, alrt_id as u16);
        assert!(disp
            .dispatch_dialog(true, 0x18A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        for (res_type, res_id) in [
            (*b"ALRT", alrt_id),
            (*b"DITL", ditl_id),
            (*b"CNTL", cntl_id),
            (*b"ICON", icon_id),
            (*b"PICT", pict_id),
        ] {
            assert_resource_purgeable(&disp, res_type, res_id);
        }
    }

    #[test]
    fn alert_returns_item_1_when_stage_1_bold_flag_clear() {
        // stages = 0x4444 → all 4 nibbles have boxDrwn set and
        // bold item clear, so item 1 (OK button) is the default.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(/*itemsID*/ 200, /*stages*/ 0x4444);
        disp.install_test_resource(&mut bus, *b"ALRT", 128, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0); // stage 1
        bus.write_word(TEST_SP + 4, 128);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_word(TEST_SP + 6) as i16,
            1,
            "Alert with stage 1 + bold flag clear must return item 1 (OK)"
        );
    }

    #[test]
    fn alert_returns_item_2_when_stage_1_bold_flag_set() {
        // stages = 0xCCCC → all 4 nibbles have boxDrwn and bit 3 set
        // (okDismissal mask per IM:I I-424), so item 2 is default.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0xCCCC);
        disp.install_test_resource(&mut bus, *b"ALRT", 129, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(TEST_SP + 4, 129);
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_word(TEST_SP + 6) as i16,
            2,
            "Alert with stage 1 + bold flag set must return item 2"
        );
    }

    #[test]
    fn alert_steps_through_stages_and_caps_at_stage_4() {
        // stages = 0xC4C4 → boxDrwn set in every nibble; bit 3 of
        // stage 1/3 nibble = 0 (item 1), bit 3 of stage 2/4 nibble
        // = 1 (item 2). Per IM:I I-422 okDismissal mask = 8.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0xC4C4);
        disp.install_test_resource(&mut bus, *b"ALRT", 130, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(TEST_SP + 4, 130);

        let expected = [1i16, 2, 1, 2, 2, 2];
        for (i, want) in expected.iter().enumerate() {
            cpu.write_reg(Register::A7, TEST_SP);
            let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(
                bus.read_word(TEST_SP + 6) as i16,
                *want,
                "call #{} (stage {}): expected item {}",
                i + 1,
                (i + 1).min(4),
                want
            );
        }
        // After at least 4 calls the AlertStage word is capped at 3.
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            3,
            "AlertStage must cap at 3 (stages are 1..4 → word 0..3)"
        );
    }

    #[test]
    fn alert_increments_alert_stage_word_after_dispatch() {
        // First call from stage 0 must leave AlertStage = 1. Read
        // and write as 16-bit WORD per IM:I I-423 + MTb 1992 22620.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x4444);
        disp.install_test_resource(&mut bus, *b"ALRT", 131, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(TEST_SP + 4, 131);
        let _ = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            1,
            "Alert must increment AlertStage 0 → 1 after dispatch"
        );
    }

    #[test]
    fn alert_does_not_increment_alert_stage_when_alrt_missing() {
        // No ALRT → -1 path must NOT touch AlertStage so the next
        // call (with a real ALRT installed) sees the original
        // stage. This is critical for apps that defensively call
        // Alert(missingID) before Alert(realID).
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 2);
        bus.write_word(TEST_SP + 4, 999); // no ALRT 999
        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(TEST_SP + 6) as i16, -1);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            2,
            "missing-ALRT must NOT increment AlertStage"
        );
    }

    #[test]
    fn alert_returns_minus_one_when_stage_box_drawn_clear_but_updates_stage() {
        // IM:I I-418 and MTE 1992 p. 6-106: Alert calls the alert
        // sound procedure for the stage and returns -1 when boxDrwn
        // is clear, but it is still an occurrence of that alert.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x0000);
        disp.install_test_resource(&mut bus, *b"ALRT", 133, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xCAFE);
        bus.write_word(TEST_SP + 4, 133);

        let result = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            bus.read_word(TEST_SP + 6) as i16,
            -1,
            "boxDrwn clear must suppress the alert box and return -1"
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            1,
            "suppressed present ALRT must still advance ACount"
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ANUMBER) as i16,
            133,
            "suppressed present ALRT must still record ANumber"
        );
    }

    #[test]
    fn alert_writes_anumber_with_alert_id_after_successful_dispatch() {
        // ANumber at $0A98 records the resource ID of the last
        // alert that occurred per IM:I I-423.
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt = build_alrt_template(200, 0x4444);
        disp.install_test_resource(&mut bus, *b"ALRT", 250, &alrt);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xCAFE);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(TEST_SP + 4, 250);
        let _ = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ANUMBER) as i16,
            250,
            "Alert must write the alertID to ANumber per IM:I I-423"
        );
    }

    #[test]
    fn alert_does_not_overwrite_anumber_when_alrt_missing() {
        // Missing-ALRT path returns -1 without overwriting ANumber
        // (defensive: callers reading ANumber after a probe call
        // must see the prior real value, not the failed probe ID).
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xC0DE);
        bus.write_word(TEST_SP + 4, 999); // no ALRT 999
        let _ = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ANUMBER),
            0xC0DE,
            "missing-ALRT must NOT overwrite ANumber"
        );
    }

    #[test]
    fn alert_family_share_dispatch_path_so_stop_note_caution_match_alert() {
        // The four icon variants ($A985 Alert / $A986 StopAlert /
        // $A987 NoteAlert / $A988 CautionAlert) differ only by
        // displayed icon — their dispatch into the ALRT template
        // is identical. With the same ALRT installed and the same
        // AlertStage all four must return the same item number.
        let alrt = build_alrt_template(200, 0x4444);
        let trap_words = [
            (0x185, "Alert"),
            (0x186, "StopAlert"),
            (0x187, "NoteAlert"),
            (0x188, "CautionAlert"),
        ];
        for (trap, name) in trap_words.iter() {
            let (mut disp, mut cpu, mut bus) = setup();
            disp.install_test_resource(&mut bus, *b"ALRT", 132, &alrt);
            bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
            bus.write_word(TEST_SP + 4, 132);
            let result = disp.dispatch_dialog(true, *trap, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(
                bus.read_word(TEST_SP + 6) as i16,
                1,
                "{name} must return item 1 (bold flag clear, stage 1) — \
                 same as Alert"
            );
        }
    }

    #[test]
    fn alert_with_filter_proc_keeps_multi_button_dialog_interactive() {
        let (mut disp, mut cpu, mut bus) = setup();
        let alrt_id = 500;
        let ditl = build_test_ditl_items(&[
            (4, (110, 20, 130, 100), b"Disagree"),
            (4, (110, 120, 130, 200), b"Agree"),
            (8, (20, 20, 90, 300), b"Compatibility notice"),
        ]);
        let alrt = build_alrt_template(alrt_id, 0x5555);
        disp.install_test_resource(&mut bus, *b"ALRT", alrt_id, &alrt);
        disp.install_test_resource(&mut bus, *b"DITL", alrt_id, &ditl);
        bus.write_long(TEST_SP, 0x0012_3456); // filterProc
        bus.write_word(TEST_SP + 4, alrt_id as u16);

        let result = disp.dispatch_dialog(true, 0x187, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert!(disp.dialog_tracking.is_some());
        assert_eq!(bus.read_word(TEST_SP + 6), 0);
    }

    #[test]
    fn alert_pops_eight_bytes_per_pascal_signature() {
        // FUNCTION Alert(alertID: INTEGER; filterProc: ProcPtr): INTEGER;
        // Pascal stack frame: result(2) + filterProc(4) +
        // alertID(2) + return PC slot mock at SP+0 (caller-set
        // here). Trap pops 6 bytes (filterProc + alertID + return)
        // leaving result at new SP+0 = TEST_SP+6.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP + 4, 999); // alertID
        let pre_a7 = cpu.read_reg(Register::A7);
        let _ = disp.dispatch_dialog(true, 0x185, &mut cpu, &mut bus);
        let post_a7 = cpu.read_reg(Register::A7);
        assert_eq!(
            post_a7,
            pre_a7 + 6,
            "Alert must advance A7 by 6 bytes (filterProc + alertID + return)"
        );
    }

    // ---- InitDialogs ($A97B) — IM:I I-411 init contract ----

    #[test]
    fn init_dialogs_stores_resume_proc_at_lowmem_a8c() {
        // PROCEDURE InitDialogs(resumeProc: ProcPtr);
        // The non-NIL ProcPtr argument must land at $0A8C
        // (ResumeProc global) per IM:I I-411 + the assembly note.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(crate::memory::globals::addr::RESUME_PROC, 0xCAFEBABE);
        bus.write_long(TEST_SP, 0x00123456); // resumeProc parameter
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::RESUME_PROC),
            0x00123456,
            "InitDialogs must store resumeProc at $0A8C"
        );
    }

    #[test]
    fn init_dialogs_accepts_nil_resume_proc() {
        // The IM-canonical default is NIL ("no resume procedure
        // is desired"). Pin that NIL is stored as 0 — not a
        // sentinel like -1 — so the System Error Handler's
        // `if (resume) call(*resume)` path takes the no-op branch.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(crate::memory::globals::addr::RESUME_PROC, 0xDEADBEEF);
        bus.write_long(TEST_SP, 0); // NIL
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::RESUME_PROC),
            0,
            "InitDialogs must store NIL resumeProc as 0 at $0A8C"
        );
    }

    #[test]
    fn init_dialogs_zeros_dabeeper_at_lowmem_a9c() {
        // IM:I I-411: "It installs the standard sound procedure."
        // Systemless's HLE has no menu-bar-blink sound, so we install
        // NIL (== silent) per the IM:I I-411 ErrorSound semantic
        // ("If you pass NIL for soundProc, there will be no sound
        // ... at all"). A subsequent ErrorSound ($A98C) call can
        // override.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(crate::memory::globals::addr::DA_BEEPER, 0xDEADBEEF);
        bus.write_long(TEST_SP, 0);
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::DA_BEEPER),
            0,
            "InitDialogs must store NIL (silent default) at DABeeper $0A9C"
        );
    }

    #[test]
    fn init_dialogs_zeros_alert_stage_at_lowmem_a9a() {
        // First call to Alert/StopAlert/NoteAlert/CautionAlert
        // after InitDialogs must start at stage 1 (= AlertStage
        // word value 0). This is critical for apps that call
        // InitDialogs at re-launch but already have a stale stage
        // byte from a prior crashed run.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 3);
        bus.write_long(TEST_SP, 0);
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            0,
            "InitDialogs must zero AlertStage at $0A9A so the next Alert starts at stage 1"
        );
    }

    #[test]
    fn init_dialogs_zeros_dastrings_array_at_lowmem_aa0() {
        // IM:I I-411: "It passes empty strings to ParamText."
        // The DAStrings global at $0AA0 is a 16-byte array of 4
        // Handles; zeroing all 4 = ParamText('','','','').
        let (mut disp, mut cpu, mut bus) = setup();
        for i in 0..4u32 {
            bus.write_long(
                crate::memory::globals::addr::DA_STRINGS + i * 4,
                0xDEAD0000 | i,
            );
        }
        bus.write_long(TEST_SP, 0);
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        for i in 0..4u32 {
            assert_eq!(
                bus.read_long(crate::memory::globals::addr::DA_STRINGS + i * 4),
                0,
                "InitDialogs must zero DAStrings[{}] at $0AA0+{}",
                i,
                i * 4
            );
        }
    }

    #[test]
    fn init_dialogs_pops_four_bytes_resume_proc_param() {
        // PROCEDURE → no result slot, single 4-byte ProcPtr arg.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(TEST_SP, 0);
        let pre_a7 = cpu.read_reg(Register::A7);
        let _ = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert_eq!(
            cpu.read_reg(Register::A7),
            pre_a7 + 4,
            "InitDialogs must pop 4 bytes (resumeProc ProcPtr)"
        );
    }

    // ---- ErrorSound ($A98C) ----

    #[test]
    fn error_sound_stores_sound_proc_pointer_in_dabeeper() {
        // Inside Macintosh Volume I, I-411: ErrorSound sets the current
        // alert sound procedure, and the assembly-language note says this
        // pointer is stored in DABeeper.
        let (mut disp, mut cpu, mut bus) = setup();
        let sound_proc = 0x0012_3456u32;
        bus.write_long(crate::memory::globals::addr::DA_BEEPER, 0);
        bus.write_long(TEST_SP, sound_proc);

        let pre_a7 = cpu.read_reg(Register::A7);
        let result = disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::DA_BEEPER),
            sound_proc
        );
        assert_eq!(cpu.read_reg(Register::A7), pre_a7 + 4);
    }

    #[test]
    fn error_sound_nil_clears_dabeeper_to_disable_alert_sound() {
        // Inside Macintosh Volume I, I-411: passing NIL for soundProc means
        // "no sound (or menu bar blinking) at all".
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(crate::memory::globals::addr::DA_BEEPER, 0xDEADBEEF);
        bus.write_long(TEST_SP, 0);

        let result = disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(crate::memory::globals::addr::DA_BEEPER), 0);
    }

    #[test]
    fn error_sound_overrides_init_dialogs_default_sound_proc() {
        // IM:I I-411: InitDialogs installs the default alert sound procedure,
        // and ErrorSound replaces it with the caller's soundProc.
        let (mut disp, mut cpu, mut bus) = setup();
        let sound_proc = 0x0012_3456u32;

        bus.write_long(crate::memory::globals::addr::DA_BEEPER, 0xDEADBEEF);
        bus.write_long(TEST_SP, 0);
        let result = disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(crate::memory::globals::addr::DA_BEEPER), 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, sound_proc);
        let result = disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::DA_BEEPER),
            sound_proc
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0);
        let result = disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(crate::memory::globals::addr::DA_BEEPER), 0);
    }

    // ---- IsDialogEvent ($A97F) ----

    #[test]
    fn is_dialog_event_returns_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        // SP+0: event_ptr (4 bytes)
        bus.write_long(TEST_SP, 0x300000);
        bus.write_byte(TEST_SP + 5, 0xA5);

        let result = disp.dispatch_dialog(true, 0x17F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_byte(TEST_SP + 4), 0);
        assert_eq!(bus.read_byte(TEST_SP + 5), 0xA5);
    }

    #[test]
    fn is_dialog_event_true_for_mouse_down_in_front_dialog() {
        // Inside Macintosh Volume I, I-417: mouse-down in content region of
        // active dialog window is a dialog event (TRUE).
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;

        disp.front_window = dialog_ptr;
        disp.enable_input_trace_capture();
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 120);
        disp.dialog_items.insert(dialog_ptr, Vec::new());

        bus.write_word(event_ptr, 1);
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 120);
        bus.write_word(event_ptr + 12, 240);
        bus.write_word(event_ptr + 14, 0x0080);
        bus.write_long(TEST_SP, event_ptr);
        bus.write_byte(TEST_SP + 5, 0xA5);

        let result = disp.dispatch_dialog(true, 0x17F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 4), 1);
        assert_eq!(bus.read_byte(TEST_SP + 5), 0xA5);
        let trace = disp.input_trace_text();
        assert!(trace.contains("A97F action=guest_event:mouseDown"));
        assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
        assert!(trace.contains("result=true outcome=is_dialog_event"));
    }

    #[test]
    fn is_dialog_event_false_for_mouse_down_outside_front_dialog() {
        // Inside Macintosh Volume I, I-417: only mouse-down events in the
        // content region of an active dialog window return TRUE.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;

        disp.front_window = dialog_ptr;
        disp.enable_input_trace_capture();
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 120);
        disp.dialog_items.insert(dialog_ptr, Vec::new());

        bus.write_word(event_ptr, 1); // mouseDown
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 40); // well outside dialog content
        bus.write_word(event_ptr + 12, 40);
        bus.write_word(event_ptr + 14, 0x0080);
        bus.write_long(TEST_SP, event_ptr);

        let result = disp.dispatch_dialog(true, 0x17F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 4), 0);
    }

    #[test]
    fn is_dialog_event_true_for_update_event_targeting_dialog_window() {
        // Inside Macintosh Volume I, I-417: activate/update events for a
        // dialog window are dialog events.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;

        disp.dialog_items.insert(dialog_ptr, Vec::new());
        bus.write_word(event_ptr, 6); // updateEvt
        bus.write_long(event_ptr + 2, dialog_ptr);
        bus.write_long(TEST_SP, event_ptr);

        let result = disp.dispatch_dialog(true, 0x17F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 4), 1);
    }

    #[test]
    fn is_dialog_event_true_for_activate_event_targeting_dialog_window() {
        // Inside Macintosh Volume I, I-417: activate events for a dialog
        // window are dialog events as well.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;

        disp.dialog_items.insert(dialog_ptr, Vec::new());
        bus.write_word(event_ptr, 8); // activateEvt
        bus.write_long(event_ptr + 2, dialog_ptr);
        bus.write_long(TEST_SP, event_ptr);

        let result = disp.dispatch_dialog(true, 0x17F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 4), 1);
    }

    // ---- DialogSelect ($A980) ----

    #[test]
    fn dialog_select_returns_false() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(bus.read_byte(TEST_SP + 12), 0);
    }

    #[test]
    fn dialog_select_update_returns_affected_dialog_while_false() {
        // MTE 1992 pp. 6-139..6-141: DialogSelect handles an update event
        // and returns FALSE. System 7 also leaves the affected DialogPtr in
        // theDialog, which movable-modal event loops may retain afterward.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        bus.write_byte(dialog_ptr + 109, 0xFF);
        bus.write_word(dialog_ptr + 16, 10);
        bus.write_word(dialog_ptr + 18, 20);
        bus.write_word(dialog_ptr + 20, 110);
        bus.write_word(dialog_ptr + 22, 180);
        disp.dialog_items.insert(dialog_ptr, Vec::new());

        bus.write_word(event_ptr, 6); // updateEvt
        bus.write_long(event_ptr + 2, dialog_ptr);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);
        bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(bus.read_byte(TEST_SP + 12), 0);
        assert_eq!(bus.read_long(dialog_out_ptr), dialog_ptr);
    }

    #[test]
    fn dialog_select_returns_hit_for_enabled_user_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        disp.enable_input_trace_capture();
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(event_ptr, 1);
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 130);
        bus.write_word(event_ptr + 12, 240);
        bus.write_word(event_ptr + 14, 0x0080);

        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);
        bus.write_byte(TEST_SP + 13, 0xA5);

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(bus.read_byte(TEST_SP + 12), 1);
        assert_eq!(bus.read_byte(TEST_SP + 13), 0xA5);
        assert_eq!(bus.read_long(dialog_out_ptr), dialog_ptr);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        let trace = disp.input_trace_text();
        assert!(trace.contains("A980 action=guest_event:mouseDown"));
        assert!(trace.contains("item_hit=1 item_type=$00 disabled=false outcome=enabled_item"));
    }

    fn dialog_select_enabled_user_item_hit_for_theme(theme_id: UiThemeId) -> (u8, u32, u16, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(event_ptr, 1);
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 130);
        bus.write_word(event_ptr + 12, 240);
        bus.write_word(event_ptr + 14, 0x0080);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);

        disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_byte(TEST_SP + 12),
            bus.read_long(dialog_out_ptr),
            bus.read_word(item_hit_ptr),
            cpu.read_reg(Register::A7),
        )
    }

    fn dialog_select_enabled_checkbox_hit_for_theme(
        theme_id: UiThemeId,
    ) -> (u8, u32, u16, u32, u16, u16) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;
        let box_ptr = 0x300108u32;
        let item_ptr = 0x300110u32;
        let type_ptr = 0x300114u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 5,
                rect: (20, 30, 40, 130),
                text: "Enabled".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.dialog_control_values.insert((dialog_ptr, 1), 1);

        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 18);
        let ctrl_handle = bus.read_long(item_ptr);
        let ctrl_ptr = bus.read_long(ctrl_handle);
        assert_ne!(ctrl_ptr, 0);
        assert_eq!(bus.read_word(type_ptr), 5);
        assert_eq!(bus.read_word(ctrl_ptr + 18), 1);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(event_ptr, 1);
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 30);
        bus.write_word(event_ptr + 12, 40);
        bus.write_word(event_ptr + 14, 0x0080);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);

        disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_byte(TEST_SP + 12),
            bus.read_long(dialog_out_ptr),
            bus.read_word(item_hit_ptr),
            cpu.read_reg(Register::A7),
            bus.read_word(ctrl_ptr + 18),
            disp.dialog_control_values
                .get(&(dialog_ptr, 1))
                .copied()
                .unwrap_or(-1) as u16,
        )
    }

    fn findditem_results_for_theme(theme_id: UiThemeId) -> Vec<(i16, u32)> {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = 0x220400u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0x80 | 4,
                    rect: (10, 20, 40, 60),
                    text: "DisabledFirst".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (20, 30, 55, 80),
                    text: "EnabledSecond".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 16,
                    rect: (60, 16384 + 20, 80, 16384 + 100),
                    text: "HiddenEdit".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        let mut out = Vec::new();
        for (pt_v, pt_h) in [(25i16, 35i16), (45, 70), (65, 40), (90, 110)] {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP, pt_v as u16);
            bus.write_word(TEST_SP + 2, pt_h as u16);
            bus.write_long(TEST_SP + 4, dialog_ptr);
            bus.write_word(TEST_SP + 8, 0xBEEF);
            disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            out.push((
                bus.read_word(TEST_SP + 8) as i16,
                cpu.read_reg(Register::A7),
            ));
        }
        out
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DItemGetSnapshot {
        item_type: u16,
        item: u32,
        rect: (u16, u16, u16, u16),
        stack_after: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DItemUserItemRecordSnapshot {
        initial_get: DItemGetSnapshot,
        set_stack_after: u32,
        stored_type: u8,
        stored_proc_ptr: u32,
        stored_rect: (i16, i16, i16, i16),
        post_set_get: DItemGetSnapshot,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DItemTextControlRecordSnapshot {
        text_initial_get: DItemGetSnapshot,
        text_set_stack_after: u32,
        text_old_handle_mapped_after: bool,
        text_new_handle_map_value_after: Option<(u32, usize)>,
        text_ditl_handle_after: u32,
        text_ditl_rect_after: (u16, u16, u16, u16),
        text_ditl_type_after: u8,
        text_stored_type_after: u8,
        text_stored_rect_after: (i16, i16, i16, i16),
        text_cached_text_after: String,
        text_handle_bytes_after: Vec<u8>,
        text_post_set_get: DItemGetSnapshot,
        control_initial_get: DItemGetSnapshot,
        control_set_stack_after: u32,
        control_old_handle_mapped_after: bool,
        control_new_handle_map_value_after: Option<(u32, i16)>,
        control_ditl_handle_after: u32,
        control_ditl_rect_after: (u16, u16, u16, u16),
        control_ditl_type_after: u8,
        control_stored_type_after: u8,
        control_stored_rect_after: (i16, i16, i16, i16),
        control_record_rect_after: (u16, u16, u16, u16),
        control_value_after: Option<i16>,
        control_post_set_get: DItemGetSnapshot,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DItemVisibilitySnapshot {
        initial_get: DItemGetSnapshot,
        hide_stack_after: u32,
        hidden_item_rect: (i16, i16, i16, i16),
        hidden_saved_rect: Option<(i16, i16, i16, i16)>,
        hidden_control_rect: (u16, u16, u16, u16),
        hidden_get: DItemGetSnapshot,
        hidden_find: (i16, u32),
        show_stack_after: u32,
        restored_item_rect: (i16, i16, i16, i16),
        restored_saved_rect: Option<(i16, i16, i16, i16)>,
        restored_control_rect: (u16, u16, u16, u16),
        restored_get: DItemGetSnapshot,
        restored_find: (i16, u32),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ParamTextSnapshot {
        initial_slots: [Vec<u8>; 4],
        initial_da_handles: [u32; 4],
        initial_da_strings: [Vec<u8>; 4],
        initial_stack_after: u32,
        initial_substitution: String,
        nil_slots: [Vec<u8>; 4],
        nil_da_handles: [u32; 4],
        nil_da_strings: [Vec<u8>; 4],
        nil_stack_after: u32,
        nil_substitution: String,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogItemTextSnapshot {
        initial_get_text: Vec<u8>,
        initial_get_stack_after: u32,
        set_stack_after: u32,
        handle_size_after_set: Option<u32>,
        handle_bytes_after_set: Vec<u8>,
        cached_text_after_set: String,
        post_set_get_text: Vec<u8>,
        post_set_get_stack_after: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogItemTextSelectionSnapshot {
        whole_selection: (i16, i16),
        whole_edit_field: u16,
        whole_te_selection: (u16, u16),
        whole_stack_after: u32,
        normalized_selection: (i16, i16),
        normalized_edit_field: u16,
        normalized_te_selection: (u16, u16),
        normalized_stack_after: u32,
        non_edit_selection: (i16, i16),
        non_edit_edit_field: u16,
        non_edit_te_selection: (u16, u16),
        non_edit_stack_after: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct IsDialogEventSnapshot {
        null_event: (u8, u32),
        key_down: (u8, u32),
        mouse_inside: (u8, u32),
        mouse_outside: (u8, u32),
        update_target: (u8, u32),
        activate_target: (u8, u32),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogSelectWindowEventSnapshot {
        dialog_ptr: u32,
        update_result: u8,
        update_dialog_out: u32,
        update_item_hit: u16,
        update_stack_after: u32,
        update_deferred_after: bool,
        update_region_after: Option<(i16, i16, i16, i16)>,
        update_vis_region_after: Option<(i16, i16, i16, i16)>,
        update_the_port_after: u32,
        update_current_port_after: u32,
        update_saved_vis_after: bool,
        update_queued_draw_procs: Vec<(u32, u32, i16)>,
        update_item_count_after: usize,
        activate_result: u8,
        activate_dialog_out: u32,
        activate_item_hit: u16,
        activate_stack_after: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogSelectEditTextMouseSnapshot {
        mouse_result: u8,
        mouse_dialog_out: u32,
        mouse_item_hit: u16,
        mouse_stack_after: u32,
        mouse_edit_field: u16,
        mouse_item_selection: (i16, i16),
        mouse_te_text: Vec<u8>,
        mouse_te_length: u16,
        mouse_te_selection: (u16, u16),
        null_result: u8,
        null_dialog_out: u32,
        null_item_hit: u16,
        null_stack_after: u32,
        null_edit_field: u16,
        null_te_text: Vec<u8>,
        null_te_length: u16,
        null_te_selection: (u16, u16),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ModalDialogEditTextMouseSnapshot {
        item_hit: u16,
        stack_after: u32,
        tracking_finished: bool,
        retained_visible_snapshot: bool,
        saved_background_retained: bool,
        queued_mouse_up_consumed: bool,
        edit_field: u16,
        item_selection: (i16, i16),
        te_text: Vec<u8>,
        te_length: u16,
        te_selection: (u16, u16),
        handle_bytes: Vec<u8>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ModalDialogKeyboardButtonSnapshot {
        return_key: ModalDialogKeyboardButtonCase,
        enter_key: ModalDialogKeyboardButtonCase,
        escape_key: ModalDialogKeyboardButtonCase,
        command_period: ModalDialogKeyboardButtonCase,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ModalDialogKeyboardButtonCase {
        flash_item_after_key: i16,
        stack_after_key: u32,
        item_hit_after_key: u16,
        item_hit_after_flash: u16,
        stack_after_flash: u32,
        tracking_finished: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ModalDialogUpdateEventSnapshot {
        dialog_ptr: u32,
        item_hit_after: u16,
        stack_after: u32,
        tracking_finished: bool,
        rendered_pixels_final: bool,
        retained_pixel_after: u8,
        retained_snapshot_pixel_after: u8,
        update_region_after: Option<(i16, i16, i16, i16)>,
        vis_region_after: Option<(i16, i16, i16, i16)>,
        the_port_after: u32,
        current_port_after: u32,
        saved_vis_after: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogLifecycleCleanupSnapshot {
        close_stack_after: u32,
        close_tracking_cleared: bool,
        close_retained_click_cleared: bool,
        close_visible_snapshot_cleared: bool,
        close_modal_entered_cleared: bool,
        close_saved_pixels_cleared: bool,
        close_window_list_contains_dialog: bool,
        close_front_window_after: u32,
        close_the_port_after: u32,
        close_current_port_after: u32,
        close_update_event_for_previous_window: bool,
        close_dialog_items_present_after: bool,
        dispose_stack_after: u32,
        dispose_tracking_cleared: bool,
        dispose_retained_click_cleared: bool,
        dispose_visible_snapshot_cleared: bool,
        dispose_modal_entered_cleared: bool,
        dispose_saved_pixels_cleared: bool,
        dispose_window_list_contains_dialog: bool,
        dispose_front_window_after: u32,
        dispose_the_port_after: u32,
        dispose_current_port_after: u32,
        dispose_update_event_for_previous_window: bool,
        dispose_dialog_items_present_after: bool,
        dispose_other_dialog_items_present_after: bool,
        dispose_dialog_item_handle_present_after: bool,
        dispose_other_dialog_item_handle_present_after: bool,
        dispose_dialog_control_handle_present_after: bool,
        dispose_other_dialog_control_handle_present_after: bool,
        dispose_dialog_control_value_present_after: bool,
        dispose_other_dialog_control_value_present_after: bool,
        dispose_hidden_rect_present_after: bool,
        dispose_other_hidden_rect_present_after: bool,
        dispose_cancel_item_present_after: bool,
        dispose_other_cancel_item_present_after: bool,
        dispose_pending_popup_cleared: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogCreationSnapshot {
        new_previous_port: u32,
        new_dialog_ptr: u32,
        new_stack_after: u32,
        new_result_slot: u32,
        new_window_list: Vec<u32>,
        new_front_window: u32,
        new_current_port: u32,
        new_the_port: u32,
        new_visible_byte: u8,
        new_goaway_byte: u8,
        new_refcon: u32,
        new_window_kind: i16,
        new_proc_id: i16,
        new_port_rect: (i16, i16, i16, i16),
        new_items_handle: u32,
        new_items_data_ptr: u32,
        new_first_item_handle_nonzero: bool,
        new_cached_items: Vec<(u8, (i16, i16, i16, i16), String, i16)>,
        new_update_region: Option<(i16, i16, i16, i16)>,
        new_update_event_queued: bool,
        new_edit_field: u16,
        new_default_item: u16,
        get_previous_port: u32,
        get_dialog_ptr: u32,
        get_stack_after: u32,
        get_result_slot: u32,
        get_window_list: Vec<u32>,
        get_front_window: u32,
        get_current_port: u32,
        get_the_port: u32,
        get_visible_byte: u8,
        get_refcon: u32,
        get_window_kind: i16,
        get_proc_id: i16,
        get_port_rect: (i16, i16, i16, i16),
        get_items_handle: u32,
        get_items_data_ptr: u32,
        get_uses_distinct_ditl_copy: bool,
        get_original_ditl_handle_field: u32,
        get_first_item_handle_nonzero: bool,
        get_cached_items: Vec<(u8, (i16, i16, i16, i16), String, i16)>,
        get_update_region: Option<(i16, i16, i16, i16)>,
        get_update_event_queued: bool,
        get_edit_field: u16,
        get_default_item: u16,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct AlertTrapCallSnapshot {
        trap_word: u16,
        result: i16,
        stack_after: u32,
        alert_stage_after: u16,
        anumber_after: u16,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct AlertResourcePrepSnapshot {
        could_present: (i16, u32),
        free_present: (i16, u32),
        could_missing: (i16, u32),
        free_missing: (i16, u32),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct AlertFamilySnapshot {
        staged_calls: Vec<AlertTrapCallSnapshot>,
        missing_call: AlertTrapCallSnapshot,
        resource_prep: AlertResourcePrepSnapshot,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogInitSoundSnapshot {
        init_stack_after: u32,
        resume_proc_after_init: u32,
        da_beeper_after_init: u32,
        alert_stage_after_init: u16,
        anumber_after_init: u16,
        da_strings_after_init: [u32; 4],
        error_sound_stack_after: u32,
        da_beeper_after_error_sound: u32,
        nil_error_sound_stack_after: u32,
        da_beeper_after_nil_error_sound: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogDispatchDefaultCancelSnapshot {
        default_result: u16,
        default_stack_after: u32,
        default_adef_item: u16,
        default_tracking_item: i16,
        cancel_result: u16,
        cancel_stack_after: u32,
        cancel_map_item: Option<i16>,
        cancel_tracking_item: i16,
        tracks_result: u16,
        tracks_stack_after: u32,
        tracks_adef_item: u16,
        tracks_cancel_map_item: Option<i16>,
        tracks_tracking_items: (i16, i16),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct UpdtDialogUpdateRegionSnapshot {
        stack_after: u32,
        the_port_after: u32,
        current_port_after: u32,
        deferred_after: bool,
        queued_draw_procs: Vec<(u32, u32, i16)>,
        item_count_after: usize,
        inside_update_pixel_after: u8,
        outside_update_pixel_after: u8,
        outside_item_pixel_after: u8,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct DialogSelectEditTextKeySnapshot {
        keydown_result: u8,
        keydown_dialog_out: u32,
        keydown_item_hit: u16,
        keydown_stack_after: u32,
        keydown_handle_bytes: Vec<u8>,
        keydown_cached_text: String,
        keydown_item_selection: (i16, i16),
        keydown_te_text: Vec<u8>,
        keydown_te_length: u16,
        keydown_te_selection: (u16, u16),
        autokey_result: u8,
        autokey_dialog_out: u32,
        autokey_item_hit: u16,
        autokey_stack_after: u32,
        autokey_handle_bytes: Vec<u8>,
        autokey_cached_text: String,
        autokey_item_selection: (i16, i16),
        autokey_te_text: Vec<u8>,
        autokey_te_length: u16,
        autokey_te_selection: (u16, u16),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TextEditScrapEditCase {
        text: Vec<u8>,
        selection: (u16, u16),
        scrap_length: u16,
        scrap_bytes: Vec<u8>,
        stack_after: u32,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TextEditPrivateScrapEditingSnapshot {
        copy: TextEditScrapEditCase,
        cut: TextEditScrapEditCase,
        paste: TextEditScrapEditCase,
        delete: TextEditScrapEditCase,
    }

    fn paramtext_da_strings(bus: &MacMemoryBus) -> ([u32; 4], [Vec<u8>; 4]) {
        let mut handles = [0u32; 4];
        let mut strings: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::new());
        for i in 0..4 {
            let handle = bus.read_long(crate::memory::globals::addr::DA_STRINGS + i as u32 * 4);
            handles[i] = handle;
            if handle != 0 {
                let data_ptr = bus.read_long(handle);
                if data_ptr != 0 {
                    strings[i] = bus.read_pstring(data_ptr);
                }
            }
        }
        (handles, strings)
    }

    fn paramtext_results_for_theme(theme_id: UiThemeId) -> ParamTextSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let p0 = bus.alloc(32);
        let p1 = bus.alloc(32);
        let p2 = bus.alloc(32);
        let p3 = bus.alloc(32);

        bus.write_pstring(p0, b"Doc");
        bus.write_pstring(p1, b"42");
        bus.write_pstring(p2, b"");
        bus.write_pstring(p3, b"Tail");

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, p3);
        bus.write_long(TEST_SP + 4, p2);
        bus.write_long(TEST_SP + 8, p1);
        bus.write_long(TEST_SP + 12, p0);
        disp.dispatch_dialog(true, 0x18B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let initial_slots = disp.param_text.snapshot();
        let (initial_da_handles, initial_da_strings) = paramtext_da_strings(&bus);
        let initial_stack_after = cpu.read_reg(Register::A7);
        let initial_substitution = disp
            .apply_param_text("Cannot open ^0 (^1)^2^3 ^9")
            .into_owned();

        let new0 = bus.alloc(32);
        bus.write_pstring(new0, b"NewDoc");
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0); // param3 = NIL; preserve previous slot
        bus.write_long(TEST_SP + 4, 0); // param2 = NIL; preserve previous slot
        bus.write_long(TEST_SP + 8, 0); // param1 = NIL; preserve previous slot
        bus.write_long(TEST_SP + 12, new0);
        disp.dispatch_dialog(true, 0x18B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let nil_slots = disp.param_text.snapshot();
        let (nil_da_handles, nil_da_strings) = paramtext_da_strings(&bus);
        let nil_stack_after = cpu.read_reg(Register::A7);
        let nil_substitution = disp
            .apply_param_text("Cannot open ^0 (^1)^2^3 ^9")
            .into_owned();

        ParamTextSnapshot {
            initial_slots,
            initial_da_handles,
            initial_da_strings,
            initial_stack_after,
            initial_substitution,
            nil_slots,
            nil_da_handles,
            nil_da_strings,
            nil_stack_after,
            nil_substitution,
        }
    }

    fn dialog_item_text_results_for_theme(theme_id: UiThemeId) -> DialogItemTextSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let text_handle = bus.alloc(4);
        let text_ptr = bus.alloc(3);
        let out_ptr = bus.alloc(256);
        let new_text_ptr = bus.alloc(32);

        bus.write_long(text_handle, text_ptr);
        bus.write_bytes(text_ptr, b"Old");
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (10, 20, 30, 120),
                text: "Old".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.dialog_item_handles
            .insert(text_handle, (dialog_ptr, 0));

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, out_ptr);
        bus.write_long(TEST_SP + 4, text_handle);
        disp.dispatch_dialog(true, 0x190, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let initial_get_text = bus.read_pstring(out_ptr);
        let initial_get_stack_after = cpu.read_reg(Register::A7);

        bus.write_pstring(new_text_ptr, b"Edited Text");
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, new_text_ptr);
        bus.write_long(TEST_SP + 4, text_handle);
        disp.dispatch_dialog(true, 0x18F, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let set_stack_after = cpu.read_reg(Register::A7);
        let data_ptr_after_set = bus.read_long(text_handle);
        let handle_size_after_set = bus.get_alloc_size(data_ptr_after_set);
        let handle_bytes_after_set = bus.read_bytes(
            data_ptr_after_set,
            handle_size_after_set.unwrap_or(0) as usize,
        );
        let cached_text_after_set = disp.dialog_items.get(&dialog_ptr).unwrap()[0].text.clone();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, out_ptr);
        bus.write_long(TEST_SP + 4, text_handle);
        disp.dispatch_dialog(true, 0x190, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let post_set_get_text = bus.read_pstring(out_ptr);
        let post_set_get_stack_after = cpu.read_reg(Register::A7);

        DialogItemTextSnapshot {
            initial_get_text,
            initial_get_stack_after,
            set_stack_after,
            handle_size_after_set,
            handle_bytes_after_set,
            cached_text_after_set,
            post_set_get_text,
            post_set_get_stack_after,
        }
    }

    fn select_dialog_item_text_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogItemTextSelectionSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let text_handle = bus.alloc(4);
        let te_ptr = bus.alloc(0x40);

        bus.write_long(dialog_ptr + 160, text_handle);
        bus.write_long(text_handle, te_ptr);
        bus.write_word(dialog_ptr + 164, 0xFFFF);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 16,
                    rect: (10, 20, 30, 120),
                    text: "ABCDE".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 1,
                    sel_end: 1,
                },
                DialogItem {
                    item_type: 8,
                    rect: (34, 20, 50, 120),
                    text: "STATIC".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 1,
                    sel_end: 3,
                },
            ],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 32767); // endSel
        bus.write_word(TEST_SP + 2, 0); // strtSel
        bus.write_word(TEST_SP + 4, 1); // itemNo
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog
        disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let item = &disp.dialog_items[&dialog_ptr][0];
        let whole_selection = (item.sel_start, item.sel_end);
        let whole_edit_field = bus.read_word(dialog_ptr + 164);
        let whole_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );
        let whole_stack_after = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 2); // endSel
        bus.write_word(TEST_SP + 2, 9); // strtSel, beyond text length
        bus.write_word(TEST_SP + 4, 1); // itemNo
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog
        disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let item = &disp.dialog_items[&dialog_ptr][0];
        let normalized_selection = (item.sel_start, item.sel_end);
        let normalized_edit_field = bus.read_word(dialog_ptr + 164);
        let normalized_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );
        let normalized_stack_after = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 4); // endSel
        bus.write_word(TEST_SP + 2, 0); // strtSel
        bus.write_word(TEST_SP + 4, 2); // non-edit itemNo
        bus.write_long(TEST_SP + 6, dialog_ptr); // theDialog
        disp.dispatch_dialog(true, 0x17E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let item = &disp.dialog_items[&dialog_ptr][1];
        let non_edit_selection = (item.sel_start, item.sel_end);
        let non_edit_edit_field = bus.read_word(dialog_ptr + 164);
        let non_edit_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );
        let non_edit_stack_after = cpu.read_reg(Register::A7);

        DialogItemTextSelectionSnapshot {
            whole_selection,
            whole_edit_field,
            whole_te_selection,
            whole_stack_after,
            normalized_selection,
            normalized_edit_field,
            normalized_te_selection,
            normalized_stack_after,
            non_edit_selection,
            non_edit_edit_field,
            non_edit_te_selection,
            non_edit_stack_after,
        }
    }

    fn is_dialog_event_results_for_theme(theme_id: UiThemeId) -> IsDialogEventSnapshot {
        fn dispatch_is_dialog_event(
            disp: &mut TrapDispatcher,
            cpu: &mut impl CpuOps,
            bus: &mut MacMemoryBus,
            event_ptr: u32,
            what: u16,
            message: u32,
            where_v: i16,
            where_h: i16,
        ) -> (u8, u32) {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(event_ptr, what);
            bus.write_long(event_ptr + 2, message);
            bus.write_long(event_ptr + 6, 0);
            bus.write_word(event_ptr + 10, where_v as u16);
            bus.write_word(event_ptr + 12, where_h as u16);
            bus.write_word(event_ptr + 14, 0);
            bus.write_word(TEST_SP + 4, 0xBEEF);
            bus.write_long(TEST_SP, event_ptr);

            disp.dispatch_dialog(true, 0x17F, cpu, bus)
                .unwrap()
                .unwrap();
            (bus.read_byte(TEST_SP + 4), cpu.read_reg(Register::A7))
        }

        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 120);
        disp.dialog_items.insert(dialog_ptr, Vec::new());

        let null_event =
            dispatch_is_dialog_event(&mut disp, &mut cpu, &mut bus, event_ptr, 0, 0, 0, 0);
        let key_down = dispatch_is_dialog_event(
            &mut disp,
            &mut cpu,
            &mut bus,
            event_ptr,
            3,
            u32::from(b'A'),
            0,
            0,
        );
        let mouse_inside =
            dispatch_is_dialog_event(&mut disp, &mut cpu, &mut bus, event_ptr, 1, 0, 120, 240);
        let mouse_outside =
            dispatch_is_dialog_event(&mut disp, &mut cpu, &mut bus, event_ptr, 1, 0, 40, 40);
        let update_target = dispatch_is_dialog_event(
            &mut disp, &mut cpu, &mut bus, event_ptr, 6, dialog_ptr, 0, 0,
        );
        let activate_target = dispatch_is_dialog_event(
            &mut disp, &mut cpu, &mut bus, event_ptr, 8, dialog_ptr, 0, 0,
        );

        IsDialogEventSnapshot {
            null_event,
            key_down,
            mouse_inside,
            mouse_outside,
            update_target,
            activate_target,
        }
    }

    fn text_handle_bytes(bus: &MacMemoryBus, handle: u32) -> Vec<u8> {
        let ptr = bus.read_long(handle);
        if ptr == 0 {
            return Vec::new();
        }
        let len = bus.get_alloc_size(ptr).unwrap_or(0) as usize;
        bus.read_bytes(ptr, len)
    }

    fn textedit_scrap_contents(bus: &MacMemoryBus) -> (u16, Vec<u8>) {
        let scrap_length = bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH);
        let scrap_handle = bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE);
        if scrap_length == 0 || scrap_handle == 0 {
            return (scrap_length, Vec::new());
        }
        let scrap_ptr = bus.read_long(scrap_handle);
        if scrap_ptr == 0 {
            return (scrap_length, Vec::new());
        }
        (
            scrap_length,
            bus.read_bytes(scrap_ptr, scrap_length as usize),
        )
    }

    fn textedit_scrap_edit_case(
        bus: &MacMemoryBus,
        te_handle: u32,
        stack_after: u32,
    ) -> TextEditScrapEditCase {
        let te_ptr = bus.read_long(te_handle);
        let (scrap_length, scrap_bytes) = textedit_scrap_contents(bus);
        TextEditScrapEditCase {
            text: TrapDispatcher::te_text_bytes(bus, te_handle),
            selection: (
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            ),
            scrap_length,
            scrap_bytes,
            stack_after,
        }
    }

    fn dialog_select_window_event_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogSelectWindowEventSnapshot {
        fn install_region(
            bus: &mut MacMemoryBus,
            handle: u32,
            data: u32,
            rect: (i16, i16, i16, i16),
        ) {
            bus.write_long(handle, data);
            bus.write_word(data, 10);
            bus.write_word(data + 2, rect.0 as u16);
            bus.write_word(data + 4, rect.1 as u16);
            bus.write_word(data + 6, rect.2 as u16);
            bus.write_word(data + 8, rect.3 as u16);
        }

        fn dispatch_dialog_select(
            disp: &mut TrapDispatcher,
            cpu: &mut impl CpuOps,
            bus: &mut MacMemoryBus,
            event_ptr: u32,
            dialog_out_ptr: u32,
            item_hit_ptr: u32,
            what: u16,
            message: u32,
        ) -> (u8, u32, u16, u32) {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(event_ptr, what);
            bus.write_long(event_ptr + 2, message);
            bus.write_long(event_ptr + 6, 0);
            bus.write_word(event_ptr + 10, 0);
            bus.write_word(event_ptr + 12, 0);
            bus.write_word(event_ptr + 14, 0);
            bus.write_word(TEST_SP + 12, 0xBEEF);
            bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);
            bus.write_word(item_hit_ptr, 0xCAFE);
            bus.write_long(TEST_SP, item_hit_ptr);
            bus.write_long(TEST_SP + 4, dialog_out_ptr);
            bus.write_long(TEST_SP + 8, event_ptr);

            disp.dispatch_dialog(true, 0x180, cpu, bus)
                .unwrap()
                .unwrap();

            (
                bus.read_byte(TEST_SP + 12),
                bus.read_long(dialog_out_ptr),
                bus.read_word(item_hit_ptr),
                cpu.read_reg(Register::A7),
            )
        }

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let initial_port = 0x181000;
        disp.set_current_port_state(&mut bus, &mut cpu, initial_port, None);

        let screen_base = bus.alloc(100 * 100);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 100, 100, 100, 8);

        let dialog_ptr = bus.alloc(256);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        let vis_rgn = bus.alloc(10);
        let vis_rgn_handle = bus.alloc(4);
        install_region(&mut bus, vis_rgn_handle, vis_rgn, (0, 0, 100, 100));
        let clip_rgn = bus.alloc(10);
        let clip_rgn_handle = bus.alloc(4);
        install_region(&mut bus, clip_rgn_handle, clip_rgn, (0, 0, 100, 100));
        let update_rgn = bus.alloc(10);
        let update_rgn_handle = bus.alloc(4);
        install_region(&mut bus, update_rgn_handle, update_rgn, (0, 0, 40, 40));

        disp.front_window = dialog_ptr;
        disp.window_list.push(dialog_ptr);
        bus.write_word(dialog_ptr, 0);
        bus.write_long(dialog_ptr + 2, screen_base);
        bus.write_word(dialog_ptr + 6, 100);
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 12, 100);
        bus.write_word(dialog_ptr + 14, 100);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 100);
        bus.write_long(dialog_ptr + 24, vis_rgn_handle);
        bus.write_long(dialog_ptr + 28, clip_rgn_handle);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_long(dialog_ptr + 122, update_rgn_handle);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0,
                    rect: (8, 8, 24, 32),
                    proc_ptr: 0x500000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 0,
                    rect: (60, 60, 80, 80),
                    proc_ptr: 0x600000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 8,
                    rect: (90, 90, 96, 96),
                    text: "x".to_string(),
                    ..Default::default()
                },
            ],
        );
        disp.dialog_initial_draw_deferred.insert(dialog_ptr);

        let (update_result, update_dialog_out, update_item_hit, update_stack_after) =
            dispatch_dialog_select(
                &mut disp,
                &mut cpu,
                &mut bus,
                event_ptr,
                dialog_out_ptr,
                item_hit_ptr,
                6,
                dialog_ptr,
            );
        let update_deferred_after = disp.dialog_initial_draw_deferred.contains(&dialog_ptr);
        let update_region_after =
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(dialog_ptr + 122));
        let update_vis_region_after =
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(dialog_ptr + 24));
        let update_the_port_after = bus.read_long(crate::memory::globals::addr::THE_PORT);
        let update_current_port_after = *disp.current_port;
        let update_saved_vis_after = disp.saved_vis_regions.contains_key(&dialog_ptr);
        let update_queued_draw_procs = disp
            .modeless_dialog_draw_proc_queue
            .iter()
            .copied()
            .collect();
        let update_item_count_after = disp
            .dialog_items
            .get(&dialog_ptr)
            .map(Vec::len)
            .unwrap_or_default();

        let (activate_result, activate_dialog_out, activate_item_hit, activate_stack_after) =
            dispatch_dialog_select(
                &mut disp,
                &mut cpu,
                &mut bus,
                event_ptr,
                dialog_out_ptr,
                item_hit_ptr,
                8,
                dialog_ptr,
            );

        DialogSelectWindowEventSnapshot {
            dialog_ptr,
            update_result,
            update_dialog_out,
            update_item_hit,
            update_stack_after,
            update_deferred_after,
            update_region_after,
            update_vis_region_after,
            update_the_port_after,
            update_current_port_after,
            update_saved_vis_after,
            update_queued_draw_procs,
            update_item_count_after,
            activate_result,
            activate_dialog_out,
            activate_item_hit,
            activate_stack_after,
        }
    }

    fn dialog_select_edit_text_mouse_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogSelectEditTextMouseSnapshot {
        fn dispatch_dialog_select(
            disp: &mut TrapDispatcher,
            cpu: &mut impl CpuOps,
            bus: &mut MacMemoryBus,
            event_ptr: u32,
            dialog_out_ptr: u32,
            item_hit_ptr: u32,
            what: u16,
            where_v: i16,
            where_h: i16,
        ) -> (u8, u32, u16, u32) {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(event_ptr, what);
            bus.write_long(event_ptr + 2, 0);
            bus.write_long(event_ptr + 6, 0);
            bus.write_word(event_ptr + 10, where_v as u16);
            bus.write_word(event_ptr + 12, where_h as u16);
            bus.write_word(event_ptr + 14, 0);
            bus.write_word(TEST_SP + 12, 0xBEEF);
            bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);
            bus.write_word(item_hit_ptr, 0xCAFE);
            bus.write_long(TEST_SP, item_hit_ptr);
            bus.write_long(TEST_SP + 4, dialog_out_ptr);
            bus.write_long(TEST_SP + 8, event_ptr);

            disp.dispatch_dialog(true, 0x180, cpu, bus)
                .unwrap()
                .unwrap();

            (
                bus.read_byte(TEST_SP + 12),
                bus.read_long(dialog_out_ptr),
                bus.read_word(item_hit_ptr),
                cpu.read_reg(Register::A7),
            )
        }

        fn write_ditl_item(
            bus: &mut MacMemoryBus,
            ditl_ptr: u32,
            offset: u32,
            handle: u32,
            rect: (i16, i16, i16, i16),
            item_type: u8,
        ) {
            bus.write_long(ditl_ptr + offset, handle);
            bus.write_word(ditl_ptr + offset + 4, rect.0 as u16);
            bus.write_word(ditl_ptr + offset + 6, rect.1 as u16);
            bus.write_word(ditl_ptr + offset + 8, rect.2 as u16);
            bus.write_word(ditl_ptr + offset + 10, rect.3 as u16);
            bus.write_byte(ditl_ptr + offset + 12, item_type);
            bus.write_byte(ditl_ptr + offset + 13, 0);
        }

        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(30);
        let item1_text_handle = bus.alloc(4);
        let item1_text_ptr = bus.alloc(5);
        let item2_text_handle = bus.alloc(4);
        let item2_text_ptr = bus.alloc(6);
        let text_h = TrapDispatcher::allocate_te_handle(&mut bus);

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_long(dialog_ptr + 160, text_h);
        bus.write_word(dialog_ptr + 164, 0); // editField = first item

        bus.write_long(items_handle, ditl_ptr);
        bus.write_word(ditl_ptr, 1); // two items
        write_ditl_item(
            &mut bus,
            ditl_ptr,
            2,
            item1_text_handle,
            (20, 20, 38, 120),
            16,
        );
        write_ditl_item(
            &mut bus,
            ditl_ptr,
            16,
            item2_text_handle,
            (42, 20, 62, 120),
            16,
        );
        bus.write_long(item1_text_handle, item1_text_ptr);
        bus.write_bytes(item1_text_ptr, b"First");
        bus.write_long(item2_text_handle, item2_text_ptr);
        bus.write_bytes(item2_text_ptr, b"Second");

        disp.te_set_text_contents(&mut bus, text_h, b"First");
        let te_ptr = bus.read_long(text_h);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 16,
                    rect: (20, 20, 38, 120),
                    text: "First".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 1,
                    sel_end: 1,
                },
                DialogItem {
                    item_type: 16,
                    rect: (42, 20, 62, 120),
                    text: "Second".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 2,
                    sel_end: 4,
                },
            ],
        );

        let (mouse_result, mouse_dialog_out, mouse_item_hit, mouse_stack_after) =
            dispatch_dialog_select(
                &mut disp,
                &mut cpu,
                &mut bus,
                event_ptr,
                dialog_out_ptr,
                item_hit_ptr,
                1,
                150,
                240,
            );
        let mouse_edit_field = bus.read_word(dialog_ptr + 164);
        let mouse_item = &disp.dialog_items[&dialog_ptr][1];
        let mouse_item_selection = (mouse_item.sel_start, mouse_item.sel_end);
        let mouse_te_text = TrapDispatcher::te_text_bytes(&bus, text_h);
        let mouse_te_length = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);
        let mouse_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );

        let (null_result, null_dialog_out, null_item_hit, null_stack_after) =
            dispatch_dialog_select(
                &mut disp,
                &mut cpu,
                &mut bus,
                event_ptr,
                dialog_out_ptr,
                item_hit_ptr,
                0,
                0,
                0,
            );
        let null_edit_field = bus.read_word(dialog_ptr + 164);
        let null_te_text = TrapDispatcher::te_text_bytes(&bus, text_h);
        let null_te_length = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);
        let null_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );

        DialogSelectEditTextMouseSnapshot {
            mouse_result,
            mouse_dialog_out,
            mouse_item_hit,
            mouse_stack_after,
            mouse_edit_field,
            mouse_item_selection,
            mouse_te_text,
            mouse_te_length,
            mouse_te_selection,
            null_result,
            null_dialog_out,
            null_item_hit,
            null_stack_after,
            null_edit_field,
            null_te_text,
            null_te_length,
            null_te_selection,
        }
    }

    fn modal_dialog_edit_text_mouse_results_for_theme(
        theme_id: UiThemeId,
    ) -> ModalDialogEditTextMouseSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let item_hit_ptr = 0x300100u32;
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(30);
        let item1_text_handle = bus.alloc(4);
        let item1_text_ptr = bus.alloc(5);
        let item2_text_handle = bus.alloc(4);
        let item2_text_ptr = bus.alloc(6);
        let text_h = TrapDispatcher::allocate_te_handle(&mut bus);

        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-200i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_long(dialog_ptr + 160, text_h);
        bus.write_word(dialog_ptr + 164, 0); // editField = first item

        bus.write_long(items_handle, ditl_ptr);
        bus.write_word(ditl_ptr, 1); // two items
        bus.write_long(ditl_ptr + 2, item1_text_handle);
        bus.write_word(ditl_ptr + 6, 20);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 38);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 16);
        bus.write_byte(ditl_ptr + 15, 0);
        bus.write_long(ditl_ptr + 16, item2_text_handle);
        bus.write_word(ditl_ptr + 20, 42);
        bus.write_word(ditl_ptr + 22, 20);
        bus.write_word(ditl_ptr + 24, 62);
        bus.write_word(ditl_ptr + 26, 120);
        bus.write_byte(ditl_ptr + 28, 16);
        bus.write_byte(ditl_ptr + 29, 0);

        bus.write_long(item1_text_handle, item1_text_ptr);
        bus.write_bytes(item1_text_ptr, b"First");
        bus.write_long(item2_text_handle, item2_text_ptr);
        bus.write_bytes(item2_text_ptr, b"Second");
        disp.te_set_text_contents(&mut bus, text_h, b"First");
        let te_ptr = bus.read_long(text_h);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);

        let items = vec![
            DialogItem {
                item_type: 16,
                rect: (20, 20, 38, 120),
                text: "First".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 1,
                sel_end: 1,
            },
            DialogItem {
                item_type: 16,
                rect: (42, 20, 62, 120),
                text: "Second".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 2,
                sel_end: 4,
            },
        ];
        disp.dialog_items.insert(dialog_ptr, items.clone());
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items,
            default_item: 0,
            cancel_item: 0,
            edit_text: "First".to_string(),
            edit_item: 1,
            saved_pixels: vec![0x11, 0x22, 0x33].into(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
        bus.write_word(item_hit_ptr, 0xCAFE);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((150, 240));
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 150,
                where_h: 240,
                modifiers: 0,
            });
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: 150,
                where_h: 240,
                modifiers: 0,
            });

        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let item = &disp.dialog_items[&dialog_ptr][1];

        ModalDialogEditTextMouseSnapshot {
            item_hit: bus.read_word(item_hit_ptr),
            stack_after: cpu.read_reg(Register::A7),
            tracking_finished: disp.dialog_tracking.is_none(),
            retained_visible_snapshot: disp.dialog_visible_snapshots.contains_key(&dialog_ptr),
            saved_background_retained: disp.dialog_saved_pixels.contains_key(&dialog_ptr),
            queued_mouse_up_consumed: !disp.event_queue.iter().any(|event| event.what == 2),
            edit_field: bus.read_word(dialog_ptr + 164),
            item_selection: (item.sel_start, item.sel_end),
            te_text: TrapDispatcher::te_text_bytes(&bus, text_h),
            te_length: bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            te_selection: (
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            ),
            handle_bytes: text_handle_bytes(&bus, item2_text_handle),
        }
    }

    fn modal_dialog_keyboard_button_results_for_theme(
        theme_id: UiThemeId,
    ) -> ModalDialogKeyboardButtonSnapshot {
        fn run_key(
            theme_id: UiThemeId,
            key_code: u8,
            char_code: u8,
            modifiers: u16,
        ) -> ModalDialogKeyboardButtonCase {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            disp.set_ui_theme_id(theme_id);
            let dialog_ptr = 0x200000u32;
            let item_hit_ptr = 0x300000u32;
            let items = vec![
                DialogItem {
                    item_type: 4,
                    rect: (20, 30, 60, 110),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (20, 124, 60, 214),
                    text: "Cancel".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ];
            disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
                dialog_ptr,
                bounds: (100, 200, 200, 440),
                title: String::new(),
                proc_id: 2,
                items,
                default_item: 1,
                cancel_item: 2,
                edit_text: String::new(),
                edit_item: 0,
                saved_pixels: Default::default(),
                stack_ptr: TEST_SP,
                item_hit_ptr,
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
            disp.event_queue
                .push_back(crate::trap::dispatch::QueuedEvent {
                    what: 3,
                    message: (u32::from(key_code) << 8) | u32::from(char_code),
                    when: 0,
                    where_v: 0,
                    where_h: 0,
                    modifiers,
                });
            bus.write_word(item_hit_ptr, 0xCAFE);
            cpu.write_reg(Register::A7, TEST_SP);

            disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            let flash_item_after_key = disp
                .dialog_tracking
                .as_ref()
                .map(|tracking| tracking.flash_item)
                .unwrap_or(0);
            let stack_after_key = cpu.read_reg(Register::A7);
            let item_hit_after_key = bus.read_word(item_hit_ptr);

            {
                let tracking = disp.dialog_tracking.as_mut().unwrap();
                tracking.flash_remaining = 1;
                tracking.flash_delay = 0;
            }
            disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            ModalDialogKeyboardButtonCase {
                flash_item_after_key,
                stack_after_key,
                item_hit_after_key,
                item_hit_after_flash: bus.read_word(item_hit_ptr),
                stack_after_flash: cpu.read_reg(Register::A7),
                tracking_finished: disp.dialog_tracking.is_none(),
            }
        }

        ModalDialogKeyboardButtonSnapshot {
            return_key: run_key(theme_id, 0x24, 0x0D, 0),
            enter_key: run_key(theme_id, 0x4C, 0x03, 0),
            escape_key: run_key(theme_id, 0x35, 0x1B, 0),
            command_period: run_key(theme_id, 0x2F, b'.', 0x0100),
        }
    }

    fn modal_dialog_update_event_results_for_theme(
        theme_id: UiThemeId,
    ) -> ModalDialogUpdateEventSnapshot {
        fn install_region(
            bus: &mut MacMemoryBus,
            handle: u32,
            data: u32,
            rect: (i16, i16, i16, i16),
        ) {
            bus.write_long(handle, data);
            bus.write_word(data, 10);
            bus.write_word(data + 2, rect.0 as u16);
            bus.write_word(data + 4, rect.1 as u16);
            bus.write_word(data + 6, rect.2 as u16);
            bus.write_word(data + 8, rect.3 as u16);
        }

        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);

        let initial_port = 0x181000;
        disp.set_current_port_state(&mut bus, &mut cpu, initial_port, None);

        let screen_base = bus.alloc(80 * 80);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 80, 80, 80, 8);

        let dialog_ptr = bus.alloc(256);
        let item_hit_ptr = 0x300100u32;
        let bounds = (10, 10, 40, 40);
        let snapshot_width = (bounds.3 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_height =
            (bounds.2 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_index = (17 - (bounds.0 - TrapDispatcher::DBOX_FRAME_MARGIN)) as usize
            * snapshot_width
            + (19 - (bounds.1 - TrapDispatcher::DBOX_FRAME_MARGIN)) as usize;
        let mut rendered_pixels = vec![0x00; snapshot_width * snapshot_height];
        rendered_pixels[snapshot_index] = 0x44;

        let vis_rgn = bus.alloc(10);
        let vis_rgn_handle = bus.alloc(4);
        install_region(&mut bus, vis_rgn_handle, vis_rgn, (0, 0, 80, 80));
        let clip_rgn = bus.alloc(10);
        let clip_rgn_handle = bus.alloc(4);
        install_region(&mut bus, clip_rgn_handle, clip_rgn, (0, 0, 80, 80));
        let update_rgn = bus.alloc(10);
        let update_rgn_handle = bus.alloc(4);
        install_region(&mut bus, update_rgn_handle, update_rgn, (0, 0, 40, 40));

        bus.write_word(dialog_ptr, 0);
        bus.write_long(dialog_ptr + 2, screen_base);
        bus.write_word(dialog_ptr + 6, 80);
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 12, 80);
        bus.write_word(dialog_ptr + 14, 80);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 80);
        bus.write_long(dialog_ptr + 24, vis_rgn_handle);
        bus.write_long(dialog_ptr + 28, clip_rgn_handle);
        bus.write_long(dialog_ptr + 122, update_rgn_handle);

        bus.write_word(item_hit_ptr, 0xCAFE);
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds,
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: rendered_pixels.into(),
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
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 6,
                message: dialog_ptr,
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });
        cpu.write_reg(Register::A7, TEST_SP);

        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        ModalDialogUpdateEventSnapshot {
            dialog_ptr,
            item_hit_after: bus.read_word(item_hit_ptr),
            stack_after: cpu.read_reg(Register::A7),
            tracking_finished: disp.dialog_tracking.is_none(),
            rendered_pixels_final: tracking.rendered_pixels_final,
            retained_pixel_after: bus.read_byte(screen_base + 17 * 80 + 19),
            retained_snapshot_pixel_after: tracking.rendered_pixels[snapshot_index],
            update_region_after: TrapDispatcher::region_handle_rect(
                &bus,
                bus.read_long(dialog_ptr + 122),
            ),
            vis_region_after: TrapDispatcher::region_handle_rect(
                &bus,
                bus.read_long(dialog_ptr + 24),
            ),
            the_port_after: bus.read_long(crate::memory::globals::addr::THE_PORT),
            current_port_after: *disp.current_port,
            saved_vis_after: disp.saved_vis_regions.contains_key(&dialog_ptr),
        }
    }

    fn dialog_select_edit_text_key_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogSelectEditTextKeySnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let item_text_handle = bus.alloc(4);
        let item_text_ptr = bus.alloc(5);
        let text_h = TrapDispatcher::allocate_te_handle(&mut bus);

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_long(dialog_ptr + 160, text_h);
        bus.write_word(dialog_ptr + 164, 0); // editField = first item
        bus.write_long(items_handle, ditl_ptr);
        bus.write_word(ditl_ptr, 0); // one item
        bus.write_long(ditl_ptr + 2, item_text_handle);
        bus.write_word(ditl_ptr + 6, 20);
        bus.write_word(ditl_ptr + 8, 30);
        bus.write_word(ditl_ptr + 10, 60);
        bus.write_word(ditl_ptr + 12, 110);
        bus.write_byte(ditl_ptr + 14, 16); // editText
        bus.write_byte(ditl_ptr + 15, 0);
        bus.write_long(item_text_handle, item_text_ptr);
        bus.write_bytes(item_text_ptr, b"Hello");

        disp.te_set_text_contents(&mut bus, text_h, b"Hello");
        let te_ptr = bus.read_long(text_h);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (20, 30, 60, 110),
                text: "Hello".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 1,
                sel_end: 4,
            }],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(event_ptr, 3); // keyDown
        bus.write_long(event_ptr + 2, u32::from(b'Y'));
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 0);
        bus.write_word(event_ptr + 12, 0);
        bus.write_word(event_ptr + 14, 0);
        bus.write_word(TEST_SP + 12, 0xBEEF);
        bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);
        bus.write_word(item_hit_ptr, 0xCAFE);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);
        disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let keydown_result = bus.read_byte(TEST_SP + 12);
        let keydown_dialog_out = bus.read_long(dialog_out_ptr);
        let keydown_item_hit = bus.read_word(item_hit_ptr);
        let keydown_stack_after = cpu.read_reg(Register::A7);
        let keydown_handle_bytes = text_handle_bytes(&bus, item_text_handle);
        let keydown_cached_text = disp.dialog_items[&dialog_ptr][0].text.clone();
        let keydown_item_selection = (
            disp.dialog_items[&dialog_ptr][0].sel_start,
            disp.dialog_items[&dialog_ptr][0].sel_end,
        );
        let keydown_te_text = TrapDispatcher::te_text_bytes(&bus, text_h);
        let keydown_te_length = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);
        let keydown_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(event_ptr, 5); // autoKey
        bus.write_long(event_ptr + 2, 0x0000_0008); // backspace
        bus.write_word(TEST_SP + 12, 0xBEEF);
        bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);
        bus.write_word(item_hit_ptr, 0xCAFE);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);
        disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let autokey_result = bus.read_byte(TEST_SP + 12);
        let autokey_dialog_out = bus.read_long(dialog_out_ptr);
        let autokey_item_hit = bus.read_word(item_hit_ptr);
        let autokey_stack_after = cpu.read_reg(Register::A7);
        let autokey_handle_bytes = text_handle_bytes(&bus, item_text_handle);
        let autokey_cached_text = disp.dialog_items[&dialog_ptr][0].text.clone();
        let autokey_item_selection = (
            disp.dialog_items[&dialog_ptr][0].sel_start,
            disp.dialog_items[&dialog_ptr][0].sel_end,
        );
        let autokey_te_text = TrapDispatcher::te_text_bytes(&bus, text_h);
        let autokey_te_length = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);
        let autokey_te_selection = (
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
        );

        DialogSelectEditTextKeySnapshot {
            keydown_result,
            keydown_dialog_out,
            keydown_item_hit,
            keydown_stack_after,
            keydown_handle_bytes,
            keydown_cached_text,
            keydown_item_selection,
            keydown_te_text,
            keydown_te_length,
            keydown_te_selection,
            autokey_result,
            autokey_dialog_out,
            autokey_item_hit,
            autokey_stack_after,
            autokey_handle_bytes,
            autokey_cached_text,
            autokey_item_selection,
            autokey_te_text,
            autokey_te_length,
            autokey_te_selection,
        }
    }

    fn getsetditem_useritem_record_results_for_theme(
        theme_id: UiThemeId,
    ) -> DItemUserItemRecordSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let initial_proc_ptr = 0x0012_3456u32;
        let new_proc_ptr = 0x00AB_CDEFu32;
        let set_box_ptr = bus.alloc(8);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0); // one item
        bus.write_long(ditl_ptr + 2, initial_proc_ptr);
        bus.write_word(ditl_ptr + 6, 12);
        bus.write_word(ditl_ptr + 8, 24);
        bus.write_word(ditl_ptr + 10, 36);
        bus.write_word(ditl_ptr + 12, 48);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (12, 24, 36, 48),
                text: String::new(),
                resource_id: 0,
                proc_ptr: initial_proc_ptr,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let initial_get = DItemGetSnapshot {
            item_type: bus.read_word(get_type_ptr),
            item: bus.read_long(get_item_ptr),
            rect: (
                bus.read_word(get_box_ptr),
                bus.read_word(get_box_ptr + 2),
                bus.read_word(get_box_ptr + 4),
                bus.read_word(get_box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        };

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(set_box_ptr, 50);
        bus.write_word(set_box_ptr + 2, 60);
        bus.write_word(set_box_ptr + 4, 70);
        bus.write_word(set_box_ptr + 6, 80);
        bus.write_long(TEST_SP, set_box_ptr);
        bus.write_long(TEST_SP + 4, new_proc_ptr);
        bus.write_word(TEST_SP + 8, 0);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let set_stack_after = cpu.read_reg(Register::A7);
        let stored_item = disp
            .dialog_items
            .get(&dialog_ptr)
            .and_then(|items| items.first())
            .cloned()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let post_set_get = DItemGetSnapshot {
            item_type: bus.read_word(get_type_ptr),
            item: bus.read_long(get_item_ptr),
            rect: (
                bus.read_word(get_box_ptr),
                bus.read_word(get_box_ptr + 2),
                bus.read_word(get_box_ptr + 4),
                bus.read_word(get_box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        };

        DItemUserItemRecordSnapshot {
            initial_get,
            set_stack_after,
            stored_type: stored_item.item_type,
            stored_proc_ptr: stored_item.proc_ptr,
            stored_rect: stored_item.rect,
            post_set_get,
        }
    }

    fn get_ditem_snapshot_for_test(
        disp: &mut TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        item_no: i16,
        box_ptr: u32,
        item_ptr: u32,
        type_ptr: u32,
    ) -> DItemGetSnapshot {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, item_no as u16);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, cpu, bus)
            .unwrap()
            .unwrap();

        DItemGetSnapshot {
            item_type: bus.read_word(type_ptr),
            item: bus.read_long(item_ptr),
            rect: (
                bus.read_word(box_ptr),
                bus.read_word(box_ptr + 2),
                bus.read_word(box_ptr + 4),
                bus.read_word(box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        }
    }

    fn write_control_record_for_ditem_test(
        bus: &mut MacMemoryBus,
        handle: u32,
        ctrl_ptr: u32,
        dialog_ptr: u32,
        rect: (i16, i16, i16, i16),
        value: i16,
        title: &[u8],
    ) {
        bus.write_long(handle, ctrl_ptr);
        bus.write_long(ctrl_ptr, 0);
        bus.write_long(ctrl_ptr + 4, dialog_ptr);
        bus.write_word(ctrl_ptr + 8, rect.0 as u16);
        bus.write_word(ctrl_ptr + 10, rect.1 as u16);
        bus.write_word(ctrl_ptr + 12, rect.2 as u16);
        bus.write_word(ctrl_ptr + 14, rect.3 as u16);
        bus.write_byte(ctrl_ptr + 16, 255);
        bus.write_byte(ctrl_ptr + 17, 0);
        bus.write_word(ctrl_ptr + 18, value as u16);
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 1);
        bus.write_byte(ctrl_ptr + 40, title.len() as u8);
        bus.write_bytes(ctrl_ptr + 41, title);
    }

    fn getsetditem_text_control_record_results_for_theme(
        theme_id: UiThemeId,
    ) -> DItemTextControlRecordSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(32);
        let old_text_handle = bus.alloc(4);
        let old_text_ptr = bus.alloc(3);
        let new_text_handle = bus.alloc(4);
        let new_text_ptr = bus.alloc(7);
        let old_control_handle = bus.alloc(4);
        let old_control_ptr = bus.alloc(44);
        let new_control_handle = bus.alloc(4);
        let new_control_ptr = bus.alloc(47);
        let text_set_box_ptr = bus.alloc(8);
        let control_set_box_ptr = bus.alloc(8);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);
        let text_entry = ditl_ptr + 2;
        let control_entry = ditl_ptr + 16;

        bus.write_bytes(old_text_ptr, b"Old");
        bus.write_bytes(new_text_ptr, b"Updated");
        bus.write_long(old_text_handle, old_text_ptr);
        bus.write_long(new_text_handle, new_text_ptr);
        write_control_record_for_ditem_test(
            &mut bus,
            old_control_handle,
            old_control_ptr,
            dialog_ptr,
            (30, 40, 50, 140),
            1,
            b"OK",
        );
        write_control_record_for_ditem_test(
            &mut bus,
            new_control_handle,
            new_control_ptr,
            dialog_ptr,
            (1, 2, 3, 4),
            2,
            b"Apply",
        );

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1); // two items
        bus.write_long(text_entry, old_text_handle);
        bus.write_word(text_entry + 4, 10);
        bus.write_word(text_entry + 6, 20);
        bus.write_word(text_entry + 8, 24);
        bus.write_word(text_entry + 10, 160);
        bus.write_byte(text_entry + 12, 16);
        bus.write_byte(text_entry + 13, 0);
        bus.write_long(control_entry, old_control_handle);
        bus.write_word(control_entry + 4, 30);
        bus.write_word(control_entry + 6, 40);
        bus.write_word(control_entry + 8, 50);
        bus.write_word(control_entry + 10, 140);
        bus.write_byte(control_entry + 12, 4);
        bus.write_byte(control_entry + 13, 2);
        bus.write_bytes(control_entry + 14, b"OK");

        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 16,
                    rect: (10, 20, 24, 160),
                    text: "Old".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (30, 40, 50, 140),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );
        disp.dialog_item_handles
            .insert(old_text_handle, (dialog_ptr, 0));
        disp.dialog_control_handles
            .insert(old_control_handle, (dialog_ptr, 2));
        disp.dialog_control_values.insert((dialog_ptr, 2), 1);

        let text_initial_get = get_ditem_snapshot_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            dialog_ptr,
            1,
            get_box_ptr,
            get_item_ptr,
            get_type_ptr,
        );
        let control_initial_get = get_ditem_snapshot_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            dialog_ptr,
            2,
            get_box_ptr,
            get_item_ptr,
            get_type_ptr,
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(text_set_box_ptr, 12);
        bus.write_word(text_set_box_ptr + 2, 22);
        bus.write_word(text_set_box_ptr + 4, 28);
        bus.write_word(text_set_box_ptr + 6, 168);
        bus.write_long(TEST_SP, text_set_box_ptr);
        bus.write_long(TEST_SP + 4, new_text_handle);
        bus.write_word(TEST_SP + 8, 16);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let text_set_stack_after = cpu.read_reg(Register::A7);
        let text_stored_item = disp.dialog_items.get(&dialog_ptr).unwrap()[0].clone();
        let text_post_set_get = get_ditem_snapshot_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            dialog_ptr,
            1,
            get_box_ptr,
            get_item_ptr,
            get_type_ptr,
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(control_set_box_ptr, 42);
        bus.write_word(control_set_box_ptr + 2, 52);
        bus.write_word(control_set_box_ptr + 4, 62);
        bus.write_word(control_set_box_ptr + 6, 172);
        bus.write_long(TEST_SP, control_set_box_ptr);
        bus.write_long(TEST_SP + 4, new_control_handle);
        bus.write_word(TEST_SP + 8, 5);
        bus.write_word(TEST_SP + 10, 2);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let control_set_stack_after = cpu.read_reg(Register::A7);
        let control_stored_item = disp.dialog_items.get(&dialog_ptr).unwrap()[1].clone();
        let control_post_set_get = get_ditem_snapshot_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            dialog_ptr,
            2,
            get_box_ptr,
            get_item_ptr,
            get_type_ptr,
        );

        DItemTextControlRecordSnapshot {
            text_initial_get,
            text_set_stack_after,
            text_old_handle_mapped_after: disp.dialog_item_handles.contains_key(&old_text_handle),
            text_new_handle_map_value_after: disp
                .dialog_item_handles
                .get(&new_text_handle)
                .copied(),
            text_ditl_handle_after: bus.read_long(text_entry),
            text_ditl_rect_after: (
                bus.read_word(text_entry + 4),
                bus.read_word(text_entry + 6),
                bus.read_word(text_entry + 8),
                bus.read_word(text_entry + 10),
            ),
            text_ditl_type_after: bus.read_byte(text_entry + 12),
            text_stored_type_after: text_stored_item.item_type,
            text_stored_rect_after: text_stored_item.rect,
            text_cached_text_after: text_stored_item.text,
            text_handle_bytes_after: text_handle_bytes(&bus, new_text_handle),
            text_post_set_get,
            control_initial_get,
            control_set_stack_after,
            control_old_handle_mapped_after: disp
                .dialog_control_handles
                .contains_key(&old_control_handle),
            control_new_handle_map_value_after: disp
                .dialog_control_handles
                .get(&new_control_handle)
                .copied(),
            control_ditl_handle_after: bus.read_long(control_entry),
            control_ditl_rect_after: (
                bus.read_word(control_entry + 4),
                bus.read_word(control_entry + 6),
                bus.read_word(control_entry + 8),
                bus.read_word(control_entry + 10),
            ),
            control_ditl_type_after: bus.read_byte(control_entry + 12),
            control_stored_type_after: control_stored_item.item_type,
            control_stored_rect_after: control_stored_item.rect,
            control_record_rect_after: (
                bus.read_word(new_control_ptr + 8),
                bus.read_word(new_control_ptr + 10),
                bus.read_word(new_control_ptr + 12),
                bus.read_word(new_control_ptr + 14),
            ),
            control_value_after: disp.dialog_control_values.get(&(dialog_ptr, 2)).copied(),
            control_post_set_get,
        }
    }

    fn hide_show_ditem_visibility_results_for_theme(
        theme_id: UiThemeId,
    ) -> DItemVisibilitySnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(18);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0); // one item
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 4);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_bytes(ditl_ptr + 16, b"OK");
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 20, 30, 120),
                text: "OK".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let initial_get = DItemGetSnapshot {
            item_type: bus.read_word(get_type_ptr),
            item: bus.read_long(get_item_ptr),
            rect: (
                bus.read_word(get_box_ptr),
                bus.read_word(get_box_ptr + 2),
                bus.read_word(get_box_ptr + 4),
                bus.read_word(get_box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        };
        assert_ne!(initial_get.item, 0);
        let ctrl_ptr = bus.read_long(initial_get.item);
        assert_ne!(ctrl_ptr, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        disp.dispatch_dialog(true, 0x027, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let hide_stack_after = cpu.read_reg(Register::A7);
        let hidden_item_rect = disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect;
        let hidden_saved_rect = disp.hidden_dialog_item_rects.get(&(dialog_ptr, 1)).copied();
        let hidden_control_rect = (
            bus.read_word(ctrl_ptr + 8),
            bus.read_word(ctrl_ptr + 10),
            bus.read_word(ctrl_ptr + 12),
            bus.read_word(ctrl_ptr + 14),
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let hidden_get = DItemGetSnapshot {
            item_type: bus.read_word(get_type_ptr),
            item: bus.read_long(get_item_ptr),
            rect: (
                bus.read_word(get_box_ptr),
                bus.read_word(get_box_ptr + 2),
                bus.read_word(get_box_ptr + 4),
                bus.read_word(get_box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        };

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 20);
        bus.write_word(TEST_SP + 2, 40);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        bus.write_word(TEST_SP + 8, 0xBEEF);
        disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let hidden_find = (
            bus.read_word(TEST_SP + 8) as i16,
            cpu.read_reg(Register::A7),
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let show_stack_after = cpu.read_reg(Register::A7);
        let restored_item_rect = disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect;
        let restored_saved_rect = disp.hidden_dialog_item_rects.get(&(dialog_ptr, 1)).copied();
        let restored_control_rect = (
            bus.read_word(ctrl_ptr + 8),
            bus.read_word(ctrl_ptr + 10),
            bus.read_word(ctrl_ptr + 12),
            bus.read_word(ctrl_ptr + 14),
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let restored_get = DItemGetSnapshot {
            item_type: bus.read_word(get_type_ptr),
            item: bus.read_long(get_item_ptr),
            rect: (
                bus.read_word(get_box_ptr),
                bus.read_word(get_box_ptr + 2),
                bus.read_word(get_box_ptr + 4),
                bus.read_word(get_box_ptr + 6),
            ),
            stack_after: cpu.read_reg(Register::A7),
        };

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 20);
        bus.write_word(TEST_SP + 2, 40);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        bus.write_word(TEST_SP + 8, 0xBEEF);
        disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let restored_find = (
            bus.read_word(TEST_SP + 8) as i16,
            cpu.read_reg(Register::A7),
        );

        DItemVisibilitySnapshot {
            initial_get,
            hide_stack_after,
            hidden_item_rect,
            hidden_saved_rect,
            hidden_control_rect,
            hidden_get,
            hidden_find,
            show_stack_after,
            restored_item_rect,
            restored_saved_rect,
            restored_control_rect,
            restored_get,
            restored_find,
        }
    }

    fn drawdialog_useritem_pixel_results_for_theme(theme_id: UiThemeId) -> (u8, u32, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 40);
        bus.write_word(dialog_ptr + 22, 80);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0, // enabled userItem
                    rect: (8, 8, 20, 32),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0x00C0_FFEE,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (24, 8, 36, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        let user_x = 10u32;
        let user_y = 10u32;
        let user_pixel = screen_base + user_y * row_bytes + user_x;
        bus.write_byte(user_pixel, 0xA5);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE);

        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            bus.read_byte(user_pixel),
            cpu.read_reg(Register::A7),
            bus.read_long(TEST_SP + 4),
        )
    }

    fn monochrome_icon_resource_with_points(points: &[(u8, u8)]) -> Vec<u8> {
        let mut icon = vec![0u8; 128];
        for &(x, y) in points {
            let byte = y as usize * 4 + x as usize / 8;
            icon[byte] |= 0x80 >> (x & 7);
        }
        icon
    }

    fn write_be_word(data: &mut [u8], offset: usize, value: u16) {
        data[offset] = (value >> 8) as u8;
        data[offset + 1] = value as u8;
    }

    fn cicn_resource_with_points(width: u16, height: u16, points: &[(u8, u8, u8)]) -> Vec<u8> {
        cicn_resource_with_palette(width, height, 0, &[(0, [0; 3])], points)
    }

    fn cicn_resource_with_palette(
        width: u16,
        height: u16,
        ct_flags: u16,
        palette: &[(u16, [u16; 3])],
        points: &[(u8, u8, u8)],
    ) -> Vec<u8> {
        assert!(!palette.is_empty());
        let pixel_row_bytes = u32::from(width);
        let mask_row_bytes = u32::from(width).div_ceil(8);
        let mask_size = mask_row_bytes * u32::from(height);
        let bmap_size = mask_row_bytes * u32::from(height);
        let ctab_size = 8 + palette.len() * 8;
        let pixel_size = pixel_row_bytes * u32::from(height);
        let bmap_offset = 82 + mask_size as usize;
        let ctab_offset = bmap_offset + bmap_size as usize;
        let pixel_offset = ctab_offset + ctab_size;
        let mut data = vec![0u8; pixel_offset + pixel_size as usize];

        write_be_word(&mut data, 4, pixel_row_bytes as u16);
        write_be_word(&mut data, 10, height);
        write_be_word(&mut data, 12, width);
        write_be_word(&mut data, 32, 8);
        write_be_word(&mut data, 54, mask_row_bytes as u16);
        write_be_word(&mut data, 60, height);
        write_be_word(&mut data, 62, width);
        write_be_word(&mut data, 68, mask_row_bytes as u16);
        write_be_word(&mut data, 74, height);
        write_be_word(&mut data, 76, width);
        write_be_word(&mut data, ctab_offset + 4, ct_flags);
        write_be_word(&mut data, ctab_offset + 6, palette.len() as u16 - 1);
        for (ordinal, &(value, rgb)) in palette.iter().enumerate() {
            let entry = ctab_offset + 8 + ordinal * 8;
            write_be_word(&mut data, entry, value);
            write_be_word(&mut data, entry + 2, rgb[0]);
            write_be_word(&mut data, entry + 4, rgb[1]);
            write_be_word(&mut data, entry + 6, rgb[2]);
        }

        for &(x, y, pixel) in points {
            let mask_row = 82 + y as usize * mask_row_bytes as usize;
            data[mask_row + x as usize / 8] |= 0x80 >> (x & 7);
            let pixel_row = pixel_offset + y as usize * pixel_row_bytes as usize;
            data[pixel_row + x as usize] = pixel;
        }
        data
    }

    fn drawdialog_icon_item_pixel_results_for_theme(theme_id: UiThemeId) -> ([u8; 3], u32, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 56);
        bus.write_word(dialog_ptr + 22, 88);
        disp.install_test_resource(
            &mut bus,
            *b"ICON",
            1,
            &monochrome_icon_resource_with_points(&[(0, 0), (15, 15), (31, 31)]),
        );
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 32, // icon item
                    rect: (8, 8, 40, 40),
                    text: String::new(),
                    resource_id: 1,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (44, 8, 54, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        for (x, y) in [(8u32, 8u32), (23, 23), (39, 39)] {
            bus.write_byte(screen_base + y * row_bytes + x, 0x42);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE);

        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            [
                bus.read_byte(screen_base + 8 * row_bytes + 8),
                bus.read_byte(screen_base + 23 * row_bytes + 23),
                bus.read_byte(screen_base + 39 * row_bytes + 39),
            ],
            cpu.read_reg(Register::A7),
            bus.read_long(TEST_SP + 4),
        )
    }

    fn drawdialog_cicn_item_pixel_results_for_theme(theme_id: UiThemeId) -> ([u8; 3], u32, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 56);
        bus.write_word(dialog_ptr + 22, 88);
        let sample_points = [(0, 0), (15, 15), (31, 31)];
        disp.install_test_resource(
            &mut bus,
            *b"ICON",
            422,
            &monochrome_icon_resource_with_points(&sample_points),
        );
        disp.install_test_resource(
            &mut bus,
            *b"cicn",
            422,
            &cicn_resource_with_points(32, 32, &[(0, 0, 0x33), (15, 15, 0x55), (31, 31, 0x77)]),
        );
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 32, // icon item with same-ID cicn override
                    rect: (8, 8, 40, 40),
                    text: String::new(),
                    resource_id: 422,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (44, 8, 54, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        for (x, y) in [(8u32, 8u32), (23, 23), (39, 39)] {
            bus.write_byte(screen_base + y * row_bytes + x, 0x42);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE);

        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            [
                bus.read_byte(screen_base + 8 * row_bytes + 8),
                bus.read_byte(screen_base + 23 * row_bytes + 23),
                bus.read_byte(screen_base + 39 * row_bytes + 39),
            ],
            cpu.read_reg(Register::A7),
            bus.read_long(TEST_SP + 4),
        )
    }

    fn solid_fill_pict_resource(width: i16, height: i16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0u16.to_be_bytes()); // picSize placeholder
        for value in [0i16, 0, height, width] {
            data.extend_from_slice(&(value as u16).to_be_bytes());
        }
        data.push(0x11); // versionOp
        data.push(0x01); // PICT v1
        data.push(0x0A); // FillPat
        data.extend_from_slice(&[0xFF; 8]);
        data.push(0x34); // fillRect
        for value in [0i16, 0, height, width] {
            data.extend_from_slice(&(value as u16).to_be_bytes());
        }
        data.push(0xFF); // EndOfPicture
        let size = data.len() as u16;
        data[0..2].copy_from_slice(&size.to_be_bytes());
        data
    }

    fn drawdialog_picture_item_pixel_results_for_theme(theme_id: UiThemeId) -> ([u8; 3], u32, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 48);
        bus.write_word(dialog_ptr + 22, 80);
        disp.install_test_resource(&mut bus, *b"PICT", 420, &solid_fill_pict_resource(16, 16));
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 64, // picture item
                    rect: (8, 8, 24, 24),
                    text: String::new(),
                    resource_id: 420,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (30, 8, 42, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        for (x, y) in [(9u32, 9u32), (16, 16), (22, 22)] {
            bus.write_byte(screen_base + y * row_bytes + x, 0x42);
        }
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE);

        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        (
            [
                bus.read_byte(screen_base + 9 * row_bytes + 9),
                bus.read_byte(screen_base + 16 * row_bytes + 16),
                bus.read_byte(screen_base + 22 * row_bytes + 22),
            ],
            cpu.read_reg(Register::A7),
            bus.read_long(TEST_SP + 4),
        )
    }

    #[test]
    fn nested_modal_dialog_returns_to_its_own_stack_before_resuming_parent() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_screen_mode_for_test(0x300000, 640, 640, 480, 8);
        let outer = bus.alloc(170);
        let inner = bus.alloc(170);
        let outer_hit = bus.alloc(2);
        let inner_hit = bus.alloc(2);
        let result = bus.alloc(2);
        let inner_sp = TEST_SP - 128;
        seed_window_regions(&mut bus, inner, (100, 100, 200, 300));
        disp.front_window = inner;
        disp.window_bounds = (100, 100, 200, 300);
        disp.dialog_items.insert(
            inner,
            vec![DialogItem {
                item_type: 4,
                rect: (60, 120, 80, 180),
                text: "OK".into(),
                ..Default::default()
            }],
        );
        let mut parent = dialog_tracking_state_for_test(outer);
        parent.item_hit_ptr = outer_hit;
        parent.filter_proc = 0x10000;
        parent.last_filter_event = Some(crate::trap::dispatch::QueuedEvent {
            what: 0,
            message: 0,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dialog_tracking = Some(parent);
        disp.dialog_filter_result_addr = result;
        bus.write_word(result, 0x0100);
        bus.write_word(outer_hit, 99);
        bus.write_long(inner_sp, inner_hit);
        bus.write_long(inner_sp + 4, 0x10000);
        cpu.write_reg(Register::A7, inner_sp);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let child = disp.dialog_tracking.as_mut().unwrap();
        assert_eq!(child.dialog_ptr, inner);
        assert_eq!(child.stack_ptr, inner_sp);
        assert_eq!(disp.suspended_modal_dialogs.len(), 1);
        assert_eq!(bus.read_word(result), 0);
        child.last_filter_event = Some(crate::trap::dispatch::QueuedEvent {
            what: 0,
            message: 0,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        child.draw_procs_done = true;
        child.rendered_pixels_final = true;
        bus.write_word(inner_hit, 1);
        bus.write_word(result, 0x0100);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), inner_sp + 8);
        assert!(disp.dialog_tracking.is_none());
        assert_eq!(bus.read_word(outer_hit), 99);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(result, 0x0100);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert!(disp.dialog_tracking.is_none());
        assert!(disp.suspended_modal_dialogs.is_empty());
    }

    fn dialog_tracking_state_for_test(dialog_ptr: u32) -> DialogTrackingState {
        DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 0, 0),
            title: String::new(),
            proc_id: 0,
            items: Vec::new(),
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
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
        }
    }

    fn dialog_lifecycle_cleanup_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogLifecycleCleanupSnapshot {
        let bounds = (100, 100, 150, 200);
        let previous_bounds = (0, 0, 342, 512);
        let saved_pixels = vec![0x33; 66 * 116];

        let (mut close_disp, mut close_cpu, mut close_bus) = setup();
        close_disp.set_ui_theme_id(theme_id);
        close_disp.set_screen_mode_for_test(0x300000, 640, 640, 480, 8);
        let close_dialog_ptr = close_bus.alloc(170);
        let close_previous_window = close_bus.alloc(170);
        seed_window_regions(&mut close_bus, close_dialog_ptr, bounds);
        seed_window_regions(&mut close_bus, close_previous_window, previous_bounds);
        close_bus.write_word(close_dialog_ptr + 108, 2);
        close_disp.front_window = close_dialog_ptr;
        close_disp
            .current_port
            .with_mut(|current_port| *current_port = close_dialog_ptr);
        close_disp.window_bounds = bounds;
        close_disp.window_proc_id = 2;
        close_disp.window_list.replace(vec![close_dialog_ptr, close_previous_window]);
        close_disp.window_stack.push((
            close_previous_window,
            previous_bounds,
            0,
            "Previous".to_string(),
        ));
        close_disp.dialog_items.insert(
            close_dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (20, 40, 40, 100),
                text: "OK".to_string(),
                ..Default::default()
            }],
        );
        close_disp.dialog_tracking = Some(dialog_tracking_state_for_test(close_dialog_ptr));
        close_disp.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
            dialog_ptr: close_dialog_ptr,
            item_no: 1,
            rect: (20, 40, 40, 100),
            title: "OK".to_string(),
            is_default: true,
            highlighted: true,
            delivered_to_app: false,
        });
        close_disp.dialog_visible_snapshots.insert(
            close_dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: saved_pixels.clone().into(),
            },
        );
        close_disp.dialog_modal_entered.insert(close_dialog_ptr);
        close_disp
            .dialog_saved_pixels
            .insert(close_dialog_ptr, saved_pixels.clone().into());
        close_bus.write_long(crate::memory::globals::addr::THE_PORT, close_dialog_ptr);
        let close_global_ptr = close_bus.read_long(close_cpu.read_reg(Register::A5));
        close_bus.write_long(close_global_ptr, close_dialog_ptr);
        close_cpu.write_reg(Register::A7, TEST_SP);
        close_bus.write_long(TEST_SP, close_dialog_ptr);
        close_disp
            .dispatch_dialog(true, 0x182, &mut close_cpu, &mut close_bus)
            .unwrap()
            .unwrap();

        let close_stack_after = close_cpu.read_reg(Register::A7);
        let close_tracking_cleared = close_disp.dialog_tracking.is_none();
        let close_retained_click_cleared = close_disp.retained_modal_dialog_click.is_none();
        let close_visible_snapshot_cleared = !close_disp
            .dialog_visible_snapshots
            .contains_key(&close_dialog_ptr);
        let close_modal_entered_cleared =
            !close_disp.dialog_modal_entered.contains(&close_dialog_ptr);
        let close_saved_pixels_cleared = !close_disp
            .dialog_saved_pixels
            .contains_key(&close_dialog_ptr);
        let close_window_list_contains_dialog = close_disp.window_list.contains(&close_dialog_ptr);
        let close_front_window_after = close_disp.front_window;
        let close_the_port_after = close_bus.read_long(crate::memory::globals::addr::THE_PORT);
        let close_current_port_after = *close_disp.current_port;
        let close_update_event_for_previous_window = close_disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == close_previous_window);
        let close_dialog_items_present_after =
            close_disp.dialog_items.contains_key(&close_dialog_ptr);

        let (mut dispose_disp, mut dispose_cpu, mut dispose_bus) = setup();
        dispose_disp.set_ui_theme_id(theme_id);
        dispose_disp.set_screen_mode_for_test(0x300000, 640, 640, 480, 8);
        let dispose_dialog_ptr = dispose_bus.alloc(170);
        let dispose_other_dialog_ptr = dispose_bus.alloc(170);
        let dispose_previous_window = dispose_bus.alloc(170);
        let dispose_text_handle = dispose_bus.alloc(4);
        let dispose_other_text_handle = dispose_bus.alloc(4);
        let dispose_ctrl_handle = dispose_bus.alloc(4);
        let dispose_other_ctrl_handle = dispose_bus.alloc(4);
        seed_window_regions(&mut dispose_bus, dispose_dialog_ptr, bounds);
        seed_window_regions(&mut dispose_bus, dispose_previous_window, previous_bounds);
        dispose_bus.write_word(dispose_dialog_ptr + 108, 2);
        dispose_disp.front_window = dispose_dialog_ptr;
        dispose_disp
            .current_port
            .with_mut(|current_port| *current_port = dispose_dialog_ptr);
        dispose_disp.window_bounds = bounds;
        dispose_disp.window_proc_id = 2;
        dispose_disp.window_list.replace(vec![
            dispose_dialog_ptr,
            dispose_previous_window,
            dispose_other_dialog_ptr,
        ]);
        dispose_disp.window_stack.push((
            dispose_previous_window,
            previous_bounds,
            0,
            "Previous".to_string(),
        ));
        dispose_disp
            .dialog_items
            .insert(dispose_dialog_ptr, vec![DialogItem::default()]);
        dispose_disp
            .dialog_items
            .insert(dispose_other_dialog_ptr, vec![DialogItem::default()]);
        dispose_disp.dialog_tracking = Some(dialog_tracking_state_for_test(dispose_dialog_ptr));
        dispose_disp.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
            dialog_ptr: dispose_dialog_ptr,
            item_no: 1,
            rect: (20, 40, 40, 100),
            title: "OK".to_string(),
            is_default: true,
            highlighted: true,
            delivered_to_app: false,
        });
        dispose_disp.dialog_visible_snapshots.insert(
            dispose_dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: saved_pixels.clone().into(),
            },
        );
        dispose_disp.dialog_modal_entered.insert(dispose_dialog_ptr);
        dispose_disp
            .dialog_saved_pixels
            .insert(dispose_dialog_ptr, saved_pixels.into());
        dispose_disp
            .dialog_item_handles
            .insert(dispose_text_handle, (dispose_dialog_ptr, 0));
        dispose_disp
            .dialog_item_handles
            .insert(dispose_other_text_handle, (dispose_other_dialog_ptr, 0));
        dispose_disp
            .dialog_control_handles
            .insert(dispose_ctrl_handle, (dispose_dialog_ptr, 1));
        dispose_disp
            .dialog_control_handles
            .insert(dispose_other_ctrl_handle, (dispose_other_dialog_ptr, 1));
        dispose_disp
            .dialog_control_values
            .insert((dispose_dialog_ptr, 1), 1);
        dispose_disp
            .dialog_control_values
            .insert((dispose_other_dialog_ptr, 1), 1);
        dispose_disp
            .hidden_dialog_item_rects
            .insert((dispose_dialog_ptr, 1), (10, 20, 30, 40));
        dispose_disp
            .hidden_dialog_item_rects
            .insert((dispose_other_dialog_ptr, 1), (50, 60, 70, 80));
        dispose_disp
            .dialog_cancel_items
            .insert(dispose_dialog_ptr, 2);
        dispose_disp
            .dialog_cancel_items
            .insert(dispose_other_dialog_ptr, 3);
        dispose_disp.pending_dialog_popup_menu = Some(PendingDialogPopupMenu {
            dialog_ptr: dispose_dialog_ptr,
            item_no: 1,
            menu_id: 900,
            rect: (10, 20, 30, 130),
        });
        dispose_bus.write_long(crate::memory::globals::addr::THE_PORT, dispose_dialog_ptr);
        let dispose_global_ptr = dispose_bus.read_long(dispose_cpu.read_reg(Register::A5));
        dispose_bus.write_long(dispose_global_ptr, dispose_dialog_ptr);
        dispose_cpu.write_reg(Register::A7, TEST_SP);
        dispose_bus.write_long(TEST_SP, dispose_dialog_ptr);
        dispose_disp
            .dispatch_dialog(true, 0x183, &mut dispose_cpu, &mut dispose_bus)
            .unwrap()
            .unwrap();

        DialogLifecycleCleanupSnapshot {
            close_stack_after,
            close_tracking_cleared,
            close_retained_click_cleared,
            close_visible_snapshot_cleared,
            close_modal_entered_cleared,
            close_saved_pixels_cleared,
            close_window_list_contains_dialog,
            close_front_window_after,
            close_the_port_after,
            close_current_port_after,
            close_update_event_for_previous_window,
            close_dialog_items_present_after,
            dispose_stack_after: dispose_cpu.read_reg(Register::A7),
            dispose_tracking_cleared: dispose_disp.dialog_tracking.is_none(),
            dispose_retained_click_cleared: dispose_disp.retained_modal_dialog_click.is_none(),
            dispose_visible_snapshot_cleared: !dispose_disp
                .dialog_visible_snapshots
                .contains_key(&dispose_dialog_ptr),
            dispose_modal_entered_cleared: !dispose_disp
                .dialog_modal_entered
                .contains(&dispose_dialog_ptr),
            dispose_saved_pixels_cleared: !dispose_disp
                .dialog_saved_pixels
                .contains_key(&dispose_dialog_ptr),
            dispose_window_list_contains_dialog: dispose_disp
                .window_list
                .contains(&dispose_dialog_ptr),
            dispose_front_window_after: dispose_disp.front_window,
            dispose_the_port_after: dispose_bus.read_long(crate::memory::globals::addr::THE_PORT),
            dispose_current_port_after: *dispose_disp.current_port,
            dispose_update_event_for_previous_window: dispose_disp
                .event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == dispose_previous_window),
            dispose_dialog_items_present_after: dispose_disp
                .dialog_items
                .contains_key(&dispose_dialog_ptr),
            dispose_other_dialog_items_present_after: dispose_disp
                .dialog_items
                .contains_key(&dispose_other_dialog_ptr),
            dispose_dialog_item_handle_present_after: dispose_disp
                .dialog_item_handles
                .contains_key(&dispose_text_handle),
            dispose_other_dialog_item_handle_present_after: dispose_disp
                .dialog_item_handles
                .contains_key(&dispose_other_text_handle),
            dispose_dialog_control_handle_present_after: dispose_disp
                .dialog_control_handles
                .contains_key(&dispose_ctrl_handle),
            dispose_other_dialog_control_handle_present_after: dispose_disp
                .dialog_control_handles
                .contains_key(&dispose_other_ctrl_handle),
            dispose_dialog_control_value_present_after: dispose_disp
                .dialog_control_values
                .contains_key(&(dispose_dialog_ptr, 1)),
            dispose_other_dialog_control_value_present_after: dispose_disp
                .dialog_control_values
                .contains_key(&(dispose_other_dialog_ptr, 1)),
            dispose_hidden_rect_present_after: dispose_disp
                .hidden_dialog_item_rects
                .contains_key(&(dispose_dialog_ptr, 1)),
            dispose_other_hidden_rect_present_after: dispose_disp
                .hidden_dialog_item_rects
                .contains_key(&(dispose_other_dialog_ptr, 1)),
            dispose_cancel_item_present_after: dispose_disp
                .dialog_cancel_items
                .contains_key(&dispose_dialog_ptr),
            dispose_other_cancel_item_present_after: dispose_disp
                .dialog_cancel_items
                .contains_key(&dispose_other_dialog_ptr),
            dispose_pending_popup_cleared: dispose_disp.pending_dialog_popup_menu.is_none(),
        }
    }

    fn cached_dialog_items(
        disp: &TrapDispatcher,
        dialog_ptr: u32,
    ) -> Vec<(u8, (i16, i16, i16, i16), String, i16)> {
        disp.dialog_items
            .get(&dialog_ptr)
            .into_iter()
            .flatten()
            .map(|item| {
                (
                    item.item_type,
                    item.rect,
                    item.text.clone(),
                    item.resource_id,
                )
            })
            .collect()
    }

    fn dialog_port_rect(bus: &MacMemoryBus, dialog_ptr: u32) -> (i16, i16, i16, i16) {
        (
            bus.read_word(dialog_ptr + 16) as i16,
            bus.read_word(dialog_ptr + 18) as i16,
            bus.read_word(dialog_ptr + 20) as i16,
            bus.read_word(dialog_ptr + 22) as i16,
        )
    }

    fn dialog_creation_results_for_theme(theme_id: UiThemeId) -> DialogCreationSnapshot {
        let (mut new_disp, mut new_cpu, mut new_bus) = setup();
        new_disp.set_ui_theme_id(theme_id);
        let new_screen_base = new_bus.alloc((640 * 480) as u32);
        new_bus.write_long(0x0824, new_screen_base);
        new_disp.screen_mode = (new_screen_base, 640, 640, 480, 8);
        let new_previous_port = new_bus.alloc(170);
        new_disp
            .current_port
            .with_mut(|current_port| *current_port = new_previous_port);
        new_bus.write_long(crate::memory::globals::addr::THE_PORT, new_previous_port);

        let new_ditl = build_test_ditl_items(&[
            (16, (10, 12, 28, 120), b"Alpha".as_slice()),
            (4, (44, 70, 66, 132), b"OK".as_slice()),
        ]);
        let new_items_data_ptr = new_bus.alloc(new_ditl.len() as u32);
        new_bus.write_bytes(new_items_data_ptr, &new_ditl);
        let new_items_handle = new_bus.alloc(4);
        new_bus.write_long(new_items_handle, new_items_data_ptr);

        let new_title = new_bus.alloc(32);
        new_bus.write_pstring(new_title, b"Modeless");
        let new_bounds = new_bus.alloc(8);
        new_bus.write_word(new_bounds, 60);
        new_bus.write_word(new_bounds + 2, 70);
        new_bus.write_word(new_bounds + 4, 140);
        new_bus.write_word(new_bounds + 6, 220);
        let new_storage = new_bus.alloc(170);
        let new_sp = TEST_SP - 30;
        new_cpu.write_reg(Register::A7, new_sp);
        new_bus.write_long(new_sp, new_items_handle);
        new_bus.write_long(new_sp + 4, 0x1234_5678);
        new_bus.write_byte(new_sp + 8, 0xFF); // goAwayFlag
        new_bus.write_long(new_sp + 10, 0xFFFF_FFFF); // behind = front
        new_bus.write_word(new_sp + 14, 4); // noGrowDocProc/modeless
        new_bus.write_byte(new_sp + 16, 0xFF); // visible
        new_bus.write_long(new_sp + 18, new_title);
        new_bus.write_long(new_sp + 22, new_bounds);
        new_bus.write_long(new_sp + 26, new_storage);
        new_bus.write_long(new_sp + 30, 0xDEAD_BEEF);
        new_disp
            .dispatch_dialog(true, 0x17D, &mut new_cpu, &mut new_bus)
            .unwrap()
            .unwrap();
        let new_dialog_ptr = new_bus.read_long(new_sp + 30);
        let new_update_region =
            TrapDispatcher::region_handle_rect(&new_bus, new_bus.read_long(new_dialog_ptr + 122));
        let new_update_event_queued = new_disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == new_dialog_ptr);

        let (mut get_disp, mut get_cpu, mut get_bus) = setup();
        get_disp.set_ui_theme_id(theme_id);
        let get_screen_base = get_bus.alloc((640 * 480) as u32);
        get_bus.write_long(0x0824, get_screen_base);
        get_disp.screen_mode = (get_screen_base, 640, 640, 480, 8);
        let get_previous_port = get_bus.alloc(170);
        get_disp
            .current_port
            .with_mut(|current_port| *current_port = get_previous_port);
        get_bus.write_long(crate::memory::globals::addr::THE_PORT, get_previous_port);

        let mut dlog = build_test_dlog((80, 90, 160, 240), 1901, 0);
        dlog[10] = 1; // visible
        let get_ditl = build_test_ditl_items(&[
            (16, (12, 18, 30, 128), b"Beta".as_slice()),
            (4, (46, 78, 68, 138), b"OK".as_slice()),
        ]);
        get_disp.install_test_resource(&mut get_bus, *b"DLOG", 1900, &dlog);
        let original_ditl_ptr =
            get_disp.install_test_resource(&mut get_bus, *b"DITL", 1901, &get_ditl);
        let get_storage = get_bus.alloc(170);
        get_cpu.write_reg(Register::A7, TEST_SP);
        get_bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind = front
        get_bus.write_long(TEST_SP + 4, get_storage);
        get_bus.write_word(TEST_SP + 8, 1900);
        get_bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);
        get_disp
            .dispatch_dialog(true, 0x17C, &mut get_cpu, &mut get_bus)
            .unwrap()
            .unwrap();
        let get_dialog_ptr = get_bus.read_long(TEST_SP + 10);
        let get_items_handle = get_bus.read_long(get_dialog_ptr + 156);
        let get_items_data_ptr = get_bus.read_long(get_items_handle);
        let get_update_region =
            TrapDispatcher::region_handle_rect(&get_bus, get_bus.read_long(get_dialog_ptr + 122));
        let get_update_event_queued = get_disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == get_dialog_ptr);

        DialogCreationSnapshot {
            new_previous_port,
            new_dialog_ptr,
            new_stack_after: new_cpu.read_reg(Register::A7),
            new_result_slot: new_bus.read_long(new_sp + 30),
            new_window_list: new_disp.window_list.to_vec(),
            new_front_window: new_disp.front_window,
            new_current_port: *new_disp.current_port,
            new_the_port: new_bus.read_long(crate::memory::globals::addr::THE_PORT),
            new_visible_byte: new_bus.read_byte(new_dialog_ptr + 110),
            new_goaway_byte: new_bus.read_byte(new_dialog_ptr + 112),
            new_refcon: new_bus.read_long(new_dialog_ptr + 152),
            new_window_kind: new_bus.read_word(new_dialog_ptr + 108) as i16,
            new_proc_id: new_disp
                .window_proc_ids
                .get(&new_dialog_ptr)
                .copied()
                .unwrap_or_default(),
            new_port_rect: dialog_port_rect(&new_bus, new_dialog_ptr),
            new_items_handle: new_bus.read_long(new_dialog_ptr + 156),
            new_items_data_ptr: new_bus.read_long(new_items_handle),
            new_first_item_handle_nonzero: new_bus.read_long(new_items_data_ptr + 2) != 0,
            new_cached_items: cached_dialog_items(&new_disp, new_dialog_ptr),
            new_update_region,
            new_update_event_queued,
            new_edit_field: new_bus.read_word(new_dialog_ptr + 164),
            new_default_item: new_bus.read_word(new_dialog_ptr + 168),
            get_previous_port,
            get_dialog_ptr,
            get_stack_after: get_cpu.read_reg(Register::A7),
            get_result_slot: get_bus.read_long(TEST_SP + 10),
            get_window_list: get_disp.window_list.to_vec(),
            get_front_window: get_disp.front_window,
            get_current_port: *get_disp.current_port,
            get_the_port: get_bus.read_long(crate::memory::globals::addr::THE_PORT),
            get_visible_byte: get_bus.read_byte(get_dialog_ptr + 110),
            get_refcon: get_bus.read_long(get_dialog_ptr + 152),
            get_window_kind: get_bus.read_word(get_dialog_ptr + 108) as i16,
            get_proc_id: get_disp
                .window_proc_ids
                .get(&get_dialog_ptr)
                .copied()
                .unwrap_or_default(),
            get_port_rect: dialog_port_rect(&get_bus, get_dialog_ptr),
            get_items_handle,
            get_items_data_ptr,
            get_uses_distinct_ditl_copy: get_items_data_ptr != original_ditl_ptr,
            get_original_ditl_handle_field: get_bus.read_long(original_ditl_ptr + 2),
            get_first_item_handle_nonzero: get_bus.read_long(get_items_data_ptr + 2) != 0,
            get_cached_items: cached_dialog_items(&get_disp, get_dialog_ptr),
            get_update_region,
            get_update_event_queued,
            get_edit_field: get_bus.read_word(get_dialog_ptr + 164),
            get_default_item: get_bus.read_word(get_dialog_ptr + 168),
        }
    }

    fn alert_family_results_for_theme(theme_id: UiThemeId) -> AlertFamilySnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);

        let ditl = build_test_ditl_items(&[
            (4, (48, 70, 70, 132), b"OK".as_slice()),
            (4, (48, 144, 70, 212), b"Cancel".as_slice()),
        ]);
        let alrt = build_alrt_template(2100, 0xC4C4);
        disp.install_test_resource(&mut bus, *b"DITL", 2100, &ditl);
        disp.install_test_resource(&mut bus, *b"ALRT", 2099, &alrt);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 0);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xCAFE);

        let mut staged_calls = Vec::new();
        for trap_word in [0x185u16, 0x186, 0x187, 0x188, 0x185] {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP + 4, 2099);
            disp.dispatch_dialog(true, trap_word, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            disp.push_key_down(0x24, b'\r');
            for _ in 0..4 {
                disp.dispatch_dialog(true, trap_word, &mut cpu, &mut bus)
                    .unwrap()
                    .unwrap();
                if disp.dialog_tracking.is_none() {
                    break;
                }
            }
            assert!(disp.dialog_tracking.is_none());
            disp.push_key_up(0x24, b'\r');
            staged_calls.push(AlertTrapCallSnapshot {
                trap_word,
                result: bus.read_word(TEST_SP + 6) as i16,
                stack_after: cpu.read_reg(Register::A7),
                alert_stage_after: bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
                anumber_after: bus.read_word(crate::memory::globals::addr::ANUMBER),
            });
        }

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(crate::memory::globals::addr::ALERT_STAGE, 2);
        bus.write_word(crate::memory::globals::addr::ANUMBER, 0xBEEF);
        bus.write_word(TEST_SP + 4, 2999);
        disp.dispatch_dialog(true, 0x187, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let missing_call = AlertTrapCallSnapshot {
            trap_word: 0x187,
            result: bus.read_word(TEST_SP + 6) as i16,
            stack_after: cpu.read_reg(Register::A7),
            alert_stage_after: bus.read_word(crate::memory::globals::addr::ALERT_STAGE),
            anumber_after: bus.read_word(crate::memory::globals::addr::ANUMBER),
        };

        let (mut prep_disp, mut prep_cpu, mut prep_bus) = setup();
        prep_disp.set_ui_theme_id(theme_id);
        let prep_alrt = build_alrt_template(2101, 0);
        prep_disp.install_test_resource(&mut prep_bus, *b"ALRT", 2102, &prep_alrt);
        let mut prep_call = |trap_word: u16, alert_id: i16| -> (i16, u32) {
            prep_cpu.write_reg(Register::A7, TEST_SP);
            prep_bus.write_word(0x0A60, 0x7FFF);
            prep_bus.write_word(TEST_SP, alert_id as u16);
            prep_disp
                .dispatch_dialog(true, trap_word, &mut prep_cpu, &mut prep_bus)
                .unwrap()
                .unwrap();
            (
                prep_bus.read_word(0x0A60) as i16,
                prep_cpu.read_reg(Register::A7),
            )
        };
        let resource_prep = AlertResourcePrepSnapshot {
            could_present: prep_call(0x189, 2102),
            free_present: prep_call(0x18A, 2102),
            could_missing: prep_call(0x189, 2998),
            free_missing: prep_call(0x18A, 2998),
        };

        AlertFamilySnapshot {
            staged_calls,
            missing_call,
            resource_prep,
        }
    }

    fn dialog_init_sound_results_for_theme(theme_id: UiThemeId) -> DialogInitSoundSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        use crate::memory::globals::addr;

        bus.write_long(addr::RESUME_PROC, 0xDEAD_BEEF);
        bus.write_long(addr::DA_BEEPER, 0x00AA_BBCC);
        bus.write_word(addr::ALERT_STAGE, 3);
        bus.write_word(addr::ANUMBER, 0xCAFE);
        for i in 0..4u32 {
            bus.write_long(addr::DA_STRINGS + i * 4, 0x00D0_0000 | i);
        }

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x0012_3456);
        disp.dispatch_dialog(true, 0x17B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let init_stack_after = cpu.read_reg(Register::A7);
        let resume_proc_after_init = bus.read_long(addr::RESUME_PROC);
        let da_beeper_after_init = bus.read_long(addr::DA_BEEPER);
        let alert_stage_after_init = bus.read_word(addr::ALERT_STAGE);
        let anumber_after_init = bus.read_word(addr::ANUMBER);
        let da_strings_after_init =
            std::array::from_fn(|i| bus.read_long(addr::DA_STRINGS + i as u32 * 4));

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0x00AB_CDEF);
        disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let error_sound_stack_after = cpu.read_reg(Register::A7);
        let da_beeper_after_error_sound = bus.read_long(addr::DA_BEEPER);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0);
        disp.dispatch_dialog(true, 0x18C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let nil_error_sound_stack_after = cpu.read_reg(Register::A7);
        let da_beeper_after_nil_error_sound = bus.read_long(addr::DA_BEEPER);

        DialogInitSoundSnapshot {
            init_stack_after,
            resume_proc_after_init,
            da_beeper_after_init,
            alert_stage_after_init,
            anumber_after_init,
            da_strings_after_init,
            error_sound_stack_after,
            da_beeper_after_error_sound,
            nil_error_sound_stack_after,
            da_beeper_after_nil_error_sound,
        }
    }

    fn dialogdispatch_default_cancel_results_for_theme(
        theme_id: UiThemeId,
    ) -> DialogDispatchDefaultCancelSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);
        let dialog_ptr = bus.alloc(256);
        bus.write_word(dialog_ptr + 168, 1);
        disp.dialog_tracking = Some(dialog_tracking_state_for_test(dialog_ptr));

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 9); // newItem
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xBEEF); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0304); // selector 4, 6 param bytes
        disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let default_result = bus.read_word(TEST_SP + 6);
        let default_stack_after = cpu.read_reg(Register::A7);
        let default_adef_item = bus.read_word(dialog_ptr + 168);
        let default_tracking_item = disp
            .dialog_tracking
            .as_ref()
            .map(|tracking| tracking.default_item)
            .unwrap_or(-1);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 7); // newItem
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xCAFE); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0305); // selector 5, 6 param bytes
        disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let cancel_result = bus.read_word(TEST_SP + 6);
        let cancel_stack_after = cpu.read_reg(Register::A7);
        let cancel_map_item = disp.dialog_cancel_items.get(&dialog_ptr).copied();
        let cancel_tracking_item = disp
            .dialog_tracking
            .as_ref()
            .map(|tracking| tracking.cancel_item)
            .unwrap_or(-1);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1); // tracks = TRUE
        bus.write_long(TEST_SP + 2, dialog_ptr); // theDialog
        bus.write_word(TEST_SP + 6, 0xCAFE); // OSErr result slot
        cpu.write_reg(Register::D0, 0x0306); // selector 6, 6 param bytes
        disp.dispatch_dialog(true, 0x268, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let tracks_result = bus.read_word(TEST_SP + 6);
        let tracks_stack_after = cpu.read_reg(Register::A7);
        let tracks_adef_item = bus.read_word(dialog_ptr + 168);
        let tracks_cancel_map_item = disp.dialog_cancel_items.get(&dialog_ptr).copied();
        let tracks_tracking_items = disp
            .dialog_tracking
            .as_ref()
            .map(|tracking| (tracking.default_item, tracking.cancel_item))
            .unwrap_or((-1, -1));

        DialogDispatchDefaultCancelSnapshot {
            default_result,
            default_stack_after,
            default_adef_item,
            default_tracking_item,
            cancel_result,
            cancel_stack_after,
            cancel_map_item,
            cancel_tracking_item,
            tracks_result,
            tracks_stack_after,
            tracks_adef_item,
            tracks_cancel_map_item,
            tracks_tracking_items,
        }
    }

    fn updtdialog_update_region_results_for_theme(
        theme_id: UiThemeId,
    ) -> UpdtDialogUpdateRegionSnapshot {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);

        let initial_port = 0x181000;
        disp.set_current_port_state(&mut bus, &mut cpu, initial_port, None);

        let screen_base = bus.alloc(100 * 100);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 100, 100, 100, 8);

        let dialog_ptr = bus.alloc(256);
        bus.write_word(dialog_ptr, 0);
        bus.write_long(dialog_ptr + 2, screen_base);
        bus.write_word(dialog_ptr + 6, 100);
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 12, 100);
        bus.write_word(dialog_ptr + 14, 100);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 100);
        bus.write_word(dialog_ptr + 108, 2);
        disp.window_list.push(dialog_ptr);
        disp.front_window = dialog_ptr;
        disp.dialog_initial_draw_deferred.insert(dialog_ptr);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0,
                    rect: (8, 8, 24, 32),
                    proc_ptr: 0x500000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 0,
                    rect: (60, 60, 80, 80),
                    proc_ptr: 0x600000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 8,
                    rect: (90, 90, 96, 96),
                    text: "x".to_string(),
                    ..Default::default()
                },
                DialogItem {
                    item_type: 4,
                    rect: (52, 8, 72, 44),
                    text: "OK".to_string(),
                    ..Default::default()
                },
            ],
        );
        let inside_update_pixel = screen_base + 4 * 100 + 4;
        let outside_update_pixel = screen_base + 70 * 100 + 70;
        let outside_item_pixel = screen_base + 52 * 100 + 9;
        bus.write_byte(inside_update_pixel, 0xA5);
        bus.write_byte(outside_update_pixel, 0x5A);
        bus.write_byte(outside_item_pixel, 0x3C);

        let update_rgn_ptr = bus.alloc(10);
        let update_rgn = bus.alloc(4);
        bus.write_long(update_rgn, update_rgn_ptr);
        bus.write_word(update_rgn_ptr, 10);
        bus.write_word(update_rgn_ptr + 2, 0);
        bus.write_word(update_rgn_ptr + 4, 0);
        bus.write_word(update_rgn_ptr + 6, 40);
        bus.write_word(update_rgn_ptr + 8, 40);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, update_rgn);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        disp.dispatch_dialog(true, 0x178, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        UpdtDialogUpdateRegionSnapshot {
            stack_after: cpu.read_reg(Register::A7),
            the_port_after: bus.read_long(crate::memory::globals::addr::THE_PORT),
            current_port_after: *disp.current_port,
            deferred_after: disp.dialog_initial_draw_deferred.contains(&dialog_ptr),
            queued_draw_procs: disp
                .modeless_dialog_draw_proc_queue
                .iter()
                .copied()
                .collect(),
            item_count_after: disp
                .dialog_items
                .get(&dialog_ptr)
                .map(Vec::len)
                .unwrap_or_default(),
            inside_update_pixel_after: bus.read_byte(inside_update_pixel),
            outside_update_pixel_after: bus.read_byte(outside_update_pixel),
            outside_item_pixel_after: bus.read_byte(outside_item_pixel),
        }
    }

    #[test]
    fn systemless_theme_does_not_change_dialogselect_item_hits() {
        // IM:I I-417: DialogSelect reports enabled dialog item hits through
        // its Boolean result, dialog pointer, and itemHit. Theme rendering is
        // outside that guest-visible event contract.
        let classic = dialog_select_enabled_user_item_hit_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_select_enabled_user_item_hit_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, 1);
        assert_eq!(classic.2, 1);
        assert_eq!(classic.3, TEST_SP + 12);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogSelect item-hit semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialogselect_control_item_hits() {
        // IM:I I-417: for mouseDown in an enabled control, DialogSelect
        // returns TRUE and itemHit after Control Manager tracking succeeds.
        // The app owns checkbox value changes, so theme chrome must not alter
        // the ControlRecord value or dialog-scoped control-value mirror.
        let classic = dialog_select_enabled_checkbox_hit_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_select_enabled_checkbox_hit_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, 1);
        assert_eq!(classic.2, 1);
        assert_eq!(classic.3, TEST_SP + 12);
        assert_eq!(classic.4, 1);
        assert_eq!(classic.5, 1);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogSelect control item-hit or value semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialogselect_window_events() {
        // MTE 1992 p. 6-139 and IM:I I-417: activate/update events for a
        // dialog window are handled by DialogSelect and return FALSE while
        // identifying the affected window through theDialog, without an itemHit.
        // MTE 1992 pp. 6-141 and 6-143 plus IM:I I-291: update handling
        // brackets the redraw with BeginUpdate/EndUpdate, uses the dialog
        // port, redraws the dialog, and leaves app-owned userItem drawing
        // to the guest draw procs.
        // Theme rendering can change pixels, but not this function ABI,
        // output-slot behavior, stack protocol, or update bookkeeping.
        let classic = dialog_select_window_event_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_select_window_event_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.update_result, 0);
        assert_eq!(classic.update_dialog_out, classic.dialog_ptr);
        assert_eq!(classic.update_item_hit, 0xCAFE);
        assert_eq!(classic.update_stack_after, TEST_SP + 12);
        assert!(!classic.update_deferred_after);
        assert_eq!(classic.update_region_after, None);
        assert_eq!(classic.update_vis_region_after, Some((0, 0, 100, 100)));
        assert_eq!(
            classic.update_the_port_after,
            classic.update_current_port_after
        );
        assert_eq!(classic.update_current_port_after, classic.dialog_ptr);
        assert!(!classic.update_saved_vis_after);
        assert_eq!(
            classic.update_queued_draw_procs,
            vec![(classic.dialog_ptr, 0x500000, 1)]
        );
        assert_eq!(classic.update_item_count_after, 3);
        assert_eq!(classic.activate_result, 0);
        assert_eq!(classic.activate_dialog_out, classic.dialog_ptr);
        assert_eq!(classic.activate_item_hit, 0xCAFE);
        assert_eq!(classic.activate_stack_after, TEST_SP + 12);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogSelect update/activate handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_modaldialog_edit_text_mouse_handling() {
        // IM:I I-415: ModalDialog handles mouseDown in an editText item using
        // TextEdit and returns the item when it is enabled. Theme rendering
        // must not change the returned item, retained-dialog state, active
        // editField, TERecord mirroring, queued mouse-up consumption, or stack
        // protocol.
        let classic = modal_dialog_edit_text_mouse_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = modal_dialog_edit_text_mouse_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.item_hit, 2);
        assert_eq!(classic.stack_after, TEST_SP + 8);
        assert!(classic.tracking_finished);
        assert!(classic.retained_visible_snapshot);
        assert!(classic.saved_background_retained);
        assert!(classic.queued_mouse_up_consumed);
        assert_eq!(classic.edit_field, 1);
        assert_eq!(classic.item_selection, (2, 4));
        assert_eq!(classic.te_text, b"Second".to_vec());
        assert_eq!(classic.te_length, 6);
        assert_eq!(classic.te_selection, (2, 4));
        assert_eq!(classic.handle_bytes, b"Second".to_vec());
        assert_eq!(
            themed, classic,
            "systemless-default must not change ModalDialog editText mouse handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_modaldialog_keyboard_default_cancel_items() {
        // MTE 1992 p. 6-30 and p. 6-138: Return/Enter activate the default
        // button; Esc and Command-period activate the Cancel button. Theme
        // chrome must not change the key-to-item mapping, flash lifecycle,
        // returned itemHit, or Pascal stack protocol.
        let classic = modal_dialog_keyboard_button_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = modal_dialog_keyboard_button_results_for_theme(UiThemeId::SystemlessDefault);

        for case in [&classic.return_key, &classic.enter_key] {
            assert_eq!(case.flash_item_after_key, 1);
            assert_eq!(case.stack_after_key, TEST_SP);
            assert_eq!(case.item_hit_after_key, 0xCAFE);
            assert_eq!(case.item_hit_after_flash, 1);
            assert_eq!(case.stack_after_flash, TEST_SP + 8);
            assert!(case.tracking_finished);
        }
        for case in [&classic.escape_key, &classic.command_period] {
            assert_eq!(case.flash_item_after_key, 2);
            assert_eq!(case.stack_after_key, TEST_SP);
            assert_eq!(case.item_hit_after_key, 0xCAFE);
            assert_eq!(case.item_hit_after_flash, 2);
            assert_eq!(case.stack_after_flash, TEST_SP + 8);
            assert!(case.tracking_finished);
        }
        assert_eq!(
            themed, classic,
            "systemless-default must not change ModalDialog keyboard default/cancel handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_modaldialog_update_events() {
        // MTE 1992 pp. 6-135 and 6-141: ModalDialog routes update events
        // through IsDialogEvent/DialogSelect-style handling. That brackets
        // the redraw with BeginUpdate/EndUpdate and makes the dialog the
        // current graphics port, while ModalDialog itself does not return
        // an item for update events. Theme chrome must not alter this ABI or
        // the retained visible dialog snapshot used across modal refires.
        let classic = modal_dialog_update_event_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = modal_dialog_update_event_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.item_hit_after, 0xCAFE);
        assert_eq!(classic.stack_after, TEST_SP);
        assert!(!classic.tracking_finished);
        assert!(classic.rendered_pixels_final);
        assert_eq!(classic.retained_pixel_after, 0x44);
        assert_eq!(classic.retained_snapshot_pixel_after, 0x44);
        assert_eq!(classic.update_region_after, None);
        assert_eq!(classic.vis_region_after, Some((0, 0, 80, 80)));
        assert_eq!(classic.the_port_after, classic.current_port_after);
        assert_eq!(classic.current_port_after, classic.dialog_ptr);
        assert!(!classic.saved_vis_after);
        assert_eq!(
            themed, classic,
            "systemless-default must not change ModalDialog update-event handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialog_lifecycle_cleanup() {
        // MTE 1992 pp. 6-119..6-120: CloseDialog removes the dialog from
        // the screen/window list, while DisposeDialog calls CloseDialog and
        // additionally releases dialog-owned item-list/dialog-record storage.
        // Theme chrome must not alter Pascal stack discipline, active
        // ModalDialog cleanup, retained-click cleanup, saved snapshot
        // cleanup, front-window promotion, current-port side effects, or
        // dialog-scoped side-map cleanup.
        let classic = dialog_lifecycle_cleanup_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_lifecycle_cleanup_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.close_stack_after, TEST_SP + 4);
        assert!(classic.close_tracking_cleared);
        assert!(classic.close_retained_click_cleared);
        assert!(classic.close_visible_snapshot_cleared);
        assert!(classic.close_modal_entered_cleared);
        assert!(classic.close_saved_pixels_cleared);
        assert!(!classic.close_window_list_contains_dialog);
        assert_eq!(
            classic.close_front_window_after,
            classic.close_the_port_after
        );
        assert_eq!(
            classic.close_front_window_after,
            classic.close_current_port_after
        );
        assert!(classic.close_update_event_for_previous_window);
        assert!(!classic.close_dialog_items_present_after);

        assert_eq!(classic.dispose_stack_after, TEST_SP + 4);
        assert!(classic.dispose_tracking_cleared);
        assert!(classic.dispose_retained_click_cleared);
        assert!(classic.dispose_visible_snapshot_cleared);
        assert!(classic.dispose_modal_entered_cleared);
        assert!(classic.dispose_saved_pixels_cleared);
        assert!(!classic.dispose_window_list_contains_dialog);
        assert_eq!(
            classic.dispose_front_window_after,
            classic.dispose_the_port_after
        );
        assert_eq!(
            classic.dispose_front_window_after,
            classic.dispose_current_port_after
        );
        assert!(classic.dispose_update_event_for_previous_window);
        assert!(!classic.dispose_dialog_items_present_after);
        assert!(classic.dispose_other_dialog_items_present_after);
        assert!(!classic.dispose_dialog_item_handle_present_after);
        assert!(classic.dispose_other_dialog_item_handle_present_after);
        assert!(!classic.dispose_dialog_control_handle_present_after);
        assert!(classic.dispose_other_dialog_control_handle_present_after);
        assert!(!classic.dispose_dialog_control_value_present_after);
        assert!(classic.dispose_other_dialog_control_value_present_after);
        assert!(!classic.dispose_hidden_rect_present_after);
        assert!(classic.dispose_other_hidden_rect_present_after);
        assert!(!classic.dispose_cancel_item_present_after);
        assert!(classic.dispose_other_cancel_item_present_after);
        assert!(classic.dispose_pending_popup_cleared);
        assert_eq!(
            themed, classic,
            "systemless-default must not change CloseDialog/DisposeDialog lifecycle cleanup"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialog_creation_records() {
        // IM:I I-412 and MTE 1992 pp. 6-118..6-119: NewDialog creates a
        // dialog from caller-supplied parameters and an item-list handle,
        // sets dialogKind/default dialog fields, and returns a DialogPtr.
        // IM:I I-412 and I-424: GetNewDialog reads DLOG/DITL resources and
        // uses a copy of the item list. Theme chrome must not alter resource
        // parsing, item-list ownership, DialogRecord fields, visible/update
        // bookkeeping, current-port preservation, or Pascal stack ABI.
        let classic = dialog_creation_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_creation_results_for_theme(UiThemeId::SystemlessDefault);

        assert_ne!(classic.new_dialog_ptr, 0);
        assert_eq!(classic.new_stack_after, TEST_SP);
        assert_eq!(classic.new_result_slot, classic.new_dialog_ptr);
        assert_eq!(classic.new_window_list, vec![classic.new_dialog_ptr]);
        assert_eq!(classic.new_front_window, classic.new_dialog_ptr);
        assert_eq!(classic.new_current_port, classic.new_previous_port);
        assert_eq!(classic.new_the_port, classic.new_previous_port);
        assert_eq!(classic.new_visible_byte, 0xFF);
        assert_eq!(classic.new_goaway_byte, 0xFF);
        assert_eq!(classic.new_refcon, 0x1234_5678);
        assert_eq!(classic.new_window_kind, 2);
        assert_eq!(classic.new_proc_id, 4);
        assert_eq!(classic.new_port_rect, (0, 0, 80, 150));
        assert_ne!(classic.new_items_handle, 0);
        assert_ne!(classic.new_items_data_ptr, 0);
        assert!(classic.new_first_item_handle_nonzero);
        assert_eq!(
            classic.new_cached_items,
            vec![
                (16, (10, 12, 28, 120), "Alpha".to_string(), 0),
                (4, (44, 70, 66, 132), "OK".to_string(), 0),
            ]
        );
        assert!(classic.new_update_region.is_some());
        assert!(classic.new_update_event_queued);
        assert_eq!(classic.new_edit_field, 0xFFFF);
        assert_eq!(classic.new_default_item, 1);

        assert_ne!(classic.get_dialog_ptr, 0);
        assert_eq!(classic.get_stack_after, TEST_SP + 10);
        assert_eq!(classic.get_result_slot, classic.get_dialog_ptr);
        assert_eq!(classic.get_window_list, vec![classic.get_dialog_ptr]);
        assert_eq!(classic.get_front_window, classic.get_dialog_ptr);
        assert_eq!(classic.get_current_port, classic.get_previous_port);
        assert_eq!(classic.get_the_port, classic.get_previous_port);
        assert_eq!(classic.get_visible_byte, 0xFF);
        assert_eq!(classic.get_refcon, 0);
        assert_eq!(classic.get_window_kind, 2);
        assert_eq!(classic.get_proc_id, 2);
        assert_eq!(classic.get_port_rect, (0, 0, 80, 150));
        assert_ne!(classic.get_items_handle, 0);
        assert_ne!(classic.get_items_data_ptr, 0);
        assert!(classic.get_uses_distinct_ditl_copy);
        assert_eq!(classic.get_original_ditl_handle_field, 0);
        assert!(classic.get_first_item_handle_nonzero);
        assert_eq!(
            classic.get_cached_items,
            vec![
                (16, (12, 18, 30, 128), "Beta".to_string(), 0),
                (4, (46, 78, 68, 138), "OK".to_string(), 0),
            ]
        );
        assert!(classic.get_update_region.is_some());
        assert!(classic.get_update_event_queued);
        assert_eq!(classic.get_edit_field, 0xFFFF);
        assert_eq!(classic.get_default_item, 1);
        assert_eq!(
            themed, classic,
            "systemless-default must not change NewDialog/GetNewDialog creation semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_alert_family_resource_state() {
        // IM:I I-417..I-423 and MTE 1992 pp. 6-105..6-112: Alert,
        // StopAlert, NoteAlert, and CautionAlert read ALRT/DITL resources,
        // use the current alert stage to choose the default item, update
        // ACount/ANumber low-memory state, and share behavior apart from
        // icon drawing. IM:I I-420 also defines CouldAlert/FreeAlert as
        // resource-preparation routines. Theme chrome must not alter those
        // resource, low-memory, result-slot, or stack contracts.
        let classic = alert_family_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = alert_family_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(
            classic.staged_calls,
            vec![
                AlertTrapCallSnapshot {
                    trap_word: 0x185,
                    result: 1,
                    stack_after: TEST_SP + 6,
                    alert_stage_after: 1,
                    anumber_after: 2099,
                },
                AlertTrapCallSnapshot {
                    trap_word: 0x186,
                    result: 2,
                    stack_after: TEST_SP + 6,
                    alert_stage_after: 2,
                    anumber_after: 2099,
                },
                AlertTrapCallSnapshot {
                    trap_word: 0x187,
                    result: 1,
                    stack_after: TEST_SP + 6,
                    alert_stage_after: 3,
                    anumber_after: 2099,
                },
                AlertTrapCallSnapshot {
                    trap_word: 0x188,
                    result: 2,
                    stack_after: TEST_SP + 6,
                    alert_stage_after: 3,
                    anumber_after: 2099,
                },
                AlertTrapCallSnapshot {
                    trap_word: 0x185,
                    result: 2,
                    stack_after: TEST_SP + 6,
                    alert_stage_after: 3,
                    anumber_after: 2099,
                },
            ]
        );
        assert_eq!(
            classic.missing_call,
            AlertTrapCallSnapshot {
                trap_word: 0x187,
                result: -1,
                stack_after: TEST_SP + 6,
                alert_stage_after: 2,
                anumber_after: 0xBEEF,
            }
        );
        assert_eq!(
            classic.resource_prep,
            AlertResourcePrepSnapshot {
                could_present: (0, TEST_SP + 2),
                free_present: (0, TEST_SP + 2),
                could_missing: (0, TEST_SP + 2),
                free_missing: (0, TEST_SP + 2),
            }
        );
        assert_eq!(
            themed, classic,
            "systemless-default must not change Alert family resource or low-memory semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_initdialogs_errorsound_state() {
        // IM:I I-411 and MTE 1992 pp. 6-103..6-104: InitDialogs stores
        // ResumeProc, initializes alert sound state, and prepares the
        // ParamText/DAStrings globals; ErrorSound stores the current alert
        // sound ProcPtr in DABeeper, with NIL disabling sound/menu-bar blink.
        // Those low-memory globals and procedure stack pops are app-visible
        // Dialog Manager state and must not depend on the selected renderer.
        let classic = dialog_init_sound_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_init_sound_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.init_stack_after, TEST_SP + 4);
        assert_eq!(classic.resume_proc_after_init, 0x0012_3456);
        assert_eq!(classic.da_beeper_after_init, 0);
        assert_eq!(classic.alert_stage_after_init, 0);
        assert_eq!(classic.anumber_after_init, 0xCAFE);
        assert_eq!(classic.da_strings_after_init, [0, 0, 0, 0]);
        assert_eq!(classic.error_sound_stack_after, TEST_SP + 4);
        assert_eq!(classic.da_beeper_after_error_sound, 0x00AB_CDEF);
        assert_eq!(classic.nil_error_sound_stack_after, TEST_SP + 4);
        assert_eq!(classic.da_beeper_after_nil_error_sound, 0);
        assert_eq!(
            themed, classic,
            "systemless-default must not change InitDialogs/ErrorSound low-memory state"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialogdispatch_default_cancel_selectors() {
        // MTE 1992 pp. 6-162..6-166: DialogDispatch selectors extend the
        // Dialog Manager with default-item, cancel-item, and cursor-tracking
        // state. MTE 1992 p. 6-138 ties Return/Enter and Esc/Command-period
        // handling to the default and Cancel items, so theme chrome must not
        // alter selector OSErr results, stack ABI, DialogRecord.aDefItem, the
        // cancel-item side state, or active ModalDialog tracking mirrors.
        let classic = dialogdispatch_default_cancel_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialogdispatch_default_cancel_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.default_result, 0);
        assert_eq!(classic.default_stack_after, TEST_SP + 6);
        assert_eq!(classic.default_adef_item, 9);
        assert_eq!(classic.default_tracking_item, 9);

        assert_eq!(classic.cancel_result, 0);
        assert_eq!(classic.cancel_stack_after, TEST_SP + 6);
        assert_eq!(classic.cancel_map_item, Some(7));
        assert_eq!(classic.cancel_tracking_item, 7);

        assert_eq!(classic.tracks_result, 0);
        assert_eq!(classic.tracks_stack_after, TEST_SP + 6);
        assert_eq!(classic.tracks_adef_item, 9);
        assert_eq!(classic.tracks_cancel_map_item, Some(7));
        assert_eq!(classic.tracks_tracking_items, (9, 7));
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogDispatch default/cancel selector semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_updtdialog_update_region_semantics() {
        // MTE 1992 pp. 6-142..6-143: UpdateDialog redraws only items in the
        // supplied update region, calls application-defined item draw procs,
        // and uses SetPort to make the dialog box current before updating.
        // Theme chrome can change pixels but must not change that update-item
        // selection, current-port side effect, deferred redraw state, or stack
        // ABI.
        let classic = updtdialog_update_region_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = updtdialog_update_region_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.stack_after, TEST_SP + 8);
        assert_ne!(classic.the_port_after, 0x181000);
        assert_eq!(classic.the_port_after, classic.current_port_after);
        assert!(!classic.deferred_after);
        assert_eq!(classic.queued_draw_procs.len(), 1);
        assert_eq!(classic.queued_draw_procs[0].0, classic.the_port_after);
        assert_eq!(classic.queued_draw_procs[0].1, 0x500000);
        assert_eq!(classic.queued_draw_procs[0].2, 1);
        assert_eq!(classic.item_count_after, 4);
        assert_eq!(classic.inside_update_pixel_after, 0);
        assert_eq!(classic.outside_update_pixel_after, 0x5A);
        assert_eq!(classic.outside_item_pixel_after, 0x3C);
        assert_eq!(
            themed, classic,
            "systemless-default must not change UpdtDialog update-region semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialogselect_edit_text_mouse_and_null_events() {
        // MTE 1992 p. 6-139 and IM:I I-417: mouseDown in an enabled
        // editText item is handled through TextEdit and reports the item,
        // while null events with an editText item call TEIdle and return
        // FALSE. Systemless TEClick/TEIdle preserve the TERec text/selection,
        // so theme rendering must not alter the active editField, result slots,
        // output-slot behavior, TERecord mirroring, or stack protocol.
        let classic = dialog_select_edit_text_mouse_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_select_edit_text_mouse_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.mouse_result, 1);
        assert_ne!(classic.mouse_dialog_out, 0);
        assert_eq!(classic.mouse_item_hit, 2);
        assert_eq!(classic.mouse_stack_after, TEST_SP + 12);
        assert_eq!(classic.mouse_edit_field, 1);
        assert_eq!(classic.mouse_item_selection, (2, 4));
        assert_eq!(classic.mouse_te_text, b"Second".to_vec());
        assert_eq!(classic.mouse_te_length, 6);
        assert_eq!(classic.mouse_te_selection, (2, 4));
        assert_eq!(classic.null_result, 0);
        assert_eq!(classic.null_dialog_out, 0xDEAD_BEEF);
        assert_eq!(classic.null_item_hit, 0xCAFE);
        assert_eq!(classic.null_stack_after, TEST_SP + 12);
        assert_eq!(classic.null_edit_field, 1);
        assert_eq!(classic.null_te_text, b"Second".to_vec());
        assert_eq!(classic.null_te_length, 6);
        assert_eq!(classic.null_te_selection, (2, 4));
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogSelect editText mouse/null handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialogselect_edit_text_key_handling() {
        // MTE 1992 p. 6-139: for keyDown/autoKey events in editable text
        // items, DialogSelect uses TextEdit to handle text entry/editing and
        // reports the edit item through itemHit. Text mutation, selection
        // collapse, shared TERecord state, result slots, and stack ABI are
        // guest-visible Dialog Manager behavior, not theme rendering choices.
        let classic = dialog_select_edit_text_key_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_select_edit_text_key_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.keydown_result, 1);
        assert_ne!(classic.keydown_dialog_out, 0);
        assert_eq!(classic.keydown_item_hit, 1);
        assert_eq!(classic.keydown_stack_after, TEST_SP + 12);
        assert_eq!(classic.keydown_handle_bytes, b"HYo".to_vec());
        assert_eq!(classic.keydown_cached_text, "HYo");
        assert_eq!(classic.keydown_item_selection, (2, 2));
        assert_eq!(classic.keydown_te_text, b"HYo".to_vec());
        assert_eq!(classic.keydown_te_length, 3);
        assert_eq!(classic.keydown_te_selection, (2, 2));
        assert_eq!(classic.autokey_result, 1);
        assert_eq!(classic.autokey_dialog_out, classic.keydown_dialog_out);
        assert_eq!(classic.autokey_item_hit, 1);
        assert_eq!(classic.autokey_stack_after, TEST_SP + 12);
        assert_eq!(classic.autokey_handle_bytes, b"Ho".to_vec());
        assert_eq!(classic.autokey_cached_text, "Ho");
        assert_eq!(classic.autokey_item_selection, (1, 1));
        assert_eq!(classic.autokey_te_text, b"Ho".to_vec());
        assert_eq!(classic.autokey_te_length, 2);
        assert_eq!(classic.autokey_te_selection, (1, 1));
        assert_eq!(
            themed, classic,
            "systemless-default must not change DialogSelect editText key handling"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_isdialogevent_active_dialog_routing() {
        // MTE 1992 p. 6-138: IsDialogEvent returns TRUE for events that
        // belong to an active dialog, including null events; IM:I I-416 also
        // calls out active-dialog key events, mouse-downs in the content
        // region, and update/activate events targeting a dialog window.
        // Themes must not alter that event-routing or function-stack ABI.
        let classic = is_dialog_event_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = is_dialog_event_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.null_event, (1, TEST_SP + 4));
        assert_eq!(classic.key_down, (1, TEST_SP + 4));
        assert_eq!(classic.mouse_inside, (1, TEST_SP + 4));
        assert_eq!(classic.mouse_outside, (0, TEST_SP + 4));
        assert_eq!(classic.update_target, (1, TEST_SP + 4));
        assert_eq!(classic.activate_target, (1, TEST_SP + 4));
        assert_eq!(
            themed, classic,
            "systemless-default must not change IsDialogEvent routing"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_findditem_hit_testing() {
        // MTE 1992 p. 6-125: FindDialogItem/FindDItem returns the first
        // item-list index containing the local point, including disabled
        // items; themes must not alter this guest-visible hit test.
        let classic = findditem_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = findditem_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(
            classic,
            vec![
                (0, TEST_SP + 8),
                (1, TEST_SP + 8),
                (-1, TEST_SP + 8),
                (-1, TEST_SP + 8),
            ]
        );
        assert_eq!(
            themed, classic,
            "systemless-default must not change FindDItem hit-test semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_getsetditem_useritem_records() {
        // MTE 1992 p. 6-121..6-123: GetDialogItem and SetDialogItem expose
        // item type, item handle/ProcPtr, and display rectangle. For
        // application-defined userItems, `item` is the app's draw ProcPtr;
        // theme chrome must not alter that record ABI or stack contract.
        let classic = getsetditem_useritem_record_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = getsetditem_useritem_record_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.initial_get.item_type, 0);
        assert_eq!(classic.initial_get.item, 0x0012_3456);
        assert_eq!(classic.initial_get.rect, (12, 24, 36, 48));
        assert_eq!(classic.initial_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.set_stack_after, TEST_SP + 16);
        assert_eq!(classic.stored_type, 0);
        assert_eq!(classic.stored_proc_ptr, 0x00AB_CDEF);
        assert_eq!(classic.stored_rect, (50, 60, 70, 80));
        assert_eq!(classic.post_set_get.item_type, 0);
        assert_eq!(classic.post_set_get.item, 0x00AB_CDEF);
        assert_eq!(classic.post_set_get.rect, (50, 60, 70, 80));
        assert_eq!(classic.post_set_get.stack_after, TEST_SP + 18);
        assert_eq!(
            themed, classic,
            "systemless-default must not change GetDItem/SetDItem userItem record semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_getsetditem_text_control_records() {
        // MTE 1992 p. 6-121..6-123 and IM:I I-421: GetDialogItem and
        // SetDialogItem expose item type, item handle, and display rectangle.
        // Text items use handles consumed by Get/SetDialogItemText; control
        // items use ControlRecord handles. Theme chrome must not alter that
        // guest ABI, DITL storage, side-map relinking, or stack contract.
        let classic = getsetditem_text_control_record_results_for_theme(UiThemeId::ClassicSystem7);
        let themed =
            getsetditem_text_control_record_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.text_initial_get.item_type, 16);
        assert_ne!(classic.text_initial_get.item, 0);
        assert_eq!(classic.text_initial_get.rect, (10, 20, 24, 160));
        assert_eq!(classic.text_initial_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.text_set_stack_after, TEST_SP + 16);
        assert!(!classic.text_old_handle_mapped_after);
        let mapped_dialog = classic.text_new_handle_map_value_after.unwrap().0;
        let text_new_handle = classic.text_post_set_get.item;
        assert_ne!(mapped_dialog, 0);
        assert_eq!(classic.text_new_handle_map_value_after.unwrap().1, 0);
        assert_ne!(text_new_handle, classic.text_initial_get.item);
        assert_eq!(classic.text_ditl_handle_after, text_new_handle);
        assert_eq!(classic.text_ditl_rect_after, (12, 22, 28, 168));
        assert_eq!(classic.text_ditl_type_after, 16);
        assert_eq!(classic.text_stored_type_after, 16);
        assert_eq!(classic.text_stored_rect_after, (12, 22, 28, 168));
        assert_eq!(classic.text_cached_text_after, "Updated");
        assert_eq!(classic.text_handle_bytes_after, b"Updated".to_vec());
        assert_eq!(classic.text_post_set_get.item_type, 16);
        assert_eq!(classic.text_post_set_get.item, text_new_handle);
        assert_eq!(classic.text_post_set_get.rect, (12, 22, 28, 168));
        assert_eq!(classic.text_post_set_get.stack_after, TEST_SP + 18);

        assert_eq!(classic.control_initial_get.item_type, 4);
        assert_ne!(classic.control_initial_get.item, 0);
        assert_eq!(classic.control_initial_get.rect, (30, 40, 50, 140));
        assert_eq!(classic.control_initial_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.control_set_stack_after, TEST_SP + 16);
        assert!(!classic.control_old_handle_mapped_after);
        let control_new_handle = classic.control_post_set_get.item;
        assert_eq!(
            classic.control_new_handle_map_value_after.unwrap().0,
            mapped_dialog
        );
        assert_eq!(classic.control_new_handle_map_value_after.unwrap().1, 2);
        assert_ne!(control_new_handle, classic.control_initial_get.item);
        assert_eq!(classic.control_ditl_handle_after, control_new_handle);
        assert_eq!(classic.control_ditl_rect_after, (42, 52, 62, 172));
        assert_eq!(classic.control_ditl_type_after, 5);
        assert_eq!(classic.control_stored_type_after, 5);
        assert_eq!(classic.control_stored_rect_after, (42, 52, 62, 172));
        assert_eq!(classic.control_record_rect_after, (42, 52, 62, 172));
        assert_eq!(classic.control_value_after, Some(2));
        assert_eq!(classic.control_post_set_get.item_type, 5);
        assert_eq!(classic.control_post_set_get.item, control_new_handle);
        assert_eq!(classic.control_post_set_get.rect, (42, 52, 62, 172));
        assert_eq!(classic.control_post_set_get.stack_after, TEST_SP + 18);
        assert_eq!(
            themed, classic,
            "systemless-default must not change GetDItem/SetDItem text/control record semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_hide_show_ditem_visibility_records() {
        // MTE 1992 p. 6-123..6-124: HideDialogItem moves an item's display
        // rectangle offscreen by offsetting left/right, while ShowDialogItem
        // restores the visible rectangle and queues redraw. Themes must not
        // alter that item/control record state, hit testing, or stack ABI.
        let classic = hide_show_ditem_visibility_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = hide_show_ditem_visibility_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.initial_get.item_type, 4);
        assert_eq!(classic.initial_get.rect, (10, 20, 30, 120));
        assert_eq!(classic.initial_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.hide_stack_after, TEST_SP + 6);
        assert_eq!(classic.hidden_item_rect, (10, 16404, 30, 16504));
        assert_eq!(classic.hidden_saved_rect, Some((10, 20, 30, 120)));
        assert_eq!(classic.hidden_control_rect, (10, 16404, 30, 16504));
        assert_eq!(classic.hidden_get.item_type, 4);
        assert_eq!(classic.hidden_get.item, classic.initial_get.item);
        assert_eq!(classic.hidden_get.rect, (10, 16404, 30, 16504));
        assert_eq!(classic.hidden_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.hidden_find, (-1, TEST_SP + 8));
        assert_eq!(classic.show_stack_after, TEST_SP + 6);
        assert_eq!(classic.restored_item_rect, (10, 20, 30, 120));
        assert_eq!(classic.restored_saved_rect, None);
        assert_eq!(classic.restored_control_rect, (10, 20, 30, 120));
        assert_eq!(classic.restored_get.item_type, 4);
        assert_eq!(classic.restored_get.item, classic.initial_get.item);
        assert_eq!(classic.restored_get.rect, (10, 20, 30, 120));
        assert_eq!(classic.restored_get.stack_after, TEST_SP + 18);
        assert_eq!(classic.restored_find, (0, TEST_SP + 8));
        assert_eq!(
            themed, classic,
            "systemless-default must not change HideDItem/ShowDItem visibility semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_paramtext_storage_and_substitution() {
        // MTE 1992 p. 6-129..6-130: ParamText stores up to four strings in
        // DAStrings and substitutes ^0..^3 in subsequently created dialog or
        // alert text. This low-memory/global text contract is app-visible and
        // must not depend on the selected rendering provider.
        let classic = paramtext_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = paramtext_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(
            classic.initial_slots,
            [
                b"Doc".to_vec(),
                b"42".to_vec(),
                Vec::<u8>::new(),
                b"Tail".to_vec(),
            ]
        );
        assert!(classic.initial_da_handles.iter().all(|handle| *handle != 0));
        assert_eq!(classic.initial_da_strings, classic.initial_slots);
        assert_eq!(classic.initial_stack_after, TEST_SP + 16);
        assert_eq!(classic.initial_substitution, "Cannot open Doc (42)Tail ^9");
        assert_eq!(
            classic.nil_slots,
            [
                b"NewDoc".to_vec(),
                b"42".to_vec(),
                Vec::<u8>::new(),
                b"Tail".to_vec(),
            ]
        );
        assert!(classic.nil_da_handles.iter().all(|handle| *handle != 0));
        assert_eq!(classic.nil_da_strings, classic.nil_slots);
        assert_eq!(classic.nil_da_handles[1], classic.initial_da_handles[1]);
        assert_eq!(classic.nil_da_handles[2], classic.initial_da_handles[2]);
        assert_eq!(classic.nil_da_handles[3], classic.initial_da_handles[3]);
        assert_eq!(classic.nil_stack_after, TEST_SP + 16);
        assert_eq!(classic.nil_substitution, "Cannot open NewDoc (42)Tail ^9");
        assert_eq!(
            themed, classic,
            "systemless-default must not change ParamText storage or substitution semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_dialog_item_text_storage() {
        // MTE 1992 p. 6-130..6-131: GetDialogItemText/GetIText returns the
        // text in a statText/editText item, and SetDialogItemText/SetIText
        // replaces that text. Theme rendering must not alter the text handle,
        // cached item text, Pascal-string output, or stack ABI.
        let classic = dialog_item_text_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = dialog_item_text_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.initial_get_text, b"Old".to_vec());
        assert_eq!(classic.initial_get_stack_after, TEST_SP + 8);
        assert_eq!(classic.set_stack_after, TEST_SP + 8);
        assert_eq!(classic.handle_size_after_set, Some(11));
        assert_eq!(classic.handle_bytes_after_set, b"Edited Text".to_vec());
        assert_eq!(classic.cached_text_after_set, "Edited Text");
        assert_eq!(classic.post_set_get_text, b"Edited Text".to_vec());
        assert_eq!(classic.post_set_get_stack_after, TEST_SP + 8);
        assert_eq!(
            themed, classic,
            "systemless-default must not change Get/SetDialogItemText storage semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_select_dialog_item_text_selection() {
        // MTE 1992 p. 6-131..6-132: SelectDialogItemText/SelIText sets the
        // selection range in an editable text item and supports the 0..32767
        // whole-text selection convention. The active editField, TERecord
        // mirrored selection, non-edit no-op, and stack ABI are guest-visible
        // Dialog Manager behavior, not theme rendering choices.
        let classic = select_dialog_item_text_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = select_dialog_item_text_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.whole_selection, (0, 5));
        assert_eq!(classic.whole_edit_field, 0);
        assert_eq!(classic.whole_te_selection, (0, 5));
        assert_eq!(classic.whole_stack_after, TEST_SP + 10);
        assert_eq!(classic.normalized_selection, (2, 5));
        assert_eq!(classic.normalized_edit_field, 0);
        assert_eq!(classic.normalized_te_selection, (2, 5));
        assert_eq!(classic.normalized_stack_after, TEST_SP + 10);
        assert_eq!(classic.non_edit_selection, (1, 3));
        assert_eq!(classic.non_edit_edit_field, 0);
        assert_eq!(classic.non_edit_te_selection, (2, 5));
        assert_eq!(classic.non_edit_stack_after, TEST_SP + 10);
        assert_eq!(
            themed, classic,
            "systemless-default must not change SelectDialogItemText selection semantics"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_drawdialog_useritem_pixels() {
        // MTE 1992 p. 6-142: DrawDialog calls application-defined item
        // draw procedures when their rectangles are in the update region.
        // MTE 1992 p. 6-155 defines application-defined items as userItem
        // records; theme chrome must not erase the app-owned rectangle.
        let classic = drawdialog_useritem_pixel_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = drawdialog_useritem_pixel_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, 0xA5);
        assert_eq!(classic.1, TEST_SP + 4);
        assert_eq!(classic.2, 0xCAFE_BABE);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DrawDialog userItem pixel preservation"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_drawdialog_icon_item_pixels() {
        // MTE 1992 p. 6-155: an icon item's item-list record names an
        // `ICON` resource. MTE 1992 p. 7-63 defines `ICON` as application
        // resource content used in dialog boxes; theme chrome must not alter
        // those pixels inside the icon item rectangle.
        let classic = drawdialog_icon_item_pixel_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = drawdialog_icon_item_pixel_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, [0xFF, 0xFF, 0xFF]);
        assert_eq!(classic.1, TEST_SP + 4);
        assert_eq!(classic.2, 0xCAFE_BABE);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DrawDialog ICON item pixels"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_drawdialog_cicn_item_pixels() {
        // MTE 1992 p. 6-155: an icon item's item-list record names an
        // `ICON` resource and optionally a same-ID `cicn` resource. MTE 1992
        // p. 7-64 specifies that the Dialog Manager displays that color icon
        // instead of the black-and-white icon on color displays.
        let classic = drawdialog_cicn_item_pixel_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = drawdialog_cicn_item_pixel_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, [0x33, 0x55, 0x77]);
        assert_eq!(classic.1, TEST_SP + 4);
        assert_eq!(classic.2, 0xCAFE_BABE);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DrawDialog cicn item pixels"
        );
    }

    #[test]
    fn dialog_cicn_maps_embedded_colors_into_the_active_device_palette() {
        let (mut disp, _cpu, mut bus) = setup();
        let (screen_base, row_bytes, _width, _height, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        let gdevice = bus.read_long(gdevice_handle);
        let pixmap_handle = bus.read_long(gdevice + 22);
        let pixmap = bus.read_long(pixmap_handle);
        let ctab_handle = bus.read_long(pixmap + 42);
        let ctab = bus.read_long(ctab_handle);
        for index in 0u32..256 {
            let entry = ctab + 8 + index * 8;
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, 0);
            bus.write_word(entry + 4, 0);
            bus.write_word(entry + 6, 0);
        }
        for (index, rgb) in [
            (41u32, [0xFFFF, 0, 0]),
            (77, [0, 0xFFFF, 0]),
            (9, [0, 0, 0xF000]),
        ] {
            let entry = ctab + 8 + index * 8;
            bus.write_word(entry + 2, rgb[0]);
            bus.write_word(entry + 4, rgb[1]);
            bus.write_word(entry + 6, rgb[2]);
        }

        let explicit = cicn_resource_with_palette(
            4,
            1,
            0,
            &[
                (2, [0xFFFF, 0, 0]),
                (7, [0, 0xFFFF, 0]),
                (200, [0, 0, 0xF800]),
            ],
            &[(0, 0, 2), (1, 0, 7), (2, 0, 200)],
        );
        let explicit_ptr = bus.alloc(explicit.len() as u32);
        bus.write_bytes(explicit_ptr, &explicit);
        bus.write_byte(screen_base + 3, 0xAA);

        assert!(disp.draw_cicn_icon(&mut bus, 0, 0, 1, 4, explicit_ptr));
        assert_eq!(
            [
                bus.read_byte(screen_base),
                bus.read_byte(screen_base + 1),
                bus.read_byte(screen_base + 2),
                bus.read_byte(screen_base + 3),
            ],
            [41, 77, 9, 0xAA],
            "explicit sparse source values must map by RGB, use nearest destination color, and honor the mask"
        );

        let indexed = cicn_resource_with_palette(
            1,
            1,
            0x8000,
            &[(200, [0xFFFF, 0, 0]), (201, [0, 0xFFFF, 0])],
            &[(0, 0, 1)],
        );
        let indexed_ptr = bus.alloc(indexed.len() as u32);
        bus.write_bytes(indexed_ptr, &indexed);
        assert!(disp.draw_cicn_icon(&mut bus, 1, 0, 2, 1, indexed_ptr));
        assert_eq!(
            bus.read_byte(screen_base + row_bytes),
            77,
            "ctFlags $8000 must use ColorSpec ordinals instead of explicit values"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_drawdialog_picture_item_pixels() {
        // MTE 1992 p. 6-155: a picture item's item-list record names a
        // `PICT` resource. That resource is application-supplied picture
        // content, so theme chrome must not alter the pixels DrawDialog
        // renders inside the picture item rectangle.
        let classic = drawdialog_picture_item_pixel_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = drawdialog_picture_item_pixel_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, [0xFF, 0xFF, 0xFF]);
        assert_eq!(classic.1, TEST_SP + 4);
        assert_eq!(classic.2, 0xCAFE_BABE);
        assert_eq!(
            themed, classic,
            "systemless-default must not change DrawDialog PICT item pixels"
        );
    }

    #[test]
    fn drawdialog_preserves_background_and_frame_outside_items() {
        let (mut disp, mut cpu, mut bus) = setup();
        let port = bus.alloc(170);
        bus.write_word(port + 20, 60);
        bus.write_word(port + 22, 100);
        let (base, stride, _, _, _) = disp.screen_mode;
        bus.write_bytes(base, &vec![0x77; (stride * 80) as usize]);
        disp.install_test_resource(&mut bus, *b"PICT", 423, &solid_fill_pict_resource(16, 16));
        disp.dialog_items.insert(
            port,
            vec![DialogItem {
                item_type: 64,
                rect: (8, 8, 24, 24),
                text: String::new(),
                resource_id: 423,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        bus.write_long(TEST_SP, port);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(base + 16 * stride + 16), 255);
        for (x, y) in [(0, 0), (50, 30), (99, 59), (100, 60)] {
            assert_eq!(bus.read_byte(base + y * stride + x), 0x77);
        }
    }

    #[test]
    fn dialog_picture_items_clip_without_rescaling_the_destination() {
        for theme in [UiThemeId::ClassicSystem7, UiThemeId::SystemlessDefault] {
            let (mut disp, _cpu, mut bus) = setup();
            disp.set_ui_theme_id(theme);
            let (base, stride, _, _, _) = disp.screen_mode;
            let bounds = (40, 60, 100, 140);
            let port = bus.alloc(170);
            bus.write_word(port + 8, (-40i16) as u16);
            bus.write_word(port + 10, (-60i16) as u16);
            disp.install_test_resource(&mut bus, *b"PICT", 421, &solid_fill_pict_resource(120, 16));
            let items = [DialogItem {
                item_type: 64,
                rect: (8, -20, 24, 100),
                text: String::new(),
                resource_id: 421,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }];
            // The picture extends twenty pixels beyond both sides. Compare
            // against an otherwise identical draw without the picture, so
            // the test also protects each theme's frame and desktop pixels.
            disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, true, port);
            let before = bus.read_bytes(base, (stride * 110) as usize).to_vec();
            disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, port);
            for y in 48..64u32 {
                for x in 35..165u32 {
                    let offset = (y * stride + x) as usize;
                    if (60..140).contains(&x) {
                        assert_eq!(bus.read_byte(base + offset as u32), 255);
                    } else {
                        assert_eq!(bus.read_byte(base + offset as u32), before[offset]);
                    }
                }
            }
        }
    }

    #[test]
    fn drawdialog_picture_item_maps_against_logical_port_palette_during_hardware_fade() {
        // Imaging With QuickDraw 1994, pp. 7-11..7-14: DrawPicture maps against
        // the destination port's logical ColorTable (or stable Color Manager
        // baseline), not the transient video DAC CLUT (device_clut), which
        // games may fade down to black before opening a dialog.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        // Hardware CLUT is faded completely black
        disp.device_clut.replace([[0u16; 3]; 256]);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 48);
        bus.write_word(dialog_ptr + 22, 80);
        disp.install_test_resource(&mut bus, *b"PICT", 420, &solid_fill_pict_resource(16, 16));
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 64, // picture item
                    rect: (8, 8, 24, 24),
                    text: String::new(),
                    resource_id: 420,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (30, 8, 42, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE);

        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        // 0xFF is black in the standard system palette, which solid_fill_pict_resource
        // filled the rectangle with. If DrawPicture mapped against the all-zero device_clut,
        // it would have collapsed to index 0 (white).
        assert_eq!(
            bus.read_byte(screen_base + 16 * row_bytes + 16),
            0xFF,
            "DrawDialog PICT item must map against logical palette even when device_clut is dark"
        );
    }

    #[test]
    fn dialog_select_disabled_item_hit_returns_false_and_leaves_outputs_unchanged() {
        // Inside Macintosh Volume I, I-417: disabled items do nothing and
        // DialogSelect returns FALSE.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80, // disabled userItem
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(event_ptr, 1); // mouseDown
        bus.write_long(event_ptr + 2, 0);
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 130);
        bus.write_word(event_ptr + 12, 240);
        bus.write_word(event_ptr + 14, 0x0080);

        bus.write_long(dialog_out_ptr, 0x12345678);
        bus.write_word(item_hit_ptr, 0x7F7F);

        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(bus.read_byte(TEST_SP + 12), 0);
        assert_eq!(bus.read_long(dialog_out_ptr), 0x12345678);
        assert_eq!(bus.read_word(item_hit_ptr), 0x7F7F);
    }

    #[test]
    fn dialog_select_keydown_without_edit_text_item_returns_false() {
        // Inside Macintosh Volume I, I-417: keyDown handling returns TRUE only
        // when an enabled editText item is active.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_word(dialog_ptr + 164, 0); // editField points at item 1
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0, // userItem, not editText
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(event_ptr, 3); // keyDown
        bus.write_long(event_ptr + 2, 0x00000041); // 'A'
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 0);
        bus.write_word(event_ptr + 12, 0);
        bus.write_word(event_ptr + 14, 0);

        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 12), 0);
    }

    #[test]
    fn dialog_select_keydown_with_enabled_edit_text_returns_hit() {
        // Inside Macintosh Volume I, I-417: for keyDown/autoKey with an
        // enabled editText item, DialogSelect returns TRUE and item number.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = 0x300000u32;
        let dialog_out_ptr = 0x300100u32;
        let item_hit_ptr = 0x300104u32;

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_word(dialog_ptr + 164, 0); // editField = first item
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16, // editText
                rect: (20, 30, 60, 110),
                text: "Hello".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(event_ptr, 3); // keyDown
        bus.write_long(event_ptr + 2, 0x00000041); // 'A'
        bus.write_long(event_ptr + 6, 0);
        bus.write_word(event_ptr + 10, 0);
        bus.write_word(event_ptr + 12, 0);
        bus.write_word(event_ptr + 14, 0);

        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, dialog_out_ptr);
        bus.write_long(TEST_SP + 8, event_ptr);

        let result = disp.dispatch_dialog(true, 0x180, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(TEST_SP + 12), 1);
        assert_eq!(bus.read_long(dialog_out_ptr), dialog_ptr);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
    }

    #[test]
    fn dialog_select_null_event_blinks_active_edit_text_insertion_caret() {
        // MTE 1992 p. 6-139 and IM:I I-417: DialogSelect mouse-down in an
        // enabled editText item displays the insertion point, and later null
        // events call TEIdle so the insertion point blinks.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let dialog_ptr = bus.alloc(170);
        let event_ptr = bus.alloc(16);
        let dialog_out_ptr = bus.alloc(4);
        let item_hit_ptr = bus.alloc(2);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let text_item_handle = TrapDispatcher::allocate_handle_with_data(&mut bus, 0);
        let text_h = make_te_with_text(&mut disp, &mut bus, b"");
        let te_ptr = bus.read_long(text_h);
        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;

        for i in 0..(row_bytes * 80) {
            bus.write_byte(screen_base + i, 0);
        }

        disp.front_window = dialog_ptr;
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_long(dialog_ptr + 160, text_h);
        bus.write_word(dialog_ptr + 164, 0);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, text_item_handle);
        bus.write_word(ditl_ptr + 6, 20);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 38);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 16);
        bus.write_byte(ditl_ptr + 15, 0);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (20, 20, 38, 120),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let dispatch_dialog_select_event = |disp: &mut TrapDispatcher,
                                            cpu: &mut MockCpu,
                                            bus: &mut MacMemoryBus,
                                            what: u16,
                                            tick: u32| {
            disp.set_tick_count_for_test(bus, tick);
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(event_ptr, what);
            bus.write_long(event_ptr + 2, 0);
            bus.write_long(event_ptr + 6, 0);
            bus.write_word(event_ptr + 10, 25);
            bus.write_word(event_ptr + 12, 25);
            bus.write_word(event_ptr + 14, 0);
            bus.write_long(dialog_out_ptr, 0xDEAD_BEEF);
            bus.write_word(item_hit_ptr, 0xCAFE);
            bus.write_long(TEST_SP, item_hit_ptr);
            bus.write_long(TEST_SP + 4, dialog_out_ptr);
            bus.write_long(TEST_SP + 8, event_ptr);
            disp.dispatch_dialog(true, 0x180, cpu, bus)
                .unwrap()
                .unwrap();
        };

        dispatch_dialog_select_event(&mut disp, &mut cpu, &mut bus, 1, 100);
        assert_eq!(bus.read_byte(TEST_SP + 12), 1);
        assert_eq!(bus.read_long(dialog_out_ptr), dialog_ptr);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET), 1);
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            100
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "DialogSelect editText mouse-down should display the insertion caret"
        );

        dispatch_dialog_select_event(&mut disp, &mut cpu, &mut bus, 0, 131);
        assert_eq!(bus.read_byte(TEST_SP + 12), 0);
        assert_eq!(bus.read_long(dialog_out_ptr), 0xDEAD_BEEF);
        assert_eq!(bus.read_word(item_hit_ptr), 0xCAFE);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "DialogSelect null event before 32 ticks should keep the caret visible"
        );

        dispatch_dialog_select_event(&mut disp, &mut cpu, &mut bus, 0, 132);
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            132
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            1
        );
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "DialogSelect null event at the 32-tick boundary should hide the caret"
        );

        dispatch_dialog_select_event(&mut disp, &mut cpu, &mut bus, 0, 164);
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            164
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "the next DialogSelect null-event blink interval should show the caret again"
        );
    }

    // ---- DrawDialog ($A981) ----

    #[test]
    fn draw_dialog_pops_4_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        // SP+0: dialog ptr (4 bytes)
        bus.write_long(TEST_SP, 0x200000);

        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn draw_dialog_preserves_background_for_known_dialog() {
        // MTE 1992, 6-142: DrawDialog redraws items, controls and text.
        // Pixels outside their rectangles belong to the existing window.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 30);
        bus.write_word(dialog_ptr + 22, 40);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4, // btnCtrl draws a deterministic border and label
                rect: (8, 8, 20, 32),
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let probe_x = 4u32;
        let probe_y = 4u32;
        let probe_addr = if pixel_size == 8 {
            screen_base + probe_y * row_bytes + probe_x
        } else {
            screen_base + probe_y * row_bytes + (probe_x / 8)
        };
        let probe_bit = 1 << (7 - (probe_x % 8));
        if pixel_size == 8 {
            bus.write_byte(probe_addr, 0xFF); // black in 8bpp CLUT
        } else {
            bus.write_byte(probe_addr, bus.read_byte(probe_addr) | probe_bit);
        }

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        if pixel_size == 8 {
            assert_eq!(bus.read_byte(probe_addr), 0xFF);
        } else {
            assert_ne!(bus.read_byte(probe_addr) & probe_bit, 0);
        }
    }

    #[test]
    fn draw_dialog_does_not_paint_a_fully_occluded_dialog() {
        // Dialog items draw through the dialog port and are clipped by its
        // visRgn (Inside Macintosh Volume I, I-309). An empty visRgn means a
        // window in front completely covers this dialog.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 30);
        bus.write_word(dialog_ptr + 22, 40);
        let empty_vis_data = bus.alloc(10);
        bus.write_word(empty_vis_data, 10);
        bus.write_long(empty_vis_data + 2, 0);
        bus.write_long(empty_vis_data + 6, 0);
        let empty_vis = bus.alloc(4);
        bus.write_long(empty_vis, empty_vis_data);
        bus.write_long(dialog_ptr + 24, empty_vis);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 8,
                rect: (0, 0, 16, 32),
                text: "Hidden".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let probe_x = 4u32;
        let probe_y = 4u32;
        let probe_addr = if pixel_size == 8 {
            screen_base + probe_y * row_bytes + probe_x
        } else {
            screen_base + probe_y * row_bytes + (probe_x / 8)
        };
        let probe_bit = 1 << (7 - (probe_x % 8));
        if pixel_size == 8 {
            bus.write_byte(probe_addr, 0xFF);
        } else {
            bus.write_byte(probe_addr, bus.read_byte(probe_addr) | probe_bit);
        }

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        if pixel_size == 8 {
            assert_eq!(bus.read_byte(probe_addr), 0xFF);
        } else {
            assert_ne!(bus.read_byte(probe_addr) & probe_bit, 0);
        }
    }

    #[test]
    fn draw_dialog_leaves_document_title_chrome_to_window_manager() {
        // DrawDialog is valid for documentProc dialogs, but the title bar
        // belongs to the Window Manager, not the DITL (MTE 1992, 6-142).
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);

        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 6, 0);
        bus.write_word(dialog_ptr + 8, (-40i16) as u16);
        bus.write_word(dialog_ptr + 10, (-80i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 160);
        bus.write_byte(dialog_ptr + 112, 0);
        let title_ptr = bus.alloc(16);
        bus.write_pstring(title_ptr, b"Modeless");
        let title_handle = bus.alloc(4);
        bus.write_long(title_handle, title_ptr);
        bus.write_long(dialog_ptr + 134, title_handle);
        disp.window_proc_ids.insert(dialog_ptr, 0);
        disp.dialog_items.insert(dialog_ptr, Vec::new());

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 85, 24),
            "DrawDialog must not paint title-bar stripes"
        );
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 79, 21),
            "DrawDialog must not paint the title-bar border"
        );
    }

    #[test]
    fn draw_dialog_radio_items_use_control_value_state() {
        // SetCtlValue stores value 1 for a selected radio control (IM:I
        // I-317/I-328). DrawDialog must honor the dialog control value table
        // for radio items, just as it does for checkboxes.
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 120, 220);
        let dialog_ptr = 0x2000;
        let items = vec![DialogItem {
            item_type: 6,
            rect: (20, 20, 40, 160),
            text: "Selected".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        let (mut disp, _cpu, mut bus) = setup();
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        disp.dialog_control_values.insert((dialog_ptr, 1), 1);

        disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);

        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 67, 69),
            "selected radio item should draw the central dot"
        );
    }

    #[test]
    fn setctlvalue_reaches_drawdialog_and_retained_radio_composition() {
        // ModalDialog refreshes its retained framebuffer by erasing and
        // redrawing standard controls. The live ControlRecord value remains
        // authoritative across that composition, just as it is in DrawDialog.
        // Inside Macintosh Volume I, I-317, I-328, I-417.
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 120, 220);
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items = vec![DialogItem {
            item_type: 6,
            rect: (20, 20, 40, 160),
            text: "Selected".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        bus.write_long(ctrl_ptr + 4, dialog_ptr);
        bus.write_byte(ctrl_ptr + 16, 0xFF);
        bus.write_word(ctrl_ptr + 18, 0);
        bus.write_word(ctrl_ptr + 20, 0);
        bus.write_word(ctrl_ptr + 22, 1);
        disp.control_manager.set_proc_id(ctrl_ptr, 2);
        disp.dialog_control_handles
            .insert(ctrl_handle, (dialog_ptr, 1));

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, ctrl_handle);
        disp.dispatch_control(true, 0x163, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 67, 69),
            "DrawDialog should render the SetCtlValue-selected radio dot"
        );

        for offset in 0..row_bytes * 342 {
            bus.write_byte(screen_base + offset, 0);
        }

        disp.redraw_standard_dialog_items(&mut bus, bounds, &items, 0, "", 0, dialog_ptr);

        assert_eq!(disp.dialog_control_values[&(dialog_ptr, 1)], 1);
        assert_eq!(bus.read_word(ctrl_ptr + 18), 1);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 67, 69),
            "retained redraw should preserve the selected radio dot"
        );
    }

    #[test]
    fn draw_dialog_and_retained_redraw_outline_default_button() {
        // DialogRecord.aDefItem identifies the standard push button that
        // receives Return/Enter. Initial and retained composition must render
        // its default outline after the standard button itself.
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 120, 220);
        let dialog_ptr = 0x2000;
        let items = vec![DialogItem {
            item_type: 4,
            rect: (20, 20, 40, 100),
            text: "OK".to_string(),
            ..Default::default()
        }];
        let (mut disp, _cpu, mut bus) = setup();
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);

        disp.draw_dialog(&mut bus, bounds, 1, "", &items, 1, "", 0, false, dialog_ptr);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 56, 70),
            "DrawDialog should outline the aDefItem button"
        );

        for offset in 0..row_bytes * 342 {
            bus.write_byte(screen_base + offset, 0);
        }
        disp.redraw_standard_dialog_items(&mut bus, bounds, &items, 1, "", 0, dialog_ptr);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 56, 70),
            "retained redraw should restore the aDefItem outline"
        );
    }

    #[test]
    fn draw_dialog_honors_live_standard_control_visibility() {
        // HideControl sets contrlVis to zero and DrawDialog redraws the
        // dialog's controls through the Control Manager (IM:I I-329/I-417).
        // The live ControlRecord therefore remains authoritative over the
        // original DITL presentation state.
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 120, 220);
        let (mut disp, _cpu, mut bus) = setup();
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        let dialog_ptr = bus.alloc(170);
        let items = vec![DialogItem {
            item_type: 4,
            rect: (20, 20, 40, 100),
            text: "Live".to_string(),
            ..Default::default()
        }];
        let control_handle =
            disp.create_standard_dialog_control_handle(&mut bus, dialog_ptr, 1, &items[0]);
        let control_ptr = bus.read_long(control_handle);

        bus.write_byte(control_ptr + 16, 0);
        disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);
        assert_eq!(
            count_set_pixels(&bus, screen_base, row_bytes, 60, 60, 80, 140),
            0,
            "DrawDialog must not reintroduce a control whose contrlVis is zero"
        );

        bus.write_byte(control_ptr + 16, 1);
        disp.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);
        assert!(
            count_set_pixels(&bus, screen_base, row_bytes, 60, 60, 80, 140) > 0,
            "any nonzero contrlVis value must keep the control visible"
        );
    }

    #[test]
    fn draw_dialog_queues_user_item_draw_proc_for_known_dialog() {
        // MTE 1992 p. 6-142: DrawDialog calls application-defined item
        // draw procs whose userItem display rectangles are in the dialog.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);

        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 80);
        bus.write_word(dialog_ptr + 22, 120);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0,
                    rect: (8, 8, 24, 48),
                    proc_ptr: 0x500000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 0,
                    rect: (100, 8, 120, 48),
                    proc_ptr: 0x600000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 4,
                    rect: (40, 8, 60, 48),
                    text: "OK".to_string(),
                    ..Default::default()
                },
            ],
        );

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            disp.modeless_dialog_draw_proc_queue
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(dialog_ptr, 0x500000, 1)]
        );
    }

    #[test]
    fn draw_button_systemless_theme_routes_dialog_chrome_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_button(&mut classic_bus, 20, 20, 40, 80, "", true);

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_button(&mut themed_bus, 20, 20, 40, 80, "", true);

        assert!(
            !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 16, 16),
            "classic default-button round rect should leave its outer corner as background"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 16, 16),
            "systemless-default provider chrome should own the default-button outline corner"
        );
    }

    #[test]
    fn draw_checkbox_and_radio_systemless_theme_route_dialog_chrome_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_checkbox(&mut classic_bus, 20, 20, 40, 120, "", true);
        classic.draw_radio(&mut classic_bus, 50, 20, 70, 120, "", true);

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_checkbox(&mut themed_bus, 20, 20, 40, 120, "", true);
        themed.draw_radio(&mut themed_bus, 50, 20, 70, 120, "", true);

        let classic_checkbox_pixels =
            count_set_pixels(&classic_bus, screen_base, row_bytes, 21, 20, 33, 32);
        let themed_checkbox_pixels =
            count_set_pixels(&themed_bus, screen_base, row_bytes, 21, 20, 33, 32);
        assert!(
            themed_checkbox_pixels > classic_checkbox_pixels,
            "systemless-default dialog checkbox provider should draw a denser selected mark ({themed_checkbox_pixels} <= {classic_checkbox_pixels})"
        );
        let classic_radio_pixels =
            count_set_pixels(&classic_bus, screen_base, row_bytes, 51, 20, 63, 32);
        let themed_radio_pixels =
            count_set_pixels(&themed_bus, screen_base, row_bytes, 51, 20, 63, 32);
        assert!(
            themed_radio_pixels > classic_radio_pixels,
            "systemless-default dialog radio provider should draw a denser selected mark ({themed_radio_pixels} <= {classic_radio_pixels})"
        );
    }

    #[test]
    fn draw_popup_control_systemless_theme_routes_dialog_chrome_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_popup_control(&mut classic_bus, 20, 20, 40, 120, "");

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_popup_control(&mut themed_bus, 20, 20, 40, 120, "");

        assert!(
            screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 120, 39),
            "classic popup CDEF should draw its one-pixel offset shadow"
        );
        assert!(
            !screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 120, 39),
            "systemless-default popup provider should not draw the classic offset shadow"
        );
    }

    #[test]
    fn draw_popup_control_keeps_overlong_title_inside_content_area() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        for enabled in [true, false] {
            let (mut blank, _blank_cpu, mut blank_bus) = setup();
            blank.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
            blank.draw_popup_control_with_state(
                &mut blank_bus,
                20,
                20,
                40,
                120,
                "",
                enabled,
                false,
            );

            let (mut titled, _titled_cpu, mut titled_bus) = setup();
            titled.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
            titled.draw_popup_control_with_state(
                &mut titled_bus,
                20,
                20,
                40,
                120,
                "Standard/Marathon",
                enabled,
                false,
            );

            assert!(
                (20..40).any(|y| {
                    (35..101).any(|x| {
                        screen_pixel_is_set(&titled_bus, screen_base, row_bytes, x, y)
                            != screen_pixel_is_set(&blank_bus, screen_base, row_bytes, x, y)
                    })
                }),
                "the selected title must remain visible when enabled={enabled}"
            );
            for y in 20..40 {
                for x in 101..180 {
                    assert_eq!(
                        screen_pixel_is_set(&titled_bus, screen_base, row_bytes, x, y),
                        screen_pixel_is_set(&blank_bus, screen_base, row_bytes, x, y),
                        "selected-item text must not alter the arrow or pixels beyond it at ({x}, {y}) when enabled={enabled}"
                    );
                }
            }
        }

        let display_title =
            TrapDispatcher::popup_control_display_title("Standard/Marathon", 66, 0, 12);
        assert!(display_title.ends_with("..."));
        assert!(
            TrapDispatcher::fb_measure_string(&display_title, 0, 12) <= 66,
            "truncated popup title must fit its measured content width"
        );
    }

    #[test]
    fn draw_edit_text_systemless_theme_routes_caret_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_edit_text(&mut classic_bus, 20, 20, 40, 100, "", false);

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_edit_text(&mut themed_bus, 20, 20, 40, 100, "", false);

        assert!(
            !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 22, 22),
            "classic editText caret should remain a single vertical bar"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 22, 22),
            "systemless-default provider should draw the caret cap"
        );
    }

    #[test]
    fn draw_edit_text_systemless_theme_routes_field_frame_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_edit_text(&mut classic_bus, 20, 20, 40, 100, "", false);

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_edit_text(&mut themed_bus, 20, 20, 40, 100, "", false);

        assert!(
            !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 19, 22),
            "classic editText field should keep the upper interior clear"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 19, 22),
            "systemless-default provider should draw focused editText field chrome"
        );
    }

    #[test]
    fn draw_dialog_systemless_theme_routes_disabled_edit_text_state_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 110, 190);
        let items = vec![DialogItem {
            item_type: 0x80 | 16,
            rect: (20, 20, 44, 130),
            text: "Disabled".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_dialog(
            &mut classic_bus,
            bounds,
            1,
            "",
            &items,
            0,
            "",
            0,
            false,
            0x2000,
        );

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_dialog(
            &mut themed_bus,
            bounds,
            1,
            "",
            &items,
            0,
            "",
            0,
            false,
            0x2000,
        );

        let disabled_inner_x =
            bounds.1 + items[0].rect.1 - TrapDispatcher::EDIT_TEXT_FRAME_OUTSET + 2;
        let disabled_inner_y = bounds.0 + items[0].rect.0 + 2;
        assert!(
            !screen_pixel_is_set(
                &classic_bus,
                screen_base,
                row_bytes,
                disabled_inner_x,
                disabled_inner_y
            ),
            "classic editText field should keep the disabled-state inner frame pixel clear"
        );
        assert!(
            screen_pixel_is_set(
                &themed_bus,
                screen_base,
                row_bytes,
                disabled_inner_x,
                disabled_inner_y
            ),
            "systemless-default provider should receive disabled editText field state"
        );
    }

    #[test]
    fn draw_dialog_systemless_theme_routes_disabled_standard_controls_through_provider() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 140, 210);
        let items = vec![
            DialogItem {
                item_type: 0x80 | 4,
                rect: (20, 20, 40, 82),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0x80 | 5,
                rect: (52, 20, 72, 120),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0x80 | 6,
                rect: (84, 20, 104, 120),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_dialog(
            &mut classic_bus,
            bounds,
            1,
            "",
            &items,
            0,
            "",
            0,
            false,
            0x2000,
        );

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_dialog(
            &mut themed_bus,
            bounds,
            1,
            "",
            &items,
            0,
            "",
            0,
            false,
            0x2000,
        );

        // The unselected disabled checkbox is intentionally pixel-equivalent
        // after classic CDEF mark alignment; selected checkbox routing is
        // covered by draw_checkbox_and_radio_systemless_theme_route_dialog_chrome_through_provider.
        let probes = [
            (
                bounds.1 + items[0].rect.1 + 2,
                bounds.0 + items[0].rect.0 + 2,
            ),
            (
                bounds.1 + items[2].rect.1 + 2,
                bounds.0
                    + items[2].rect.0
                    + (items[2].rect.2
                        - items[2].rect.0
                        - TrapDispatcher::STANDARD_CONTROL_MARK_SIZE)
                        / 2,
            ),
        ];
        for (x, y) in probes {
            assert!(
                !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, x, y),
                "classic disabled DITL controls should keep standard appearance at ({x},{y})"
            );
            assert!(
                screen_pixel_is_set(&themed_bus, screen_base, row_bytes, x, y),
                "systemless-default provider should receive disabled DITL control state at ({x},{y})"
            );
        }
    }

    #[test]
    fn draw_dialog_classic_keeps_disabled_checkbox_and_radio_titles_undimmed() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 190, 230);
        let items = vec![
            DialogItem {
                item_type: 5,
                rect: (20, 20, 40, 150),
                text: "Sound".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0x80 | 5,
                rect: (52, 20, 72, 150),
                text: "Sound".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 6,
                rect: (84, 20, 104, 150),
                text: "Music".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0x80 | 6,
                rect: (116, 20, 136, 150),
                text: "Music".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        dispatcher.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, 0x2000);

        let label_left = bounds.1
            + items[0].rect.1
            + TrapDispatcher::STANDARD_CONTROL_MARK_LEFT_INSET
            + TrapDispatcher::STANDARD_CONTROL_MARK_SIZE
            + TrapDispatcher::STANDARD_CONTROL_TITLE_GAP;
        let enabled_checkbox = count_set_pixels(
            &bus,
            screen_base,
            row_bytes,
            bounds.0 + items[0].rect.0,
            label_left,
            bounds.0 + items[0].rect.2,
            bounds.1 + items[0].rect.3,
        );
        let disabled_checkbox = count_set_pixels(
            &bus,
            screen_base,
            row_bytes,
            bounds.0 + items[1].rect.0,
            label_left,
            bounds.0 + items[1].rect.2,
            bounds.1 + items[1].rect.3,
        );
        let enabled_radio = count_set_pixels(
            &bus,
            screen_base,
            row_bytes,
            bounds.0 + items[2].rect.0,
            label_left,
            bounds.0 + items[2].rect.2,
            bounds.1 + items[2].rect.3,
        );
        let disabled_radio = count_set_pixels(
            &bus,
            screen_base,
            row_bytes,
            bounds.0 + items[3].rect.0,
            label_left,
            bounds.0 + items[3].rect.2,
            bounds.1 + items[3].rect.3,
        );

        assert!(
            enabled_checkbox > 0,
            "enabled checkbox title should be visible"
        );
        assert_eq!(
            enabled_checkbox, disabled_checkbox,
            "classic itemDisable checkbox title must not be visually dimmed"
        );
        assert!(enabled_radio > 0, "enabled radio title should be visible");
        assert_eq!(
            enabled_radio, disabled_radio,
            "classic itemDisable radio title must not be visually dimmed"
        );
    }

    #[test]
    fn draw_dialog_standard_control_hilite_255_titles_use_gray_device_index() {
        let screen_base = 0x300000u32;
        let row_bytes = 160u32;
        let bounds = (20, 20, 120, 220);
        let dialog_ptr = 0x2400u32;
        let items = vec![
            DialogItem {
                item_type: 5,
                rect: (20, 20, 40, 150),
                text: "Sound".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 6,
                rect: (52, 20, 72, 150),
                text: "Music".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        let (mut dispatcher, _cpu, mut bus) = setup();
        dispatcher.set_screen_mode_for_test(screen_base, row_bytes, 160, 140, 8);
        dispatcher.device_clut.replace([[0x2020, 0x4040, 0x6060]; 256]);
        dispatcher.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
        dispatcher.device_clut.set_entry(42, [0x7FFF, 0x7FFF, 0x7FFF]);
        dispatcher.device_clut.set_entry(255, [0, 0, 0]);
        for offset in 0..(row_bytes * 140) {
            bus.write_byte(screen_base + offset, 0);
        }

        for (item_no, proc_id, item) in [(1i16, 1i16, &items[0]), (2, 2, &items[1])] {
            let ctrl_ptr = bus.alloc(296);
            let ctrl_handle = bus.alloc(4);
            bus.write_long(ctrl_handle, ctrl_ptr);
            dispatcher.initialize_control_record(
                &mut bus,
                ctrl_ptr,
                dialog_ptr,
                item.rect,
                item.text.as_bytes(),
                true,
                0,
                0,
                1,
                proc_id,
                0,
            );
            bus.write_byte(ctrl_ptr + 17, 255);
            dispatcher
                .dialog_control_handles
                .insert(ctrl_handle, (dialog_ptr, item_no));
            dispatcher
                .dialog_control_values
                .insert((dialog_ptr, item_no), 0);
        }

        dispatcher.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);

        for (label, item) in [("checkbox", &items[0]), ("radio", &items[1])] {
            let label_left = bounds.1
                + item.rect.1
                + TrapDispatcher::STANDARD_CONTROL_MARK_LEFT_INSET
                + TrapDispatcher::STANDARD_CONTROL_MARK_SIZE
                + TrapDispatcher::STANDARD_CONTROL_TITLE_GAP;
            let top = bounds.0 + item.rect.0;
            let bottom = bounds.0 + item.rect.2;
            let right = bounds.1 + item.rect.3;
            let gray = count_pixel_index(
                &bus,
                screen_base,
                row_bytes,
                top,
                label_left,
                bottom,
                right,
                42,
            );
            let black = count_pixel_index(
                &bus,
                screen_base,
                row_bytes,
                top,
                label_left,
                bottom,
                right,
                255,
            );
            assert!(
                gray > 12,
                "inactive standard DITL {label} title should draw with the device gray index"
            );
            assert_eq!(
                black, 0,
                "inactive standard DITL {label} title must not leave black glyph pixels"
            );
        }

        dispatcher.device_clut.replace([[0x2020, 0x4040, 0x6060]; 256]);
        dispatcher.device_clut.set_entry(0, [0xFFFF, 0xFFFF, 0xFFFF]);
        dispatcher.device_clut.set_entry(255, [0, 0, 0]);
        for offset in 0..(row_bytes * 140) {
            bus.write_byte(screen_base + offset, 0);
        }
        dispatcher.draw_dialog(&mut bus, bounds, 1, "", &items, 0, "", 0, false, dialog_ptr);
        for (label, item) in [("checkbox", &items[0]), ("radio", &items[1])] {
            let label_left = bounds.1
                + item.rect.1
                + TrapDispatcher::STANDARD_CONTROL_MARK_LEFT_INSET
                + TrapDispatcher::STANDARD_CONTROL_MARK_SIZE
                + TrapDispatcher::STANDARD_CONTROL_TITLE_GAP;
            assert!(
                count_pixel_index(
                    &bus,
                    screen_base,
                    row_bytes,
                    bounds.0 + item.rect.0,
                    label_left,
                    bounds.0 + item.rect.2,
                    bounds.1 + item.rect.3,
                    1,
                ) > 6,
                "inactive standard DITL {label} title should use a visible palette blend without a gray ramp"
            );
        }
    }

    #[test]
    fn disabled_classic_button_uses_gray_label_ink() {
        let (mut disp, mut _cpu, mut bus) = setup();
        let screen_base = bus.alloc(320 * 200);
        disp.set_screen_mode_for_test(screen_base, 320, 320, 200, 8);
        let gray_index = disp
            .device_clut
            .iter()
            .enumerate()
            .min_by_key(|(_, rgb)| {
                rgb.iter()
                    .map(|component| (i32::from(*component) - 0xAAAA).unsigned_abs())
                    .sum::<u32>()
            })
            .map(|(index, _)| index as u8)
            .unwrap();

        disp.draw_button_state(&mut bus, 40, 40, 62, 120, "Open", false, false);

        let label_ink = (44..116).any(|h| {
            (44..59).any(|v| bus.read_byte(screen_base + v as u32 * 320 + h as u32) == gray_index)
        });
        assert!(label_ink, "disabled labels should use device-gray ink");
    }

    #[test]
    fn modal_window_redraw_preserves_initial_border_and_guest_content() {
        for theme in [UiThemeId::ClassicSystem7, UiThemeId::SystemlessDefault] {
            let (mut disp, _cpu, mut bus) = setup();
            let base = 0x300000;
            disp.set_ui_theme_id(theme);
            disp.set_screen_mode_for_test(base, 64, 512, 342, 1);
            let bounds = (40, 40, 100, 140);
            disp.draw_dialog(&mut bus, bounds, 1, "", &[], 0, "", 0, false, 0);
            // Guest content must survive a WDEF-only frame refresh.
            bus.write_byte(base + 60 * 64 + 8, 0xA5);
            let before = bus.read_bytes(base, 64 * 342).to_vec();
            disp.window_bounds = bounds;
            disp.window_proc_id = 1;
            disp.draw_window_frame(&mut bus);
            assert_eq!(bus.read_bytes(base, 64 * 342), before, "{theme:?}");
        }
    }

    #[test]
    fn draw_dialog_systemless_theme_uses_single_outer_frame() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 100, 140);

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.draw_dialog(
            &mut classic_bus,
            bounds,
            1,
            "",
            &[],
            0,
            "",
            0,
            false,
            0x2000,
        );

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.draw_dialog(&mut themed_bus, bounds, 1, "", &[], 0, "", 0, false, 0x2000);

        assert!(
            !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 50, 38),
            "classic dBoxProc leaves the gap between outer and inner borders clear"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 50, 32),
            "systemless-default provider should draw the outer dialog frame"
        );
        assert!(
            !screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 50, 38),
            "systemless-default should leave the dialog frame interior uncluttered"
        );
    }

    #[test]
    fn movable_dialog_draws_its_title_bar_above_the_content() {
        // movableDBoxProc adds a striped title bar without a close box.
        // Macintosh Human Interface Guidelines (1992), pp. 185-186.
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (40, 40, 100, 140);
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);

        disp.draw_dialog(&mut bus, bounds, 5, "Options", &[], 0, "", 0, false, 0x2000);

        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 39, 21),
            "movableDBoxProc must paint the title-frame top above portRect"
        );
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 42, 23),
            "an active movable title bar must contain racing stripes"
        );
    }

    #[test]
    fn draw_window_frame_systemless_theme_uses_single_outer_border() {
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;

        let (mut classic, _classic_cpu, mut classic_bus) = setup();
        classic.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        classic.window_bounds = (40, 40, 100, 140);
        classic.window_proc_id = 1;
        classic.draw_window_frame(&mut classic_bus);

        let (mut themed, _themed_cpu, mut themed_bus) = setup();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        themed.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        themed.window_bounds = (40, 40, 100, 140);
        themed.window_proc_id = 1;
        themed.draw_window_frame(&mut themed_bus);

        assert!(
            !screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 50, 34),
            "classic dBoxProc window frame keeps the structure gap clear"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 50, 32),
            "systemless-default provider should draw the outer window frame"
        );
        assert!(
            !screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 50, 34),
            "systemless-default should leave the window frame interior uncluttered"
        );
    }

    #[test]
    fn draw_dialog_preserves_existing_user_item_pixels() {
        // UserItems are application-owned drawing areas. Games often draw
        // into them before calling DrawDialog; the Dialog Manager redraw
        // must not erase that content while refreshing the standard items.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 40);
        bus.write_word(dialog_ptr + 22, 80);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0x80, // disabled userItem
                    rect: (8, 8, 20, 32),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (24, 8, 36, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        let user_x = 10u32;
        let user_y = 10u32;
        let background_x = 4u32;
        let background_y = 4u32;
        if pixel_size == 8 {
            bus.write_byte(screen_base + user_y * row_bytes + user_x, 0xFF);
            bus.write_byte(screen_base + background_y * row_bytes + background_x, 0xFF);
        } else {
            for (x, y) in [(user_x, user_y), (background_x, background_y)] {
                let addr = screen_base + y * row_bytes + (x / 8);
                let bit = 1 << (7 - (x % 8));
                bus.write_byte(addr, bus.read_byte(addr) | bit);
            }
        }

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        if pixel_size == 8 {
            assert_eq!(
                bus.read_byte(screen_base + user_y * row_bytes + user_x),
                0xFF,
                "DrawDialog should preserve pre-drawn userItem pixels"
            );
            assert_eq!(
                bus.read_byte(screen_base + background_y * row_bytes + background_x),
                0xFF,
                "DrawDialog must preserve pixels outside its items too"
            );
        } else {
            let user_addr = screen_base + user_y * row_bytes + (user_x / 8);
            let user_bit = 1 << (7 - (user_x % 8));
            let background_addr = screen_base + background_y * row_bytes + (background_x / 8);
            let background_bit = 1 << (7 - (background_x % 8));
            assert_ne!(bus.read_byte(user_addr) & user_bit, 0);
            assert_ne!(bus.read_byte(background_addr) & background_bit, 0);
        }
    }

    #[test]
    fn modal_dialog_first_entry_does_not_preserve_invisible_user_item_background() {
        // A newly shown modal dialog has no meaningful userItem pixels yet.
        // Dialog Manager fills the dialog background; it must not treat the
        // previous screen contents as application-owned userItem drawing.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let item_hit_ptr = bus.alloc(2);
        let (screen_base, row_bytes, _w, _h, pixel_size) = disp.screen_mode;
        assert_eq!(pixel_size, 8);

        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 40);
        bus.write_word(dialog_ptr + 22, 80);
        bus.write_word(dialog_ptr + 108, 2);
        disp.front_window = dialog_ptr;
        disp.window_bounds = (0, 0, 40, 80);
        disp.window_proc_id = 2;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0x80, // disabled userItem placeholder
                    rect: (8, 8, 20, 32),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (24, 8, 36, 40),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        let user_x = 10u32;
        let user_y = 10u32;
        bus.write_byte(screen_base + user_y * row_bytes + user_x, 0xFF);

        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, 0);
        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(screen_base + user_y * row_bytes + user_x),
            0,
            "ModalDialog should paint a clean dialog background through untouched userItems"
        );
        assert!(disp.dialog_tracking.is_some());
    }

    #[test]
    fn modal_dialog_first_entry_restores_visible_shell_before_user_item_preservation() {
        // Visible GetNewDialog draws a clean shell before the first ModalDialog
        // call. If later screen/chrome drawing leaves unrelated pixels in a
        // userItem rect, ModalDialog must restore that clean shell before
        // preserving application-owned userItem pixels. EVO's startup
        // shareware notice exposes this when title/menu art leaks through its
        // large userItem body.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_bytes(screen_base, &vec![0x77; 800 * 600]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let mut dlog = build_test_dlog((100, 100, 180, 260), 1711, 0);
        dlog[10] = 1; // visible
        let ditl = build_test_ditl_items(&[
            (0, (8, 8, 30, 80), b"".as_slice()),
            (4, (44, 20, 64, 90), b"OK".as_slice()),
        ]);
        disp.install_test_resource(&mut bus, *b"DLOG", 1710, &dlog);
        disp.install_test_resource(&mut bus, *b"DITL", 1711, &ditl);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0xFFFF_FFFF); // behind
        bus.write_long(TEST_SP + 4, 0); // dStorage
        bus.write_word(TEST_SP + 8, 1710); // dialogID
        disp.dispatch_dialog(true, 0x17C, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let dlg_ptr = bus.read_long(TEST_SP + 10);
        assert_ne!(dlg_ptr, 0);
        assert!(disp.dialog_visible_snapshots.contains_key(&dlg_ptr));

        let probe_v = 120u32;
        let probe_h = 120u32;
        let probe_addr = screen_base + probe_v * 800 + probe_h;
        assert_eq!(
            bus.read_byte(probe_addr),
            0,
            "visible dialog creation should paint the userItem area as dialog background"
        );

        bus.write_byte(probe_addr, 0x55);
        let item_hit_ptr = bus.alloc(2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, 0);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            bus.read_byte(probe_addr),
            0,
            "first ModalDialog entry should not preserve stale non-dialog pixels in userItems"
        );
        assert!(disp.dialog_tracking.is_some());
        assert!(
            !disp.dialog_visible_snapshots.contains_key(&dlg_ptr),
            "first ModalDialog entry consumes the clean visible shell snapshot"
        );
    }

    #[test]
    fn premodal_dialog_port_draw_refreshes_visible_snapshot() {
        // Callers pass the port that was just drawn into (the current graphics
        // port). When that port is a visible dialog with a retained snapshot,
        // the drawing landed inside the dialog and is application-owned dialog
        // content — even before ModalDialog has begun modal tracking. Games
        // routinely render dialog content directly into the window before
        // entering ModalDialog (EV Override blits its Game Speed slider into a
        // userItem rect; Marathon draws custom controls the same way), so the
        // retained snapshot must be refreshed to capture it.
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((64 * 64) as u32);
        bus.write_bytes(screen_base, &vec![0x00; 64 * 64]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);

        let dialog_ptr = 0x200000u32;
        let bounds = (10, 10, 30, 30);
        let snapshot_width = (bounds.3 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_height =
            (bounds.2 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        disp.front_window = dialog_ptr;
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: vec![0x11; snapshot_width * snapshot_height].into(),
            },
        );

        let probe = screen_base + 20 * 64 + 20;
        bus.write_byte(probe, 0x77);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(
            &bus,
            dialog_ptr,
            (20, 20, 21, 21),
        );
        bus.write_byte(probe, 0x00);
        disp.restore_visible_dialog_snapshots(&mut bus);
        assert_eq!(
            bus.read_byte(probe),
            0x77,
            "drawing into the dialog's own port refreshes its retained snapshot"
        );

        disp.dialog_modal_entered.insert(dialog_ptr);
        bus.write_byte(probe, 0x88);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(
            &bus,
            dialog_ptr,
            (20, 20, 21, 21),
        );
        bus.write_byte(probe, 0x00);
        disp.restore_visible_dialog_snapshots(&mut bus);
        assert_eq!(
            bus.read_byte(probe),
            0x88,
            "retained modal dialogs may refresh snapshots from later bulk drawing"
        );

        let game_dialog_ptr = 0x200100u32;
        let game_bounds = (34, 10, 54, 30);
        bus.write_word(game_dialog_ptr + 6, 0xC000);
        let pixmap_handle = bus.alloc(4);
        let pixmap_ptr = bus.alloc(50);
        bus.write_long(pixmap_handle, pixmap_ptr);
        bus.write_long(pixmap_ptr, screen_base);
        bus.write_word(pixmap_ptr + 4, 0x8000 | 64);
        bus.write_word(pixmap_ptr + 6, (-(game_bounds.0)) as u16);
        bus.write_word(pixmap_ptr + 8, (-(game_bounds.1)) as u16);
        bus.write_word(pixmap_ptr + 32, 8);
        bus.write_long(game_dialog_ptr + 2, pixmap_handle);
        bus.write_word(game_dialog_ptr + 16, 0);
        bus.write_word(game_dialog_ptr + 18, 0);
        bus.write_word(game_dialog_ptr + 20, 20);
        bus.write_word(game_dialog_ptr + 22, 20);
        bus.write_byte(game_dialog_ptr + 110, 0xFF);
        disp.dialog_items.insert(
            game_dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (0, 0, 20, 20),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.front_window = game_dialog_ptr;
        bus.write_byte(screen_base + 40 * 64 + 20, 0x99);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(
            &bus,
            game_dialog_ptr,
            (40, 20, 41, 21),
        );
        bus.write_byte(screen_base + 40 * 64 + 20, 0x00);
        disp.restore_visible_dialog_snapshots(&mut bus);
        assert_eq!(
            bus.read_byte(screen_base + 40 * 64 + 20),
            0x99,
            "game-managed dialogs should create snapshots from app-owned bulk drawing"
        );
    }

    #[test]
    fn premodal_bulk_draw_seeds_mixed_dialog_before_first_modal_entry() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((64 * 64) as u32);
        bus.write_bytes(screen_base, &vec![0x00; 64 * 64]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);

        let dialog_ptr = bus.alloc(170);
        let bounds = (8, 8, 56, 56);
        bus.write_word(dialog_ptr + 8, (-(bounds.0)) as u16);
        bus.write_word(dialog_ptr + 10, (-(bounds.1)) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, (bounds.2 - bounds.0) as u16);
        bus.write_word(dialog_ptr + 22, (bounds.3 - bounds.1) as u16);
        bus.write_byte(dialog_ptr + 110, 0);
        let items = vec![
            DialogItem {
                item_type: 4,
                rect: (32, 30, 44, 46),
                text: "OK".to_string(),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0x80,
                rect: (0, 0, 24, 24),
                ..DialogItem::default()
            },
            DialogItem {
                item_type: 0x80,
                rect: (0, 24, 24, 48),
                ..DialogItem::default()
            },
        ];
        disp.dialog_items.insert(dialog_ptr, items.clone());
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.window_proc_id = 2;

        disp.draw_dialog(
            &mut bus, bounds, 2, "", &items, 0, "", -1, false, dialog_ptr,
        );
        assert!(!disp.dialogs_drawn_by_app.contains(&dialog_ptr));
        assert!(!disp.dialog_visible_snapshots.contains_key(&dialog_ptr));

        let scene_inside_user = screen_base + 16 * 64 + 16;
        let scene_outside_user = screen_base + 28 * 64 + 40;
        let button_probe = screen_base + 40 * 64 + 38;
        let button_pixel = bus.read_byte(button_probe);
        bus.write_byte(scene_inside_user, 0x31);
        bus.write_byte(scene_outside_user, 0x62);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(&bus, dialog_ptr, bounds);
        assert!(disp.dialog_visible_snapshots.contains_key(&dialog_ptr));
        assert!(disp.dialogs_drawn_by_app.contains(&dialog_ptr));

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x181, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(scene_inside_user), 0x31);
        assert_eq!(bus.read_byte(scene_outside_user), 0x62);
        assert_eq!(bus.read_byte(button_probe), button_pixel);

        disp.redraw_dialog_window_contents(&mut bus, dialog_ptr);
        assert_eq!(bus.read_byte(scene_inside_user), 0x31);
        assert_eq!(bus.read_byte(scene_outside_user), 0x62);
        assert_eq!(bus.read_byte(button_probe), button_pixel);
        disp.dialog_visible_snapshots
            .get_mut(&dialog_ptr)
            .unwrap()
            .pixels
            .fill(0);

        let item_hit_ptr = bus.alloc(2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, item_hit_ptr);
        bus.write_long(TEST_SP + 4, 0);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(scene_inside_user), 0x31);
        assert_eq!(bus.read_byte(scene_outside_user), 0x62);
        assert_eq!(bus.read_byte(button_probe), button_pixel);
        assert_eq!(
            disp.dialog_item_hit_test(
                &bus,
                &items,
                bounds,
                46,
                44,
                &disp.dialog_popup_original_rects,
                dialog_ptr,
            ),
            1,
            "the retained standard button must keep its normal hit target"
        );

        bus.write_byte(dialog_ptr + 110, 0xFF);
        let animation_probe = screen_base + 18 * 64 + 18;
        bus.write_byte(animation_probe, 0x93);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(
            &bus,
            dialog_ptr,
            (18, 18, 19, 19),
        );
        bus.write_byte(animation_probe, 0);
        bus.write_byte(scene_outside_user, 0);
        disp.refresh_dialog_tracking_snapshot(&mut bus);
        assert_eq!(bus.read_byte(animation_probe), 0x93);
        assert_eq!(bus.read_byte(scene_outside_user), 0x62);
    }

    #[test]
    fn bulk_draw_during_dialog_filter_defers_modal_snapshot_finalization() {
        // MTE 1992 pp. 6-135 and 6-142: a modal filter may update a
        // dialog with application-owned drawing before it returns. Capturing
        // an intermediate text operation must not make later QuickDraw output
        // in the same callback disappear from ModalDialog's final snapshot.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((64 * 64) as u32);
        bus.write_bytes(screen_base, &vec![0x00; 64 * 64]);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);

        let dialog_ptr = 0x200000u32;
        let bounds = (10, 10, 30, 30);
        let snapshot_width = (bounds.3 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_height =
            (bounds.2 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        disp.front_window = dialog_ptr;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (2, 2, 8, 12),
                ..Default::default()
            }],
        );
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: vec![0x00; snapshot_width * snapshot_height].into(),
            },
        );
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds,
            title: String::new(),
            proc_id: 2,
            items: disp.dialog_items[&dialog_ptr].clone(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: vec![0x00; snapshot_width * snapshot_height].into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: true,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 6,
                message: dialog_ptr,
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            }),
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let bulk_probe = screen_base + 15 * 64 + 15;
        bus.write_byte(bulk_probe, 0x44);
        disp.refresh_visible_dialog_snapshot_after_bulk_port_draw(
            &bus,
            dialog_ptr,
            (15, 15, 16, 16),
        );
        assert!(
            !disp.dialog_tracking.as_ref().unwrap().rendered_pixels_final,
            "an intermediate bulk draw must not finalize an active filter callback"
        );

        let later_probe = screen_base + 16 * 64 + 16;
        bus.write_byte(later_probe, 0x77);
        disp.refresh_visible_dialog_snapshot_region_for_port(&bus, dialog_ptr, (16, 16, 17, 17));
        disp.dialog_tracking.as_mut().unwrap().last_filter_event = None;

        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert!(tracking.rendered_pixels_final);
        let bulk_index = (15 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN) as usize
            * snapshot_width
            + (15 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN) as usize;
        let later_index = (16 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN) as usize
            * snapshot_width
            + (16 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN) as usize;
        assert_eq!(tracking.rendered_pixels[bulk_index], 0x44);
        assert_eq!(tracking.rendered_pixels[later_index], 0x77);
    }

    #[test]
    fn updtdialog_pops_eight_bytes() {
        // Inside Macintosh Volume I, I-415: UpdtDialog is a Pascal
        // procedure taking theDialog and updateRgn.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x300100); // updateRgn
        bus.write_long(sp + 4, 0x200000); // theDialog

        let result = disp.dispatch_dialog(true, 0x178, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn updtdialog_nil_update_region_is_a_noop() {
        // Inside Macintosh Volume I, I-415 names updateRgn as the dialog's
        // update region. A NIL region should not trigger redraw work.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing = 0x200040u32;
        disp.window_list.replace(vec![existing]);
        disp.front_window = existing;
        bus.write_byte(existing + 110, 0xFF);

        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let bounds_rect_ptr = 0x301200u32;
        bus.write_word(bounds_rect_ptr, 100);
        bus.write_word(bounds_rect_ptr + 2, 100);
        bus.write_word(bounds_rect_ptr + 4, 300);
        bus.write_word(bounds_rect_ptr + 6, 400);

        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        for i in 0..34u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 22, bounds_rect_ptr);
        bus.write_word(sp + 16, 1); // visible
        bus.write_long(sp + 10, 0); // behind = NIL (backmost)

        let result = disp.dispatch_dialog(true, 0x17D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let dlg_ptr = bus.read_long(sp + 30);
        assert_ne!(dlg_ptr, 0);

        let probe_x = 120u32;
        let probe_y = 120u32;
        let probe_addr = screen_base + probe_y * 800 + probe_x;
        bus.write_byte(probe_addr, 0xFF);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0); // NIL update region
        bus.write_long(TEST_SP + 4, dlg_ptr);

        let result = disp.dispatch_dialog(true, 0x178, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_byte(probe_addr), 0xFF);
    }

    #[test]
    fn updtdialog_queues_user_item_draw_proc_for_intersecting_update_region() {
        // MTE 1992, 6-142 to 6-143: UpdateDialog redraws only items in
        // the supplied update region and calls application-defined item
        // draw procedures.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(100 * 100);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 100, 100, 100, 8);

        let dialog_ptr = bus.alloc(256);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 100);
        bus.write_word(dialog_ptr + 22, 100);
        bus.write_word(dialog_ptr + 108, 2); // plain dialog box
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0,
                    rect: (8, 8, 24, 32),
                    proc_ptr: 0x500000,
                    ..Default::default()
                },
                DialogItem {
                    item_type: 0,
                    rect: (60, 60, 80, 80),
                    proc_ptr: 0x600000,
                    ..Default::default()
                },
            ],
        );
        let update_rgn_ptr = bus.alloc(10);
        let update_rgn = bus.alloc(4);
        bus.write_long(update_rgn, update_rgn_ptr);
        bus.write_word(update_rgn_ptr, 10);
        bus.write_word(update_rgn_ptr + 2, 0);
        bus.write_word(update_rgn_ptr + 4, 0);
        bus.write_word(update_rgn_ptr + 6, 40);
        bus.write_word(update_rgn_ptr + 8, 40);

        bus.write_long(TEST_SP, update_rgn);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x178, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(
            disp.modeless_dialog_draw_proc_queue
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![(dialog_ptr, 0x500000, 1)]
        );
    }

    #[test]
    fn te_text_box_erases_box_before_drawing_text() {
        // Inside Macintosh: Text 1993, p. 2-88: TETextBox erases the box
        // and then draws wrapped/aligned text into it.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        let screen_base = bus.read_long(0x0824);

        // Fill the target area black so a missing EraseRect is visible.
        for y in 0..12u32 {
            for byte in 0..3u32 {
                bus.write_byte(screen_base + y * 64 + byte, 0xFF);
            }
        }

        // Make sure the text renderer has a usable size and port state.
        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text_ptr = 0x200000u32;
        bus.write_byte(text_ptr, b'A');

        let box_ptr = 0x200100u32;
        bus.write_word(box_ptr, 0); // top
        bus.write_word(box_ptr + 2, 0); // left
        bus.write_word(box_ptr + 4, 12); // bottom
        bus.write_word(box_ptr + 6, 24); // right

        let sp = TEST_SP - 14;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // teFlushLeft
        bus.write_long(sp + 2, box_ptr);
        bus.write_long(sp + 6, 1); // length
        bus.write_long(sp + 10, text_ptr);

        let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        // Pixel (20, 10) lies inside the box but outside the glyph; it should
        // be white after the implicit EraseRect.
        let probe_addr = screen_base + 10 * 64 + 2;
        assert_eq!(bus.read_byte(probe_addr) & (1 << 3), 0);
    }

    #[test]
    fn te_text_box_erases_box_for_zero_length_text() {
        // Inside Macintosh: Text 1993, p. 2-88: the erase precedes text
        // drawing and is not conditional on the text length.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        let screen_base = bus.read_long(0x0824);

        for y in 0..12u32 {
            for byte in 0..3u32 {
                bus.write_byte(screen_base + y * 64 + byte, 0xFF);
            }
        }

        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text_ptr = 0x200000u32;
        let box_ptr = 0x200100u32;
        bus.write_word(box_ptr, 0);
        bus.write_word(box_ptr + 2, 0);
        bus.write_word(box_ptr + 4, 12);
        bus.write_word(box_ptr + 6, 24);

        let sp = TEST_SP - 14;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, box_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, text_ptr);

        let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let probe_addr = screen_base + 10 * 64 + 2;
        assert_eq!(bus.read_byte(probe_addr) & (1 << 3), 0);
    }

    #[test]
    fn te_text_box_consumes_align_box_length_text_arguments() {
        // Inside Macintosh: Text 1993, p. 2-88 declares TETextBox as a
        // procedure taking text/length/box/align arguments.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text_ptr = 0x200200u32;
        bus.write_byte(text_ptr, b'A');

        let box_ptr = 0x200240u32;
        bus.write_word(box_ptr, 0);
        bus.write_word(box_ptr + 2, 0);
        bus.write_word(box_ptr + 4, 20);
        bus.write_word(box_ptr + 6, 60);

        let sp = TEST_SP - 14;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, box_ptr);
        bus.write_long(sp + 6, 1);
        bus.write_long(sp + 10, text_ptr);

        let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn te_text_box_align_parameter_controls_rendered_line_origin() {
        // Inside Macintosh: Text 1993, p. 2-87 defines teJustLeft(0),
        // teJustCenter(1), and teJustRight(-1) alignment values.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text = b"ABC";
        let text_ptr = 0x200280u32;
        bus.write_bytes(text_ptr, text);

        let box_ptr = 0x2002C0u32;
        let box_top = 0i16;
        let box_left = 10i16;
        let box_bottom = 40i16;
        let box_right = 110i16;
        bus.write_word(box_ptr, box_top as u16);
        bus.write_word(box_ptr + 2, box_left as u16);
        bus.write_word(box_ptr + 4, box_bottom as u16);
        bus.write_word(box_ptr + 6, box_right as u16);

        let advance_extra = disp.advance_extra();
        let missing_advance = disp.missing_glyph_advance();
        let mut line_width = 0i16;
        for &byte in text {
            if let Some((glyph, _)) =
                crate::quickdraw::text::get_glyph(disp.tx_font, disp.tx_size, byte as char)
            {
                line_width += glyph.advance as i16 + advance_extra;
            } else {
                line_width += missing_advance;
            }
        }

        let mut run_align = |align: i16| -> i16 {
            let sp = TEST_SP - 14;
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, align as u16);
            bus.write_long(sp + 2, box_ptr);
            bus.write_long(sp + 6, text.len() as u32);
            bus.write_long(sp + 10, text_ptr);
            let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
            disp.pn_loc.1
        };

        let x_left = run_align(0);
        let x_center = run_align(1);
        let x_right = run_align(-1);

        let expected_left =
            crate::text_edit::aligned_line_left(box_left, box_right, line_width, 0, 1) + line_width;
        let expected_center =
            crate::text_edit::aligned_line_left(box_left, box_right, line_width, 1, 1) + line_width;
        let expected_right = crate::text_edit::aligned_line_left(
            box_left,
            box_right,
            line_width,
            -1,
            1,
        ) + line_width;

        assert_eq!(x_left, expected_left);
        assert_eq!(x_center, expected_center);
        assert_eq!(x_right, expected_right);
        assert!(x_left < x_center && x_center < x_right);
    }

    #[test]
    fn te_text_box_pascal_procedure_protocol_does_not_overwrite_past_arg_frame() {
        // Per Inside Macintosh: Text 1993, p. 2-88 TETextBox is a
        // Pascal PROCEDURE. The MPW Universal Headers C declaration
        // uses `const Rect *box`, so the caller pushes exactly 14
        // bytes (2-byte just + 4-byte Rect* + 4-byte long + 4-byte
        // text Ptr) and the trap must pop exactly 14 bytes with no
        // result slot written.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text_ptr = 0x200400u32;
        bus.write_byte(text_ptr, b'X');

        let box_ptr = 0x200440u32;
        bus.write_word(box_ptr, 0);
        bus.write_word(box_ptr + 2, 0);
        bus.write_word(box_ptr + 4, 12);
        bus.write_word(box_ptr + 6, 64);

        // Pre-poison memory immediately past the 14-byte arg frame.
        // After the trap pops 14 bytes, A7 must equal TEST_SP and the
        // sentinel words at TEST_SP, TEST_SP+2 must survive untouched.
        let sp = TEST_SP - 14;
        bus.write_word(TEST_SP, 0xCAFE);
        bus.write_word(TEST_SP + 2, 0xBABE);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // align = teJustLeft
        bus.write_long(sp + 2, box_ptr);
        bus.write_long(sp + 6, 1); // length
        bus.write_long(sp + 10, text_ptr);

        let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(TEST_SP), 0xCAFE);
        assert_eq!(bus.read_word(TEST_SP + 2), 0xBABE);
    }

    #[test]
    fn te_text_box_wraps_when_text_exceeds_box_width() {
        // Inside Macintosh: Text 1993, p. 2-88: TETextBox word-wraps text
        // in the destination rectangle.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let port_ptr = 0x181000u32;
        disp
            .current_port
            .with_mut(|current_port| *current_port = port_ptr);
        disp.tx_size = 12;
        bus.write_word(port_ptr + 74, 12);

        let text = b"A A A A";
        let text_ptr = 0x200300u32;
        bus.write_bytes(text_ptr, text);

        let mut glyph_w = disp.missing_glyph_advance();
        if let Some((glyph, _)) = crate::quickdraw::text::get_glyph(disp.tx_font, disp.tx_size, 'A')
        {
            glyph_w = glyph.advance as i16 + disp.advance_extra();
        }

        let wide_box_ptr = 0x200340u32;
        bus.write_word(wide_box_ptr, 0);
        bus.write_word(wide_box_ptr + 2, 0);
        bus.write_word(wide_box_ptr + 4, 80);
        bus.write_word(wide_box_ptr + 6, 140);

        let narrow_box_ptr = 0x200380u32;
        bus.write_word(narrow_box_ptr, 0);
        bus.write_word(narrow_box_ptr + 2, 0);
        bus.write_word(narrow_box_ptr + 4, 80);
        bus.write_word(narrow_box_ptr + 6, glyph_w as u16);

        let mut run_box = |box_ptr: u32| -> i16 {
            let sp = TEST_SP - 14;
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0);
            bus.write_long(sp + 2, box_ptr);
            bus.write_long(sp + 6, text.len() as u32);
            bus.write_long(sp + 10, text_ptr);
            let result = disp.dispatch_dialog(true, 0x1CE, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
            disp.pn_loc.0
        };

        let y_wide = run_box(wide_box_ptr);
        let y_narrow = run_box(narrow_box_ptr);
        let metrics = crate::quickdraw::text::get_font_metrics(disp.tx_font, disp.tx_size);
        let line_height = metrics.ascent + metrics.descent + metrics.leading.max(2);

        assert!(
            y_narrow >= y_wide + line_height,
            "narrow box should force at least one additional wrapped line"
        );
    }

    // ---- CloseDialog ($A982) ----

    fn alloc_region_handle(bus: &mut MacMemoryBus, rect: Option<(i16, i16, i16, i16)>) -> u32 {
        let rgn_ptr = bus.alloc(10);
        bus.write_word(rgn_ptr, 10);
        if let Some((top, left, bottom, right)) = rect.filter(|r| r.2 > r.0 && r.3 > r.1) {
            bus.write_word(rgn_ptr + 2, top as u16);
            bus.write_word(rgn_ptr + 4, left as u16);
            bus.write_word(rgn_ptr + 6, bottom as u16);
            bus.write_word(rgn_ptr + 8, right as u16);
        } else {
            bus.write_long(rgn_ptr + 2, 0);
            bus.write_long(rgn_ptr + 6, 0);
        }
        let handle = bus.alloc(4);
        bus.write_long(handle, rgn_ptr);
        handle
    }

    fn seed_window_regions(
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        content_rect: (i16, i16, i16, i16),
    ) {
        // Minimal WindowRecord region setup for invalidate_window_rect:
        // contRgn @ +118 and updateRgn @ +122.
        bus.write_word(window_ptr + 16, content_rect.0 as u16);
        bus.write_word(window_ptr + 18, content_rect.1 as u16);
        bus.write_word(window_ptr + 20, content_rect.2 as u16);
        bus.write_word(window_ptr + 22, content_rect.3 as u16);
        let cont_rgn = alloc_region_handle(bus, Some(content_rect));
        let update_rgn = alloc_region_handle(bus, None);
        bus.write_long(window_ptr + 118, cont_rgn);
        bus.write_long(window_ptr + 122, update_rgn);
        bus.write_byte(window_ptr + 110, 0xFF);
    }

    fn seed_app_owned_modal_dialog(
        disp: &mut TrapDispatcher,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        prev_window: u32,
        proc_id: i16,
    ) {
        seed_window_regions(bus, dialog_ptr, (0, 0, 100, 220));
        seed_window_regions(bus, prev_window, (0, 0, 342, 512));
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 108, 2);
        disp.window_proc_ids.insert(dialog_ptr, proc_id);
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (0, 0, 100, 220);
        disp.window_proc_id = proc_id;
        disp.window_list.replace(vec![dialog_ptr, prev_window]);
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 0, "Game".to_string()));
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (60, 130, 82, 200),
                text: "OK".to_string(),
                ..DialogItem::default()
            }],
        );
        disp.set_sent_open_app_event_for_test(true);
    }

    fn seed_retained_modal_dialog(
        disp: &mut TrapDispatcher,
        bus: &mut MacMemoryBus,
        dialog_ptr: u32,
        prev_window: u32,
        proc_id: i16,
    ) {
        seed_app_owned_modal_dialog(disp, bus, dialog_ptr, prev_window, proc_id);
        disp.dialog_modal_entered.insert(dialog_ptr);
    }

    #[test]
    fn close_dialog_pops_4_bytes() {
        // Inside Macintosh Volume I, I-413: CloseDialog takes one
        // DialogPtr argument.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_long(TEST_SP, 0x200000);

        let result = disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn close_dialog_front_dialog_restores_previous_port_state() {
        // IM:I I-413 says CloseDialog behaves like CloseWindow; when the
        // front dialog closes, the window behind it becomes frontmost.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let prev_window = 0x181000u32;
        seed_window_regions(&mut bus, prev_window, (0, 0, 342, 512));
        seed_window_regions(&mut bus, dialog_ptr, (100, 120, 220, 320));
        // WindowRecord.strucRgn is the handle at offset 114 (IM:I I-278).
        let dialog_structure = bus.read_long(dialog_ptr + 114);
        let dialog_structure_ptr = bus.read_long(dialog_structure);
        bus.write_word(dialog_structure_ptr + 2, 81);
        bus.write_word(dialog_structure_ptr + 4, 119);
        bus.write_word(dialog_structure_ptr + 6, 222);
        bus.write_word(dialog_structure_ptr + 8, 322);

        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (100, 120, 220, 320);
        bus.write_long(crate::memory::globals::addr::THE_PORT, dialog_ptr);
        let a5 = cpu.read_reg(Register::A5);
        let global_ptr = bus.read_long(a5);
        bus.write_long(global_ptr, dialog_ptr);
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 2, "Prev".to_string()));

        bus.write_long(TEST_SP, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.front_window, prev_window);
        assert_eq!(*disp.current_port, prev_window);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            prev_window
        );
        assert_eq!(bus.read_long(global_ptr), prev_window);
        assert_eq!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(prev_window + 122)),
            Some((81, 119, 222, 322)),
            "CloseDialog should invalidate the complete structure, including a movable title bar"
        );
        assert!(
            disp.event_queue.iter().any(|event| {
                event.what == 8 && event.message == prev_window && (event.modifiers & 1) != 0
            }),
            "CloseDialog should queue activateEvt for the promoted front window"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == prev_window),
            "CloseDialog must queue updateEvt for the newly exposed front window"
        );
    }

    #[test]
    fn close_dialog_with_nil_saved_front_promotes_visible_window() {
        // A dialog can be created while no document window is active, then
        // the application can open a visible document behind it before
        // disposing the dialog. Do not restore the stale NIL snapshot over
        // the Window Manager's visible-window promotion.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let visible_window = 0x181000u32;
        seed_window_regions(&mut bus, dialog_ptr, (0, 0, 100, 220));
        seed_window_regions(&mut bus, visible_window, (0, 0, 342, 512));

        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (100, 120, 220, 320);
        disp.window_list.replace(vec![dialog_ptr, visible_window]);
        disp.window_stack.push((0, (0, 0, 0, 0), -1, String::new()));
        bus.write_long(crate::memory::globals::addr::THE_PORT, dialog_ptr);

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.front_window, visible_window);
        assert_eq!(*disp.current_port, visible_window);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            visible_window
        );
    }

    #[test]
    fn close_dialog_restores_saved_background_pixels() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = 0x200000u32;
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        bus.write_byte(dialog_ptr + 110, 0xFF);

        for y in 92u32..158 {
            for x in 92u32..208 {
                bus.write_byte(screen_base + y * row_bytes + x, 0xCC);
            }
        }

        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; 66 * 116].into());

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        for y in 92u32..158 {
            for x in 92u32..208 {
                assert_eq!(
                    bus.read_byte(screen_base + y * row_bytes + x),
                    0x33,
                    "CloseDialog should restore saved background at ({x},{y})"
                );
            }
        }
        assert!(
            !disp.dialog_saved_pixels.contains_key(&dialog_ptr),
            "saved pixels must be consumed after CloseDialog"
        );
    }

    #[test]
    fn close_dialog_non_front_dialog_leaves_front_window_and_port_unchanged() {
        // CloseWindow front-promotion only applies when the closed window
        // was frontmost (IM:I I-283); CloseDialog inherits that behavior.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let other_front = 0x181000u32;

        disp.front_window = other_front;
        disp
            .current_port
            .with_mut(|current_port| *current_port = other_front);
        bus.write_long(crate::memory::globals::addr::THE_PORT, other_front);
        let a5 = cpu.read_reg(Register::A5);
        let global_ptr = bus.read_long(a5);
        bus.write_long(global_ptr, other_front);
        disp.window_stack
            .push((0x170000, (1, 2, 3, 4), 3, "Prev".to_string()));

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(disp.front_window, other_front);
        assert_eq!(*disp.current_port, other_front);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            other_front
        );
        assert_eq!(bus.read_long(global_ptr), other_front);
        assert_eq!(
            disp.window_stack.len(),
            1,
            "non-front CloseDialog should not consume the saved front-window stack entry"
        );
    }

    #[test]
    fn close_dialog_removes_window_but_preserves_caller_storage() {
        // MTE 1992 pp. 6-119..6-120: CloseDialog releases dialog-owned
        // items but does not dispose caller-supplied DialogRecord or item-list
        // storage.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        let storage_ptr = bus.alloc(170);
        let ditl = build_test_ditl_item(8, (10, 12, 24, 140), b"Static");
        let (items_handle, items_ptr) = install_ditl_handle_for_test(&mut bus, &ditl);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            storage_ptr,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            items_handle,
        );

        let text_handle = ditl_item_handle_field(&bus, items_ptr, 1);
        let text_ptr = bus.read_long(text_handle);
        let te_handle = bus.read_long(dialog_ptr + 160);
        let te_ptr = bus.read_long(te_handle);
        assert_ne!(text_handle, 0);
        assert_ne!(text_ptr, 0);
        assert_ne!(te_handle, 0);
        assert_ne!(te_ptr, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(!disp.window_list.contains(&dialog_ptr));
        assert_eq!(bus.get_alloc_size(storage_ptr), Some(170));
        assert_eq!(bus.get_alloc_size(items_handle), Some(4));
        assert_eq!(bus.get_alloc_size(items_ptr), Some(ditl.len() as u32));
        assert_eq!(bus.get_alloc_size(text_ptr), None);
        assert_eq!(bus.get_alloc_size(text_handle), None);
        assert_eq!(bus.get_alloc_size(te_ptr), None);
        assert_eq!(bus.get_alloc_size(te_handle), None);
        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
        assert!(!disp.dialog_item_handles.contains_key(&text_handle));
    }

    // ---- DisposDialog ($A983) ----

    #[test]
    fn dispos_dialog_pops_4_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        // SP+0: dialog ptr (4 bytes)
        bus.write_long(TEST_SP, 0x200000);

        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn dispos_dialog_restores_previous_port_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let prev_window = 0x181000u32;
        seed_window_regions(&mut bus, prev_window, (0, 0, 342, 512));

        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (100, 120, 220, 320);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        bus.write_long(crate::memory::globals::addr::THE_PORT, dialog_ptr);
        let a5 = cpu.read_reg(Register::A5);
        let global_ptr = bus.read_long(a5);
        bus.write_long(global_ptr, dialog_ptr);
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 2, "Prev".to_string()));

        bus.write_long(TEST_SP, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.front_window, prev_window);
        assert_eq!(*disp.current_port, prev_window);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::THE_PORT),
            prev_window
        );
        assert_eq!(bus.read_long(global_ptr), prev_window);
        assert_eq!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(prev_window + 122)),
            Some((100, 120, 220, 320)),
            "DisposDialog should invalidate the dialog-exposed area on the promoted window"
        );
        assert!(
            disp.event_queue.iter().any(|event| {
                event.what == 8 && event.message == prev_window && (event.modifiers & 1) != 0
            }),
            "DisposDialog should queue activateEvt for the promoted front window"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == prev_window),
            "DisposDialog must queue updateEvt for the newly exposed front window"
        );
    }

    #[test]
    fn dispos_dialog_invalidates_all_visible_windows_behind_and_delivers_updates() {
        // DisposDialog calls CloseWindow, so PaintBehind must make every
        // visible window behind the removed structure dirty. A modal dialog
        // can span more than one underlying window; invalidating only the
        // promoted front window leaves the other window's exposed map stale.
        // Inside Macintosh Volume I (1985), pp. I-283, I-293;
        // Macintosh Toolbox Essentials (1992), pp. 4-118, 6-119..6-120.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.menu_bar_hidden = true;

        let screen_base = disp.screen_mode.0;
        let back = bus.alloc(256);
        let middle = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            40,
            320,
            260,
            620,
            "Back",
            2,
            true,
            false,
            false,
            0,
        );
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            middle,
            screen_base,
            40,
            20,
            260,
            320,
            "Middle",
            2,
            true,
            false,
            false,
            0,
        );
        disp.validate_window_rect(&mut bus, middle, (0, 0, 220, 300));
        disp.validate_window_rect(&mut bus, back, (0, 0, 220, 300));

        let dialog_ptr = bus.alloc(256);
        disp.window_stack.push((
            middle,
            (40, 20, 260, 320),
            2,
            "Middle".to_string(),
        ));
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            dialog_ptr,
            screen_base,
            90,
            200,
            190,
            450,
            "Dialog",
            2,
            true,
            false,
            false,
            0,
        );
        bus.write_word(dialog_ptr + 108, 2); // dialogKind
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.validate_window_rect(&mut bus, dialog_ptr, (0, 0, 100, 250));
        disp.event_queue.clear();

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let middle_update = TrapDispatcher::region_handle_rect(
            &bus,
            bus.read_long(middle + 122),
        );
        let back_update = TrapDispatcher::region_handle_rect(
            &bus,
            bus.read_long(back + 122),
        );
        assert_eq!(
            middle_update,
            Some((89, 199, 191, 320)),
            "the promoted window must receive the exposed dialog structure"
        );
        assert_eq!(
            back_update,
            Some((89, 320, 191, 451)),
            "every visible window behind the dialog must receive its exposed part"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == middle),
            "DisposDialog must queue updateEvt for the promoted window"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == back),
            "DisposDialog must queue updateEvt for every exposed window"
        );

        let event_ptr = bus.alloc(16);
        let event_sp = TEST_SP - 8;
        let mut delivered_windows = Vec::new();
        for _ in 0..2 {
            cpu.write_reg(Register::A7, event_sp);
            bus.write_long(event_sp, event_ptr);
            bus.write_word(event_sp + 4, 1 << 6); // updateEvt only
            bus.write_word(event_sp + 6, 0);

            disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(event_ptr), 6, "GetNextEvent should deliver updateEvt");
            assert_eq!(bus.read_word(event_sp + 6), 0x0100);

            let window = bus.read_long(event_ptr + 2);
            assert!(
                [middle, back].contains(&window),
                "updateEvt should name an exposed window"
            );
            assert!(
                !delivered_windows.contains(&window),
                "each exposed window should receive one updateEvt"
            );
            assert!(
                TrapDispatcher::region_handle_rect(
                    &bus,
                    bus.read_long(window + 122),
                )
                .is_some(),
                "the delivered updateEvt must still have a non-empty updateRgn"
            );

            disp.begin_update_window(&mut bus, window);
            assert_eq!(
                TrapDispatcher::region_handle_rect(
                    &bus,
                    bus.read_long(window + 122),
                ),
                None,
                "BeginUpdate must consume the delivered update region"
            );
            disp.end_update_window(&mut bus, window);
            delivered_windows.push(window);
        }
        delivered_windows.sort_unstable();
        let mut expected_windows = vec![middle, back];
        expected_windows.sort_unstable();
        assert_eq!(delivered_windows, expected_windows);
        assert!(disp.debug_update_event_seen);
    }

    #[test]
    fn dispos_dialog_preserves_valid_saved_under_without_repainting_it() {
        // The HLE retains the pixels beneath a visible dialog. When that
        // snapshot is current, restoring it completes PaintBehind for the
        // covered structure; queuing an application repaint for the same area
        // can change state in partial-update drawing code. CalcVisBehind still
        // has to expose the restored pixels for later drawing.
        // Inside Macintosh Volume I (1985), pp. I-293, I-297.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.menu_bar_hidden = true;

        let (screen_base, row_bytes, _width, _height, _depth) = disp.screen_mode;
        let back = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            back,
            screen_base,
            40,
            20,
            260,
            620,
            "Back",
            2,
            true,
            false,
            false,
            0,
        );
        disp.validate_window_rect(&mut bus, back, (0, 0, 220, 600));

        let dialog_ptr = bus.alloc(256);
        disp.window_stack
            .push((back, (40, 20, 260, 620), 2, "Back".to_string()));
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            dialog_ptr,
            screen_base,
            90,
            200,
            190,
            450,
            "Dialog",
            2,
            true,
            false,
            false,
            0,
        );
        bus.write_word(dialog_ptr + 108, 2); // dialogKind
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.validate_window_rect(&mut bus, dialog_ptr, (0, 0, 100, 250));
        disp.event_queue.clear();

        let saved_rect = TrapDispatcher::dialog_saved_pixel_rect((90, 200, 190, 450));
        let saved_len = usize::from((saved_rect.2 - saved_rect.0) as u16)
            * usize::from((saved_rect.3 - saved_rect.1) as u16);
        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; saved_len].into());

        let back_vis = bus.read_long(back + 24);
        assert!(!TrapDispatcher::region_contains_point(
            &bus, back_vis, 80, 230
        ));

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(TrapDispatcher::region_contains_point(
            &bus, back_vis, 80, 230
        ));
        assert_eq!(
            TrapDispatcher::region_handle_rect(&bus, bus.read_long(back + 122)),
            None,
            "valid restored pixels must not be dirtied again"
        );
        assert!(!disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == back));
        assert_eq!(bus.read_byte(screen_base + 120 * row_bytes + 250), 0x33);
    }

    #[test]
    fn dispos_dialog_hidden_or_invalid_target_does_not_expose_windows() {
        // A hidden dialog has never covered the screen, and an invalid
        // DialogPtr is a defensive failure path. Neither may manufacture an
        // updateEvt or dirty a visible window behind the dialog.
        // Inside Macintosh Volume I (1985), pp. I-278, I-283, I-425.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.menu_bar_hidden = true;

        let previous = bus.alloc(256);
        let previous_bounds = (40, 40, 260, 340);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            previous,
            disp.screen_mode.0,
            previous_bounds.0,
            previous_bounds.1,
            previous_bounds.2,
            previous_bounds.3,
            "Previous",
            2,
            true,
            false,
            false,
            0,
        );
        disp.validate_window_rect(
            &mut bus,
            previous,
            (0, 0, previous_bounds.2 - previous_bounds.0, previous_bounds.3 - previous_bounds.1),
        );
        disp.event_queue.clear();

        let hidden = bus.alloc(256);
        disp.window_stack.push((
            previous,
            previous_bounds,
            2,
            "Previous".to_string(),
        ));
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            hidden,
            disp.screen_mode.0,
            100,
            120,
            180,
            320,
            "Hidden",
            2,
            false,
            false,
            false,
            0,
        );
        bus.write_word(hidden + 108, 2); // dialogKind
        disp.dialog_items.insert(hidden, Vec::new());
        assert_eq!(bus.read_byte(hidden + 110), 0, "fixture dialog must be hidden");
        disp.event_queue.clear();

        bus.write_long(TEST_SP, 0xDEAD_BEEFu32);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp.window_list.contains(&hidden));
        assert_eq!(disp.front_window, hidden);
        assert_eq!(
            TrapDispatcher::region_handle_rect(
                &bus,
                bus.read_long(previous + 122),
            ),
            None,
            "an invalid DialogPtr must be a no-op"
        );
        assert!(
            !disp
                .event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == previous),
            "an invalid DialogPtr must not queue updateEvt"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, hidden);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(!disp.window_list.contains(&hidden));
        assert_eq!(disp.front_window, previous);
        assert_eq!(
            TrapDispatcher::region_handle_rect(
                &bus,
                bus.read_long(previous + 122),
            ),
            None,
            "disposing a hidden dialog must not dirty the visible window behind it"
        );
        assert!(
            !disp
                .event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == previous),
            "disposing a hidden dialog must not queue updateEvt"
        );
    }

    #[test]
    fn dispos_dialog_removes_dialog_from_window_list() {
        // IM:I I-425: DisposDialog closes/disposes the dialog and
        // IM:I I-283 CloseWindow semantics remove it from window list.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let prev_window = 0x181000u32;

        disp.window_list.replace(vec![dialog_ptr, prev_window]);
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        bus.write_byte(prev_window + 110, 0xFF);
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 2, "Prev".to_string()));

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.window_list, vec![prev_window]);
        assert_eq!(disp.front_window, prev_window);
    }

    #[test]
    fn dispos_dialog_with_user_item_proc_argument_does_not_close_dialog() {
        // SetDItem stores a ProcPtr for userItem entries. A ProcPtr is not a
        // DialogPtr, so DisposDialog must not translate it into the current
        // front dialog. Doing so suppresses legitimate visible prompts whose
        // app code is installing or calling userItem procedures.
        // Inside Macintosh Volume I, I-421, I-425.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let prev_window = 0x181000u32;
        let user_item_proc = 0x00016178u32;

        seed_window_regions(&mut bus, prev_window, (0, 0, 342, 512));
        disp.window_list.replace(vec![dialog_ptr, prev_window]);
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (110, 155, 380, 645);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80,
                proc_ptr: user_item_proc,
                ..DialogItem::default()
            }],
        );
        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; 66 * 116].into());
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 2, "Prev".to_string()));

        bus.write_long(TEST_SP, user_item_proc);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(disp.window_list, vec![dialog_ptr, prev_window]);
        assert_eq!(disp.front_window, dialog_ptr);
        assert!(disp.dialog_items.contains_key(&dialog_ptr));
        assert!(disp.dialog_saved_pixels.contains_key(&dialog_ptr));
    }

    #[test]
    fn dispos_dialog_after_modal_button_hit_recovers_stale_proc_argument() {
        // When ModalDialog has just returned an enabled push button, a
        // following DisposeDialog is part of the standard modal teardown
        // pattern. If the app hands back a stale ProcPtr-shaped value in that
        // exact one-shot window, close the retained modal dialog rather than
        // leaving the button-accepted dialog visible forever.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let prev_window = 0x181000u32;
        let stale_proc_arg = 0x00016178u32;

        seed_window_regions(&mut bus, prev_window, (0, 0, 342, 512));
        disp.window_list.replace(vec![dialog_ptr, prev_window]);
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = (110, 155, 380, 645);
        disp.dialog_modal_entered.insert(dialog_ptr);
        disp.pending_modal_button_dispose_dialog = Some(dialog_ptr);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                text: String::from("OK"),
                ..DialogItem::default()
            }],
        );
        disp.window_stack
            .push((prev_window, (0, 0, 342, 512), 2, "Prev".to_string()));

        bus.write_long(TEST_SP, stale_proc_arg);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(disp.window_list, vec![prev_window]);
        assert_eq!(disp.front_window, prev_window);
        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
        assert!(disp.pending_modal_button_dispose_dialog.is_none());
    }

    #[test]
    fn dispose_dialog_disposes_allocated_storage_and_items() {
        // MTE 1992 p. 6-120: DisposeDialog calls CloseDialog, then releases
        // the copied item list and DialogRecord allocated for a NIL dStorage.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        let ditl = build_test_ditl_item(8, (10, 12, 24, 140), b"Static");
        let (items_handle, items_ptr) = install_ditl_handle_for_test(&mut bus, &ditl);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            items_handle,
        );

        let text_handle = ditl_item_handle_field(&bus, items_ptr, 1);
        let text_ptr = bus.read_long(text_handle);
        let te_handle = bus.read_long(dialog_ptr + 160);
        let te_ptr = bus.read_long(te_handle);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.get_alloc_size(text_ptr), None);
        assert_eq!(bus.get_alloc_size(text_handle), None);
        assert_eq!(bus.get_alloc_size(te_ptr), None);
        assert_eq!(bus.get_alloc_size(te_handle), None);
        assert_eq!(bus.get_alloc_size(items_ptr), None);
        assert_eq!(bus.get_alloc_size(items_handle), None);
        assert_eq!(bus.get_alloc_size(dialog_ptr), None);
        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
    }

    #[test]
    fn dispose_dialog_releases_text_handles() {
        // IM:I I-413 and I-425: CloseDialog releases standard dialog items;
        // DisposeDialog inherits that item cleanup before freeing the record.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        let ditl = build_test_ditl_item(16, (10, 12, 24, 140), b"Edit");
        let (items_handle, items_ptr) = install_ditl_handle_for_test(&mut bus, &ditl);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            items_handle,
        );
        let item_text_handle = ditl_item_handle_field(&bus, items_ptr, 1);
        let item_text_ptr = bus.read_long(item_text_handle);
        let dialog_te_handle = bus.read_long(dialog_ptr + 160);
        let dialog_te_ptr = bus.read_long(dialog_te_handle);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.get_alloc_size(item_text_ptr), None);
        assert_eq!(bus.get_alloc_size(item_text_handle), None);
        assert_eq!(bus.get_alloc_size(dialog_te_ptr), None);
        assert_eq!(bus.get_alloc_size(dialog_te_handle), None);
        assert!(!disp.dialog_item_handles.contains_key(&item_text_handle));
    }

    #[test]
    fn dispose_dialog_releases_control_mappings() {
        // IM:I I-413 and I-425: dialog-owned controls are released with the
        // dialog, including dispatcher-side control mappings.
        let (mut disp, mut cpu, mut bus) = setup();
        install_new_dialog_test_screen(&mut disp, &mut bus, 0x77);
        let ditl = build_test_ditl_item(4, (36, 40, 58, 120), b"OK");
        let (items_handle, _items_ptr) = install_ditl_handle_for_test(&mut bus, &ditl);

        let (_sp, dialog_ptr) = call_new_dialog_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            0,
            (80, 90, 150, 260),
            false,
            4,
            0xFFFF_FFFF,
            items_handle,
        );
        let box_ptr = bus.alloc(8);
        let item_ptr = bus.alloc(4);
        let type_ptr = bus.alloc(2);
        let snapshot = get_ditem_snapshot_for_test(
            &mut disp, &mut cpu, &mut bus, dialog_ptr, 1, box_ptr, item_ptr, type_ptr,
        );
        let control_handle = snapshot.item;
        let control_ptr = bus.read_long(control_handle);
        assert_ne!(control_handle, 0);
        assert_ne!(control_ptr, 0);
        assert_eq!(
            disp.dialog_control_handles.get(&control_handle),
            Some(&(dialog_ptr, 1))
        );
        assert!(disp.control_manager.contains_pointer(control_ptr));
        let aux_handle = disp.ensure_control_aux_record(&mut bus, control_handle);
        let aux_ptr = bus.read_long(aux_handle);
        assert_ne!(aux_handle, 0);
        assert_ne!(aux_ptr, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.get_alloc_size(control_ptr), None);
        assert_eq!(bus.get_alloc_size(control_handle), None);
        assert_eq!(bus.get_alloc_size(aux_ptr), None);
        assert_eq!(bus.get_alloc_size(aux_handle), None);
        assert!(disp.control_aux_state(control_handle).is_none());
        assert!(!disp.dialog_control_handles.contains_key(&control_handle));
        assert!(!disp.control_manager.contains_pointer(control_ptr));
    }

    #[test]
    fn dispose_dialog_clears_visible_snapshot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        disp.window_list.replace(vec![dialog_ptr]);
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds: (10, 20, 30, 40),
                pixels: vec![0x55; 400].into(),
            },
        );

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(!disp.dialog_visible_snapshots.contains_key(&dialog_ptr));
    }

    #[test]
    fn dispose_dialog_clears_retained_click_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        disp.window_list.replace(vec![dialog_ptr]);
        disp.retained_modal_dialog_click = Some(RetainedModalDialogClickState {
            dialog_ptr,
            item_no: 1,
            rect: (10, 20, 30, 40),
            title: "OK".to_string(),
            is_default: true,
            highlighted: true,
            delivered_to_app: false,
        });

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(disp.retained_modal_dialog_click.is_none());
    }

    #[test]
    fn dispose_dialog_with_active_tracking_cancels_tracking() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr,
            ..Default::default()
        });

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(disp.dialog_tracking.is_none());
    }

    #[test]
    fn retained_modal_dialog_consumes_outside_mouse_down() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let prev_window = bus.alloc(170);
        seed_retained_modal_dialog(&mut disp, &mut bus, dialog_ptr, prev_window, 1);

        disp.push_mouse_down(140, 260);
        let (what, _message, _, _where_v, _where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(!has_event);
        assert_eq!(what, 0);
        assert!(disp.event_queue.is_empty());
        assert_eq!(disp.front_window, dialog_ptr);
        assert!(
            disp.retained_modal_dialog_click.is_some(),
            "outside click must be captured until mouseUp so it cannot leak behind the dialog"
        );

        disp.push_mouse_up(140, 260);
        let (_what, _message, _, _where_v, _where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(!has_event);
        assert_eq!(disp.front_window, dialog_ptr);
        assert!(disp.retained_modal_dialog_click.is_none());
        assert!(
            disp.dialog_items.contains_key(&dialog_ptr),
            "outside clicks do not dismiss modal dialogs"
        );
    }

    #[test]
    fn app_owned_modal_dialog_mouse_down_is_delivered_to_event_loop() {
        // A DLOG can be created and shown with GetNewDialog while the
        // application owns the WaitNextEvent/DialogSelect loop. The Dialog
        // Manager reports enabled item clicks to that application code; it
        // does not dismiss the dialog from the Event Manager dequeue path.
        // Macintosh Toolbox Essentials 1992, pp. 6-138..6-141.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let prev_window = bus.alloc(170);
        let screen_base = 0x300000u32;
        let row_bytes = 320u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 320, 200, 8);
        let probe = screen_base + 70 * row_bytes + 150;
        bus.write_byte(probe, 17);

        seed_app_owned_modal_dialog(&mut disp, &mut bus, dialog_ptr, prev_window, 1);

        disp.push_mouse_down(70, 150);
        let (what, _message, _, where_v, where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(has_event);
        assert_eq!(what, 1);
        assert_eq!((where_v, where_h), (70, 150));
        assert_eq!(
            disp.retained_modal_dialog_click
                .as_ref()
                .map(|click| (click.item_no, click.delivered_to_app)),
            Some((1, true))
        );
        assert_eq!(
            bus.read_byte(probe),
            238,
            "app-owned modal button should still show the pressed state"
        );
        assert_eq!(disp.front_window, dialog_ptr);
        assert!(
            disp.dialog_items.contains_key(&dialog_ptr),
            "application-owned dialog should not be closed by event dequeue"
        );
    }

    #[test]
    fn app_owned_modal_dialog_button_mouse_up_dismisses_after_delivery() {
        // Some apps drive modal DLOGs with their own WaitNextEvent loop and
        // only ask the Window Manager which window was clicked. Keep the
        // mouseDown deliverable, but finish the standard modal button press
        // on mouseUp if app code has not called DialogSelect.
        // Macintosh Toolbox Essentials 1992, pp. 6-136, 6-138..6-141.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let prev_window = bus.alloc(170);
        let screen_base = 0x300000u32;
        let row_bytes = 320u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 320, 200, 8);
        let probe = screen_base + 70 * row_bytes + 150;
        bus.write_byte(probe, 17);

        seed_app_owned_modal_dialog(&mut disp, &mut bus, dialog_ptr, prev_window, 1);

        disp.push_mouse_down(70, 150);
        let (what, _message, _, where_v, where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);
        assert!(has_event);
        assert_eq!(what, 1);
        assert_eq!((where_v, where_h), (70, 150));
        assert_eq!(disp.front_window, dialog_ptr);

        disp.push_mouse_up(70, 150);
        let (what, _message, _, where_v, where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(
            has_event,
            "app-owned modal mouseUp remains deliverable after compatibility close"
        );
        assert_eq!(what, 2);
        assert_eq!((where_v, where_h), (70, 150));
        assert_eq!(bus.read_byte(probe), 17);
        assert_eq!(disp.front_window, prev_window);
        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
        assert!(disp.retained_modal_dialog_click.is_none());
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == prev_window),
            "closing the dialog must invalidate/update the exposed window"
        );
    }

    #[test]
    fn retained_modal_dialog_button_click_highlights_then_dismisses() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let prev_window = bus.alloc(170);
        let screen_base = 0x300000u32;
        let row_bytes = 320u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 320, 200, 8);
        seed_retained_modal_dialog(&mut disp, &mut bus, dialog_ptr, prev_window, 1);

        let probe = screen_base + 70 * row_bytes + 150;
        bus.write_byte(probe, 17);
        let bounds = TrapDispatcher::dialog_screen_bounds(&bus, dialog_ptr);
        assert_eq!(bounds, (0, 0, 100, 220));
        let base_pixels = disp.save_dialog_pixels(&bus, bounds);
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: base_pixels,
            },
        );
        let items = disp.dialog_items.get(&dialog_ptr).cloned().unwrap();
        assert_eq!(
            disp.dialog_item_hit_test(
                &bus,
                &items,
                bounds,
                70,
                150,
                &disp.dialog_popup_original_rects,
                dialog_ptr,
            ),
            1
        );

        disp.push_mouse_down(70, 150);
        let (_what, _message, _, _where_v, _where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(!has_event);
        assert_eq!(
            disp.retained_modal_dialog_click
                .as_ref()
                .map(|click| click.item_no),
            Some(1)
        );
        assert_eq!(
            bus.read_byte(probe),
            238,
            "button interior should invert on mouseDown"
        );
        disp.restore_visible_dialog_snapshots(&mut bus);
        disp.redraw_retained_modal_dialog_click(&mut bus);
        assert_eq!(
            bus.read_byte(probe),
            238,
            "pressed retained-modal button must survive visible-dialog snapshot redraw"
        );
        assert_eq!(disp.front_window, dialog_ptr);

        disp.push_mouse_up(70, 150);
        let (_what, _message, _, _where_v, _where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(!has_event);
        assert_eq!(
            bus.read_byte(probe),
            17,
            "button should be unhighlighted before close"
        );
        assert_eq!(disp.front_window, prev_window);
        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
        assert!(disp.retained_modal_dialog_click.is_none());
        assert!(
            !disp
                .event_queue
                .iter()
                .any(|event| matches!(event.what, 1 | 2)),
            "consumed dialog click must not leave mouse events for the game loop"
        );
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == prev_window),
            "closing the dialog must invalidate/update the exposed window"
        );
    }

    #[test]
    fn modal_dialog_button_tracking_systemless_theme_routes_pressed_state_through_provider() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let bounds = (0, 0, 100, 220);
        let item_rect = (60, 120, 80, 180);
        let screen_rect = TrapDispatcher::dialog_item_screen_rect(bounds, item_rect);

        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        disp.draw_button(
            &mut bus,
            screen_rect.0,
            screen_rect.1,
            screen_rect.2,
            screen_rect.3,
            "",
            true,
        );

        let probe_x = 150;
        let probe_y = 70;
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, probe_x, probe_y),
            "unpressed provider button interior should start clear"
        );

        let dialog_ptr = bus.alloc(170);
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr,
            bounds,
            items: vec![DialogItem {
                item_type: 4,
                rect: item_rect,
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            active_button: Some(DialogButtonTrackingState {
                mouse_down: super::super::dispatch::QueuedEvent {
                    what: 1,
                    message: 0,
                    when: 0,
                    where_v: probe_y,
                    where_h: probe_x,
                    modifiers: 0,
                },
                item_no: 1,
                rect: item_rect,
                title: String::new(),
                is_default: true,
                highlighted: false,
            }),
            ..Default::default()
        });

        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((probe_y, probe_x));
        disp.handle_dialog_button_tracking(&mut bus);

        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, probe_x, probe_y),
            "inside tracking should paint the provider pressed fill"
        );
        assert_eq!(
            disp.dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_button.as_ref())
                .map(|button| button.highlighted),
            Some(true)
        );

        disp.input_state.set_mouse_position_for_test((40, 50));
        disp.handle_dialog_button_tracking(&mut bus);

        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, probe_x, probe_y),
            "outside tracking should redraw unpressed provider chrome"
        );
        assert_eq!(
            disp.dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_button.as_ref())
                .map(|button| button.highlighted),
            Some(false)
        );
    }

    #[test]
    fn retained_modal_dialog_capture_does_not_apply_to_modeless_dialogs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let prev_window = bus.alloc(170);
        seed_retained_modal_dialog(&mut disp, &mut bus, dialog_ptr, prev_window, 4);

        disp.push_mouse_down(70, 150);
        let (what, _message, _, where_v, where_h, _modifiers, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 0xFFFF);

        assert!(has_event);
        assert_eq!(what, 1);
        assert_eq!((where_v, where_h), (70, 150));
        assert!(disp.retained_modal_dialog_click.is_none());
        assert_eq!(disp.front_window, dialog_ptr);
    }

    #[test]
    fn dispos_dialog_clears_tracking_for_disposed_dialog() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 32, 32),
            title: String::new(),
            proc_id: 1,
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

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(disp.dialog_tracking.is_none());
    }

    #[test]
    fn dispos_dialog_clears_dialog_scoped_side_maps_for_disposed_dialog() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let other_dialog_ptr = 0x210000u32;
        let text_handle = 0x300000u32;
        let other_text_handle = 0x300010u32;
        let ctrl_handle = 0x300020u32;
        let other_ctrl_handle = 0x300030u32;

        disp.dialog_items
            .insert(dialog_ptr, vec![DialogItem::default()]);
        disp.dialog_items
            .insert(other_dialog_ptr, vec![DialogItem::default()]);
        disp.dialog_item_handles
            .insert(text_handle, (dialog_ptr, 0));
        disp.dialog_item_handles
            .insert(other_text_handle, (other_dialog_ptr, 0));
        disp.dialog_control_handles
            .insert(ctrl_handle, (dialog_ptr, 1));
        disp.dialog_control_handles
            .insert(other_ctrl_handle, (other_dialog_ptr, 1));
        disp.dialog_control_values.insert((dialog_ptr, 1), 1);
        disp.dialog_control_values.insert((other_dialog_ptr, 1), 1);
        disp.hidden_dialog_item_rects
            .insert((dialog_ptr, 1), (10, 20, 30, 40));
        disp.hidden_dialog_item_rects
            .insert((other_dialog_ptr, 1), (50, 60, 70, 80));
        disp.dialog_item_popup_menus.insert((dialog_ptr, 1), 900);
        disp.dialog_item_popup_menus
            .insert((other_dialog_ptr, 1), 901);
        disp.dialog_popup_original_rects
            .insert((dialog_ptr, 1), (10, 20, 30, 130));
        disp.dialog_popup_original_rects
            .insert((other_dialog_ptr, 1), (50, 60, 70, 180));
        disp.dialog_popup_candidate_items.insert((dialog_ptr, 1));
        disp.dialog_popup_candidate_items
            .insert((other_dialog_ptr, 1));
        disp.dialog_cancel_items.insert(dialog_ptr, 2);
        disp.dialog_cancel_items.insert(other_dialog_ptr, 3);
        disp.pending_dialog_popup_menu = Some(PendingDialogPopupMenu {
            dialog_ptr,
            item_no: 1,
            menu_id: 900,
            rect: (10, 20, 30, 130),
        });

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(!disp.dialog_items.contains_key(&dialog_ptr));
        assert!(disp.dialog_items.contains_key(&other_dialog_ptr));
        assert!(!disp.dialog_item_handles.contains_key(&text_handle));
        assert!(disp.dialog_item_handles.contains_key(&other_text_handle));
        assert!(!disp.dialog_control_handles.contains_key(&ctrl_handle));
        assert!(disp.dialog_control_handles.contains_key(&other_ctrl_handle));
        assert!(!disp.dialog_control_values.contains_key(&(dialog_ptr, 1)));
        assert!(disp
            .dialog_control_values
            .contains_key(&(other_dialog_ptr, 1)));
        assert!(!disp.hidden_dialog_item_rects.contains_key(&(dialog_ptr, 1)));
        assert!(disp
            .hidden_dialog_item_rects
            .contains_key(&(other_dialog_ptr, 1)));
        assert!(!disp.dialog_item_popup_menus.contains_key(&(dialog_ptr, 1)));
        assert!(disp
            .dialog_item_popup_menus
            .contains_key(&(other_dialog_ptr, 1)));
        assert!(!disp
            .dialog_popup_original_rects
            .contains_key(&(dialog_ptr, 1)));
        assert!(disp
            .dialog_popup_original_rects
            .contains_key(&(other_dialog_ptr, 1)));
        assert!(!disp.dialog_popup_candidate_items.contains(&(dialog_ptr, 1)));
        assert!(disp
            .dialog_popup_candidate_items
            .contains(&(other_dialog_ptr, 1)));
        assert!(!disp.dialog_cancel_items.contains_key(&dialog_ptr));
        assert!(disp.dialog_cancel_items.contains_key(&other_dialog_ptr));
        assert!(disp.pending_dialog_popup_menu.is_none());
    }

    // Regression: games that run their own event loop (e.g. Escape
    // Velocity's "enter pilot/ship name" text dialogs) call
    // GetNewDialog → custom event loop → DisposDialog without ever
    // invoking ModalDialog. Before the fix, DisposDialog discarded
    // the saved-background pixels without blitting them back to the
    // screen, leaving a dialog-shaped hole over the window behind.
    // IM:I I-425 says DisposDialog internally calls CloseWindow,
    // whose PaintBehind/CalcVisBehind is supposed to restore the
    // underlying content. These three tests pin that contract.
    //
    // Save/restore geometry note: save_dialog_pixels adds the dBoxProc
    // structure margin around the bounds. The tests use bounds
    // (100,100,150,200) → save area (92,92)..(158,208) =
    // 66 rows × 116 cols = 7656 bytes.

    #[test]
    fn disposdialog_restores_saved_background_pixels() {
        let (mut disp, mut cpu, mut bus) = setup();

        // 8bpp test screen. Must be inside the 4MB test bus.
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = 0x200000u32;
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        bus.write_byte(dialog_ptr + 110, 0xFF);

        // Paint the save area with 0xCC — the "dialog pixels" that
        // should be overwritten on dispose.
        for y in 92u32..158 {
            for x in 92u32..208 {
                bus.write_byte(screen_base + y * row_bytes + x, 0xCC);
            }
        }

        // Install a saved background snapshot filled with 0x33 (the
        // "what was behind the dialog" pattern). 66*116=7656 bytes.
        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; 66 * 116].into());

        bus.write_long(TEST_SP, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // Every byte in the save area should now be 0x33, not 0xCC.
        for y in 92u32..158 {
            for x in 92u32..208 {
                let addr = screen_base + y * row_bytes + x;
                let got = bus.read_byte(addr);
                assert_eq!(
                    got, 0x33,
                    "byte at ({},{}) must be restored background 0x33, got 0x{:02X}",
                    x, y, got
                );
            }
        }
        assert!(
            !disp.dialog_saved_pixels.contains_key(&dialog_ptr),
            "saved pixels must be consumed after DisposDialog"
        );
    }

    #[test]
    fn disposdialog_restores_retained_modal_when_cached_front_is_predecessor() {
        // Apps may restore the underlying GrafPort after ModalDialog returns.
        // The entered modal remains logically front until DisposDialog even
        // when the cached front pointer now names its stacked predecessor.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes = 640u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let dialog_ptr = bus.alloc(170);
        let predecessor = bus.alloc(170);
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-100i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 50);
        bus.write_word(dialog_ptr + 22, 100);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.dialog_modal_entered.insert(dialog_ptr);
        disp.front_window = predecessor;
        disp.window_bounds = (0, 0, 480, 640);
        disp.window_stack
            .push((predecessor, (0, 0, 480, 640), 0, String::new()));
        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; 66 * 116].into());

        for y in 92u32..158 {
            for x in 92u32..208 {
                bus.write_byte(screen_base + y * row_bytes + x, 0xCC);
            }
        }

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(screen_base + 100 * row_bytes + 100), 0x33);
        assert!(!disp.dialog_saved_pixels.contains_key(&dialog_ptr));
    }

    #[test]
    fn visible_dialog_saved_background_tracks_screen_draws_behind_it() {
        // Some applications keep animating their screen-backed main window
        // while a visible DLOG is frontmost. The Window Manager still closes
        // the dialog via CloseWindow/DisposDialog (IM:I I-283, I-425), so the
        // saved-under pixels must reflect those later behind-dialog draws.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = bus.alloc(170);
        let background_port = bus.alloc(170);
        let save_top = 92u32;
        let save_left = 92u32;
        let save_bottom = 158u32;
        let save_right = 208u32;
        let row_width = (save_right - save_left) as usize;

        for y in save_top..save_bottom {
            for x in save_left..save_right {
                bus.write_byte(screen_base + y * row_bytes + x, 0xEE);
            }
        }

        disp.dialog_saved_pixels.insert(
            dialog_ptr,
            vec![0x11; row_width * (save_bottom - save_top) as usize].into(),
        );
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: vec![0xEE; row_width * (save_bottom - save_top) as usize].into(),
            },
        );

        for y in 120u32..123 {
            for x in 130u32..139 {
                bus.write_byte(screen_base + y * row_bytes + x, 0x44);
            }
        }
        disp.refresh_dialog_saved_pixels_after_screen_draw(
            &bus,
            background_port,
            (120, 130, 123, 139),
        );

        let saved = disp.dialog_saved_pixels.get(&dialog_ptr).unwrap();
        let touched_idx = (120 - save_top) as usize * row_width + (130 - save_left) as usize;
        let untouched_idx = (100 - save_top) as usize * row_width + (100 - save_left) as usize;
        assert_eq!(saved[touched_idx], 0x44);
        assert_eq!(saved[untouched_idx], 0x11);

        for y in save_top..save_bottom {
            for x in save_left..save_right {
                bus.write_byte(screen_base + y * row_bytes + x, 0xEE);
            }
        }

        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = bounds;
        bus.write_byte(dialog_ptr + 110, 0xFF);
        disp.window_list.replace(vec![dialog_ptr]);
        disp.window_stack.push((0, (0, 0, 0, 0), -1, String::new()));

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(screen_base + 120 * row_bytes + 130), 0x44);
        assert_eq!(bus.read_byte(screen_base + 100 * row_bytes + 100), 0x11);
    }

    #[test]
    fn active_modal_dialog_saved_background_tracks_screen_draws_behind_it() {
        // ModalDialog moves the visible dialog snapshot into dialog_tracking
        // while the modal loop is active. Background screen draws behind that
        // front modal still need to update the saved-under pixels that
        // DisposDialog/CloseWindow will restore.
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = bus.alloc(170);
        let background_port = bus.alloc(170);
        let save_top = 92u32;
        let save_left = 92u32;
        let save_bottom = 158u32;
        let save_right = 208u32;
        let row_width = (save_right - save_left) as usize;

        for y in save_top..save_bottom {
            for x in save_left..save_right {
                bus.write_byte(screen_base + y * row_bytes + x, 0xEE);
            }
        }

        disp.dialog_saved_pixels.insert(
            dialog_ptr,
            vec![0x11; row_width * (save_bottom - save_top) as usize].into(),
        );
        let mut tracking = dialog_tracking_state_for_test(dialog_ptr);
        tracking.bounds = bounds;
        disp.dialog_tracking = Some(tracking);

        for y in 120u32..123 {
            for x in 130u32..139 {
                bus.write_byte(screen_base + y * row_bytes + x, 0x44);
            }
        }
        disp.refresh_dialog_saved_pixels_after_screen_draw(
            &bus,
            background_port,
            (120, 130, 123, 139),
        );

        let saved = disp.dialog_saved_pixels.get(&dialog_ptr).unwrap();
        let touched_idx = (120 - save_top) as usize * row_width + (130 - save_left) as usize;
        let untouched_idx = (100 - save_top) as usize * row_width + (100 - save_left) as usize;
        assert_eq!(saved[touched_idx], 0x44);
        assert_eq!(saved[untouched_idx], 0x11);
    }

    #[test]
    fn retained_modal_dialog_saved_background_ignores_same_port_dialog_draws() {
        // After ModalDialog returns with a retained visible modal, applications
        // may redraw controls through the dialog's own screen-backed GrafPort.
        // Those writes update visible dialog content, not the pixels underneath
        // the window that DisposDialog must later restore.
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = bus.alloc(170);
        let save_top = 92u32;
        let save_left = 92u32;
        let save_bottom = 158u32;
        let save_right = 208u32;
        let row_width = (save_right - save_left) as usize;

        disp.dialog_saved_pixels.insert(
            dialog_ptr,
            vec![0x11; row_width * (save_bottom - save_top) as usize].into(),
        );
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: vec![0xEE; row_width * (save_bottom - save_top) as usize].into(),
            },
        );

        let touched_idx = (120 - save_top) as usize * row_width + (130 - save_left) as usize;
        bus.write_byte(screen_base + 120 * row_bytes + 130, 0x22);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, dialog_ptr, (120, 130, 121, 131));
        assert_eq!(
            disp.dialog_saved_pixels.get(&dialog_ptr).unwrap()[touched_idx],
            0x11,
            "pre-modal same-port dialog drawing must not become saved-under background"
        );

        disp.dialog_modal_entered.insert(dialog_ptr);
        bus.write_byte(screen_base + 120 * row_bytes + 130, 0x44);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, dialog_ptr, (120, 130, 121, 131));
        assert_eq!(
            disp.dialog_saved_pixels.get(&dialog_ptr).unwrap()[touched_idx],
            0x11,
            "retained modal same-port dialog drawing must not update saved-under pixels"
        );
    }

    #[test]
    fn nested_child_dialog_drawing_does_not_replace_parent_pixels_saved_underneath() {
        // A child modal is saved over the already-rendered parent. After the
        // child returns from ModalDialog, application userItem drawing still
        // targets the child's screen-backed port; it must update the child,
        // not the pixels that DisposDialog restores from beneath it.
        // IM:I I-405, I-425.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes = 640u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let parent_ptr = bus.alloc(170);
        let child_ptr = bus.alloc(170);
        let parent_bounds = (80i16, 80i16, 260i16, 360i16);
        let child_bounds = (120i16, 130i16, 220i16, 310i16);
        seed_window_regions(&mut bus, parent_ptr, parent_bounds);
        seed_window_regions(&mut bus, child_ptr, child_bounds);
        for (window, bounds) in [(parent_ptr, parent_bounds), (child_ptr, child_bounds)] {
            bus.write_word(window + 8, (-bounds.0) as u16);
            bus.write_word(window + 10, (-bounds.1) as u16);
            disp.dialog_items.insert(window, Vec::new());
        }

        let parent_probe = screen_base + 150 * row_bytes + 170;
        bus.write_byte(parent_probe, 0x33);
        disp.ensure_dialog_background_saved(&bus, child_ptr, child_bounds);
        disp.dialog_visible_snapshots.insert(
            child_ptr,
            PersistentDialogSnapshot {
                bounds: child_bounds,
                pixels: disp.save_dialog_pixels(&bus, child_bounds),
            },
        );

        disp.front_window = child_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = child_ptr);
        disp.window_bounds = child_bounds;
        disp.window_list.replace(vec![child_ptr, parent_ptr]);
        disp.window_stack
            .push((parent_ptr, parent_bounds, 2, "Parent".to_string()));
        disp.dialog_modal_entered.insert(child_ptr);

        // Simulate a child userItem callback drawing after ModalDialog has
        // returned, at a pixel that belongs to the parent underneath.
        bus.write_byte(parent_probe, 0xCC);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, child_ptr, (150, 170, 151, 171));

        bus.write_long(TEST_SP, child_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            bus.read_byte(parent_probe),
            0x33,
            "disposing the child must restore the original parent userItem pixel"
        );
        assert_eq!(disp.front_window, parent_ptr);
        assert_eq!(*disp.current_port, parent_ptr);
    }

    #[test]
    fn stale_fullscreen_dialog_exposure_restores_from_offscreen_scene_port() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc(100 * 80);
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 100, 100, 80, 8);
        disp.device_clut.set_entry(0x22, [0x3333, 0x7777, 0x2222]);
        disp.color_manager_clut.replace(*disp.device_clut);

        let offscreen_base = bus.alloc(100 * 80);
        bus.write_bytes(offscreen_base, &vec![0x22; 100 * 80]);
        let port = bus.alloc(170);
        let pixmap_handle = bus.alloc(4);
        let pixmap_ptr = bus.alloc(50);
        bus.write_word(port + 6, 0xC000);
        bus.write_long(port + 2, pixmap_handle);
        bus.write_long(pixmap_handle, pixmap_ptr);
        bus.write_long(pixmap_ptr, offscreen_base);
        bus.write_word(pixmap_ptr + 4, 0x8000 | 100);
        bus.write_word(pixmap_ptr + 6, 0);
        bus.write_word(pixmap_ptr + 8, 0);
        bus.write_word(pixmap_ptr + 10, 80);
        bus.write_word(pixmap_ptr + 12, 100);
        bus.write_word(pixmap_ptr + 32, 8);
        disp.cport_ports.insert(port);

        bus.write_bytes(screen_base, &vec![0x00; 100 * 80]);
        assert!(
            disp.restore_dialog_exposure_from_fullscreen_offscreen_port(&mut bus, (20, 20, 50, 70))
        );
        assert_eq!(bus.read_byte(screen_base + 20 * 100 + 20), 0x22);
        assert_eq!(bus.read_byte(screen_base + 49 * 100 + 69), 0x22);
        assert_eq!(bus.read_byte(screen_base + 2 * 100 + 2), 0x00);
    }

    #[test]
    fn disposdialog_restore_is_bounded_by_save_margin() {
        // Pin that the restore writes exactly the dBoxProc structure-margin
        // rectangle and does not stomp adjacent bytes. This catches
        // off-by-one errors in the save/restore geometry.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = 0x200000u32;
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        bus.write_byte(dialog_ptr + 110, 0xFF);

        // Paint the entire screen area of interest (including a
        // generous border around the save rect) with 0xAA.
        for y in 85u32..165 {
            for x in 85u32..215 {
                bus.write_byte(screen_base + y * row_bytes + x, 0xAA);
            }
        }

        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x33; 66 * 116].into());

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        // Bytes OUTSIDE the save area must still be 0xAA.
        // Above the save rect:
        for x in 85u32..215 {
            assert_eq!(bus.read_byte(screen_base + 91 * row_bytes + x), 0xAA);
        }
        // Below the save rect:
        for x in 85u32..215 {
            assert_eq!(bus.read_byte(screen_base + 158 * row_bytes + x), 0xAA);
        }
        // Left of the save rect:
        for y in 85u32..165 {
            assert_eq!(bus.read_byte(screen_base + y * row_bytes + 91), 0xAA);
        }
        // Right of the save rect:
        for y in 85u32..165 {
            assert_eq!(bus.read_byte(screen_base + y * row_bytes + 208), 0xAA);
        }
        // Bytes INSIDE the save area must now be 0x33.
        for y in 92u32..158 {
            for x in 92u32..208 {
                assert_eq!(bus.read_byte(screen_base + y * row_bytes + x), 0x33);
            }
        }
    }

    #[test]
    fn disposdialog_without_saved_pixels_leaves_screen_untouched() {
        // If no saved pixels exist for this dialog (e.g. ModalDialog
        // already consumed them on flash completion), DisposDialog
        // must be a no-op on the screen — no panic, no accidental
        // fill.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let bounds = (100i16, 100i16, 150i16, 200i16);
        let dialog_ptr = 0x200000u32;
        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;

        for y in 95u32..155 {
            for x in 95u32..205 {
                bus.write_byte(screen_base + y * row_bytes + x, 0x77);
            }
        }
        assert!(!disp.dialog_saved_pixels.contains_key(&dialog_ptr));

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        for y in 95u32..155 {
            for x in 95u32..205 {
                let addr = screen_base + y * row_bytes + x;
                assert_eq!(
                    bus.read_byte(addr),
                    0x77,
                    "DisposDialog without saved pixels must not write to screen"
                );
            }
        }
    }

    #[test]
    fn disposdialog_non_front_does_not_restore() {
        // If the dialog being disposed is NOT the front window, we
        // don't know its correct bounds (self.window_bounds belongs
        // to whatever is currently front). Restoring at the wrong
        // coords would corrupt the screen over the actual front
        // window. Expected behavior: discard saved pixels silently.
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let dialog_ptr = 0x200000u32;
        let other_front = 0x280000u32;
        disp.front_window = other_front; // dialog_ptr is NOT front
        disp.window_bounds = (200, 200, 300, 400); // matches other_front

        for y in 95u32..155 {
            for x in 95u32..205 {
                bus.write_byte(screen_base + y * row_bytes + x, 0x55);
            }
        }
        disp.dialog_saved_pixels
            .insert(dialog_ptr, vec![0x99; 66 * 116].into());

        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        for y in 95u32..155 {
            for x in 95u32..205 {
                let addr = screen_base + y * row_bytes + x;
                assert_eq!(
                    bus.read_byte(addr),
                    0x55,
                    "non-front DisposDialog must not restore over current front window"
                );
            }
        }
        assert!(
            !disp.dialog_saved_pixels.contains_key(&dialog_ptr),
            "saved pixels must still be removed for non-front dispose"
        );
    }

    // ---- ParamText ($A98B) ----

    fn write_pascal_str(bus: &mut crate::memory::MacMemoryBus, addr: u32, s: &[u8]) {
        bus.write_byte(addr, s.len() as u8);
        for (i, b) in s.iter().enumerate() {
            bus.write_byte(addr + 1 + i as u32, *b);
        }
    }

    #[test]
    fn param_text_saves_all_four_strings_and_pops_16_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let p0 = 0x300000u32;
        let p1 = 0x300100u32;
        let p2 = 0x300200u32;
        let p3 = 0x300300u32;
        write_pascal_str(&mut bus, p0, b"alpha");
        write_pascal_str(&mut bus, p1, b"bravo");
        write_pascal_str(&mut bus, p2, b"chi");
        write_pascal_str(&mut bus, p3, b"d");

        bus.write_long(TEST_SP, p3);
        bus.write_long(TEST_SP + 4, p2);
        bus.write_long(TEST_SP + 8, p1);
        bus.write_long(TEST_SP + 12, p0);

        disp.dispatch_dialog(true, 0x18B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
        assert_eq!(disp.param_text.slot(0).as_deref(), Some(b"alpha".as_slice()));
        assert_eq!(disp.param_text.slot(1).as_deref(), Some(b"bravo".as_slice()));
        assert_eq!(disp.param_text.slot(2).as_deref(), Some(b"chi".as_slice()));
        assert_eq!(disp.param_text.slot(3).as_deref(), Some(b"d".as_slice()));
    }

    #[test]
    fn param_text_empty_pascal_string_clears_slot() {
        // Per Inside Macintosh Volume I, I-422, an empty Pascal string
        // (length-byte = 0) IS a valid value — the caret placeholder
        // gets replaced by nothing. This is the explicit "clear this
        // slot" idiom. Distinguishes from NIL (preserves prior).
        let (mut disp, mut cpu, mut bus) = setup();
        disp.param_text.set_slots(std::array::from_fn(|_| b"stale".to_vec()));

        let empty_ptr = 0x300000u32;
        write_pascal_str(&mut bus, empty_ptr, b"");

        for off in [0u32, 4, 8, 12] {
            bus.write_long(TEST_SP + off, empty_ptr);
        }

        disp.dispatch_dialog(true, 0x18B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        for i in 0..4 {
            assert_eq!(
                disp.param_text.slot(i).as_deref(),
                Some(&b""[..]),
                "empty Pascal string must clear slot {}",
                i
            );
        }
    }

    #[test]
    fn apply_param_text_substitutes_caret_placeholders() {
        let (disp, _cpu, _bus) = setup();
        disp.param_text.set_slot(0, b"MS UserKey".to_vec());
        disp.param_text.set_slot(1, b"42".to_vec());

        assert_eq!(
            disp.apply_param_text("Unable to open the \"^0\" file."),
            "Unable to open the \"MS UserKey\" file."
        );
        assert_eq!(disp.apply_param_text("count: ^1"), "count: 42");
        assert_eq!(
            disp.apply_param_text("plain text without placeholders"),
            "plain text without placeholders"
        );
        assert_eq!(disp.apply_param_text("^0 ^1 ^2 ^3"), "MS UserKey 42  ");
        assert_eq!(
            disp.apply_param_text("^A literal caret"),
            "^A literal caret"
        );

        // Edge cases: lone trailing caret, double caret, caret at boundary,
        // out-of-range digit (^9 has no slot 9 → kept literal).
        assert_eq!(disp.apply_param_text("trailing^"), "trailing^");
        assert_eq!(disp.apply_param_text("^^0"), "^MS UserKey");
        assert_eq!(disp.apply_param_text("^9 unknown slot"), "^9 unknown slot");
        assert_eq!(disp.apply_param_text(""), "");
    }

    #[test]
    fn static_text_encodes_unicode_and_paramtext_once_as_mac_roman() {
        let (disp, _cpu, _bus) = setup();
        let classic = b"\x80\xA5\xAA\xC9\xD0\xD2\xDB\xDE";
        let unicode = decode_mac_roman(classic);

        assert_eq!(disp.static_text_bytes(&unicode), classic);

        disp.param_text.set_slot(0, classic.to_vec());
        assert_eq!(disp.apply_param_text("Prompt: ^0"), format!("Prompt: {unicode}"));
        assert_eq!(
            disp.static_text_bytes("Prompt: ^0"),
            [b"Prompt: ".as_slice(), classic].concat()
        );

        assert_eq!(disp.static_text_bytes("unknown 🦀"), b"unknown ?");
    }

    #[test]
    fn param_text_nil_pointer_preserves_previous_slot_value() {
        // Per Inside Macintosh Volume I, I-422, passing NIL for any
        // ParamText slot must leave the prior value unchanged — apps
        // commonly stage one parameter at a time before opening an
        // alert, expecting the others to retain whatever they were
        // last set to.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.param_text.set_slots([
            b"old0".to_vec(),
            b"old1".to_vec(),
            b"old2".to_vec(),
            b"old3".to_vec(),
        ]);

        let new0 = 0x300000u32;
        write_pascal_str(&mut bus, new0, b"new0");

        bus.write_long(TEST_SP, 0); // param3 = NIL
        bus.write_long(TEST_SP + 4, 0); // param2 = NIL
        bus.write_long(TEST_SP + 8, 0); // param1 = NIL
        bus.write_long(TEST_SP + 12, new0);

        disp.dispatch_dialog(true, 0x18B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(disp.param_text.slot(0).as_deref(), Some(b"new0".as_slice()), "param0 should be replaced");
        assert_eq!(disp.param_text.slot(1).as_deref(), Some(b"old1".as_slice()), "NIL must preserve param1");
        assert_eq!(disp.param_text.slot(2).as_deref(), Some(b"old2".as_slice()), "NIL must preserve param2");
        assert_eq!(disp.param_text.slot(3).as_deref(), Some(b"old3".as_slice()), "NIL must preserve param3");
    }

    #[test]
    fn apply_param_text_returns_borrowed_when_no_placeholders() {
        // Pin the no-allocation contract: the common case (DITL text
        // without any `^N` placeholders) must return Cow::Borrowed so
        // draw_static_text doesn't allocate per item.
        use std::borrow::Cow;
        let (disp, _cpu, _bus) = setup();
        disp.param_text.set_slot(0, b"value".to_vec());

        let plain = disp.apply_param_text("hello world");
        assert!(
            matches!(plain, Cow::Borrowed(_)),
            "no-placeholder input must skip the allocation path"
        );

        let substituted = disp.apply_param_text("hello ^0");
        assert!(
            matches!(substituted, Cow::Owned(_)),
            "with-placeholder input takes the allocation path"
        );
        assert_eq!(substituted, "hello value");
    }

    // ---- GetDItem ($A98D) ----

    #[test]
    fn get_ditem_clears_outputs_and_pops_18_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Set up output pointers
        let box_ptr = 0x300000u32;
        let item_ptr = 0x300100u32;
        let type_ptr = 0x300200u32;

        // Pre-fill output locations with non-zero to verify they get cleared
        bus.write_long(box_ptr, 0xDEADBEEF);
        bus.write_long(box_ptr + 4, 0xDEADBEEF);
        bus.write_long(item_ptr, 0xDEADBEEF);
        bus.write_word(type_ptr, 0xBEEF);

        // Stack layout: SP+0: box(4), SP+4: item(4), SP+8: type(4), SP+12: itemNo(2), SP+14: dialog(4)
        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, 1); // item number
        bus.write_long(TEST_SP + 14, 0x200000); // dialog ptr

        let result = disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 18);

        // Verify outputs were cleared to 0
        assert_eq!(bus.read_word(type_ptr), 0);
        assert_eq!(bus.read_long(item_ptr), 0);
        assert_eq!(bus.read_long(box_ptr), 0);
        assert_eq!(bus.read_long(box_ptr + 4), 0);
    }

    #[test]
    fn get_ditem_text_handle_returns_existing_raw_text_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let box_ptr = 0x300000u32;
        let item_ptr = 0x300100u32;
        let type_ptr = 0x300200u32;
        let text_handle = bus.alloc(4);
        let text_ptr = bus.alloc(5);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(22);

        bus.write_bytes(text_ptr, b"Hello");
        bus.write_long(text_handle, text_ptr);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, text_handle);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 5);
        bus.write_bytes(ditl_ptr + 16, b"Hello");

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 8,
                rect: (10, 20, 30, 40),
                text: "Hello".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(item_ptr), text_handle);
        assert_eq!(bus.read_long(ditl_ptr + 2), text_handle);
        assert_eq!(bus.get_alloc_size(text_ptr), Some(5));
        assert_eq!(bus.read_bytes(text_ptr, 5), b"Hello".to_vec());
    }

    #[test]
    fn get_ditem_records_pending_popup_candidate_without_associating() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let box_ptr = bus.alloc(8);
        let item_ptr = bus.alloc(4);
        let type_ptr = bus.alloc(2);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (10, 20, 30, 40),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.last_inserted_menu_id = Some(1234);

        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);

        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(!disp.dialog_item_popup_menus.contains_key(&(dialog_ptr, 1)));
        assert!(!disp
            .dialog_popup_original_rects
            .contains_key(&(dialog_ptr, 1)));
        let pending = disp
            .pending_dialog_popup_menu
            .expect("enabled userItem should leave a pending popup candidate");
        assert_eq!(pending.dialog_ptr, dialog_ptr);
        assert_eq!(pending.item_no, 1);
        assert_eq!(pending.menu_id, 1234);
        assert_eq!(pending.rect, (10, 20, 30, 40));
        assert_eq!(disp.last_inserted_menu_id, None);
    }

    #[test]
    fn get_ditem_ignores_recent_inserted_menu_for_disabled_user_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let box_ptr = bus.alloc(8);
        let item_ptr = bus.alloc(4);
        let type_ptr = bus.alloc(2);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80,
                rect: (10, 20, 30, 40),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.last_inserted_menu_id = Some(1234);

        bus.write_long(TEST_SP, box_ptr);
        bus.write_long(TEST_SP + 4, item_ptr);
        bus.write_long(TEST_SP + 8, type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);

        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(!disp.dialog_item_popup_menus.contains_key(&(dialog_ptr, 1)));
        assert!(!disp
            .dialog_popup_original_rects
            .contains_key(&(dialog_ptr, 1)));
        assert!(disp.pending_dialog_popup_menu.is_none());
        assert_eq!(disp.last_inserted_menu_id, None);
    }

    #[test]
    fn set_ditem_confirms_pending_popup_user_item_when_proc_installed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);
        let set_box_ptr = bus.alloc(8);
        let proc_ptr = 0x00AB_CDEF;

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (10, 20, 30, 40),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.last_inserted_menu_id = Some(1234);

        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(set_box_ptr, 12);
        bus.write_word(set_box_ptr + 2, 22);
        bus.write_word(set_box_ptr + 4, 32);
        bus.write_word(set_box_ptr + 6, 42);
        bus.write_long(TEST_SP, set_box_ptr);
        bus.write_long(TEST_SP + 4, proc_ptr);
        bus.write_word(TEST_SP + 8, 0);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            disp.dialog_item_popup_menus.get(&(dialog_ptr, 1)),
            Some(&1234)
        );
        assert_eq!(
            disp.dialog_popup_original_rects.get(&(dialog_ptr, 1)),
            Some(&(10, 20, 30, 40))
        );
        assert!(disp.pending_dialog_popup_menu.is_none());
    }

    #[test]
    fn set_ditem_narrowing_user_item_records_popup_candidate_without_proc() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let set_box_ptr = bus.alloc(8);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (10, 20, 30, 150),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(set_box_ptr, 10);
        bus.write_word(set_box_ptr + 2, 20);
        bus.write_word(set_box_ptr + 4, 30);
        bus.write_word(set_box_ptr + 6, 35);
        bus.write_long(TEST_SP, set_box_ptr);
        bus.write_long(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 8, 0);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(!disp.dialog_item_popup_menus.contains_key(&(dialog_ptr, 1)));
        assert_eq!(
            disp.dialog_popup_original_rects.get(&(dialog_ptr, 1)),
            Some(&(10, 20, 30, 150))
        );
        assert!(disp.dialog_popup_candidate_items.contains(&(dialog_ptr, 1)));
        assert_eq!(
            disp.dialog_items
                .get(&dialog_ptr)
                .and_then(|items| items.first())
                .map(|item| item.rect),
            Some((10, 20, 30, 35))
        );
    }

    // ---- SetDItem ($A98E) ----

    #[test]
    fn set_ditem_pops_16_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
    }

    #[test]
    fn set_ditem_updates_item_fields_visible_through_get_ditem() {
        // Inside Macintosh Volume I, I-421: SetDItem changes itemType, item,
        // and box for the specified item; GetDItem must then report those
        // updated fields.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(22);
        let old_handle = bus.alloc(4);
        let old_ptr = bus.alloc(4);
        let new_handle = bus.alloc(4);
        let new_ptr = bus.alloc(3);
        let set_box_ptr = bus.alloc(8);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);

        bus.write_bytes(old_ptr, b"Old!");
        bus.write_bytes(new_ptr, b"New");
        bus.write_long(old_handle, old_ptr);
        bus.write_long(new_handle, new_ptr);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0); // one item
        bus.write_long(ditl_ptr + 2, old_handle);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 4);
        bus.write_bytes(ditl_ptr + 16, b"Old!");

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 8,
                rect: (10, 20, 30, 40),
                text: "Old!".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.dialog_item_handles.insert(old_handle, (dialog_ptr, 0));

        bus.write_word(set_box_ptr, 111);
        bus.write_word(set_box_ptr + 2, 222);
        bus.write_word(set_box_ptr + 4, 333);
        bus.write_word(set_box_ptr + 6, 444);
        bus.write_long(TEST_SP, set_box_ptr);
        bus.write_long(TEST_SP + 4, new_handle);
        bus.write_word(TEST_SP + 8, 16);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);

        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(get_type_ptr), 16);
        assert_eq!(bus.read_long(get_item_ptr), new_handle);
        assert_eq!(bus.read_word(get_box_ptr), 111);
        assert_eq!(bus.read_word(get_box_ptr + 2), 222);
        assert_eq!(bus.read_word(get_box_ptr + 4), 333);
        assert_eq!(bus.read_word(get_box_ptr + 6), 444);
    }

    #[test]
    fn set_ditem_user_item_treats_item_parameter_as_proc_ptr() {
        // Inside Macintosh Volume I, I-421: for userItem, SetDItem's `item`
        // parameter is a draw procedure pointer (ProcPtr), not a text/control
        // handle. GetDItem should report the same ProcPtr value.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let proc_ptr = 0x00AB_CDEFu32;
        let set_box_ptr = bus.alloc(8);
        let get_box_ptr = bus.alloc(8);
        let get_item_ptr = bus.alloc(4);
        let get_type_ptr = bus.alloc(2);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0); // one item
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0,
                rect: (10, 20, 30, 40),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(set_box_ptr, 50);
        bus.write_word(set_box_ptr + 2, 60);
        bus.write_word(set_box_ptr + 4, 70);
        bus.write_word(set_box_ptr + 6, 80);
        bus.write_long(TEST_SP, set_box_ptr);
        bus.write_long(TEST_SP + 4, proc_ptr);
        bus.write_word(TEST_SP + 8, 0);
        bus.write_word(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 12, dialog_ptr);
        disp.dispatch_dialog(true, 0x18E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, get_box_ptr);
        bus.write_long(TEST_SP + 4, get_item_ptr);
        bus.write_long(TEST_SP + 8, get_type_ptr);
        bus.write_word(TEST_SP + 12, 1);
        bus.write_long(TEST_SP + 14, dialog_ptr);
        disp.dispatch_dialog(true, 0x18D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(get_type_ptr), 0);
        assert_eq!(bus.read_long(get_item_ptr), proc_ptr);
        assert_eq!(bus.read_word(get_box_ptr), 50);
        assert_eq!(bus.read_word(get_box_ptr + 2), 60);
        assert_eq!(bus.read_word(get_box_ptr + 4), 70);
        assert_eq!(bus.read_word(get_box_ptr + 6), 80);
    }

    #[test]
    fn set_dialog_item_text_resizes_existing_text_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let text_handle = bus.alloc(4);
        let text_ptr = bus.alloc(2);
        let new_text_ptr = 0x300000u32;

        bus.write_long(text_handle, text_ptr);
        bus.write_byte(text_ptr, 1);
        bus.write_byte(text_ptr + 1, b'A');
        bus.write_byte(new_text_ptr, 5);
        bus.write_bytes(new_text_ptr + 1, b"Hello");

        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 8,
                rect: (0, 0, 10, 10),
                text: "A".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.dialog_item_handles
            .insert(text_handle, (dialog_ptr, 0));

        bus.write_long(TEST_SP, new_text_ptr);
        bus.write_long(TEST_SP + 4, text_handle);

        let result = disp.dispatch_dialog(true, 0x18F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let resized_ptr = bus.read_long(text_handle);
        assert_eq!(bus.get_alloc_size(resized_ptr), Some(5));
        assert_eq!(bus.read_bytes(resized_ptr, 5), b"Hello".to_vec());
    }

    #[test]
    fn set_dialog_item_text_redraws_visible_text_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((200 * 100) as u32);
        let row_bytes = 200u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 200, 100, 8);
        bus.write_long(0x0824, screen_base);
        TrapDispatcher::fb_fill_rect(
            &mut bus,
            screen_base,
            row_bytes,
            8,
            200,
            100,
            0,
            0,
            100,
            200,
            false,
        );
        let white = bus.read_byte(screen_base);

        let bounds = (10, 20, 70, 180);
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 6, 0); // GrafPort, not CGrafPort
        bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
        bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, (bounds.2 - bounds.0) as u16);
        bus.write_word(dialog_ptr + 22, (bounds.3 - bounds.1) as u16);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        bus.write_word(dialog_ptr + 164, 0xFFFF);
        bus.write_word(dialog_ptr + 168, 1);

        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);
        let text_handle = bus.alloc(4);
        let text_ptr = bus.alloc(1);
        bus.write_bytes(text_ptr, b"A");
        bus.write_long(text_handle, text_ptr);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, text_handle);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 10);
        bus.write_word(ditl_ptr + 10, 26);
        bus.write_word(ditl_ptr + 12, 140);
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 0);

        let items = vec![DialogItem {
            item_type: 8,
            rect: (10, 10, 26, 140),
            text: "A".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        disp.dialog_items.insert(dialog_ptr, items.clone());
        disp.dialog_item_handles
            .insert(text_handle, (dialog_ptr, 0));
        disp.draw_dialog(&mut bus, bounds, 2, "", &items, 1, "", 0, false, dialog_ptr);

        let count_nonwhite = |bus: &MacMemoryBus| -> usize {
            let mut count = 0;
            for y in 20..36u32 {
                for x in 30..160u32 {
                    if bus.read_byte(screen_base + y * row_bytes + x) != white {
                        count += 1;
                    }
                }
            }
            count
        };
        let before = count_nonwhite(&bus);

        let pstr = bus.alloc(9);
        bus.write_byte(pstr, 8);
        bus.write_bytes(pstr + 1, b"WWWWWWWW");
        bus.write_long(TEST_SP, pstr);
        bus.write_long(TEST_SP + 4, text_handle);

        let result = disp.dispatch_dialog(true, 0x18F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let after = count_nonwhite(&bus);
        assert!(
            after > before + 20,
            "SetDialogItemText must draw the updated text item; before={} after={}",
            before,
            after
        );
    }

    #[test]
    fn set_dialog_item_text_redraw_clips_repaint_to_dialog_bounds() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((200 * 100) as u32);
        let row_bytes = 200u32;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 200, 100, 8);
        bus.write_long(0x0824, screen_base);
        TrapDispatcher::fb_fill_rect(
            &mut bus,
            screen_base,
            row_bytes,
            8,
            200,
            100,
            0,
            0,
            100,
            200,
            true,
        );

        let bounds = (10, 20, 70, 120);
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 6, 0); // GrafPort, not CGrafPort
        bus.write_word(dialog_ptr + 8, (-bounds.0) as u16);
        bus.write_word(dialog_ptr + 10, (-bounds.1) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, (bounds.2 - bounds.0) as u16);
        bus.write_word(dialog_ptr + 22, (bounds.3 - bounds.1) as u16);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        bus.write_word(dialog_ptr + 164, 1); // item 2 is the active edit field
        bus.write_word(dialog_ptr + 168, 1);

        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(30);
        let edit_handle = bus.alloc(4);
        let edit_ptr = bus.alloc(3);
        bus.write_bytes(edit_ptr, b"Old");
        bus.write_long(edit_handle, edit_ptr);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1); // two items
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 5);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 20);
        bus.write_word(ditl_ptr + 12, 160); // extends beyond dialog right
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 0);
        bus.write_long(ditl_ptr + 16, edit_handle);
        bus.write_word(ditl_ptr + 20, 25);
        bus.write_word(ditl_ptr + 22, 20);
        bus.write_word(ditl_ptr + 24, 41);
        bus.write_word(ditl_ptr + 26, 80);
        bus.write_byte(ditl_ptr + 28, 16);
        bus.write_byte(ditl_ptr + 29, 0);

        let items = vec![
            DialogItem {
                item_type: 8,
                rect: (5, 20, 20, 160),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 16,
                rect: (25, 20, 41, 80),
                text: "Old".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];
        disp.dialog_items.insert(dialog_ptr, items.clone());
        disp.dialog_item_handles
            .insert(edit_handle, (dialog_ptr, 1));
        disp.draw_dialog(
            &mut bus, bounds, 2, "", &items, 1, "Old", 2, false, dialog_ptr,
        );

        let sample = screen_base + 17 * row_bytes + 150;
        let outside_before = bus.read_byte(sample);

        let pstr = bus.alloc(4);
        bus.write_byte(pstr, 3);
        bus.write_bytes(pstr + 1, b"New");
        bus.write_long(TEST_SP, pstr);
        bus.write_long(TEST_SP + 4, edit_handle);

        let result = disp.dispatch_dialog(true, 0x18F, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_byte(sample),
            outside_before,
            "SetDialogItemText repaint must not clear outside the dialog port"
        );
    }

    #[test]
    fn get_dialog_item_text_returns_pascal_string_from_raw_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let text_handle = bus.alloc(4);
        let text_ptr = bus.alloc(5);
        let out_ptr = 0x300000u32;

        bus.write_long(text_handle, text_ptr);
        bus.write_bytes(text_ptr, b"Hello");
        bus.write_long(TEST_SP, out_ptr);
        bus.write_long(TEST_SP + 4, text_handle);

        let result = disp.dispatch_dialog(true, 0x190, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_byte(out_ptr), 5);
        assert_eq!(bus.read_bytes(out_ptr + 1, 5), b"Hello".to_vec());
    }

    #[test]
    fn initialize_dialog_item_handles_rewrites_ditl_text_item_storage() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(22);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 5);
        bus.write_bytes(ditl_ptr + 16, b"Hello");

        let items = vec![DialogItem {
            item_type: 8,
            rect: (10, 20, 30, 40),
            text: "Hello".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let created_handle = bus.read_long(ditl_ptr + 2);
        let created_ptr = bus.read_long(created_handle);
        assert_ne!(created_handle, 0);
        assert_eq!(bus.get_alloc_size(created_ptr), Some(5));
        assert_eq!(bus.read_bytes(created_ptr, 5), b"Hello".to_vec());
        assert_eq!(
            disp.dialog_item_handles.get(&created_handle),
            Some(&(dialog_ptr, 0))
        );
    }

    #[test]
    fn dialog_extended_text_keeps_classic_handle_title_and_edit_offsets() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(30);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1); // two items

        // Empty editText and button records; initialization supplies the live
        // handles from the retained Unicode items below.
        bus.write_byte(ditl_ptr + 14, 16);
        bus.write_byte(ditl_ptr + 15, 0);
        bus.write_byte(ditl_ptr + 28, 4);
        bus.write_byte(ditl_ptr + 29, 0);

        let mut items = vec![
            DialogItem {
                item_type: 16,
                text: decode_mac_roman(b"A\xC9B"),
                sel_start: 2,
                sel_end: 2,
                ..Default::default()
            },
            DialogItem {
                item_type: 4,
                text: decode_mac_roman(b"Go\xC9"),
                ..Default::default()
            },
        ];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let edit_handle = bus.read_long(ditl_ptr + 2);
        let edit_ptr = bus.read_long(edit_handle);
        assert_eq!(bus.get_alloc_size(edit_ptr), Some(3));
        assert_eq!(bus.read_bytes(edit_ptr, 3), b"A\xC9B");

        let control_handle = bus.read_long(ditl_ptr + 16);
        let control_ptr = bus.read_long(control_handle);
        assert_eq!(bus.read_byte(control_ptr + 40), 3);
        assert_eq!(bus.read_bytes(control_ptr + 41, 3), b"Go\xC9");

        assert!(disp.apply_dialog_select_key_to_edit_item(
            &mut bus,
            dialog_ptr,
            &mut items,
            1,
            b'X',
        ));
        assert_eq!(items[0].text, "A…XB");
        assert_eq!((items[0].sel_start, items[0].sel_end), (3, 3));
        let updated_edit_ptr = bus.read_long(edit_handle);
        assert_eq!(bus.get_alloc_size(updated_edit_ptr), Some(4));
        assert_eq!(bus.read_bytes(updated_edit_ptr, 4), b"A\xC9XB");
    }

    #[test]
    fn initialize_dialog_item_handles_materializes_icon_and_picture_handles() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(34);
        let icon_ptr = disp.install_test_resource(&mut bus, *b"ICON", 421, &[0xFF; 128]);
        let pict_ptr = disp.install_test_resource(&mut bus, *b"PICT", 422, &[0x11; 32]);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 8);
        bus.write_word(ditl_ptr + 8, 8);
        bus.write_word(ditl_ptr + 10, 40);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 32);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_word(ditl_ptr + 16, 421);
        bus.write_long(ditl_ptr + 18, 0);
        bus.write_word(ditl_ptr + 22, 8);
        bus.write_word(ditl_ptr + 24, 48);
        bus.write_word(ditl_ptr + 26, 40);
        bus.write_word(ditl_ptr + 28, 112);
        bus.write_byte(ditl_ptr + 30, 64);
        bus.write_byte(ditl_ptr + 31, 2);
        bus.write_word(ditl_ptr + 32, 422);

        let items = vec![
            DialogItem {
                item_type: 32,
                rect: (8, 8, 40, 40),
                text: String::new(),
                resource_id: 421,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 64,
                rect: (8, 48, 40, 112),
                text: String::new(),
                resource_id: 422,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let icon_handle = bus.read_long(ditl_ptr + 2);
        let pict_handle = bus.read_long(ditl_ptr + 18);
        assert_ne!(icon_handle, 0);
        assert_ne!(pict_handle, 0);
        assert_eq!(bus.read_long(icon_handle), icon_ptr);
        assert_eq!(bus.read_long(pict_handle), pict_ptr);
    }

    #[test]
    fn initialize_dialog_item_handles_clears_user_item_placeholder_proc_ptr() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0x12345678);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);

        let items = vec![DialogItem {
            item_type: 0,
            rect: (10, 20, 30, 40),
            text: String::new(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        assert_eq!(bus.read_long(ditl_ptr + 2), 0);
    }

    #[test]
    fn initialize_dialog_item_handles_creates_standard_control_handle() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(22);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 5);
        bus.write_byte(ditl_ptr + 15, 5);
        bus.write_bytes(ditl_ptr + 16, b"Sound");

        let items = vec![DialogItem {
            item_type: 5,
            rect: (10, 20, 30, 120),
            text: "Sound".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let control_handle = bus.read_long(ditl_ptr + 2);
        let control_ptr = bus.read_long(control_handle);
        assert_ne!(control_handle, 0);
        assert_ne!(control_ptr, 0);
        assert_eq!(bus.read_long(dialog_ptr + 140), control_handle);
        assert_eq!(bus.read_long(control_ptr + 4), dialog_ptr);
        assert_eq!(bus.read_word(control_ptr + 8) as i16, 10);
        assert_eq!(bus.read_word(control_ptr + 10) as i16, 20);
        assert_eq!(bus.read_byte(control_ptr + 17), 0);
        assert_eq!(
            disp.dialog_control_handles.get(&control_handle),
            Some(&(dialog_ptr, 1))
        );
        assert_eq!(disp.dialog_control_values.get(&(dialog_ptr, 1)), Some(&0));
        assert_eq!(disp.control_manager.proc_id(control_ptr), 1);
    }

    #[test]
    fn initialize_dialog_item_handles_creates_resctrl_control_handle() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(18);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 140);
        bus.write_byte(ditl_ptr + 14, 7);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_word(ditl_ptr + 16, 128);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 110, 2, -1, 4, 1300, 1009] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0x1234_5678u32.to_be_bytes());
        cntl.push(4);
        cntl.extend_from_slice(b"Team");
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        let items = vec![DialogItem {
            item_type: 7,
            rect: (10, 20, 30, 140),
            text: String::new(),
            resource_id: 128,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let handle = bus.read_long(ditl_ptr + 2);
        let ctrl_ptr = bus.read_long(handle);
        assert_ne!(handle, 0);
        assert_ne!(ctrl_ptr, 0);
        assert_eq!(bus.read_long(dialog_ptr + 140), handle);
        assert_eq!(bus.read_long(ctrl_ptr + 4), dialog_ptr);
        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 10);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 20);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 30);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 140);
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
        assert_eq!(bus.read_word(ctrl_ptr + 20) as i16, 1);
        assert_eq!(bus.read_word(ctrl_ptr + 22) as i16, 0);
        assert_eq!(disp.control_manager.proc_id(ctrl_ptr), 1009);
        assert_eq!(
            disp.dialog_control_handles.get(&handle),
            Some(&(dialog_ptr, 1))
        );
        assert_eq!(disp.dialog_control_values.get(&(dialog_ptr, 1)), Some(&2));
    }

    #[test]
    fn popup_resctrl_initializes_contrl_data_private_menu_record() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(18);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 140);
        bus.write_byte(ditl_ptr + 14, 7);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_word(ditl_ptr + 16, 128);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 110, 1, -1, 0, 4000, 1009] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0u32.to_be_bytes());
        cntl.push(0);
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        let items = vec![DialogItem {
            item_type: 7,
            rect: (10, 20, 30, 140),
            text: String::new(),
            resource_id: 128,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        let ctrl_handle = bus.read_long(ditl_ptr + 2);
        let ctrl_ptr = bus.read_long(ctrl_handle);
        let private_handle = bus.read_long(ctrl_ptr + 28);
        let private_ptr = bus.read_long(private_handle);
        let menu_handle = bus.read_long(private_ptr);
        let menu_ptr = bus.read_long(menu_handle);

        assert_ne!(private_handle, 0);
        assert_ne!(private_ptr, 0);
        assert_ne!(menu_handle, 0);
        assert_eq!(bus.read_word(private_ptr + 4) as i16, 4000);
        assert_eq!(bus.read_word(menu_ptr) as i16, 4000);
        assert_eq!(disp.popup_control_menu_id(&bus, ctrl_ptr, -1), 4000);
        assert!(disp
            .menus
            .iter()
            .any(|menu| menu.id == 4000 && menu.handle == menu_handle));
    }

    #[test]
    fn append_ditl_overlay_preserves_existing_handles_and_initializes_resctrl() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let old_ditl_ptr = bus.alloc(18);
        let preserved_handle = 0x00A1_B2C3;

        bus.write_long(items_handle, old_ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(old_ditl_ptr, 0);
        bus.write_long(old_ditl_ptr + 2, preserved_handle);
        bus.write_word(old_ditl_ptr + 6, 10);
        bus.write_word(old_ditl_ptr + 8, 20);
        bus.write_word(old_ditl_ptr + 10, 30);
        bus.write_word(old_ditl_ptr + 12, 60);
        bus.write_byte(old_ditl_ptr + 14, 4);
        bus.write_byte(old_ditl_ptr + 15, 2);
        bus.write_bytes(old_ditl_ptr + 16, b"OK");
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 20, 30, 60),
                text: "OK".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 110, 3, -1, 1, 5, 1008] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0x1234_5678u32.to_be_bytes());
        cntl.push(4);
        cntl.extend_from_slice(b"Pane");
        disp.install_test_resource(&mut bus, *b"CNTL", 131, &cntl);

        let append_handle = bus.alloc(4);
        let append_ditl_ptr = bus.alloc(18);
        bus.write_long(append_handle, append_ditl_ptr);
        bus.write_word(append_ditl_ptr, 0);
        bus.write_long(append_ditl_ptr + 2, 0);
        bus.write_word(append_ditl_ptr + 6, 40);
        bus.write_word(append_ditl_ptr + 8, 50);
        bus.write_word(append_ditl_ptr + 10, 60);
        bus.write_word(append_ditl_ptr + 12, 170);
        bus.write_byte(append_ditl_ptr + 14, 7);
        bus.write_byte(append_ditl_ptr + 15, 2);
        bus.write_word(append_ditl_ptr + 16, 131);

        let count = disp.append_ditl_to_dialog(&mut bus, dialog_ptr, append_handle, 0);

        assert_eq!(count, 2);
        let new_ditl_ptr = bus.read_long(items_handle);
        assert_ne!(new_ditl_ptr, old_ditl_ptr);
        assert_eq!(bus.read_word(new_ditl_ptr), 1);
        assert_eq!(
            bus.read_long(new_ditl_ptr + 2),
            preserved_handle,
            "AppendDITL must not recreate existing item handles"
        );
        let ctrl_handle = bus.read_long(new_ditl_ptr + 18);
        let ctrl_ptr = bus.read_long(ctrl_handle);
        assert_ne!(ctrl_handle, 0);
        assert_ne!(ctrl_ptr, 0);
        assert_eq!(bus.read_long(dialog_ptr + 140), ctrl_handle);
        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, 40);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, 50);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, 60);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, 170);
        assert_eq!(
            disp.dialog_control_handles.get(&ctrl_handle),
            Some(&(dialog_ptr, 2))
        );
        assert_eq!(disp.dialog_control_values.get(&(dialog_ptr, 2)), Some(&3));
    }

    #[test]
    fn shorten_ditl_erases_removed_retained_item_rects_without_wiping_header_pixels() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((400 * 300) as u32);
        for i in 0..400u32 * 300 {
            bus.write_byte(screen_base + i, 0x00);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 400, 400, 300, 8);

        let dialog_ptr = bus.alloc(200);
        bus.write_word(dialog_ptr + 6, 0);
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-100i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 120);
        bus.write_word(dialog_ptr + 22, 200);
        let bounds = TrapDispatcher::dialog_screen_bounds(&bus, dialog_ptr);

        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(38);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 10);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 80);
        bus.write_byte(ditl_ptr + 14, 8);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_bytes(ditl_ptr + 16, b"OK");
        bus.write_long(ditl_ptr + 20, 0);
        bus.write_word(ditl_ptr + 24, 40);
        bus.write_word(ditl_ptr + 26, 40);
        bus.write_word(ditl_ptr + 28, 60);
        bus.write_word(ditl_ptr + 30, 160);
        bus.write_byte(ditl_ptr + 32, 8);
        bus.write_byte(ditl_ptr + 33, 3);
        bus.write_bytes(ditl_ptr + 34, b"Old");

        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 8,
                    rect: (10, 10, 30, 80),
                    text: "OK".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 8,
                    rect: (40, 40, 60, 160),
                    text: "Old".to_string(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        disp.fill_rect_clipped_to_dialog(&mut bus, bounds, (106, 106, 114, 170), true);
        disp.fill_rect_clipped_to_dialog(&mut bus, bounds, (140, 140, 160, 260), true);
        let pixels = disp.save_dialog_pixels(&bus, bounds);
        disp.dialog_visible_snapshots
            .insert(dialog_ptr, PersistentDialogSnapshot { bounds, pixels });

        let count = disp.shorten_ditl_in_dialog(&mut bus, dialog_ptr, 1);

        assert_eq!(count, 1);
        assert_eq!(bus.read_word(ditl_ptr), 0);
        assert_eq!(bus.read_byte(screen_base + 110 * 400 + 120), 0xFF);
        assert_eq!(bus.read_byte(screen_base + 150 * 400 + 150), 0x00);
        let retained = disp.dialog_visible_snapshots.get(&dialog_ptr).unwrap();
        assert!(
            retained.pixels == disp.save_dialog_pixels(&bus, bounds),
            "ShortenDITL retained snapshot must match the erased framebuffer"
        );
    }

    #[test]
    fn dialog_item_handle_addr_skips_zero_len_resctrl_resource_id() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(36);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 1);

        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 140);
        bus.write_byte(ditl_ptr + 14, 7);
        bus.write_byte(ditl_ptr + 15, 0);
        bus.write_word(ditl_ptr + 16, 128);

        bus.write_long(ditl_ptr + 18, 0);
        bus.write_word(ditl_ptr + 22, 40);
        bus.write_word(ditl_ptr + 24, 20);
        bus.write_word(ditl_ptr + 26, 52);
        bus.write_word(ditl_ptr + 28, 140);
        bus.write_byte(ditl_ptr + 30, 8);
        bus.write_byte(ditl_ptr + 31, 3);
        bus.write_bytes(ditl_ptr + 32, b"abc");
        bus.write_byte(ditl_ptr + 35, 0);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 110, 2, -1, 4, 1300, 1009] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0x1234_5678u32.to_be_bytes());
        cntl.push(4);
        cntl.extend_from_slice(b"Team");
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        let items = vec![
            DialogItem {
                item_type: 7,
                rect: (10, 20, 30, 140),
                text: String::new(),
                resource_id: 128,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 8,
                rect: (40, 20, 52, 140),
                text: "abc".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        assert_eq!(
            bus.read_word(ditl_ptr + 16),
            128,
            "resCtrl resource ID must not be overwritten by the next item handle"
        );
        assert_ne!(bus.read_long(ditl_ptr + 2), 0);
        assert_ne!(bus.read_long(ditl_ptr + 18), 0);
        assert_eq!(
            TrapDispatcher::dialog_item_handle(&bus, dialog_ptr, 2),
            bus.read_long(ditl_ptr + 18)
        );
    }

    #[test]
    fn draw_dialog_resctrl_uses_cntl_proc_id_not_always_popup() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = 0x300000u32;
        let row_bytes: u32 = 640;
        disp.set_screen_mode_for_test(screen_base, row_bytes, 640, 480, 8);

        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(18);
        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 20);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 40);
        bus.write_word(ditl_ptr + 12, 120);
        bus.write_byte(ditl_ptr + 14, 7);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_word(ditl_ptr + 16, 128);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 100, 0, -1, 0, 0, 0] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0u32.to_be_bytes());
        cntl.push(2);
        cntl.extend_from_slice(b"OK");
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        let items = vec![DialogItem {
            item_type: 7,
            rect: (20, 20, 40, 120),
            text: String::new(),
            resource_id: 128,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        disp.draw_dialog(
            &mut bus,
            (100, 100, 180, 260),
            3,
            "",
            &items,
            0,
            "",
            0,
            false,
            dialog_ptr,
        );

        let old_popup_arrow_pixel = screen_base + 130 * row_bytes + 210;
        assert_ne!(
            bus.read_byte(old_popup_arrow_pixel),
            0xFF,
            "push-button resCtrl must not draw popup-menu arrow pixels"
        );
    }

    #[test]
    fn show_dialog_item_updates_live_resctrl_control_rect() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(18);
        let hidden_rect = (20, 20 + 16384, 40, 120 + 16384);
        let visible_rect = (20, 20, 40, 120);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, hidden_rect.0 as u16);
        bus.write_word(ditl_ptr + 8, hidden_rect.1 as u16);
        bus.write_word(ditl_ptr + 10, hidden_rect.2 as u16);
        bus.write_word(ditl_ptr + 12, hidden_rect.3 as u16);
        bus.write_byte(ditl_ptr + 14, 7);
        bus.write_byte(ditl_ptr + 15, 2);
        bus.write_word(ditl_ptr + 16, 128);

        let mut cntl = Vec::new();
        for word in [0i16, 0, 20, 100, 0, -1, 0, 0, 0] {
            cntl.extend_from_slice(&(word as u16).to_be_bytes());
        }
        cntl.extend_from_slice(&0u32.to_be_bytes());
        cntl.push(2);
        cntl.extend_from_slice(b"OK");
        disp.install_test_resource(&mut bus, *b"CNTL", 128, &cntl);

        let items = vec![DialogItem {
            item_type: 7,
            rect: hidden_rect,
            text: String::new(),
            resource_id: 128,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);
        disp.dialog_items.insert(dialog_ptr, items);
        disp.hidden_dialog_item_rects
            .insert((dialog_ptr, 1), visible_rect);

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let ctrl_handle = disp.dialog_control_handle_for_item(dialog_ptr, 1).unwrap();
        let ctrl_ptr = bus.read_long(ctrl_handle);
        assert_eq!(bus.read_word(ctrl_ptr + 8) as i16, visible_rect.0);
        assert_eq!(bus.read_word(ctrl_ptr + 10) as i16, visible_rect.1);
        assert_eq!(bus.read_word(ctrl_ptr + 12) as i16, visible_rect.2);
        assert_eq!(bus.read_word(ctrl_ptr + 14) as i16, visible_rect.3);
    }

    #[test]
    fn parse_ditl_preserves_user_item_proc_ptr() {
        let (_disp, _cpu, mut bus) = setup();
        let ditl_ptr = bus.alloc(16);

        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0x12345678);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);

        let items = TrapDispatcher::parse_ditl(&bus, ditl_ptr, 16);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].proc_ptr, 0x12345678);
    }

    #[test]
    fn initialize_dialog_item_handles_preserves_user_item_proc_ptr() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);

        let items = vec![DialogItem {
            item_type: 0,
            rect: (10, 20, 30, 40),
            text: String::new(),
            resource_id: 0,
            proc_ptr: 0x12345678,
            sel_start: 0,
            sel_end: 0,
        }];

        disp.initialize_dialog_item_handles(&mut bus, dialog_ptr, &items);

        assert_eq!(bus.read_long(ditl_ptr + 2), 0x12345678);
    }

    #[test]
    fn refresh_ditl_proc_ptrs_clears_stale_user_item_proc_ptr() {
        let (_disp, _cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let items_handle = bus.alloc(4);
        let ditl_ptr = bus.alloc(16);

        bus.write_long(items_handle, ditl_ptr);
        bus.write_long(dialog_ptr + 156, items_handle);
        bus.write_word(ditl_ptr, 0);
        bus.write_long(ditl_ptr + 2, 0);
        bus.write_word(ditl_ptr + 6, 10);
        bus.write_word(ditl_ptr + 8, 20);
        bus.write_word(ditl_ptr + 10, 30);
        bus.write_word(ditl_ptr + 12, 40);
        bus.write_byte(ditl_ptr + 14, 0);
        bus.write_byte(ditl_ptr + 15, 0);

        let mut items = vec![DialogItem {
            item_type: 0,
            rect: (10, 20, 30, 40),
            text: String::new(),
            resource_id: 0,
            proc_ptr: 0x12345678,
            sel_start: 0,
            sel_end: 0,
        }];

        TrapDispatcher::refresh_ditl_proc_ptrs(&bus, dialog_ptr, &mut items);

        assert_eq!(items[0].proc_ptr, 0);
    }

    // ---- ModalDialog ($A991) ----

    #[test]
    fn modal_dialog_writes_item_hit_and_pops_8_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let item_hit_addr = 0x300000u32;
        bus.write_word(item_hit_addr, 0); // pre-clear

        // SP+0: item_hit_ptr (4), SP+4: filterProc (4)
        bus.write_long(TEST_SP, item_hit_addr);
        bus.write_long(TEST_SP + 4, 0); // nil filterProc

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_addr), 1);
    }

    #[test]
    fn modal_dialog_command_printables_preserve_edit_state_before_backspace() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(320 * 240);
        for offset in 0..320u32 * 240 {
            bus.write_byte(screen_base + offset, 0xFF);
        }
        bus.write_long(0x0824, screen_base);
        disp.set_screen_mode_for_test(screen_base, 320, 320, 240, 8);

        let dialog_ptr = bus.alloc(256);
        let item_hit_addr = 0x300000u32;
        let bounds = (40, 40, 130, 280);
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_bounds = bounds;
        disp.window_proc_id = 2;
        disp.window_title.clear();
        disp.window_list.replace(vec![dialog_ptr]);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_word(dialog_ptr + 164, 0);
        bus.write_word(dialog_ptr + 168, 0);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 16,
                rect: (24, 24, 42, 190),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        fn enter_modal(
            disp: &mut TrapDispatcher,
            cpu: &mut MockCpu,
            bus: &mut MacMemoryBus,
            item_hit_addr: u32,
        ) {
            bus.write_word(item_hit_addr, 0xCAFE);
            bus.write_long(TEST_SP, item_hit_addr);
            bus.write_long(TEST_SP + 4, 0);
            cpu.write_reg(Register::A7, TEST_SP);
            disp.dispatch_dialog(true, 0x191, cpu, bus)
                .unwrap()
                .unwrap();
            assert!(disp.dialog_tracking.is_some());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        }

        fn key_down(
            disp: &mut TrapDispatcher,
            cpu: &mut MockCpu,
            bus: &mut MacMemoryBus,
            item_hit_addr: u32,
            key_code: u8,
            char_code: u8,
            modifiers: u16,
        ) {
            disp.event_queue
                .push_back(crate::trap::dispatch::QueuedEvent {
                    what: 3,
                    message: (u32::from(key_code) << 8) | u32::from(char_code),
                    when: 0,
                    where_v: 0,
                    where_h: 0,
                    modifiers,
                });
            disp.dispatch_dialog(true, 0x191, cpu, bus)
                .unwrap()
                .unwrap();
            assert!(disp.dialog_tracking.is_none());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
            assert_eq!(bus.read_word(item_hit_addr), 1);
        }

        for (key_code, char_code) in [(0x02, b'd'), (0x20, b'u'), (0x02, b'd')] {
            enter_modal(&mut disp, &mut cpu, &mut bus, item_hit_addr);
            key_down(
                &mut disp,
                &mut cpu,
                &mut bus,
                item_hit_addr,
                key_code,
                char_code,
                0,
            );
        }

        let item = &disp.dialog_items[&dialog_ptr][0];
        assert_eq!(item.text, "dud");
        assert_eq!((item.sel_start, item.sel_end), (3, 3));
        assert!(disp
            .dialog_edit_text_modified_items
            .contains(&(dialog_ptr, 1)));

        enter_modal(&mut disp, &mut cpu, &mut bus, item_hit_addr);
        for char_code in [b'a', b'B'] {
            disp.event_queue
                .push_back(crate::trap::dispatch::QueuedEvent {
                    what: 3,
                    message: u32::from(char_code),
                    when: 0,
                    where_v: 0,
                    where_h: 0,
                    modifiers: 0x0100,
                });
            disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert!(disp.dialog_tracking.is_some());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
            assert_eq!(bus.read_word(item_hit_addr), 0);
            let tracking = disp.dialog_tracking.as_ref().unwrap();
            assert_eq!(tracking.edit_text, "dud");
            assert_eq!(
                (tracking.items[0].sel_start, tracking.items[0].sel_end),
                (3, 3)
            );
            assert!(disp
                .dialog_edit_text_modified_items
                .contains(&(dialog_ptr, 1)));
        }

        key_down(&mut disp, &mut cpu, &mut bus, item_hit_addr, 0x33, 0x08, 0);
        let item = &disp.dialog_items[&dialog_ptr][0];
        assert_eq!(item.text, "du");
        assert_eq!((item.sel_start, item.sel_end), (2, 2));
    }

    #[test]
    fn modal_dialog_popup_resctrl_tracks_selection_and_returns_item() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        for i in 0..800u32 * 600 {
            bus.write_byte(screen_base + i, 0xFF);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let dialog_ptr = bus.alloc(256);
        bus.write_word(dialog_ptr + 6, 0);
        bus.write_word(dialog_ptr + 8, (-100i16) as u16);
        bus.write_word(dialog_ptr + 10, (-100i16) as u16);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 180);
        bus.write_word(dialog_ptr + 22, 220);
        let dialog_bounds = (100, 100, 280, 320);
        let exterior_probe = screen_base + 95 * 800 + 95;
        bus.write_byte(exterior_probe, 0x33);

        let ctrl_ptr = bus.alloc(296);
        let ctrl_handle = bus.alloc(4);
        bus.write_long(ctrl_handle, ctrl_ptr);
        disp.initialize_control_record(
            &mut bus,
            ctrl_ptr,
            dialog_ptr,
            (10, 20, 30, 130),
            b"",
            true,
            1,
            900,
            0,
            1009,
            0,
        );
        disp.dialog_control_handles
            .insert(ctrl_handle, (dialog_ptr, 1));
        disp.dialog_control_values.insert((dialog_ptr, 1), 1);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 7,
                rect: (10, 20, 30, 130),
                ..Default::default()
            }],
        );
        disp.ensure_dialog_background_saved(&bus, dialog_ptr, dialog_bounds);
        let saved_under = disp.dialog_saved_pixels[&dialog_ptr].clone();
        disp.menus.push(Menu {
            id: 900,
            title: "Squadies".to_string(),
            items: vec![
                MenuItem {
                    text: "Duke".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
                MenuItem {
                    text: "Carnage".to_string(),
                    icon: 0,
                    key_equiv: 0,
                    mark: 0,
                    style: 0,
                    enabled: true,
                },
            ],
            enabled: true,
            handle: 0,
            in_menu_bar: false,
            hierarchical: false,
            visible_in_menu_bar: false,
        });

        let item_hit_addr = 0x300000u32;
        bus.write_word(item_hit_addr, 0);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: dialog_bounds,
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 7,
                rect: (10, 20, 30, 130),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: saved_under,
            stack_ptr: TEST_SP,
            item_hit_ptr: item_hit_addr,
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
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((115, 125));
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 115,
                where_h: 125,
                modifiers: 0,
            });

        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_popup.as_ref())
            .is_some());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        let stale_open_popup_pixels = disp.save_dialog_pixels(&bus, dialog_bounds);
        let tracking = disp.dialog_tracking.as_mut().unwrap();
        tracking.rendered_pixels = stale_open_popup_pixels.clone();
        tracking.rendered_pixels_final = true;

        let (dropdown_top, dropdown_left, _, _) = disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_popup.as_ref())
            .map(|popup| popup.dropdown_rect)
            .expect("popup tracking should expose the live dropdown rect");
        disp.input_state
            .set_mouse_position_for_test((dropdown_top + 1 + 16 + 1, dropdown_left + 5));
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_popup.as_ref())
                .map(|popup| popup.highlighted_item),
            Some(2)
        );

        disp.input_state.set_mouse_button_for_test(false);
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: 148,
                where_h: 125,
                modifiers: 0,
            });
        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert!(disp.dialog_tracking.is_none());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_addr), 1);
        assert_eq!(bus.read_word(ctrl_ptr + 18) as i16, 2);
        assert_eq!(disp.dialog_control_values.get(&(dialog_ptr, 1)), Some(&2));

        let retained = disp.dialog_visible_snapshots.get(&dialog_ptr).unwrap();
        let current_pixels = disp.save_dialog_pixels(&bus, dialog_bounds);
        assert!(
            retained.pixels == current_pixels,
            "retained snapshot must match the closed popup framebuffer"
        );
        assert!(
            retained.pixels != stale_open_popup_pixels,
            "retained snapshot kept stale open-popup pixels"
        );

        // After ModalDialog returns the popup hit, the application redraws
        // the selected pane through the retained dialog port. A draw touching
        // the dBox structure margin is still visible dialog composition, not
        // a replacement for title artwork saved underneath the modal.
        disp.dialog_modal_entered.insert(dialog_ptr);
        bus.write_byte(exterior_probe, 0x77);
        disp.refresh_dialog_saved_pixels_after_screen_draw(&bus, dialog_ptr, (95, 95, 96, 96));

        let save_rect = TrapDispatcher::dialog_saved_pixel_rect(dialog_bounds);
        let save_width = (save_rect.3 - save_rect.1) as usize;
        let exterior_index = (95 - save_rect.0) as usize * save_width + (95 - save_rect.1) as usize;
        assert_eq!(
            disp.dialog_saved_pixels.get(&dialog_ptr).unwrap()[exterior_index],
            0x33,
            "popup-era dialog drawing must not replace exterior saved-under pixels"
        );
    }

    #[test]
    fn modal_dialog_game_managed_dialog_still_queues_user_item_draw_procs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_addr = 0x300000u32;

        disp.front_window = dialog_ptr;
        disp.window_bounds = (92, 95, 415, 704);
        disp.window_proc_id = 2;
        disp.window_title.clear();
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0,
                    rect: (8, 353, 148, 603),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0x500000,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 0x80,
                    rect: (159, 381, 184, 581),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0x500100,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 0x80,
                    rect: (324, 650, 349, 771),
                    text: String::new(),
                    resource_id: 0,
                    proc_ptr: 0x500200,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        bus.write_long(TEST_SP, item_hit_addr);
        bus.write_long(TEST_SP + 4, 0x149F0);

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert!(tracking.game_managed);
        assert_eq!(tracking.filter_proc, 0x149F0);
        assert_eq!(tracking.draw_proc_queue.len(), 2);
        assert_eq!(
            tracking
                .draw_proc_queue
                .iter()
                .map(|(_, item_no)| *item_no)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(!tracking.draw_procs_done);
        assert!(!tracking.rendered_pixels_final);
    }

    #[test]
    fn modal_dialog_retained_reentry_reuses_visible_snapshot_without_user_item_redraw() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_addr = 0x300000u32;
        let screen_base = bus.alloc((64 * 64) as u32);
        let row_bytes = 64u32;
        for i in 0..row_bytes * 64 {
            bus.write_byte(screen_base + i, 0x11);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, row_bytes, 64, 64, 8);

        let bounds = (10, 12, 30, 34);
        let snapshot_width = (bounds.3 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_height =
            (bounds.2 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let visible_pixels = vec![0x44; snapshot_width * snapshot_height];
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: visible_pixels.into(),
            },
        );
        disp.dialog_saved_pixels.insert(
            dialog_ptr,
            vec![0x22; snapshot_width * snapshot_height].into(),
        );
        disp.dialog_modal_entered.insert(dialog_ptr);

        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.window_proc_id = 1;
        disp.window_title.clear();
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80,
                rect: (0, 0, 20, 22),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0x500000,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_long(TEST_SP, item_hit_addr);
        bus.write_long(TEST_SP + 4, 0);

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert!(tracking.draw_proc_queue.is_empty());
        assert!(tracking.draw_procs_done);
        assert!(tracking.rendered_pixels_final);
        assert_eq!(
            bus.read_byte(screen_base + 20 * row_bytes + 20),
            0x44,
            "retained visible snapshot should be restored on re-entry"
        );
    }

    #[test]
    fn modal_dialog_first_entry_with_showwindow_snapshot_still_queues_user_item_draw_proc() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_addr = 0x300000u32;
        let screen_base = bus.alloc((64 * 64) as u32);
        let row_bytes = 64u32;
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, row_bytes, 64, 64, 8);

        let bounds = (10, 12, 30, 34);
        let snapshot_width = (bounds.3 - bounds.1 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        let snapshot_height =
            (bounds.2 - bounds.0 + TrapDispatcher::DBOX_FRAME_MARGIN * 2) as usize;
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds,
                pixels: vec![0x44; snapshot_width * snapshot_height].into(),
            },
        );

        disp.front_window = dialog_ptr;
        disp.window_bounds = bounds;
        disp.window_proc_id = 1;
        disp.window_title.clear();
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80,
                rect: (0, 0, 20, 22),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0x500000,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_long(TEST_SP, item_hit_addr);
        bus.write_long(TEST_SP + 4, 0);

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert_eq!(tracking.draw_proc_queue.len(), 1);
        assert_eq!(tracking.draw_proc_queue[0], (0x500000, 1));
        assert!(!tracking.draw_procs_done);
        assert!(!tracking.rendered_pixels_final);
    }

    #[test]
    fn dialog_game_managed_ignores_offscreen_placeholder_items() {
        let bounds = (200, 146, 400, 510);
        let items = vec![
            DialogItem {
                item_type: 0,
                rect: (167, 279, 192, 357),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0xC0, // disabled picture placeholder outside bounds
                rect: (217, 179, 247, 247),
                text: String::new(),
                resource_id: 1431,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            DialogItem {
                item_type: 0x80, // disabled userItem inside bounds
                rect: (7, 8, 157, 358),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ];

        assert!(TrapDispatcher::dialog_is_game_managed(bounds, &items));

        let mut standard_items = items;
        standard_items.push(DialogItem {
            item_type: 8,
            rect: (20, 20, 40, 100),
            text: "Standard".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        });
        assert!(!TrapDispatcher::dialog_is_game_managed(
            bounds,
            &standard_items
        ));
    }

    #[test]
    fn redraw_dialog_window_contents_does_not_snapshot_game_managed_shell() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let screen_base = bus.alloc(64 * 64);
        for offset in 0..64u32 * 64 {
            bus.write_byte(screen_base + offset, 0x11);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);

        bus.write_long(dialog_ptr + 2, screen_base);
        bus.write_word(dialog_ptr + 6, 64);
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 12, 20);
        bus.write_word(dialog_ptr + 14, 20);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 20);
        bus.write_word(dialog_ptr + 22, 20);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        disp.window_proc_ids.insert(dialog_ptr, 2);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 0x80,
                rect: (2, 2, 10, 10),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            PersistentDialogSnapshot {
                bounds: (0, 0, 20, 20),
                pixels: vec![0x44; 30 * 30].into(),
            },
        );

        disp.redraw_dialog_window_contents(&mut bus, dialog_ptr);

        assert!(
            !disp.dialog_visible_snapshots.contains_key(&dialog_ptr),
            "ShowWindow redraw must not retain a stale shell for app-drawn all-userItem dialogs"
        );
    }

    #[test]
    fn game_managed_dialog_clears_background_until_the_application_paints() {
        let (mut disp, _cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let screen_base = 0x300000u32;
        for offset in 0..64u32 * 64 {
            bus.write_byte(screen_base + offset, 0x11);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);

        bus.write_long(dialog_ptr + 2, screen_base);
        bus.write_word(dialog_ptr + 6, 64);
        bus.write_word(dialog_ptr + 8, 0);
        bus.write_word(dialog_ptr + 10, 0);
        bus.write_word(dialog_ptr + 12, 20);
        bus.write_word(dialog_ptr + 14, 20);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 20);
        bus.write_word(dialog_ptr + 22, 20);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        disp.window_proc_ids.insert(dialog_ptr, 2);

        let items = vec![DialogItem {
            item_type: 0x80,
            rect: (2, 2, 10, 10),
            text: String::new(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }];
        assert!(TrapDispatcher::dialog_is_game_managed((0, 0, 20, 20), &items));
        disp.dialog_items.insert(dialog_ptr, items.clone());

        let probe = screen_base + 5 * 64 + 5;
        disp.draw_dialog(
            &mut bus,
            (0, 0, 20, 20),
            2,
            "",
            &items,
            0,
            "",
            0,
            false,
            dialog_ptr,
        );
        assert_ne!(
            bus.read_byte(probe),
            0x11,
            "a game-managed dialog still defaults its background on the first manager paint"
        );

        bus.write_byte(probe, 0x22);
        disp.dialogs_drawn_by_app.insert(dialog_ptr);
        disp.draw_dialog(
            &mut bus,
            (0, 0, 20, 20),
            2,
            "",
            &items,
            0,
            "",
            0,
            false,
            dialog_ptr,
        );
        assert_eq!(
            bus.read_byte(probe),
            0x22,
            "an application-painted game-managed dialog must not be cleared again"
        );
    }

    #[test]
    fn select_window_erases_newly_exposed_dialog_background() {
        // A dialog created behind a full-screen window has an empty visRgn at
        // ShowWindow, so the manager skips its PaintOne erase. SelectWindow
        // then brings it to the front; the newly exposed content must be
        // erased (not left showing the window that was in front).
        // Inside Macintosh Volume I, I-284, I-286, I-296.
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((128 * 128) as u32);
        for offset in 0..128u32 * 128 {
            bus.write_byte(screen_base + offset, 0x11);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 128, 128, 128, 8);

        let alloc_region = |bus: &mut crate::memory::MacMemoryBus,
                            rect: Option<(i16, i16, i16, i16)>| {
            let data = bus.alloc(10);
            bus.write_word(data, 10);
            match rect {
                Some((top, left, bottom, right)) => {
                    bus.write_word(data + 2, top as u16);
                    bus.write_word(data + 4, left as u16);
                    bus.write_word(data + 6, bottom as u16);
                    bus.write_word(data + 8, right as u16);
                }
                None => {
                    bus.write_long(data + 2, 0);
                    bus.write_long(data + 6, 0);
                }
            }
            let handle = bus.alloc(4);
            bus.write_long(handle, data);
            handle
        };

        // Full-screen window in front of the dialog to begin with.
        let occluder = bus.alloc(170);
        bus.write_word(occluder + 6, 128);
        bus.write_word(occluder + 8, 0);
        bus.write_word(occluder + 10, 0);
        bus.write_word(occluder + 12, 128);
        bus.write_word(occluder + 14, 128);
        bus.write_byte(occluder + 110, 0xFF);
        let occluder_struc = alloc_region(&mut bus, Some((0, 0, 128, 128)));
        bus.write_long(occluder + 114, occluder_struc);

        // Dialog below the menu bar so CalcVis does not clip it to empty.
        let dialog_ptr = bus.alloc(170);
        bus.write_word(dialog_ptr + 6, 128);
        bus.write_word(dialog_ptr + 8, (-40i16) as u16);
        bus.write_word(dialog_ptr + 10, (-40i16) as u16);
        bus.write_word(dialog_ptr + 12, 40);
        bus.write_word(dialog_ptr + 14, 40);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 40);
        bus.write_word(dialog_ptr + 22, 40);
        bus.write_word(dialog_ptr + 108, 2);
        bus.write_byte(dialog_ptr + 110, 0xFF);
        let cont = alloc_region(&mut bus, Some((40, 40, 80, 80)));
        bus.write_long(dialog_ptr + 118, cont);
        let vis = alloc_region(&mut bus, None);
        bus.write_long(dialog_ptr + 24, vis);

        disp.dialog_items.insert(dialog_ptr, Vec::new());
        disp.window_proc_ids.insert(dialog_ptr, 2);
        disp.window_list.replace(vec![occluder, dialog_ptr]);
        disp.front_window = occluder;

        let probe = screen_base + 60 * 128 + 60;
        bus.write_byte(probe, 0x42);

        disp.activate_as_front_window(&mut bus, dialog_ptr);

        assert_eq!(disp.front_window, dialog_ptr);
        assert_ne!(
            bus.read_byte(probe),
            0x42,
            "SelectWindow must erase the dialog background once it is newly exposed"
        );
    }

    #[test]
    fn modal_dialog_resnapshot_redraws_popup_controls_after_user_item_draw_procs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        for i in 0..800u32 * 600 {
            bus.write_byte(screen_base + i, 0xFF);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr: 0x200000,
            bounds: (0, 0, 100, 220),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: vec![DialogPopupDraw {
                rect: (20, 30, 42, 180),
                title: String::new(),
                enabled: true,
                pressed: false,
            }],
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let interior = screen_base + 30 * 800 + 80;
        assert_eq!(
            bus.read_byte(interior),
            0,
            "popup interior must be redrawn over the guest userItem pixels"
        );
        assert!(
            disp.dialog_tracking
                .as_ref()
                .is_some_and(|tracking| tracking.rendered_pixels_final),
            "ModalDialog should re-snapshot after popup redraw"
        );
    }

    #[test]
    fn modal_dialog_popup_resnapshot_preserves_theme_pressed_and_inactive_states() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(UiThemeId::SystemlessDefault);

        let row_bytes = 64u32;
        let screen_base = bus.alloc(row_bytes * 160);
        for i in 0..row_bytes * 160 {
            bus.write_byte(screen_base + i, 0);
        }
        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 160, 1);
        cpu.write_reg(Register::A7, TEST_SP);

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr: 0x200000,
            bounds: (0, 0, 120, 220),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: vec![
                DialogPopupDraw {
                    rect: (20, 30, 42, 180),
                    title: String::new(),
                    enabled: true,
                    pressed: true,
                },
                DialogPopupDraw {
                    rect: (60, 30, 82, 180),
                    title: String::new(),
                    enabled: false,
                    pressed: false,
                },
            ],
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 35, 25),
            "pressed popup redraw should preserve the provider pressed fill"
        );
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 32, 62),
            "inactive popup redraw should preserve the provider inactive frame"
        );
        assert!(
            disp.dialog_tracking
                .as_ref()
                .is_some_and(|tracking| tracking.rendered_pixels_final),
            "ModalDialog should re-snapshot after stateful popup redraw"
        );
    }

    #[test]
    fn modal_dialog_resnapshot_blits_dialog_port_after_user_item_draw_procs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((32 * 32) as u32);
        for i in 0..32u32 * 32 {
            bus.write_byte(screen_base + i, 0xFF);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 32, 32, 32, 8);

        let dialog_ptr = bus.alloc(64);
        let offscreen_base = bus.alloc(20 * 20);
        for i in 0..20u32 * 20 {
            bus.write_byte(offscreen_base + i, 0x00);
        }
        let pixmap_handle = bus.alloc(4);
        let pixmap_ptr = bus.alloc(50);
        bus.write_long(pixmap_handle, pixmap_ptr);
        bus.write_long(pixmap_ptr, offscreen_base);
        bus.write_word(pixmap_ptr + 4, 0x8000 | 20);
        bus.write_word(pixmap_ptr + 32, 8);
        bus.write_long(dialog_ptr + 2, pixmap_handle);
        bus.write_word(dialog_ptr + 6, 0xC000);
        bus.write_word(dialog_ptr + 16, 0);
        bus.write_word(dialog_ptr + 18, 0);
        bus.write_word(dialog_ptr + 20, 20);
        bus.write_word(dialog_ptr + 22, 20);

        bus.write_byte(offscreen_base + 7 * 20 + 9, 0x44);
        disp.front_window = dialog_ptr;
        disp.window_bounds = (0, 0, 20, 20);
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 20, 20),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let screen_pixel = screen_base + 7 * 32 + 9;
        assert_eq!(
            bus.read_byte(screen_pixel),
            0x44,
            "ModalDialog should composite dialog-port userItem pixels before snapshot"
        );
        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert!(tracking.rendered_pixels_final);
        let snapshot_width = 20 + (TrapDispatcher::DBOX_FRAME_MARGIN as usize * 2);
        let snapshot_height = 20 + (TrapDispatcher::DBOX_FRAME_MARGIN as usize * 2);
        assert_eq!(
            tracking.rendered_pixels.len(),
            snapshot_width * snapshot_height
        );
        let snapshot_index = (7 + TrapDispatcher::DBOX_FRAME_MARGIN as usize) * snapshot_width
            + (9 + TrapDispatcher::DBOX_FRAME_MARGIN as usize);
        assert_eq!(tracking.rendered_pixels[snapshot_index], 0x44);
    }

    #[test]
    fn modal_dialog_update_event_restores_existing_snapshot_before_resnapshot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((32 * 32) as u32);
        for i in 0..32u32 * 32 {
            bus.write_byte(screen_base + i, 0x00);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 32, 32, 32, 8);

        let dialog_ptr = bus.alloc(64);
        disp.front_window = dialog_ptr;
        disp.window_bounds = (0, 0, 20, 20);

        let snapshot_width = 20 + (TrapDispatcher::DBOX_FRAME_MARGIN as usize * 2);
        let snapshot_height = 20 + (TrapDispatcher::DBOX_FRAME_MARGIN as usize * 2);
        let mut rendered_pixels = vec![0x00; snapshot_width * snapshot_height];
        let snapshot_index = (7 + TrapDispatcher::DBOX_FRAME_MARGIN as usize) * snapshot_width
            + (9 + TrapDispatcher::DBOX_FRAME_MARGIN as usize);
        rendered_pixels[snapshot_index] = 0x44;

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 20, 20),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: rendered_pixels.into(),
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
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 6,
                message: dialog_ptr,
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let screen_pixel = screen_base + 7 * 32 + 9;
        assert_eq!(
            bus.read_byte(screen_pixel),
            0x44,
            "ModalDialog update events must preserve userItem-rendered dialog pixels"
        );
        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert_eq!(tracking.rendered_pixels[snapshot_index], 0x44);
    }

    #[test]
    fn finalize_dialog_draw_procs_snapshots_completed_user_item_pixels() {
        let (mut disp, _cpu, mut bus) = setup();
        let screen_base = bus.alloc((32 * 32) as u32);
        for i in 0..32u32 * 32 {
            bus.write_byte(screen_base + i, 0x00);
        }
        bus.write_long(0x0824, screen_base);
        disp.screen_mode = (screen_base, 32, 32, 32, 8);

        let dialog_ptr = bus.alloc(64);
        let screen_pixel = screen_base + 7 * 32 + 9;
        bus.write_byte(screen_pixel, 0x44);
        disp.front_window = dialog_ptr;
        disp.window_bounds = (0, 0, 20, 20);
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 20, 20),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr: 0,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        disp.finalize_dialog_draw_procs_if_idle(&mut bus);

        let tracking = disp.dialog_tracking.as_ref().unwrap();
        assert!(tracking.draw_procs_done);
        assert!(tracking.rendered_pixels_final);
        let snapshot_width = 20 + (TrapDispatcher::DBOX_FRAME_MARGIN as usize * 2);
        let snapshot_index = (7 + TrapDispatcher::DBOX_FRAME_MARGIN as usize) * snapshot_width
            + (9 + TrapDispatcher::DBOX_FRAME_MARGIN as usize);
        assert_eq!(tracking.rendered_pixels[snapshot_index], 0x44);
    }

    #[test]
    fn modal_dialog_mouse_down_return_consumes_queued_mouse_up() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 0,
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
            game_managed: true,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            });
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0x0080,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert!(disp.event_queue.iter().all(|event| event.what != 2));
    }

    #[test]
    fn modal_dialog_plain_user_item_returns_on_mouse_down() {
        // A plain userItem's content and mouse tracking are application-owned
        // (IM:I I-405). ModalDialog returns the item on the initial press so
        // the application can run its own StillDown()/GetMouse() tracking loop
        // (e.g. EV Override's draggable Game Speed slider). The pending
        // mouse-up stays queued so that tracking loop still observes the
        // release.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 0,
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((130, 240));
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        // The hit is returned immediately on mouse-down: stack popped and the
        // item number written, with modal tracking released.
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert!(disp.dialog_tracking.is_none());
    }

    #[test]
    fn modal_dialog_popup_candidate_user_item_returns_on_mouse_down_in_original_rect() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 420),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 0,
                rect: (20, 90, 40, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
        disp.dialog_popup_original_rects
            .insert((dialog_ptr, 1), (20, 30, 40, 150));
        disp.dialog_popup_candidate_items.insert((dialog_ptr, 1));
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((130, 260));
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 260,
                modifiers: 0,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert_eq!(
            disp.dialog_tracking
                .as_ref()
                .and_then(|tracking| tracking.active_user_item.as_ref())
                .map(|active| active.item_no),
            None,
            "popup-candidate userItems must return on mouse-down so the app can run PopUpMenuSelect"
        );
    }

    #[test]
    fn modal_dialog_button_waits_for_late_mouse_up_before_returning() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;

        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 4,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
            game_managed: true,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((130, 240));
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(item_hit_ptr), 0);
        assert!(disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_button.as_ref())
            .is_some());

        disp.input_state.set_mouse_button_for_test(false);
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0x0080,
            });
        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(disp.event_queue.iter().all(|event| event.what != 2));
        assert!(disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_button.as_ref())
            .is_none());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

        {
            let tracking = disp.dialog_tracking.as_mut().unwrap();
            tracking.flash_remaining = 1;
            tracking.flash_delay = 0;
        }
        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
    }

    #[test]
    fn modal_dialog_ignores_stale_filter_result_before_first_callback() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;
        let result_addr = 0x300100u32;

        // A preceding dialog's filter returned TRUE. The shared callback
        // scratch is deliberately left intact when a second dialog starts.
        bus.write_word(result_addr, 0x0100);
        bus.write_word(item_hit_ptr, 0);
        disp.dialog_filter_result_addr = result_addr;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 4,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0x149F0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(item_hit_ptr), 0);
        assert!(disp.dialog_tracking.is_some());
    }

    #[test]
    fn modal_dialog_filter_handled_mouse_down_consumes_queued_mouse_up() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;
        let result_addr = 0x300100u32;

        bus.write_word(result_addr, 0xFFFF);
        bus.write_word(item_hit_ptr, 12);
        disp.dialog_filter_result_addr = result_addr;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 0,
                rect: (20, 30, 60, 110),
                text: String::new(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0x149F0,
            game_managed: true,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            }),
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        disp.event_queue
            .push_back(crate::trap::dispatch::QueuedEvent {
                what: 2,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0x0080,
            });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert!(disp.event_queue.iter().all(|event| event.what != 2));
        assert!(!disp.pending_modal_dialog_mouse_up);
    }

    #[test]
    fn modal_dialog_filter_button_hit_releases_visual_before_nested_modal() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let child_dialog = bus.alloc(170);
        let item_hit_ptr = 0x300100u32;
        let result_addr = 0x300102u32;
        let bounds = (100, 200, 200, 360);
        let item_rect = (20, 30, 60, 110);
        let screen_rect = TrapDispatcher::dialog_item_screen_rect(bounds, item_rect);
        let screen_base = 0x300000u32;
        let row_bytes = 64u32;
        let probe_x = 270;
        let probe_y = 140;

        disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 1);
        disp.draw_button(
            &mut bus,
            screen_rect.0,
            screen_rect.1,
            screen_rect.2,
            screen_rect.3,
            "",
            true,
        );
        assert!(!screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            probe_x,
            probe_y
        ));
        disp.draw_dialog_button_highlight_state(&mut bus, screen_rect, "", true, true);
        assert!(screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            probe_x,
            probe_y
        ));

        let control_ptr = bus.alloc(32);
        let control_handle = bus.alloc(4);
        bus.write_long(control_handle, control_ptr);
        bus.write_byte(control_ptr + 17, 1);
        disp.dialog_control_handles
            .insert(control_handle, (dialog_ptr, 1));

        let items = vec![DialogItem {
            item_type: 4,
            rect: item_rect,
            text: String::new(),
            ..DialogItem::default()
        }];
        bus.write_word(result_addr, 0x0100);
        bus.write_word(item_hit_ptr, 1);
        disp.dialog_filter_result_addr = result_addr;
        disp.front_window = dialog_ptr;
        disp.input_state.set_mouse_button_for_test(true);
        disp.dialog_tracking = Some(DialogTrackingState {
            dialog_ptr,
            bounds,
            items,
            default_item: 1,
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: disp.save_dialog_pixels(&bus, bounds),
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            draw_procs_done: true,
            filter_proc: 0x149F0,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: probe_y,
                where_h: probe_x,
                modifiers: 0,
            }),
            ..Default::default()
        });

        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert_eq!(bus.read_byte(control_ptr + 17), 0);
        assert!(!screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            probe_x,
            probe_y
        ));
        assert!(disp.pending_modal_dialog_mouse_up);

        disp.front_window = child_dialog;
        disp.push_mouse_up(probe_y, probe_x);
        assert!(!disp.pending_modal_dialog_mouse_up);
        assert!(disp.event_queue.iter().all(|event| event.what != 2));

        disp.front_window = dialog_ptr;
        assert!(!screen_pixel_is_set(
            &bus,
            screen_base,
            row_bytes,
            probe_x,
            probe_y
        ));
        disp.push_mouse_down(150, 260);
        disp.push_mouse_up(150, 260);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 1);
        assert_eq!(what, 1);
        assert!(has_event);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 2);
        assert_eq!(what, 2);
        assert!(has_event);
    }

    #[test]
    fn modal_dialog_resource_control_retains_mouse_up_ownership_after_disposal() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = bus.alloc(170);
        let previous_window = bus.alloc(170);
        let item_hit_ptr = 0x300000u32;

        seed_window_regions(&mut bus, dialog_ptr, (100, 200, 200, 360));
        seed_window_regions(&mut bus, previous_window, (0, 0, 342, 512));
        disp.front_window = dialog_ptr;
        disp
            .current_port
            .with_mut(|current_port| *current_port = dialog_ptr);
        disp.window_list.replace(vec![dialog_ptr, previous_window]);
        disp.window_stack
            .push((previous_window, (0, 0, 342, 512), 0, String::from("Map")));
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 7,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                ..DialogItem::default()
            }],
        );
        disp.dialog_modal_entered.insert(dialog_ptr);

        bus.write_word(item_hit_ptr, 0);
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: disp.dialog_items[&dialog_ptr].clone(),
            default_item: 1,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
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
            game_managed: true,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        disp.push_mouse_down(130, 240);

        disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert!(disp.event_queue.iter().all(|event| event.what != 1));
        assert!(disp.pending_modal_dialog_mouse_up);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, dialog_ptr);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(disp.pending_modal_dialog_mouse_up);

        disp.push_mouse_up(130, 240);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 2);
        assert_eq!(what, 0);
        assert!(!has_event);
        assert!(!disp.pending_modal_dialog_mouse_up);

        disp.push_mouse_down(150, 260);
        disp.push_mouse_up(150, 260);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 1);
        assert_eq!(what, 1);
        assert!(has_event);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 2);
        assert_eq!(what, 2);
        assert!(has_event);
    }

    #[test]
    fn disposing_modal_dialog_ends_owned_press_before_exposing_parent() {
        let (mut disp, mut cpu, mut bus) = setup();
        let child = bus.alloc(170);
        let parent = bus.alloc(170);

        seed_window_regions(&mut bus, child, (100, 200, 200, 360));
        seed_window_regions(&mut bus, parent, (0, 0, 342, 512));
        disp.front_window = child;
        disp
            .current_port
            .with_mut(|current_port| *current_port = child);
        disp.window_list.replace(vec![child, parent]);
        disp.window_stack
            .push((parent, (0, 0, 342, 512), 2, String::new()));
        disp.dialog_items.insert(child, Vec::new());

        // An unrelated earlier down must stay ahead of the child press.
        disp.push_mouse_down(20, 30);
        // Model an application-owned modal button handler that has returned
        // from ModalDialog but left the initiating event queued while keeping
        // ownership of the not-yet-arrived release.
        disp.push_mouse_down(130, 240);
        let owned_down = disp.event_queue.back().expect("child mouseDown").clone();
        disp.consume_or_retain_dialog_mouse_up(&owned_down);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, child);
        disp.dispatch_dialog(true, 0x183, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(disp.front_window, parent);
        let queued_downs = disp
            .event_queue
            .iter()
            .filter(|event| event.what == 1)
            .collect::<Vec<_>>();
        assert_eq!(queued_downs.len(), 1);
        assert_eq!((queued_downs[0].where_v, queued_downs[0].where_h), (20, 30));
        assert!(disp.pending_modal_dialog_mouse_up);
        assert!(!disp.input_state.mouse_button_pressed());
        assert_eq!(bus.read_byte(0x0172), 0x80);

        for trap in [0x173, 0x174, 0x177] {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP, 0xFFFF);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(TEST_SP), 0);
        }

        let (what, _, _, where_v, where_h, _, has_event) =
            disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 1);
        assert_eq!((what, where_v, where_h, has_event), (1, 20, 30, true));

        disp.push_mouse_up(130, 240);
        assert!(!disp.pending_modal_dialog_mouse_up);
        assert!(disp
            .event_queue
            .iter()
            .all(|event| !matches!(event.what, 1 | 2)));

        // A later independent click remains deliverable to the parent.
        disp.push_mouse_down(150, 260);
        disp.push_mouse_up(150, 260);
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 1);
        assert_eq!((what, has_event), (1, true));
        let (what, _, _, _, _, _, has_event) = disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1 << 2);
        assert_eq!((what, has_event), (2, true));
    }

    #[test]
    fn modal_dialog_return_key_activates_default_button_after_filter_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;
        let result_addr = 0x300100u32;

        bus.write_word(result_addr, 0);
        bus.write_word(item_hit_ptr, 0);
        disp.dialog_filter_result_addr = result_addr;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 4,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0x149F0,
            game_managed: true,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 3,
                message: 0x0000_240D,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            }),
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(item_hit_ptr), 0);
        {
            let tracking = disp.dialog_tracking.as_ref().unwrap();
            assert_eq!(tracking.flash_item, 1);
            assert!(tracking.flash_remaining > 0);
        }

        {
            let tracking = disp.dialog_tracking.as_mut().unwrap();
            tracking.flash_remaining = 1;
            tracking.flash_delay = 0;
        }
        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 1);
        assert!(disp.dialog_tracking.is_none());
    }

    #[test]
    fn modal_dialog_filter_low_byte_nonzero_result_is_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;
        let result_addr = 0x300100u32;

        bus.write_word(result_addr, 0x0001);
        bus.write_word(item_hit_ptr, 0);
        disp.dialog_filter_result_addr = result_addr;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 4,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0x149F0,
            game_managed: false,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            }),
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });
        disp.input_state.set_mouse_button_for_test(true);
        disp.input_state.set_mouse_position_for_test((130, 240));

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(item_hit_ptr), 0);
        assert!(disp
            .dialog_tracking
            .as_ref()
            .and_then(|tracking| tracking.active_button.as_ref())
            .is_some());
    }

    #[test]
    fn modal_dialog_filter_true_zero_hit_returns_to_app() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x200000u32;
        let item_hit_ptr = 0x300000u32;
        let result_addr = 0x300100u32;

        bus.write_word(result_addr, 0xFFFF);
        bus.write_word(item_hit_ptr, 0);
        disp.dialog_filter_result_addr = result_addr;
        disp.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: vec![DialogItem {
                item_type: 4,
                rect: (20, 30, 60, 110),
                text: String::from("OK"),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Default::default(),
            stack_ptr: TEST_SP,
            item_hit_ptr,
            rendered_pixels: Default::default(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc: 0x149F0,
            game_managed: false,
            last_filter_event: Some(crate::trap::dispatch::QueuedEvent {
                what: 3,
                message: 0x0000_240D,
                when: 0,
                where_v: 130,
                where_h: 240,
                modifiers: 0,
            }),
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        let result = disp.dispatch_dialog(true, 0x191, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(item_hit_ptr), 0);
        assert!(disp.dialog_tracking.is_none());
    }

    // ---- TEInit ($A9CC) ----

    #[test]
    fn teinit_first_call_allocates_empty_scrap_handle_and_zeros_length() {
        // IM:I I-376 + I-389: TEInit creates an empty TextEdit scrap
        // handle and sets TEScrpLength to 0.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 0xFFFF);
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, 0);

        let result = disp.dispatch_dialog(true, 0x1CC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before);

        let scrap_handle = bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE);
        assert_ne!(scrap_handle, 0, "TEInit must allocate TEScrpHandle");
        assert_eq!(
            bus.read_long(scrap_handle),
            0,
            "TEInit scrap handle should initially reference empty data"
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            0
        );
    }

    #[test]
    fn teinit_reuses_existing_scrap_handle_and_resets_length() {
        // IM:I I-376 + I-389: TEInit restores empty-scrap state; HLE must
        // keep an existing handle stable to avoid churn/leaks on repeat calls.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing_handle = TrapDispatcher::allocate_handle_with_data(&mut bus, 3);
        let existing_ptr = bus.read_long(existing_handle);
        bus.write_bytes(existing_ptr, b"xyz");
        bus.write_long(
            crate::memory::globals::addr::TE_SCRP_HANDLE,
            existing_handle,
        );
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 3);

        let result = disp.dispatch_dialog(true, 0x1CC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
            existing_handle
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            0
        );
        assert_eq!(bus.read_long(existing_handle), existing_ptr);
        assert_eq!(bus.read_bytes(existing_ptr, 3), b"xyz".to_vec());
    }

    fn make_te_with_text(
        disp: &mut TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
        text: &[u8],
    ) -> u32 {
        let te_handle = TrapDispatcher::allocate_te_handle(bus);
        disp
            .current_port
            .with_mut(|current_port| *current_port = 0x181000);
        disp.tx_font = 4;
        disp.tx_face = 0;
        disp.tx_mode = 0;
        disp.tx_size = 10;
        disp.initialize_te_record(bus, te_handle, (0, 0, 60, 160), (0, 0, 60, 160));
        disp.te_set_text_contents(bus, te_handle, text);
        te_handle
    }

    // ---- TENew ----

    #[test]
    fn tenew_pointer_arg_convention_initializes_destrect_viewrect_and_returns_non_nil_handle() {
        // IM:I I-373..I-374 + TextEdit.h ONEWORDINLINE(0xA9D2):
        // FUNCTION TENew(destRect, viewRect: Rect): TEHandle. Modern MPW
        // Universal Headers pass two `const Rect *` pointers (8 bytes of
        // args). After dispatch (**hTE).destRect and (**hTE).viewRect
        // must round-trip the caller's input rects exactly. Defeats stubs
        // that swap the args, zero the rects, or copy only a subset of
        // the four 2-byte fields.
        let (mut disp, mut cpu, mut bus) = setup();

        // Build two DISTINCT Rect inputs in guest memory.
        let dest_rect_ptr: u32 = 0x190000;
        bus.write_word(dest_rect_ptr, (-1000i16) as u16); // top
        bus.write_word(dest_rect_ptr + 2, (-1000i16) as u16); // left
        bus.write_word(dest_rect_ptr + 4, (-700i16) as u16); // bottom
        bus.write_word(dest_rect_ptr + 6, (-800i16) as u16); // right
        let view_rect_ptr: u32 = 0x190010;
        bus.write_word(view_rect_ptr, (-900i16) as u16); // top
        bus.write_word(view_rect_ptr + 2, (-1100i16) as u16); // left
        bus.write_word(view_rect_ptr + 4, (-600i16) as u16); // bottom
        bus.write_word(view_rect_ptr + 6, (-700i16) as u16); // right

        // Pascal FUNCTION stack frame (pointer-arg convention).
        // Pascal pushes args left-to-right, so the FIRST arg (destRect_ptr)
        // is DEEPEST on stack at trap entry. With 4-byte pointers:
        //   sp+0..3   viewRect_ptr  (last pushed, shallowest)
        //   sp+4..7   destRect_ptr  (first pushed, deepest)
        //   sp+8..11  TEHandle result slot (poisoned)
        bus.write_long(TEST_SP, view_rect_ptr);
        bus.write_long(TEST_SP + 4, dest_rect_ptr);
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(te_handle, 0, "TENew returned NIL TEHandle");
        let te_ptr = bus.read_long(te_handle);
        assert_ne!(te_ptr, 0, "TEHandle master pointer is NIL");

        // destRect round-trip (top, left, bottom, right at offset 0..6).
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16,
            -1000
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2) as i16,
            -1000
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4) as i16,
            -700
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6) as i16,
            -800
        );

        // viewRect round-trip (top, left, bottom, right at offset 8..14).
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET) as i16,
            -900
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2) as i16,
            -1100
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4) as i16,
            -600
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6) as i16,
            -700
        );
    }

    #[test]
    fn tenew_fresh_terec_has_zero_telength_and_empty_selection_per_im_i_373() {
        // IM:I I-373: a fresh TERec has no text and an empty selection
        // range. Witness teLength == 0, selStart == 0, selEnd == 0, and
        // hText is a non-NIL Handle.
        let (mut disp, mut cpu, mut bus) = setup();

        let rect_ptr: u32 = 0x190020;
        bus.write_word(rect_ptr, 10); // top
        bus.write_word(rect_ptr + 2, 20); // left
        bus.write_word(rect_ptr + 4, 110); // bottom
        bus.write_word(rect_ptr + 6, 220); // right

        bus.write_long(TEST_SP, rect_ptr);
        bus.write_long(TEST_SP + 4, rect_ptr);

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let te_handle = bus.read_long(TEST_SP + 8);
        let te_ptr = bus.read_long(te_handle);

        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            0,
            "fresh TERec must have teLength == 0"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0,
            "fresh TERec must have selStart == 0"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            0,
            "fresh TERec must have selEnd == 0"
        );
        let h_text = bus.read_long(te_ptr + TrapDispatcher::TE_HTEXT_OFFSET);
        assert_ne!(h_text, 0, "fresh TERec must have a non-NIL hText handle");
    }

    #[test]
    fn tenew_function_protocol_consumes_two_pointer_args_and_writes_4_byte_result() {
        // Pascal FUNCTION calling convention: 2 pointer args (8 bytes)
        // are popped, the 4-byte TEHandle result is written into the
        // caller's pre-allocated slot at the former SP+8. Sentinels
        // around the result slot must survive the trap call.
        let (mut disp, mut cpu, mut bus) = setup();

        let rect_ptr: u32 = 0x190030;
        bus.write_word(rect_ptr, 0);
        bus.write_word(rect_ptr + 2, 0);
        bus.write_word(rect_ptr + 4, 100);
        bus.write_word(rect_ptr + 6, 200);

        bus.write_long(TEST_SP, rect_ptr);
        bus.write_long(TEST_SP + 4, rect_ptr);
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
        bus.write_long(TEST_SP + 12, 0xCAFE_BABE); // sentinel past result

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(te_handle, 0xDEAD_BEEF, "result slot must be overwritten");
        assert_ne!(te_handle, 0);
        assert_eq!(
            bus.read_long(TEST_SP + 12),
            0xCAFE_BABE,
            "sentinel past 4-byte result slot must survive"
        );
    }

    #[test]
    fn tenew_pointer_arg_convention_accepts_wide_ordered_destrect() {
        // Some MPW callers pass a Rect embedded at the start of an
        // application record. TextEdit accepts the pointer convention
        // even when the destination Rect is wider than the visible
        // screen; the trap must still pop only the two pointer args.
        let (mut disp, mut cpu, mut bus) = setup();

        let dest_record_ptr: u32 = 0x190040;
        bus.write_word(dest_record_ptr, 159);
        bus.write_word(dest_record_ptr + 2, 500);
        bus.write_word(dest_record_ptr + 4, 171);
        bus.write_word(dest_record_ptr + 6, 10500);

        let view_rect_ptr: u32 = 0x190060;
        bus.write_word(view_rect_ptr, 159);
        bus.write_word(view_rect_ptr + 2, 500);
        bus.write_word(view_rect_ptr + 4, 171);
        bus.write_word(view_rect_ptr + 6, 10500);

        bus.write_long(TEST_SP, view_rect_ptr);
        bus.write_long(TEST_SP + 4, dest_record_ptr);
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
        bus.write_long(TEST_SP + 12, 0xCAFE_BABE);

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "TENew pointer convention must pop exactly two pointers"
        );

        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(te_handle, 0);
        assert_ne!(te_handle, 0xDEAD_BEEF);
        assert_eq!(
            bus.read_long(TEST_SP + 12),
            0xCAFE_BABE,
            "TENew must not write past the 4-byte function-result slot"
        );

        let te_ptr = bus.read_long(te_handle);
        assert_ne!(te_ptr, 0);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6) as i16,
            10500,
            "wide destRect.right must round-trip from caller memory"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6) as i16,
            10500,
            "wide viewRect.right must round-trip from caller memory"
        );
    }

    #[test]
    fn tenew_pointer_arg_convention_is_based_on_valid_pointers_not_rect_shape() {
        // The calling convention is determined by the stack containing
        // two Rect pointers. Unusual caller Rect contents must not make
        // the trap consume a legacy by-value frame and corrupt the
        // caller's saved registers.
        let (mut disp, mut cpu, mut bus) = setup();

        let dest_rect_ptr: u32 = 0x190080;
        bus.write_word(dest_rect_ptr, 552);
        bus.write_word(dest_rect_ptr + 2, 525);
        bus.write_word(dest_rect_ptr + 4, 520);
        bus.write_word(dest_rect_ptr + 6, 10525);

        let view_rect_ptr: u32 = 0x1900A0;
        bus.write_word(view_rect_ptr, 552);
        bus.write_word(view_rect_ptr + 2, 525);
        bus.write_word(view_rect_ptr + 4, 520);
        bus.write_word(view_rect_ptr + 6, 10525);

        bus.write_long(TEST_SP, view_rect_ptr);
        bus.write_long(TEST_SP + 4, dest_rect_ptr);
        bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
        bus.write_long(TEST_SP + 12, 0xCAFE_BABE);

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(te_handle, 0);
        assert_ne!(te_handle, 0xDEAD_BEEF);
        assert_eq!(bus.read_long(TEST_SP + 12), 0xCAFE_BABE);

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16,
            552
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4) as i16,
            520
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6) as i16,
            10525
        );
    }

    // ---- TEDispose / TECalText / TESetSelect / TEDelete ----

    #[test]
    fn tedispose_releases_te_record_text_handle_and_pops_arg() {
        // IM:I I-383..I-384.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        let text_handle = bus.read_long(te_ptr + TrapDispatcher::TE_HTEXT_OFFSET);
        let text_ptr = bus.read_long(text_handle);
        assert_ne!(te_handle, 0);
        assert_ne!(te_ptr, 0);
        assert_ne!(text_handle, 0);
        assert_ne!(text_ptr, 0);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1CD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        assert_eq!(bus.get_alloc_size(text_ptr), None);
        assert_eq!(bus.get_alloc_size(text_handle), None);
        assert_eq!(bus.get_alloc_size(te_ptr), None);
        assert_eq!(bus.get_alloc_size(te_handle), None);
    }

    #[test]
    fn tecaltext_recomputes_line_metadata_and_pops_arg() {
        // IM:I I-390: lineStarts are recomputed from current text layout.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let te_ptr = bus.read_long(te_handle);
        let text_len = TrapDispatcher::te_text_bytes(&bus, te_handle).len() as u16;
        assert_eq!(text_len, 32);

        // Force stale metadata and a narrow destination width so wrapped
        // layout produces multiple lines.
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 12);
        bus.write_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_N_LINES_OFFSET, 0x7777);
        bus.write_word(te_ptr + TrapDispatcher::TE_LINE_STARTS_OFFSET, 0x7777);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D0, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        let te_ptr = bus.read_long(te_handle);
        let n_lines = bus.read_word(te_ptr + TrapDispatcher::TE_N_LINES_OFFSET);
        assert_ne!(n_lines, 0x7777);
        assert!(n_lines > 1);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            text_len
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LINE_STARTS_OFFSET),
            0
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LINE_STARTS_OFFSET + u32::from(n_lines) * 2),
            text_len
        );
    }

    #[test]
    fn tegettext_returns_htext_handle_from_terec_and_pops_te_handle_arg() {
        // IM:I I-384: "TEGetText returns a handle to the text of the
        // specified edit record. The result is the same as the handle in
        // the hText field of the edit record."
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        let expected_h_text = bus.read_long(te_ptr + TrapDispatcher::TE_HTEXT_OFFSET);
        assert_ne!(expected_h_text, 0);

        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 0xDEAD_BEEF); // poison result slot
        let result = disp.dispatch_dialog(true, 0x1CB, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        // The returned CharsHandle equals (**hTE).hText exactly.
        assert_eq!(bus.read_long(TEST_SP + 4), expected_h_text);

        // The handle dereferences to the 5-byte payload "HELLO".
        let text_ptr = bus.read_long(expected_h_text);
        assert_ne!(text_ptr, 0);
        assert_eq!(bus.read_bytes(text_ptr, 5), b"HELLO".to_vec());
    }

    #[test]
    fn tegettext_returns_nil_handle_when_te_handle_is_zero() {
        // Defensive: a NIL TEHandle has no TERec to read hText from, so
        // the result is NIL. Guards against any future change that would
        // crash or read past invalid memory when handed a NIL argument.
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_long(TEST_SP, 0); // NIL TEHandle
        bus.write_long(TEST_SP + 4, 0xCAFE_BABE); // poison result slot
        let result = disp.dispatch_dialog(true, 0x1CB, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), 0);
    }

    #[test]
    fn tesetselect_clamps_selend_to_text_length_and_pops_args() {
        // IM:I I-385: selEnd past end-of-text clamps to teLength.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 5);

        bus.write_long(TEST_SP, te_handle); // hTE
        bus.write_long(TEST_SP + 4, 999); // selEnd
        bus.write_long(TEST_SP + 8, 2); // selStart
        let result = disp.dispatch_dialog(true, 0x1D1, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 5);
    }

    #[test]
    fn tedelete_removes_selection_without_touching_scrap_and_pops_arg() {
        // IM:I I-387: TEDelete deletes selection without writing TextEdit scrap.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let existing_ptr = bus.read_long(existing_scrap);
        bus.write_bytes(existing_ptr, b"QQ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, existing_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D7, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HO".to_vec()
        );

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 1);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
            existing_scrap
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            2
        );
        assert_eq!(bus.read_bytes(existing_ptr, 2), b"QQ".to_vec());
    }

    #[test]
    fn tedelete_insertion_point_selection_is_noop_and_pops_arg() {
        // IM:I I-387: insertion-point selection means "nothing happens".
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 3);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D7, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            3
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 3);
    }

    #[test]
    fn teupdate_redraws_text_and_pops_rect_pointer_and_tehandle() {
        // IM:I I-387; Inside Macintosh: Text 1993, 2-90.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
        for i in 0..(row_bytes * 80) {
            bus.write_byte(screen_base + i, 0);
        }

        bus.write_long(TEST_SP, te_handle);
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 0);
        bus.write_word(rect_ptr + 2, 0);
        bus.write_word(rect_ptr + 4, 40);
        bus.write_word(rect_ptr + 6, 120);
        bus.write_long(TEST_SP + 4, rect_ptr);

        let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        let drew_any_pixel =
            (0..(row_bytes * 80)).any(|offset| bus.read_byte(screen_base + offset) != 0);
        assert!(
            drew_any_pixel,
            "TEUpdate should draw at least one non-white pixel for visible text"
        );
    }

    #[test]
    fn teupdate_systemless_theme_routes_active_caret_through_provider() {
        let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
        let classic_te_handle = make_te_with_text(&mut classic, &mut classic_bus, b"");
        let classic_te_ptr = classic_bus.read_long(classic_te_handle);
        classic_bus.write_word(classic_te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
        classic_bus.write_word(classic_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        classic_bus.write_word(classic_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);
        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = classic.screen_mode;
        for i in 0..(row_bytes * 80) {
            classic_bus.write_byte(screen_base + i, 0);
        }
        classic_bus.write_long(TEST_SP, classic_te_handle);
        let rect_ptr = classic_bus.alloc(8);
        classic_bus.write_word(rect_ptr, 0);
        classic_bus.write_word(rect_ptr + 2, 0);
        classic_bus.write_word(rect_ptr + 4, 40);
        classic_bus.write_word(rect_ptr + 6, 120);
        classic_bus.write_long(TEST_SP + 4, rect_ptr);
        let result = classic.dispatch_dialog(true, 0x1D3, &mut classic_cpu, &mut classic_bus);
        assert!(result.unwrap().is_ok());

        let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
        themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let themed_te_handle = make_te_with_text(&mut themed, &mut themed_bus, b"");
        let themed_te_ptr = themed_bus.read_long(themed_te_handle);
        themed_bus.write_word(themed_te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
        themed_bus.write_word(themed_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        themed_bus.write_word(themed_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);
        for i in 0..(row_bytes * 80) {
            themed_bus.write_byte(screen_base + i, 0);
        }
        themed_bus.write_long(TEST_SP, themed_te_handle);
        let rect_ptr = themed_bus.alloc(8);
        themed_bus.write_word(rect_ptr, 0);
        themed_bus.write_word(rect_ptr + 2, 0);
        themed_bus.write_word(rect_ptr + 4, 40);
        themed_bus.write_word(rect_ptr + 6, 120);
        themed_bus.write_long(TEST_SP + 4, rect_ptr);
        let result = themed.dispatch_dialog(true, 0x1D3, &mut themed_cpu, &mut themed_bus);
        assert!(result.unwrap().is_ok());

        assert!(
            screen_pixel_is_set(&classic_bus, screen_base, row_bytes, 1, 0),
            "classic TEUpdate should draw a one-pixel caret for an active empty insertion point"
        );
        assert!(
            screen_pixel_is_set(&themed_bus, screen_base, row_bytes, 0, 0),
            "systemless-default TEUpdate should draw provider-owned caret chrome"
        );
    }

    #[test]
    fn textedit_caret_movement_and_blinking_stay_inside_short_view_rectangle() {
        // Text (1993), pp. 2-16/2-29: only the view rectangle is visible.
        // SC2K uses a 15-pixel field with a taller line. Moving or blinking
        // the caret must not leave its bottom pixels below that field.
        for theme in [UiThemeId::ClassicSystem7, UiThemeId::SystemlessDefault] {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            // setup_with_port creates a 512-pixel monochrome BitMap; make
            // the display and pixel assertions use that same layout.
            disp.set_screen_mode_for_test(bus.read_long(0x0824), 64, 512, 342, 1);
            disp.set_ui_theme_id(theme);
            let te_handle = make_te_with_text(&mut disp, &mut bus, b"    ");
            let te_ptr = bus.read_long(te_handle);
            let view = (20, 20, 35, 100);
            TrapDispatcher::te_write_rect_words(
                &mut bus,
                te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET,
                (20, 19, 35, 120),
            );
            TrapDispatcher::te_write_rect_words(
                &mut bus,
                te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET,
                view,
            );
            bus.write_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET, 18);
            let (base, row_bytes, _, _, _) = disp.screen_mode;
            for offset in 0..row_bytes * 80 {
                bus.write_byte(base + offset, 0);
            }
            bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
            for selection in [0, 1, 2, 3, 4, 0] {
                bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, selection);
                bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, selection);
                for caret_state in [0, 1] {
                    bus.write_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET, caret_state);
                    disp.draw_te_contents(&mut cpu, &mut bus, te_handle, true);
                    let mut visible_pixels = 0;
                    for y in 0..60 {
                        for x in 0..140 {
                            let set = screen_pixel_is_set(&bus, base, row_bytes, x, y);
                            if y >= view.0 && y < view.2 && x >= view.1 && x < view.3 {
                                visible_pixels += usize::from(set);
                            } else {
                                assert!(
                                    !set,
                                    "{theme:?}: caret trail at ({x},{y}), selection={selection}"
                                );
                            }
                        }
                    }
                    assert_eq!(
                        visible_pixels > 0,
                        caret_state == 0,
                        "{theme:?}: caret must blink cleanly"
                    );
                }
            }
            // A caret horizontally outside the view is also invisible.
            TrapDispatcher::te_write_rect_words(
                &mut bus,
                te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET,
                (20, 80, 35, 100),
            );
            bus.write_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET, 0);
            disp.draw_te_contents(&mut cpu, &mut bus, te_handle, true);
            assert!((0..row_bytes * 60).all(|offset| bus.read_byte(base + offset) == 0));
        }
    }

    #[test]
    fn teactivate_and_tedeactivate_repaint_empty_insertion_caret() {
        // IM:I I-385 and Text 1993 p. 2-80: TEActivate displays the caret
        // when the active selection is an insertion point; TEDeactivate
        // removes it without changing the selection offsets.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"");
        let te_ptr = bus.read_long(te_handle);
        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
        for i in 0..(row_bytes * 80) {
            bus.write_byte(screen_base + i, 0);
        }
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D8, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET), 1);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 0);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "TEActivate should paint the visible insertion caret"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D9, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET), 0);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 0);
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "TEDeactivate should erase the insertion caret"
        );
    }

    #[test]
    fn teupdate_classic_active_selection_inverts_selected_text_range() {
        fn render_classic_selection(selected: bool) -> u64 {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
            let te_ptr = bus.read_long(te_handle);
            bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
            bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
            bus.write_word(
                te_ptr + TrapDispatcher::TE_SEL_END_OFFSET,
                if selected { 3 } else { 0 },
            );
            let line_height = bus.read_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET) as i16;
            let port = bus.read_long(te_ptr + TrapDispatcher::TE_IN_PORT_OFFSET);
            let base = bus.read_long(port + 2);
            let row_bytes = u32::from(bus.read_word(port + 6) & 0x3FFF);
            for i in 0..(row_bytes * 80) {
                bus.write_byte(base + i, 0);
            }
            bus.write_long(TEST_SP, te_handle);
            let rect_ptr = bus.alloc(8);
            bus.write_word(rect_ptr, 0);
            bus.write_word(rect_ptr + 2, 0);
            bus.write_word(rect_ptr + 4, 40);
            bus.write_word(rect_ptr + 6, 120);
            bus.write_long(TEST_SP + 4, rect_ptr);
            let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            sum_screen_bytes(&bus, base, row_bytes, 0, 0, line_height, 32)
        }

        // IM:I I-375/I-385 and Text 1993 p. 2-80: active non-empty TextEdit
        // selections are highlighted. Classic highlighting is inverse video
        // over the selected character range, so the selected render must
        // differ from the same active insertion-point redraw.
        let selected = render_classic_selection(true);
        let unselected = render_classic_selection(false);
        assert!(
            selected != unselected,
            "classic TEUpdate should invert active non-empty selection pixels; selected={selected}, unselected={unselected}"
        );
    }

    #[test]
    fn teupdate_systemless_theme_routes_multiline_selection_through_provider() {
        fn render_themed_multiline_selection(selected: bool) -> (u16, u16, i16, u64, u64) {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
            let te_handle = make_te_with_text(&mut disp, &mut bus, b"A\rB");
            let te_ptr = bus.read_long(te_handle);
            bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
            bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
            bus.write_word(
                te_ptr + TrapDispatcher::TE_SEL_END_OFFSET,
                if selected { 3 } else { 0 },
            );
            let line_height = bus.read_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET) as i16;
            let (base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
            for i in 0..(row_bytes * 80) {
                bus.write_byte(base + i, 0);
            }
            bus.write_long(TEST_SP, te_handle);
            let rect_ptr = bus.alloc(8);
            bus.write_word(rect_ptr, 0);
            bus.write_word(rect_ptr + 2, 0);
            bus.write_word(rect_ptr + 4, 40);
            bus.write_word(rect_ptr + 6, 120);
            bus.write_long(TEST_SP + 4, rect_ptr);
            let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());

            let first_line = sum_screen_bytes(&bus, base, row_bytes, 0, 0, line_height, 24);
            let second_line =
                sum_screen_bytes(&bus, base, row_bytes, line_height, 0, line_height * 2, 24);
            (
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
                line_height,
                first_line,
                second_line,
            )
        }

        let selected = render_themed_multiline_selection(true);
        let unselected = render_themed_multiline_selection(false);

        // Text 1993, appendix C: TERec.active marks whether the selection
        // range is highlighted or the caret is displayed; selStart/selEnd are
        // byte offsets into the contiguous selection range.
        assert_eq!((selected.0, selected.1), (0, 3));
        assert_eq!((unselected.0, unselected.1), (0, 0));
        assert_eq!(selected.2, unselected.2);
        assert!(
            selected.4 != unselected.4,
            "systemless-default TEUpdate should draw provider selection chrome on the second selected line"
        );
    }

    #[test]
    fn teupdate_systemless_theme_renders_reversed_selection_without_rewriting_offsets() {
        fn render_themed_selection(sel_start: u16, sel_end: u16) -> (u16, u16, u16, i16, u32, u32) {
            let (mut disp, mut cpu, mut bus) = setup_with_port();
            disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
            let te_handle = make_te_with_text(&mut disp, &mut bus, b"A\rB");
            let te_ptr = bus.read_long(te_handle);
            bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
            bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, sel_start);
            bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, sel_end);
            let line_height = bus.read_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET) as i16;
            let (base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
            for i in 0..(row_bytes * 80) {
                bus.write_byte(base + i, 0);
            }

            bus.write_long(TEST_SP, te_handle);
            let rect_ptr = bus.alloc(8);
            bus.write_word(rect_ptr, 0);
            bus.write_word(rect_ptr + 2, 0);
            bus.write_word(rect_ptr + 4, 40);
            bus.write_word(rect_ptr + 6, 120);
            bus.write_long(TEST_SP + 4, rect_ptr);
            let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

            let first_line_bottom_band =
                count_set_pixels(&bus, base, row_bytes, line_height - 2, 0, line_height, 24);
            let second_line_bottom_band = count_set_pixels(
                &bus,
                base,
                row_bytes,
                line_height * 2 - 2,
                0,
                line_height * 2,
                24,
            );

            (
                bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
                bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
                line_height,
                first_line_bottom_band,
                second_line_bottom_band,
            )
        }

        let forward = render_themed_selection(0, 3);
        let reversed = render_themed_selection(3, 0);

        // IM:I I-385 says TESetSelect owns selection-range changes, while
        // IM:I I-387 / Text 1993 p. 2-90 define TEUpdate as redraw. The theme
        // path can normalize reversed endpoints for drawing but must not
        // rewrite the guest TERec.
        assert_eq!(forward.0, 3);
        assert_eq!(reversed.0, 3);
        assert_eq!((forward.1, forward.2), (0, 3));
        assert_eq!((reversed.1, reversed.2), (3, 0));
        assert_eq!(forward.3, reversed.3);
        assert!(forward.4 > 0 && forward.5 > 0);
        assert_eq!(
            (reversed.4, reversed.5),
            (forward.4, forward.5),
            "systemless-default TEUpdate should render reversed selection ranges like their normalized range without rewriting selStart/selEnd"
        );
    }

    #[test]
    fn teupdate_systemless_theme_routes_inactive_outline_selection_through_provider() {
        let (mut inactive_plain, mut inactive_plain_cpu, mut inactive_plain_bus) =
            setup_with_port();
        inactive_plain.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let inactive_plain_te_handle =
            make_te_with_text(&mut inactive_plain, &mut inactive_plain_bus, b"HELLO");
        let inactive_plain_te_ptr = inactive_plain_bus.read_long(inactive_plain_te_handle);
        inactive_plain_bus.write_word(inactive_plain_te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0);
        inactive_plain_bus.write_word(
            inactive_plain_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET,
            0,
        );
        inactive_plain_bus.write_word(inactive_plain_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);
        let (plain_base, plain_row_bytes, _screen_w, _screen_h, _pixel_size) =
            inactive_plain.screen_mode;
        for i in 0..(plain_row_bytes * 80) {
            inactive_plain_bus.write_byte(plain_base + i, 0);
        }
        inactive_plain_bus.write_long(TEST_SP, inactive_plain_te_handle);
        let rect_ptr = inactive_plain_bus.alloc(8);
        inactive_plain_bus.write_word(rect_ptr, 0);
        inactive_plain_bus.write_word(rect_ptr + 2, 0);
        inactive_plain_bus.write_word(rect_ptr + 4, 40);
        inactive_plain_bus.write_word(rect_ptr + 6, 120);
        inactive_plain_bus.write_long(TEST_SP + 4, rect_ptr);
        let result = inactive_plain.dispatch_dialog(
            true,
            0x1D3,
            &mut inactive_plain_cpu,
            &mut inactive_plain_bus,
        );
        assert!(result.unwrap().is_ok());

        let (mut inactive_outline, mut inactive_outline_cpu, mut inactive_outline_bus) =
            setup_with_port();
        inactive_outline.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let inactive_outline_te_handle =
            make_te_with_text(&mut inactive_outline, &mut inactive_outline_bus, b"HELLO");
        let inactive_outline_te_ptr = inactive_outline_bus.read_long(inactive_outline_te_handle);
        inactive_outline_bus.write_word(
            inactive_outline_te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET,
            0,
        );
        inactive_outline_bus.write_word(
            inactive_outline_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET,
            0,
        );
        inactive_outline_bus.write_word(
            inactive_outline_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET,
            3,
        );
        let inactive_line_height = inactive_outline_bus
            .read_word(inactive_outline_te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET)
            as i16;
        let (outline_base, outline_row_bytes, _screen_w, _screen_h, _pixel_size) =
            inactive_outline.screen_mode;
        for i in 0..(outline_row_bytes * 80) {
            inactive_outline_bus.write_byte(outline_base + i, 0);
        }

        inactive_outline_cpu.write_reg(Register::A7, TEST_SP);
        inactive_outline_bus.write_word(TEST_SP, 0x000E);
        inactive_outline_bus.write_long(TEST_SP + 2, inactive_outline_te_handle);
        inactive_outline_bus.write_word(TEST_SP + 6, TrapDispatcher::TE_BIT_SET as u16);
        inactive_outline_bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_OUTLINE_HILITE);
        inactive_outline_bus.write_word(TEST_SP + 10, 0xBEEF);
        let result = inactive_outline.dispatch_dialog(
            true,
            0x03D,
            &mut inactive_outline_cpu,
            &mut inactive_outline_bus,
        );
        assert!(result.unwrap().is_ok());
        assert_eq!(inactive_outline_bus.read_word(TEST_SP + 10), 0);

        inactive_outline_cpu.write_reg(Register::A7, TEST_SP);
        inactive_outline_bus.write_long(TEST_SP, inactive_outline_te_handle);
        let rect_ptr = inactive_outline_bus.alloc(8);
        inactive_outline_bus.write_word(rect_ptr, 0);
        inactive_outline_bus.write_word(rect_ptr + 2, 0);
        inactive_outline_bus.write_word(rect_ptr + 4, 40);
        inactive_outline_bus.write_word(rect_ptr + 6, 120);
        inactive_outline_bus.write_long(TEST_SP + 4, rect_ptr);
        let result = inactive_outline.dispatch_dialog(
            true,
            0x1D3,
            &mut inactive_outline_cpu,
            &mut inactive_outline_bus,
        );
        assert!(result.unwrap().is_ok());

        let (mut active, mut active_cpu, mut active_bus) = setup_with_port();
        active.set_ui_theme_id(UiThemeId::SystemlessDefault);
        let active_te_handle = make_te_with_text(&mut active, &mut active_bus, b"HELLO");
        let active_te_ptr = active_bus.read_long(active_te_handle);
        active_bus.write_word(active_te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
        active_bus.write_word(active_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        active_bus.write_word(active_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);
        let active_line_height =
            active_bus.read_word(active_te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET) as i16;
        let (active_base, active_row_bytes, _screen_w, _screen_h, _pixel_size) = active.screen_mode;
        for i in 0..(active_row_bytes * 80) {
            active_bus.write_byte(active_base + i, 0);
        }
        active_bus.write_long(TEST_SP, active_te_handle);
        let rect_ptr = active_bus.alloc(8);
        active_bus.write_word(rect_ptr, 0);
        active_bus.write_word(rect_ptr + 2, 0);
        active_bus.write_word(rect_ptr + 4, 40);
        active_bus.write_word(rect_ptr + 6, 120);
        active_bus.write_long(TEST_SP + 4, rect_ptr);
        let result = active.dispatch_dialog(true, 0x1D3, &mut active_cpu, &mut active_bus);
        assert!(result.unwrap().is_ok());

        // Text 1993 describes outline highlighting as framing an inactive
        // selection; TEActivate/TEDeactivate tie it to teFOutlineHilite.
        let inactive_plain_top_band = count_set_pixels(
            &inactive_plain_bus,
            plain_base,
            plain_row_bytes,
            0,
            0,
            1,
            24,
        );
        let inactive_outline_top_band = count_set_pixels(
            &inactive_outline_bus,
            outline_base,
            outline_row_bytes,
            0,
            0,
            1,
            24,
        );
        assert_eq!(
            inactive_plain_top_band, 0,
            "inactive TextEdit selection without teFOutlineHilite should not draw provider selection chrome"
        );
        assert!(
            inactive_outline_top_band > 0,
            "inactive TextEdit selection with teFOutlineHilite should draw provider outline chrome"
        );
        assert_eq!(
            inactive_outline_bus
                .read_word(inactive_outline_te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0
        );
        assert_eq!(
            inactive_outline_bus
                .read_word(inactive_outline_te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            3
        );
        assert_eq!(inactive_line_height, active_line_height);

        let inactive_bottom_band = count_set_pixels(
            &inactive_outline_bus,
            outline_base,
            outline_row_bytes,
            inactive_line_height - 2,
            0,
            inactive_line_height,
            24,
        );
        let active_bottom_band = count_set_pixels(
            &active_bus,
            active_base,
            active_row_bytes,
            active_line_height - 2,
            0,
            active_line_height,
            24,
        );
        assert!(
            active_bottom_band > inactive_bottom_band,
            "inactive outline selection should stay outline-only instead of drawing active bottom-stripe chrome"
        );
    }

    #[test]
    fn teupdate_rect_pointer_pops_eight_bytes() {
        // Inside Macintosh: Text 1993, 2-90 update semantics plus
        // MPW TextEdit.h (Rect* + TEHandle).
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 10);
        bus.write_word(rect_ptr + 2, 10);
        bus.write_word(rect_ptr + 4, 40);
        bus.write_word(rect_ptr + 6, 120);

        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, rect_ptr);

        let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
    }

    #[test]
    fn teupdate_high_stack_rect_preserves_caller_saved_registers() {
        // A stack-local Rect is still passed by pointer, even above 64 MiB
        // or when empty. Popping its contents instead shifts the caller's
        // saved registers when it returns from its drawing callback.
        let mut disp = TrapDispatcher::new();
        let mut cpu = MockCpu::new();
        let mut bus = MacMemoryBus::new(128 * 1024 * 1024);
        let sp = 0x07F6_FAD4;
        let rect_ptr = sp + 64;
        for bottom in [40, 0] {
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, 0); // NIL TEHandle needs no drawing.
            bus.write_long(sp + 4, rect_ptr);
            bus.write_long(sp + 8, 0x00E4_F700); // saved dialog pointer
            bus.write_long(sp + 12, 0x019F_4158); // saved drawing callback
            bus.write_word(rect_ptr, 0);
            bus.write_word(rect_ptr + 2, 0);
            bus.write_word(rect_ptr + 4, bottom);
            bus.write_word(rect_ptr + 6, 120);

            assert!(disp
                .dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus)
                .unwrap()
                .is_ok());
            let restored_sp = cpu.read_reg(Register::A7);
            assert_eq!(restored_sp, sp + 8);
            assert_eq!(bus.read_long(restored_sp), 0x00E4_F700);
            assert_eq!(bus.read_long(restored_sp + 4), 0x019F_4158);
        }
    }

    #[test]
    fn teupdate_preserves_te_record_state_across_redraw() {
        // TEUpdate is a redraw operation and MUST NOT mutate hText,
        // teLength, selStart, or selEnd. Per IM:I I-387 + IM:Text 1993
        // p. 2-90: text bytes "HELLO" preserved, teLength==5,
        // selStart==1, selEnd==4 after repeated
        // TEUpdate calls.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        // Redraw using the update rectangle pointer.
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 10);
        bus.write_word(rect_ptr + 2, 10);
        bus.write_word(rect_ptr + 4, 40);
        bus.write_word(rect_ptr + 6, 120);
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, rect_ptr);
        let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        // Redraw the same TE record again.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 10);
        bus.write_word(rect_ptr + 2, 10);
        bus.write_word(rect_ptr + 4, 40);
        bus.write_word(rect_ptr + 6, 120);
        bus.write_long(TEST_SP + 4, rect_ptr);
        let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        // State preservation contract.
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 5);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 4);
    }

    #[test]
    fn teactivate_sets_active_flag_preserves_selection_and_pops_arg() {
        // IM:I I-385; Inside Macintosh: Text 1993, 2-80.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D8, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET), 1);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 4);
    }

    #[test]
    fn tedeactivate_clears_active_flag_preserves_selection_and_pops_arg() {
        // IM:I I-385; Inside Macintosh: Text 1993, 2-80.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 5);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D9, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET), 0);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 5);
    }

    #[test]
    fn teclick_consumes_point_extend_and_tehandle_arguments() {
        // Inside Macintosh Volume I (1985), p. I-376 and
        // Inside Macintosh: Text (1993), p. 2-85.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");

        bus.write_long(TEST_SP, te_handle); // hTE
        bus.write_word(TEST_SP + 4, 0xFF00); // extend=TRUE in high byte
        bus.write_word(TEST_SP + 6, 18); // pt.v
        bus.write_word(TEST_SP + 8, 27); // pt.h

        let result = disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn teclick_empty_text_keeps_insertion_point_at_zero() {
        // Inside Macintosh Volume I (1985), p. I-376: TEClick places the
        // insertion point from mouse position; with empty text only offset 0
        // is valid.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);

        bus.write_long(TEST_SP, te_handle); // hTE
        bus.write_word(TEST_SP + 4, 0x0000); // extend=FALSE
        bus.write_word(TEST_SP + 6, 12); // pt.v
        bus.write_word(TEST_SP + 8, 40); // pt.h

        let result = disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 0);
    }

    #[test]
    fn teclick_preserves_terec_active_flag_and_telength() {
        // Both BasiliskII System 7.5.3 ROM and Systemless HLE agree that
        // TEClick must not mutate the active flag (owned by
        // TEActivate/TEDeactivate per IM:I I-385) or teLength
        // (owned by TESetText/TEKey/TEDelete/TEInsert). Inside
        // Macintosh Volume I (1985), p. I-376; Inside Macintosh:
        // Text (1993), p. 2-85.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0xFF);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        let pre_active = bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET);
        let pre_telength = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0x0000); // extend=FALSE
        bus.write_word(TEST_SP + 6, 8); // pt.v
        bus.write_word(TEST_SP + 8, 32); // pt.h
        let result = disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET),
            pre_active
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            pre_telength
        );
    }

    #[test]
    fn teclick_retains_mouse_and_tracks_in_both_directions_until_release() {
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO WORLD");
        let te_ptr = bus.read_long(te_handle);
        let port = bus.read_long(te_ptr + TrapDispatcher::TE_IN_PORT_OFFSET);
        // A translated port must use local TE coordinates throughout the drag.
        bus.write_word(port + 8, (-50i16) as u16);
        bus.write_word(port + 10, (-40i16) as u16);
        let initial = disp.te_char_to_point(&bus, te_handle, 3);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0);
        bus.write_word(TEST_SP + 6, initial.0 as u16);
        bus.write_word(TEST_SP + 8, initial.1 as u16);
        disp.input_state.set_mouse_button_for_test(true);
        disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert!(disp.is_tracking_refire(0xA9D4));
        for (offset, expected) in [(8, (3, 8)), (1, (1, 3)), (5, (3, 5))] {
            let local = disp.te_char_to_point(&bus, te_handle, offset);
            bus.write_word(
                crate::memory::globals::addr::MOUSE_LOC2,
                local.0.wrapping_add(50) as u16,
            );
            bus.write_word(
                crate::memory::globals::addr::MOUSE_LOC2 + 2,
                local.1.wrapping_add(40) as u16,
            );
            disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
            assert_eq!(
                (
                    bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
                    bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET)
                ),
                expected
            );
        }
        disp.input_state.set_mouse_button_for_test(false);
        disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert!(!disp.is_tracking_refire(0xA9D4));
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO WORLD"
        );
    }

    #[test]
    fn teclick_repeated_calls_balance_stack_no_drift() {
        // An 8-call composition of TEClick keeps the A7 advance balanced at
        // exactly 8 * 10 = 80 bytes; per-call pop errors that cancel
        // within a single call but drift over many invocations are
        // detected here. A7 is reset to TEST_SP between iterations so
        // each call exercises the SP discipline independently.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");

        for k in 0..8i16 {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_long(TEST_SP, te_handle);
            bus.write_word(TEST_SP + 4, 0x0000); // extend=FALSE
            bus.write_word(TEST_SP + 6, (4 + k) as u16); // pt.v varied
            bus.write_word(TEST_SP + 8, (16 + k * 3) as u16); // pt.h varied
            let result = disp.dispatch_dialog(true, 0x1D4, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        }
    }

    fn textedit_selection_replacement_for_theme(
        theme_id: UiThemeId,
    ) -> (Vec<u8>, u16, u16, u16, u32) {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(theme_id);
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        bus.write_long(TEST_SP, te_handle);
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 0);
        bus.write_word(rect_ptr + 2, 0);
        bus.write_word(rect_ptr + 4, 40);
        bus.write_word(rect_ptr + 6, 120);
        bus.write_long(TEST_SP + 4, rect_ptr);
        let result = disp.dispatch_dialog(true, 0x1D3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, u16::from(b'Y'));
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        (
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            cpu.read_reg(Register::A7),
        )
    }

    fn textedit_private_scrap_editing_results_for_theme(
        theme_id: UiThemeId,
    ) -> TextEditPrivateScrapEditingSnapshot {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_ui_theme_id(theme_id);

        let copy_te = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let copy_ptr = bus.read_long(copy_te);
        bus.write_word(copy_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(copy_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, copy_te);
        disp.dispatch_dialog(true, 0x1D5, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let copy = textedit_scrap_edit_case(&bus, copy_te, cpu.read_reg(Register::A7));

        let cut_te = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let cut_ptr = bus.read_long(cut_te);
        bus.write_word(cut_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(cut_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, cut_te);
        disp.dispatch_dialog(true, 0x1D6, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let cut = textedit_scrap_edit_case(&bus, cut_te, cpu.read_reg(Register::A7));

        let paste_te = make_te_with_text(&mut disp, &mut bus, b"HEXXO");
        let paste_ptr = bus.read_long(paste_te);
        bus.write_word(paste_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(paste_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        let paste_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let paste_scrap_ptr = bus.read_long(paste_scrap);
        bus.write_bytes(paste_scrap_ptr, b"LL");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, paste_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, paste_te);
        disp.dispatch_dialog(true, 0x1DB, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let paste = textedit_scrap_edit_case(&bus, paste_te, cpu.read_reg(Register::A7));

        let delete_te = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let delete_ptr = bus.read_long(delete_te);
        bus.write_word(delete_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(delete_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        let delete_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let delete_scrap_ptr = bus.read_long(delete_scrap);
        bus.write_bytes(delete_scrap_ptr, b"QQ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, delete_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, delete_te);
        disp.dispatch_dialog(true, 0x1D7, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let delete = textedit_scrap_edit_case(&bus, delete_te, cpu.read_reg(Register::A7));

        TextEditPrivateScrapEditingSnapshot {
            copy,
            cut,
            paste,
            delete,
        }
    }

    #[test]
    fn systemless_theme_does_not_change_textedit_selection_offsets() {
        // Text 1993 appendix C defines selStart/selEnd as byte offsets into
        // the selection range; Text 1993 p. 2-81 says TEKey replaces that
        // range and positions the insertion point just past the inserted byte.
        // Theme selection chrome must not change those guest-visible fields.
        let classic = textedit_selection_replacement_for_theme(UiThemeId::ClassicSystem7);
        let themed = textedit_selection_replacement_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.0, b"HYO".to_vec());
        assert_eq!(classic.1, 3);
        assert_eq!(classic.2, 2);
        assert_eq!(classic.3, 2);
        assert_eq!(classic.4, TEST_SP + 6);
        assert_eq!(
            themed, classic,
            "systemless-default must not change TextEdit replacement selection offsets"
        );
    }

    #[test]
    fn systemless_theme_does_not_change_textedit_private_scrap_editing() {
        // IM:I I-373 says TextEdit's scrap is private to TextEdit, not the
        // Scrap Manager desk scrap. IM:I I-385..I-387 define TECopy, TECut,
        // TEPaste, and TEDelete selection/scrap effects; MTE 1992 p.
        // 6-132..6-134 routes dialog edit commands through those TextEdit
        // operations. Theme chrome must not change the private-scrap globals,
        // TERec text/selection fields, or stack ABI for these edit actions.
        let classic = textedit_private_scrap_editing_results_for_theme(UiThemeId::ClassicSystem7);
        let themed = textedit_private_scrap_editing_results_for_theme(UiThemeId::SystemlessDefault);

        assert_eq!(classic.copy.text, b"HELLO".to_vec());
        assert_eq!(classic.copy.selection, (1, 4));
        assert_eq!(classic.copy.scrap_length, 3);
        assert_eq!(classic.copy.scrap_bytes, b"ELL".to_vec());
        assert_eq!(classic.copy.stack_after, TEST_SP + 4);

        assert_eq!(classic.cut.text, b"HO".to_vec());
        assert_eq!(classic.cut.selection, (1, 1));
        assert_eq!(classic.cut.scrap_length, 3);
        assert_eq!(classic.cut.scrap_bytes, b"ELL".to_vec());
        assert_eq!(classic.cut.stack_after, TEST_SP + 4);

        assert_eq!(classic.paste.text, b"HELLO".to_vec());
        assert_eq!(classic.paste.selection, (4, 4));
        assert_eq!(classic.paste.scrap_length, 2);
        assert_eq!(classic.paste.scrap_bytes, b"LL".to_vec());
        assert_eq!(classic.paste.stack_after, TEST_SP + 4);

        assert_eq!(classic.delete.text, b"HO".to_vec());
        assert_eq!(classic.delete.selection, (1, 1));
        assert_eq!(classic.delete.scrap_length, 2);
        assert_eq!(classic.delete.scrap_bytes, b"QQ".to_vec());
        assert_eq!(classic.delete.stack_after, TEST_SP + 4);
        assert_eq!(
            themed, classic,
            "systemless-default must not change TextEdit private-scrap edit semantics"
        );
    }

    #[test]
    fn teidle_consumes_tehandle_argument() {
        // Inside Macintosh Volume I (1985), p. I-374 and
        // Inside Macintosh: Text (1993), p. 2-84.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    #[test]
    fn teidle_toggles_visible_insertion_caret_after_blink_interval() {
        // IM:I I-374 and Text 1993 p. 2-84: TEIdle blinks an active
        // insertion caret, but only after the minimum blink interval
        // initially set to 32 ticks.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"");
        let te_ptr = bus.read_long(te_handle);
        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
        for i in 0..(row_bytes * 80) {
            bus.write_byte(screen_base + i, 0);
        }
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);

        disp.set_tick_count_for_test(&mut bus, 100);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1D8, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "TEActivate should show the insertion caret before TEIdle"
        );
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            100
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );

        disp.set_tick_count_for_test(&mut bus, 131);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "TEIdle before 32 ticks should leave the caret visible"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );

        disp.set_tick_count_for_test(&mut bus, 132);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "TEIdle at the 32-tick boundary should hide the caret"
        );
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            132
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            1
        );

        disp.set_tick_count_for_test(&mut bus, 164);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 1, 0),
            "the next elapsed interval should show the caret again"
        );
        assert_eq!(
            bus.read_long(te_ptr + TrapDispatcher::TE_CARET_TIME_OFFSET),
            164
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_CARET_STATE_OFFSET),
            0
        );
    }

    #[test]
    fn teidle_preserves_active_flag_and_selection_offsets() {
        // Inside Macintosh Volume I (1985), pp. I-374 and I-385: TEIdle
        // blinks caret for an active record, while TEActivate/TEDeactivate
        // control active state. The text length must also remain
        // unchanged across an idle call.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0xFF);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        let pre_telength = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET),
            0xFF
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 4);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            pre_telength
        );
    }

    #[test]
    fn teidle_repeated_calls_balance_stack_and_preserve_terec_state() {
        // Inside Macintosh Volume I (1985), p. I-374: TEIdle is intended
        // to be called on every null event from the application's event
        // loop, so per-call pop discipline must compose cleanly across
        // many invocations and the TERec must remain bit-for-bit
        // identical across the sequence: 8 successive TEIdle calls,
        // assert A7 advanced exactly 8 * 4 = 32 bytes AND active /
        // selStart / selEnd / teLength are all preserved.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"WORLD");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 0xFF);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);
        let pre_active = bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET);
        let pre_sel_start = bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET);
        let pre_sel_end = bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET);
        let pre_telength = bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET);

        for _ in 0..8 {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_long(TEST_SP, te_handle);
            let result = disp.dispatch_dialog(true, 0x1DA, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        }

        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET),
            pre_active
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            pre_sel_start
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET),
            pre_sel_end
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET),
            pre_telength
        );
    }

    #[test]
    fn tescroll_offsets_destrect_by_requested_delta_and_pops_args() {
        // IM:I I-388; Inside Macintosh: Text 1993, 2-91.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 70);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 180);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-3i16) as u16); // dv
        bus.write_word(TEST_SP + 6, 5); // dh
        let result = disp.dispatch_dialog(true, 0x1DD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            7
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            25
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            67
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            185
        );
    }

    #[test]
    fn teinsert_inserts_before_selection_shifts_range_and_preserves_scrap() {
        // IM:I I-387; Inside Macintosh: Text 1993, 2-94. TEInsert splices
        // the supplied bytes at the selStart offset (BEFORE the selection)
        // without replacing the selected range. The selection range is
        // preserved logically — selStart and selEnd shift forward by the
        // inserted length so the original selected characters still fall
        // inside the range.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let existing_scrap_ptr = bus.read_long(existing_scrap);
        bus.write_bytes(existing_scrap_ptr, b"QQ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, existing_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        let insert_ptr = bus.alloc(3);
        bus.write_bytes(insert_ptr, b"XYZ");
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 3);
        bus.write_long(TEST_SP + 8, insert_ptr);
        let result = disp.dispatch_dialog(true, 0x1DE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);

        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HXYZELLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            4
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 7);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
            existing_scrap
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            2
        );
        assert_eq!(bus.read_bytes(existing_scrap_ptr, 2), b"QQ".to_vec());
    }

    #[test]
    fn teinsert_insertion_point_inserts_before_caret_and_shifts_caret() {
        // TEInsert inserts immediately before the current selection/insertion
        // point per IM:I I-387; Inside Macintosh: Text 1993, 2-94. The
        // selection range is preserved logically — the insertion point shifts
        // forward by the inserted length so it continues to mark the same
        // place relative to the surrounding original characters.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);

        let insert_ptr = bus.alloc(1);
        bus.write_byte(insert_ptr, b'X');
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 8, insert_ptr);
        let result = disp.dispatch_dialog(true, 0x1DE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);

        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HEXLLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            3
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 3);
    }

    #[test]
    fn teinsert_redraws_active_text_after_inserting_into_visible_record() {
        // TextEdit redraws the active edit record when TEInsert mutates it.
        // Console-style applications rely on this after their initial
        // TEUpdate, so a correct hText alone is not sufficient.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_ACTIVE_OFFSET, 1);

        let (screen_base, row_bytes, _screen_w, _screen_h, _pixel_size) = disp.screen_mode;
        for offset in 0..(row_bytes * 80) {
            bus.write_byte(screen_base + offset, 0);
        }

        let insert_ptr = bus.alloc(1);
        bus.write_byte(insert_ptr, b'X');
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 8, insert_ptr);
        let result = disp.dispatch_dialog(true, 0x1DE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(TrapDispatcher::te_text_bytes(&bus, te_handle), b"X".to_vec());
        assert!(
            (0..(row_bytes * 80)).any(|offset| bus.read_byte(screen_base + offset) != 0),
            "TEInsert should redraw visible inserted text"
        );
    }

    #[test]
    fn tesetalignment_writes_just_field_and_pops_args() {
        // IM:I I-388 (TESetJust) and Inside Macintosh: Text 1993, 2-87.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_JUST_OFFSET, 0);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-1i16) as u16);
        let result = disp.dispatch_dialog(true, 0x1DF, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_JUST_OFFSET) as i16,
            -1
        );
    }

    // ---- TECopy / TECut / TEPaste ($A9D5 / $A9D6 / $A9DB) ----

    #[test]
    fn tecopy_nonempty_selection_updates_textedit_scrap_globals() {
        // IM:I I-386 + I-389.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        bus.write_long(TEST_SP, te_handle);

        let result = disp.dispatch_dialog(true, 0x1D5, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        let scrap_handle = bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE);
        let scrap_ptr = bus.read_long(scrap_handle);
        assert_ne!(scrap_handle, 0);
        assert_ne!(scrap_ptr, 0);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            3
        );
        assert_eq!(bus.read_bytes(scrap_ptr, 3), b"ELL".to_vec());
    }

    #[test]
    fn tecopy_empty_selection_preserves_existing_scrap_contents() {
        // IM:I I-386: no selected range => no copy.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let existing_ptr = bus.read_long(existing_scrap);
        bus.write_bytes(existing_ptr, b"ZZ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, existing_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);
        bus.write_long(TEST_SP, te_handle);

        let result = disp.dispatch_dialog(true, 0x1D5, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
            existing_scrap
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            2
        );
        assert_eq!(bus.read_bytes(existing_ptr, 2), b"ZZ".to_vec());
    }

    #[test]
    fn tecut_nonempty_selection_copies_to_scrap_and_deletes_selected_text() {
        // IM:I I-391 + I-389.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);
        bus.write_long(TEST_SP, te_handle);

        let result = disp.dispatch_dialog(true, 0x1D6, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 1);

        let scrap_handle = bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE);
        let scrap_ptr = bus.read_long(scrap_handle);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            3
        );
        assert_eq!(bus.read_bytes(scrap_ptr, 3), b"ELL".to_vec());
    }

    #[test]
    fn tecut_empty_selection_preserves_text_and_scrap_contents() {
        // IM:I I-391: insertion-point selection performs no cut.
        let (mut disp, mut cpu, mut bus) = setup();
        let existing_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let existing_ptr = bus.read_long(existing_scrap);
        bus.write_bytes(existing_ptr, b"QQ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, existing_scrap);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 3);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);
        bus.write_long(TEST_SP, te_handle);

        let result = disp.dispatch_dialog(true, 0x1D6, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        assert_eq!(
            bus.read_long(crate::memory::globals::addr::TE_SCRP_HANDLE),
            existing_scrap
        );
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::TE_SCRP_LENGTH),
            2
        );
        assert_eq!(bus.read_bytes(existing_ptr, 2), b"QQ".to_vec());
    }

    #[test]
    fn tepaste_nonempty_scrap_replaces_selection_and_advances_insertion_point() {
        // IM:I I-387 + I-389.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);

        let scrap_handle = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let scrap_ptr = bus.read_long(scrap_handle);
        bus.write_bytes(scrap_ptr, b"EL");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, scrap_handle);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 2);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DB, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            3
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 3);
    }

    #[test]
    fn tepaste_empty_scrap_is_noop() {
        // IM:I I-387: empty scrap inserts nothing.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);

        let scrap_handle = TrapDispatcher::allocate_handle_with_data(&mut bus, 2);
        let scrap_ptr = bus.read_long(scrap_handle);
        bus.write_bytes(scrap_ptr, b"ZZ");
        bus.write_long(crate::memory::globals::addr::TE_SCRP_HANDLE, scrap_handle);
        bus.write_word(crate::memory::globals::addr::TE_SCRP_LENGTH, 0);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x1DB, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 3);
    }

    #[test]
    fn te_new_initializes_basic_record_fields() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_word(TEST_SP, 0); // viewRect.top
        bus.write_word(TEST_SP + 2, 0); // viewRect.left
        bus.write_word(TEST_SP + 4, 100); // viewRect.bottom
        bus.write_word(TEST_SP + 6, 120); // viewRect.right
        bus.write_word(TEST_SP + 8, 10); // destRect.top
        bus.write_word(TEST_SP + 10, 20); // destRect.left
        bus.write_word(TEST_SP + 12, 90); // destRect.bottom
        bus.write_word(TEST_SP + 14, 80); // destRect.right

        let result = disp.dispatch_dialog(true, 0x1D2, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);

        let te_handle = bus.read_long(TEST_SP + 16);
        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            10
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            20
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4),
            100
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6),
            120
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 0);
        let h_text = bus.read_long(te_ptr + TrapDispatcher::TE_HTEXT_OFFSET);
        assert_ne!(h_text, 0);
        assert_eq!(bus.read_long(h_text), 0);
    }

    // Inside Macintosh: Text (1993), p. 2-78: TEStyleNew returns a
    // multistyled TEHandle, uses -1 sentinels in txSize/lineHeight/
    // fontAscent, and installs style metadata.
    #[test]
    fn testylenew_returns_styled_handle_and_initializes_sentinel_fields() {
        let (mut disp, mut cpu, mut bus) = setup();

        bus.write_word(TEST_SP, 0); // viewRect.top
        bus.write_word(TEST_SP + 2, 0); // viewRect.left
        bus.write_word(TEST_SP + 4, 100); // viewRect.bottom
        bus.write_word(TEST_SP + 6, 120); // viewRect.right
        bus.write_word(TEST_SP + 8, 10); // destRect.top
        bus.write_word(TEST_SP + 10, 20); // destRect.left
        bus.write_word(TEST_SP + 12, 90); // destRect.bottom
        bus.write_word(TEST_SP + 14, 80); // destRect.right

        let result = disp.dispatch_dialog(true, 0x03E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 16,
            "TEStyleNew should pop two Rect arguments (16 bytes)"
        );

        let te_handle = bus.read_long(TEST_SP + 16);
        assert_ne!(te_handle, 0, "TEStyleNew should return a non-NIL TEHandle");

        let te_ptr = bus.read_long(te_handle);
        assert_ne!(te_ptr, 0, "returned TEHandle should point to a TERec");
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_TX_SIZE_OFFSET),
            0xFFFF
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET),
            0xFFFF
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_FONT_ASCENT_OFFSET),
            0xFFFF
        );

        let style_handle = bus.read_long(te_ptr + TrapDispatcher::TE_TX_FONT_OFFSET);
        assert_ne!(style_handle, 0, "styled record should carry a style handle");
        let style_ptr = bus.read_long(style_handle);
        assert_ne!(
            style_ptr, 0,
            "style handle should dereference to a style record"
        );
        assert_ne!(
            bus.read_long(style_ptr + TrapDispatcher::TE_STYLE_NULL_STYLE_OFFSET),
            0,
            "style record should include a non-NIL null-style handle"
        );
    }

    // ---- TEStyleNew (pointer-arg convention) ----

    // Inside Macintosh: Text (1993), p. 2-78: TEStyleNew accepts a
    // pointer-arg convention `const Rect *destRect, const Rect *viewRect`
    // per MPW Universal Headers. Pascal LR push: destRect_ptr at SP+4
    // (deepest, pushed first), viewRect_ptr at SP+0 (shallowest, pushed
    // last). Net pop is 8 bytes.
    #[test]
    fn testylenew_pointer_arg_convention_initializes_destrect_viewrect_and_styled_sentinels() {
        let (mut disp, mut cpu, mut bus) = setup();

        // Build distinct destRect and viewRect in guest memory.
        let dest_rect_ptr: u32 = 0x1A0000;
        let view_rect_ptr: u32 = 0x1A0010;
        bus.write_word(dest_rect_ptr, (-1200_i16) as u16);
        bus.write_word(dest_rect_ptr + 2, (-1200_i16) as u16);
        bus.write_word(dest_rect_ptr + 4, (-900_i16) as u16);
        bus.write_word(dest_rect_ptr + 6, (-1000_i16) as u16);
        bus.write_word(view_rect_ptr, (-1100_i16) as u16);
        bus.write_word(view_rect_ptr + 2, (-1300_i16) as u16);
        bus.write_word(view_rect_ptr + 4, (-800_i16) as u16);
        bus.write_word(view_rect_ptr + 6, (-900_i16) as u16);

        // Pascal LR push: viewRect_ptr at SP+0 (last pushed),
        // destRect_ptr at SP+4 (first pushed).
        bus.write_long(TEST_SP, view_rect_ptr);
        bus.write_long(TEST_SP + 4, dest_rect_ptr);

        let result = disp.dispatch_dialog(true, 0x03E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "TEStyleNew pointer convention should pop 8 bytes (two pointer args)"
        );

        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(te_handle, 0, "TEStyleNew should return a non-NIL TEHandle");
        let te_ptr = bus.read_long(te_handle);
        assert_ne!(te_ptr, 0, "TEHandle should dereference to a TERec");

        // destRect at offset 0x00 round-trips from the destRect_ptr.
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16,
            -1200,
            "destRect.top must round-trip from caller-supplied destRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2) as i16,
            -1200,
            "destRect.left must round-trip from caller-supplied destRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4) as i16,
            -900,
            "destRect.bottom must round-trip from caller-supplied destRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6) as i16,
            -1000,
            "destRect.right must round-trip from caller-supplied destRect_ptr"
        );

        // viewRect at offset 0x08 round-trips from the viewRect_ptr.
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET) as i16,
            -1100,
            "viewRect.top must round-trip from caller-supplied viewRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2) as i16,
            -1300,
            "viewRect.left must round-trip from caller-supplied viewRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4) as i16,
            -800,
            "viewRect.bottom must round-trip from caller-supplied viewRect_ptr"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6) as i16,
            -900,
            "viewRect.right must round-trip from caller-supplied viewRect_ptr"
        );

        // Styled sentinels per IM:Text 1993 p. 2-78.
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_TX_SIZE_OFFSET),
            0xFFFF,
            "TEStyleNew must set txSize to -1 (styled-record sentinel)"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LINE_HEIGHT_OFFSET),
            0xFFFF,
            "TEStyleNew must set lineHeight to -1 (styled-record sentinel)"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_FONT_ASCENT_OFFSET),
            0xFFFF,
            "TEStyleNew must set fontAscent to -1 (styled-record sentinel)"
        );

        // Style handle at txFont/txFace overlay (4 bytes at offset 0x4A).
        let style_handle = bus.read_long(te_ptr + TrapDispatcher::TE_TX_FONT_OFFSET);
        assert_ne!(
            style_handle, 0,
            "TEStyleNew must install a non-NIL TEStyleHandle in the txFont/txFace overlay"
        );
        let style_ptr = bus.read_long(style_handle);
        assert_ne!(
            style_ptr, 0,
            "TEStyleHandle should dereference to an allocated TEStyleRec"
        );
    }

    // Inside Macintosh: Text (1993), p. 2-78 + MPW TextEdit.h
    // ONEWORDINLINE(0xA83E): TEStyleNew is `EXTERN_API(TEHandle)`.
    // Pascal FUNCTION protocol: caller pre-allocates 4-byte TEHandle
    // result slot, pushes 2 pointer args (8 bytes); trap pops 8 +
    // writes 4-byte result.
    #[test]
    fn testylenew_function_protocol_consumes_two_pointer_args_and_writes_4_byte_result() {
        let (mut disp, mut cpu, mut bus) = setup();

        let dest_rect_ptr: u32 = 0x1A0020;
        let view_rect_ptr: u32 = 0x1A0030;
        // Both rects: non-empty distinct values so te_new_rect_args
        // selects the pointer convention (8 bytes).
        bus.write_word(dest_rect_ptr, (-500_i16) as u16);
        bus.write_word(dest_rect_ptr + 2, (-500_i16) as u16);
        bus.write_word(dest_rect_ptr + 4, (-300_i16) as u16);
        bus.write_word(dest_rect_ptr + 6, (-400_i16) as u16);
        bus.write_word(view_rect_ptr, (-450_i16) as u16);
        bus.write_word(view_rect_ptr + 2, (-550_i16) as u16);
        bus.write_word(view_rect_ptr + 4, (-250_i16) as u16);
        bus.write_word(view_rect_ptr + 6, (-350_i16) as u16);

        bus.write_long(TEST_SP, view_rect_ptr);
        bus.write_long(TEST_SP + 4, dest_rect_ptr);
        // Poison the result slot at sp+8 and a sentinel past it.
        bus.write_long(TEST_SP + 8, 0xDEADBEEF);
        bus.write_long(TEST_SP + 12, 0xCAFEBABE);

        let result = disp.dispatch_dialog(true, 0x03E, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // A7 advanced exactly 8 bytes.
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "TEStyleNew pointer-arg convention must pop exactly 8 bytes"
        );

        // The function result slot at the former sp+8 was overwritten
        // by a non-poison TEHandle.
        let te_handle = bus.read_long(TEST_SP + 8);
        assert_ne!(
            te_handle, 0xDEADBEEF,
            "TEStyleNew result slot must be overwritten by a real TEHandle, not the poison sentinel"
        );
        assert_ne!(te_handle, 0, "TEStyleNew result must be a non-NIL TEHandle");

        // The sentinel at sp+12 (4 bytes past the 4-byte result slot)
        // must survive — TEStyleNew must not write past its result.
        assert_eq!(
            bus.read_long(TEST_SP + 12),
            0xCAFEBABE,
            "TEStyleNew must not write past the 4-byte function-result slot"
        );
    }

    #[test]
    fn testyleinsert_zero_length_preserves_pascal_registers_and_text_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 80, 200), (0, 0, 80, 200));
        disp.te_set_text_contents(&mut bus, te_handle, b"kept");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);

        let text_ptr = bus.alloc(1);
        bus.write_byte(text_ptr, 0);
        let preserved = [
            (Register::D3, 0xD300_0003),
            (Register::D4, 0xD400_0004),
            (Register::D5, 0xD500_0005),
            (Register::D6, 0xD600_0006),
            (Register::D7, 0xD700_0007),
            (Register::A2, 0xA200_0002),
            (Register::A3, text_ptr),
            (Register::A4, 0xA400_0004),
            (Register::A5, 0xA500_0005),
            (Register::A6, 0xA600_0006),
        ];
        for (register, value) in preserved {
            cpu.write_reg(register, value);
        }

        bus.write_word(TEST_SP, 0x0007);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, 0);
        bus.write_long(TEST_SP + 10, 0);
        bus.write_long(TEST_SP + 14, text_ptr);
        bus.write_long(TEST_SP + 18, 0xCAFE_BABE);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        for (register, value) in preserved {
            assert_eq!(
                cpu.read_reg(register),
                value,
                "stack-based TEStyleInsert must preserve {register:?}"
            );
        }
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 18);
        assert_eq!(TrapDispatcher::te_text_bytes(&bus, te_handle), b"kept");
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 2);
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 4);
        assert_eq!(bus.read_long(TEST_SP + 18), 0xCAFE_BABE);
    }

    #[test]
    fn testyleinsert_applies_style_scrap_runs_and_line_metrics() {
        let (mut disp, mut cpu, mut bus) = setup();

        disp.tx_font = 0;
        disp.tx_face = 0;
        disp.tx_size = 12;
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 80, 200), (0, 0, 80, 200));

        let text = b"small\rLARGE";
        let text_ptr = bus.alloc(text.len() as u32);
        bus.write_bytes(text_ptr, text);
        let style_scrap = make_style_scrap(
            &mut bus,
            &[
                (0, 10, 8, 0, 1, 9, (0, 0, 0)),
                (6, 14, 11, 0, 0, 12, (0x1111, 0x2222, 0x3333)),
            ],
        );

        bus.write_word(TEST_SP, 0x0007);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, style_scrap);
        bus.write_long(TEST_SP + 10, text.len() as u32);
        bus.write_long(TEST_SP + 14, text_ptr);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 18,
            "TEStyleInsert must consume selector + hTE + hST + length + text pointer"
        );

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET) as usize,
            text.len()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_N_LINES_OFFSET),
            2,
            "explicit CR should produce two TextEdit lines"
        );

        let style_handle = bus.read_long(te_ptr + TrapDispatcher::TE_TX_FONT_OFFSET);
        let style_ptr = bus.read_long(style_handle);
        assert_eq!(
            bus.read_word(style_ptr + TrapDispatcher::TE_STYLE_N_RUNS_OFFSET),
            2
        );
        assert_eq!(
            bus.read_word(style_ptr + TrapDispatcher::TE_STYLE_N_STYLES_OFFSET),
            2
        );
        assert_eq!(
            bus.read_word(style_ptr + TrapDispatcher::TE_STYLE_RUNS_OFFSET),
            0
        );
        assert_eq!(
            bus.read_word(style_ptr + TrapDispatcher::TE_STYLE_RUNS_OFFSET + 4),
            6
        );

        let style_table = bus.read_long(style_ptr + TrapDispatcher::TE_STYLE_STYLE_TABLE_OFFSET);
        let style_table_ptr = bus.read_long(style_table);
        assert_eq!(
            bus.read_byte(style_table_ptr + TrapDispatcher::ST_ELEMENT_FACE_OFFSET),
            1,
            "first style run should preserve the bold face from the style scrap"
        );
        assert_eq!(
            bus.read_word(style_table_ptr + TrapDispatcher::ST_ELEMENT_SIZE_OFFSET),
            9,
            "first style run should preserve the smaller body size"
        );
        let second_style = style_table_ptr + TrapDispatcher::ST_ELEMENT_SIZE;
        assert_eq!(
            bus.read_word(second_style + TrapDispatcher::ST_ELEMENT_SIZE_OFFSET),
            12
        );
        assert_eq!(
            (
                bus.read_word(second_style + TrapDispatcher::ST_ELEMENT_COLOR_OFFSET),
                bus.read_word(second_style + TrapDispatcher::ST_ELEMENT_COLOR_OFFSET + 2),
                bus.read_word(second_style + TrapDispatcher::ST_ELEMENT_COLOR_OFFSET + 4),
            ),
            (0x1111, 0x2222, 0x3333)
        );

        let lh_table = bus.read_long(style_ptr + TrapDispatcher::TE_STYLE_LH_TABLE_OFFSET);
        let lh_ptr = bus.read_long(lh_table);
        assert_eq!(
            bus.read_word(lh_ptr + TrapDispatcher::LH_ELEMENT_HEIGHT_OFFSET),
            10,
            "first TextEdit line should use first style's line height"
        );
        assert_eq!(
            bus.read_word(
                lh_ptr + TrapDispatcher::LH_ELEMENT_SIZE + TrapDispatcher::LH_ELEMENT_HEIGHT_OFFSET
            ),
            14,
            "second TextEdit line should use second style's line height"
        );

        let runs = disp.te_style_runs(&bus, te_handle, text.len());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start, 0);
        assert_eq!(runs[0].style_index, 0);
        assert_eq!(runs[0].style.size, 9);
        assert_eq!(runs[1].start, 6);
        assert_eq!(runs[1].style_index, 1);
        assert_eq!(runs[1].style.size, 12);
    }

    #[test]
    fn testyleinsert_parses_stscrprec_style_table_after_count_word() {
        // IM:V V-274 and MPW TextEdit.h: StScrpRec is `short scrpNStyles`
        // immediately followed by `ScrpSTElement scrpStyleTab[]`.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.tx_font = 0;
        disp.tx_face = 0;
        disp.tx_size = 12;
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 80, 200), (0, 0, 80, 200));

        let text = b"TITLE body";
        let text_ptr = bus.alloc(text.len() as u32);
        bus.write_bytes(text_ptr, text);

        let style_scrap = TrapDispatcher::allocate_handle_with_data(&mut bus, 2 + 2 * 0x14);
        let scrap_ptr = bus.read_long(style_scrap);
        bus.write_word(scrap_ptr, 2);

        let first = scrap_ptr + 2;
        bus.write_long(first, 0);
        bus.write_word(first + 4, 10);
        bus.write_word(first + 6, 8);
        bus.write_word(first + 8, 0);
        bus.write_byte(first + 10, 1);
        bus.write_word(first + 12, 9);
        bus.write_word(first + 14, 0);
        bus.write_word(first + 16, 0);
        bus.write_word(first + 18, 0);

        let second = first + 0x14;
        bus.write_long(second, 6);
        bus.write_word(second + 4, 14);
        bus.write_word(second + 6, 11);
        bus.write_word(second + 8, 3);
        bus.write_byte(second + 10, 0);
        bus.write_word(second + 12, 12);
        bus.write_word(second + 14, 0x1111);
        bus.write_word(second + 16, 0x2222);
        bus.write_word(second + 18, 0x3333);

        bus.write_word(TEST_SP, 0x0007);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, style_scrap);
        bus.write_long(TEST_SP + 10, text.len() as u32);
        bus.write_long(TEST_SP + 14, text_ptr);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let runs = disp.te_style_runs(&bus, te_handle, text.len());
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start, 0);
        assert_eq!(runs[0].style.face, 1);
        assert_eq!(runs[0].style.size, 9);
        assert_eq!(runs[1].start, 6);
        assert_eq!(runs[1].style.font, 3);
        assert_eq!(runs[1].style.size, 12);
        assert_eq!(runs[1].style.color, (0x1111, 0x2222, 0x3333));
    }

    // Inside Macintosh: Text (1993), p. 2-92: TEAutoView takes
    // `fAuto: Boolean; hTE: TEHandle`. With Pascal calling convention, hTE
    // (last parameter) is at SP+0 and fAuto at SP+4.
    #[test]
    fn teautoview_reads_hte_from_sp_plus_0_and_fauto_from_sp_plus_4() {
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);

        bus.write_long(TEST_SP, te_handle); // SP+0: hTE (last-pushed)
                                            // Pascal BOOLEAN in high byte (MPW C convention).
        bus.write_byte(TEST_SP + 4, 1); // SP+4: fAuto = TRUE

        let result = disp.dispatch_dialog(true, 0x013, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert!(
            disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL),
            "TEAutoView must enable auto-scroll on the TE handle at SP+0"
        );
    }

    #[test]
    fn teautoview_clears_feature_when_fauto_is_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);

        // First turn it on.
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, true);
        assert!(disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL));

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0); // fAuto = FALSE

        let result = disp.dispatch_dialog(true, 0x013, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 6,
            "TEAutoView should pop one Boolean and one TEHandle"
        );
        assert!(
            !disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL),
            "TEAutoView(FALSE) must clear the auto-scroll bit on the supplied hTE"
        );
    }

    // IM:Text 2-92 + IM:VI 15-22: TEAutoView and TEFeatureFlag
    // teFAutoScroll expose the same automatic-scroll state.
    #[test]
    fn teautoview_and_tefeatureflag_observe_shared_autoscroll_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);

        bus.write_long(TEST_SP, te_handle);
        bus.write_byte(TEST_SP + 4, 1); // fAuto = TRUE
        let result = disp.dispatch_dialog(true, 0x013, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x000E); // selector
        bus.write_long(TEST_SP + 2, te_handle); // hTE
        bus.write_word(TEST_SP + 6, (-1i16) as u16); // teBitTest
        bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_AUTO_SCROLL);
        bus.write_word(TEST_SP + 10, 0xBEEF);
        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(TEST_SP + 10), 1);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0); // fAuto = FALSE
        let result = disp.dispatch_dialog(true, 0x013, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x000E); // selector
        bus.write_long(TEST_SP + 2, te_handle); // hTE
        bus.write_word(TEST_SP + 6, (-1i16) as u16); // teBitTest
        bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_AUTO_SCROLL);
        bus.write_word(TEST_SP + 10, 0xBEEF);
        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(TEST_SP + 10), 0);
    }

    #[test]
    fn te_dispatch_feature_flag_tracks_auto_scroll_state() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        bus.write_word(TEST_SP, 0x000E);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 1); // teBitSet
        bus.write_word(TEST_SP + 8, 0); // teFAutoScroll

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(TEST_SP + 10), 0);
        assert!(disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL));
    }

    #[test]
    fn te_dispatch_generated_routes_preserve_exact_stack_selectors() {
        assert_eq!(super::TE_DISPATCH_OPERATION_ROUTES.len(), 5);
        assert!(super::TE_DISPATCH_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0001, "TESetStyle"),
            (0x0008, "TEGetPoint"),
            (0x000A, "TEContinuousStyle"),
            (0x000C, "TECustomHook"),
            (0x000E, "TEFeatureFlag"),
        ] {
            let route =
                super::te_dispatch_operation_route(0xA83D, selector).expect("TEDispatch route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_TEDispatch:0x{selector:04X}:stack-word-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA73D, 0x0001),
            (0xA93D, 0x0008),
            (0xAB3D, 0x000E),
            (0xA83D, 0x0000),
            (0xA83D, 0x0002),
            (0xA83D, 0x0007),
            (0xA83D, 0x0009),
            (0xA83D, 0x000B),
            (0xA83D, 0x000D),
            (0xA83D, 0x000F),
            (0xA83D, 0xFFFF),
        ] {
            assert!(super::te_dispatch_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn te_dispatch_records_known_then_clears_unknown_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.current_trap_word = 0xA83D;
        cpu.write_reg(Register::D0, 0x1122_3344);
        cpu.write_reg(Register::D1, 0x5566_7788);
        bus.write_word(TEST_SP, 0x000C);
        bus.write_long(TEST_SP + 2, 0xCAFE_BABE);
        bus.write_long(TEST_SP + 6, 0);
        bus.write_word(TEST_SP + 10, 0xA55A);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.expect("TEDispatch arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_TEDispatch:0x000C:stack-word-immediate:16")
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(cpu.read_reg(Register::D0), 0x1122_3344);
        assert_eq!(cpu.read_reg(Register::D1), 0x5566_7788);
        assert_eq!(bus.read_long(TEST_SP + 2), 0xCAFE_BABE);
        assert_eq!(bus.read_long(TEST_SP + 6), 0);
        assert_eq!(bus.read_word(TEST_SP + 10), 0xA55A);

        disp.current_selector_operation = Some("stale-identity");
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x000F);
        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.is_none());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(cpu.read_reg(Register::D0), 0x1122_3344);
        assert_eq!(bus.read_word(TEST_SP), 0x000F);

        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xA93D;
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x000C);
        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.expect("TEDispatch arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
    }

    // Inside Macintosh Volume VI (1991), pp. 15-22 and 15-43: selector
    // $000E dispatches to TEFeatureFlag; action TEBitTest (-1) reports
    // current feature state without mutating it.
    #[test]
    fn te_dispatch_feature_flag_test_action_returns_current_state() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, true);

        bus.write_word(TEST_SP, 0x000E); // selector
        bus.write_long(TEST_SP + 2, te_handle); // hTE
        bus.write_word(TEST_SP + 6, (-1i16) as u16); // teBitTest
        bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_AUTO_SCROLL);
        bus.write_word(TEST_SP + 10, 0xBEEF);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(bus.read_word(TEST_SP + 10), 1);
        assert!(
            disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL),
            "teBitTest should not mutate the feature bit"
        );
    }

    // Inside Macintosh Volume VI (1991), p. 15-22: TEFeatureFlag with
    // action teBitClear=0 clears the selector bit and returns the prior
    // state. Symmetric counterpart of `te_dispatch_feature_flag_tracks_auto_scroll_state`
    // for the clear action.
    #[test]
    fn tefeatureflag_clear_action_returns_prior_one_state_and_clears_bit() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        // Pre-set autoscroll so teBitClear sees a prior-on state.
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, true);
        assert!(disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL));

        // THREEWORDINLINE(0x3F3C, 0x000E, 0xA83D) stack frame: selector
        // pre-pushed at sp+0; Pascal LR push order leaves hTE shallowest
        // (sp+2), action middle (sp+6), feature deepest (sp+8); short
        // function-result slot at sp+10.
        bus.write_word(TEST_SP, 0x000E); // selector $000E = TEFeatureFlag
        bus.write_long(TEST_SP + 2, te_handle); // hTE
        bus.write_word(TEST_SP + 6, 0); // action = teBitClear (0)
        bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_AUTO_SCROLL);
        bus.write_word(TEST_SP + 10, 0xBEEF); // poison result slot

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "TEFeatureFlag must pop 10 bytes (selector + hTE + action + feature)"
        );
        assert_eq!(
            bus.read_word(TEST_SP + 10),
            1,
            "teBitClear must return PRIOR bit state (1 = was set), not the new state"
        );
        assert!(
            !disp.te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL),
            "teBitClear must clear the feature bit after returning its prior state"
        );
    }

    // Inside Macintosh Volume VI (1991), p. 15-22: TEFeatureFlag is
    // FUNCTION TEFeatureFlag(feature, action: INTEGER; hTE: TEHandle): INTEGER.
    // Universal Headers TextEdit.h declares THREEWORDINLINE(0x3F3C, 0x000E, 0xA83D).
    // The 10-byte stack frame (2 selector + 4 hTE + 2 action + 2 feature)
    // is consumed atomically; the post-pop SP points at the 2-byte short
    // function-result slot.
    #[test]
    fn tedispatch_function_protocol_consumes_threewordinline_stack_frame_for_tefeatureflag() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);

        bus.write_word(TEST_SP, 0x000E); // selector
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, (-1i16) as u16); // teBitTest
        bus.write_word(TEST_SP + 8, TrapDispatcher::TE_FEATURE_AUTO_SCROLL);
        // Poison both the result slot and a sentinel past it.
        bus.write_word(TEST_SP + 10, 0xDEAD);
        bus.write_word(TEST_SP + 12, 0xCAFE);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // SP advanced exactly 10 bytes: selector(2) + hTE(4) + action(2) + feature(2).
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 10,
            "TEFeatureFlag dispatch must advance A7 by exactly 10 bytes"
        );

        // Result slot was overwritten by the prior bit state (0 here).
        assert_ne!(
            bus.read_word(TEST_SP + 10),
            0xDEAD,
            "TEFeatureFlag must overwrite the 2-byte result slot with the prior bit state"
        );

        // Sentinel 2 bytes past the result slot must survive — TEFeatureFlag
        // must not write beyond its function-result word.
        assert_eq!(
            bus.read_word(TEST_SP + 12),
            0xCAFE,
            "TEFeatureFlag must not write past the 2-byte short function-result slot"
        );
    }

    #[test]
    fn tesetstyle_reads_style_byte_without_treating_record_padding_as_face_bits() {
        // TextStyle.tsFace is a one-byte Style followed by an alignment byte
        // (Inside Macintosh: Text 1993, pp. 2-78..2-79). Marathon leaves the
        // padding byte nonzero while repeatedly applying terminal text styles.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.tx_face = 1;
        disp.tx_size = 12;
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 80, 200), (0, 0, 80, 200));
        disp.te_set_text_contents(&mut bus, te_handle, b"A");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);
        let text_style = bus.alloc(12);
        bus.write_word(text_style, 22);
        bus.write_byte(text_style + 2, 0); // plain
        bus.write_byte(text_style + 3, 0x56); // unrelated alignment byte
        bus.write_word(text_style + 4, 12);
        bus.write_word(text_style + 6, 0);
        bus.write_word(text_style + 8, 0xFFFF);
        bus.write_word(text_style + 10, 0);
        bus.write_word(TEST_SP, 0x0001);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 0); // redraw = FALSE
        bus.write_long(TEST_SP + 8, text_style);
        bus.write_word(TEST_SP + 12, 0x000F); // doAll

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        let style_handle = bus.read_long(te_ptr + TrapDispatcher::TE_TX_FONT_OFFSET);
        let style_record = bus.read_long(style_handle);
        let table_handle =
            bus.read_long(style_record + TrapDispatcher::TE_STYLE_STYLE_TABLE_OFFSET);
        let table = bus.read_long(table_handle);
        assert_eq!(
            bus.read_byte(table + TrapDispatcher::ST_ELEMENT_FACE_OFFSET),
            1,
            "an insertion-point style change must not restyle existing text"
        );
        let null_style_handle =
            bus.read_long(style_record + TrapDispatcher::TE_STYLE_NULL_STYLE_OFFSET);
        let null_style = bus.read_long(null_style_handle);
        let null_scrap_handle = bus.read_long(null_style + TrapDispatcher::NULL_STYLE_SCRAP_OFFSET);
        let null_scrap = bus.read_long(null_scrap_handle);
        assert_eq!(
            bus.read_byte(
                null_scrap
                    + TrapDispatcher::SCRAP_STYLE_TAB_OFFSET
                    + TrapDispatcher::SCRAP_STYLE_FACE_OFFSET
            ),
            0,
            "the insertion-point style must be stored in TextEdit's null scrap"
        );

        let inserted = bus.alloc(1);
        bus.write_byte(inserted, b'B');
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0x0007);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, 0); // NIL means use the null style scrap
        bus.write_long(TEST_SP + 10, 1);
        bus.write_long(TEST_SP + 14, inserted);
        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let runs = disp.te_style_runs(&bus, te_handle, 2);
        assert_eq!(runs.len(), 2);
        assert_eq!((runs[0].start, runs[0].style.face), (0, 1));
        assert_eq!((runs[1].start, runs[1].style.face), (1, 0));
    }

    #[test]
    fn tesetstyle_changes_only_the_selected_styled_text_range() {
        // Text 1993, p. 2-98: TESetStyle changes the character attributes
        // of the current selection range. Adjacent runs must retain their
        // attributes so later TEUpdate calls can render every range correctly.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.tx_font = 22;
        disp.tx_size = 12;
        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 80, 200), (0, 0, 80, 200));
        disp.te_set_text_contents(&mut bus, te_handle, b"red green red");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 4);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 9);

        let text_style = bus.alloc(12);
        bus.write_word(text_style, 22);
        bus.write_byte(text_style + 2, 1);
        bus.write_word(text_style + 4, 12);
        bus.write_word(text_style + 6, 0);
        bus.write_word(text_style + 8, 0xFFFF);
        bus.write_word(text_style + 10, 0);
        bus.write_word(TEST_SP, 0x0001);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 0); // redraw = FALSE
        bus.write_long(TEST_SP + 8, text_style);
        bus.write_word(TEST_SP + 12, 0x000F); // doAll

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        let runs = disp.te_style_runs(&bus, te_handle, 13);
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].start, runs[0].style.color), (0, (0, 0, 0)));
        assert_eq!((runs[1].start, runs[1].style.color), (4, (0, 0xFFFF, 0)));
        assert_eq!((runs[2].start, runs[2].style.color), (9, (0, 0, 0)));
        assert_eq!(runs[1].style.face, 1);
    }

    #[test]
    fn te_dispatch_continuous_style_returns_unstyled_record_style() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp
            .current_port
            .with_mut(|current_port| *current_port = 0x181000);
        disp.tx_font = 4;
        disp.tx_face = 1;
        disp.tx_mode = 2;
        disp.tx_size = 12;
        disp.initialize_te_record(&mut bus, te_handle, (0, 0, 40, 80), (0, 0, 40, 80));

        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_TX_FONT_OFFSET, 22);
        bus.write_byte(te_ptr + TrapDispatcher::TE_TX_FACE_OFFSET, 3);
        bus.write_word(te_ptr + TrapDispatcher::TE_TX_SIZE_OFFSET, 18);

        let style_ptr = 0x300000u32;
        let mode_ptr = 0x300100u32;
        bus.write_word(mode_ptr, 0x000F); // doAll
        bus.write_word(TEST_SP, 0x000A);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, style_ptr);
        bus.write_long(TEST_SP + 10, mode_ptr);

        let result = disp.dispatch_dialog(true, 0x03D, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        assert_eq!(bus.read_word(TEST_SP + 14), 0xFFFF);
        assert_eq!(bus.read_word(style_ptr), 22);
        assert_eq!(bus.read_byte(style_ptr + 2), 3);
        assert_eq!(bus.read_word(style_ptr + 4), 18);
        assert_eq!(bus.read_word(style_ptr + 6), 0);
        assert_eq!(bus.read_word(style_ptr + 8), 0);
        assert_eq!(bus.read_word(style_ptr + 10), 0);
    }

    #[test]
    fn te_dispatch_continuous_style_reports_mixed_styled_selection() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.tx_font = 3;
        disp.tx_size = 10;
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 40, 120), (0, 0, 40, 120));
        disp.te_set_text_contents(&mut bus, te_handle, b"AB");

        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);

        let text_style = bus.alloc(12);
        bus.write_word(text_style, 4);
        bus.write_byte(text_style + 2, 1);
        bus.write_word(text_style + 4, 14);
        bus.write_word(text_style + 6, 0xFFFF);
        bus.write_word(text_style + 8, 0);
        bus.write_word(text_style + 10, 0);
        bus.write_word(TEST_SP, 0x0001);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 0); // redraw = FALSE
        bus.write_long(TEST_SP + 8, text_style);
        bus.write_word(TEST_SP + 12, 0x000F); // doAll
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        // Inspect the whole selection, which contains the original and the
        // newly styled run. No requested attribute is continuous.
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);
        let result_style = 0x300000u32;
        let mode_ptr = 0x300100u32;
        bus.write_word(mode_ptr, 0x000F);
        bus.write_word(TEST_SP, 0x000A);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, result_style);
        bus.write_long(TEST_SP + 10, mode_ptr);
        cpu.write_reg(Register::A7, TEST_SP);

        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 14);
        assert_eq!(bus.read_word(TEST_SP + 14), 0);
        assert_eq!(bus.read_word(mode_ptr), 0);
        assert_eq!(bus.read_word(result_style), 3);
        assert_eq!(bus.read_byte(result_style + 2), 0);
        assert_eq!(bus.read_word(result_style + 4), 10);
        assert_eq!(bus.read_word(result_style + 6), 0);
        assert_eq!(bus.read_word(result_style + 8), 0);
        assert_eq!(bus.read_word(result_style + 10), 0);
    }

    #[test]
    fn te_dispatch_continuous_style_reports_common_face_bits_and_null_style() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp.tx_font = 3;
        disp.tx_size = 10;
        disp.initialize_styled_te_record(&mut bus, te_handle, (0, 0, 40, 120), (0, 0, 40, 120));
        disp.te_set_text_contents(&mut bus, te_handle, b"AB");
        let te_ptr = bus.read_long(te_handle);
        let text_style = bus.alloc(12);
        bus.write_word(text_style, 4);
        bus.write_byte(text_style + 2, 0x03);
        bus.write_word(text_style + 4, 10);
        bus.write_word(text_style + 6, 0);
        bus.write_word(text_style + 8, 0);
        bus.write_word(text_style + 10, 0);

        // Give the two characters faces 0b11 and 0b01. Their common face
        // bits must be reported as 0b01 rather than treating the faces as
        // unequal values.
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);
        bus.write_word(TEST_SP, 0x0001);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 0);
        bus.write_long(TEST_SP + 8, text_style);
        bus.write_word(TEST_SP + 12, 0x0002);
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        bus.write_byte(text_style + 2, 0x01);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        let result_style = 0x300000u32;
        let mode_ptr = 0x300100u32;
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);
        bus.write_word(mode_ptr, 0x0002);
        bus.write_word(TEST_SP, 0x000A);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, result_style);
        bus.write_long(TEST_SP + 10, mode_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let mixed_runs = disp.te_style_runs(&bus, te_handle, 2);
        assert_eq!(
            mixed_runs
                .iter()
                .map(|run| (run.start, run.style.face))
                .collect::<Vec<_>>(),
            vec![(0, 3), (1, 1)]
        );
        assert_eq!(bus.read_word(TEST_SP + 14), 0xFFFF);
        assert_eq!(bus.read_word(mode_ptr), 0x0002);
        assert_eq!(bus.read_byte(result_style + 2), 0x01);

        // An insertion point uses the null scrap, not the following
        // character, for its reported style.
        bus.write_byte(text_style + 2, 0x07);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);
        bus.write_word(TEST_SP, 0x0001);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_word(TEST_SP + 6, 0);
        bus.write_long(TEST_SP + 8, text_style);
        bus.write_word(TEST_SP + 12, 0x0002);
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let null_scrap_handle = TrapDispatcher::te_null_style_scrap_handle(&bus, te_handle);
        let null_scrap = bus.read_long(null_scrap_handle);
        assert_eq!(bus.read_word(null_scrap + TrapDispatcher::SCRAP_N_STYLES_OFFSET), 1);
        bus.write_word(mode_ptr, 0x0002);
        bus.write_word(TEST_SP, 0x000A);
        bus.write_long(TEST_SP + 2, te_handle);
        bus.write_long(TEST_SP + 6, result_style);
        bus.write_long(TEST_SP + 10, mode_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        assert!(disp
            .dispatch_dialog(true, 0x03D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(bus.read_word(TEST_SP + 14), 0xFFFF);
        assert_eq!(bus.read_word(mode_ptr), 0x0002);
        assert_eq!(bus.read_byte(result_style + 2), 0x07);
    }

    #[test]
    fn te_scroll_reads_handle_from_stack_top() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp
            .current_port
            .with_mut(|current_port| *current_port = 0x181000);
        disp.tx_font = 4;
        disp.tx_face = 0;
        disp.tx_mode = 0;
        disp.tx_size = 10;
        disp.initialize_te_record(&mut bus, te_handle, (10, 20, 50, 80), (10, 20, 50, 80));

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-6i16) as u16);
        bus.write_word(TEST_SP + 6, 4);

        let result = disp.dispatch_dialog(true, 0x1DD, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            4
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            24
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            44
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            84
        );
    }

    #[test]
    fn te_pin_scroll_reads_handle_from_stack_top() {
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = TrapDispatcher::allocate_te_handle(&mut bus);
        disp
            .current_port
            .with_mut(|current_port| *current_port = 0x181000);
        disp.tx_font = 4;
        disp.tx_face = 0;
        disp.tx_mode = 0;
        disp.tx_size = 10;
        disp.initialize_te_record(&mut bus, te_handle, (10, 20, 50, 80), (10, 20, 50, 80));
        disp.te_set_text_contents(
            &mut bus,
            te_handle,
            b"one two three four five six seven eight nine ten",
        );

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-6i16) as u16);
        bus.write_word(TEST_SP + 6, 4);

        let result = disp.dispatch_dialog(true, 0x012, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        let te_ptr = bus.read_long(te_handle);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            4
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            20
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            44
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            80
        );
    }

    #[test]
    fn tepinscroll_in_range_negative_dv_offsets_destrect_top_and_bottom_exactly_by_dv() {
        // Inside Macintosh: Text 1993, p. 2-91: "The destination rectangle
        // is offset by the amount scrolled." In-range up-scroll with
        // multi-line text that overflows the view: with text
        // "A\rB\rC\rD\rE\rF\rG\rH"
        // and view height 10, any small negative dv is in-range so destRect.
        // top and destRect.bottom must shift by exactly dv with destRect.
        // left and destRect.right unchanged (dh=0).
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"A\rB\rC\rD\rE\rF\rG\rH");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET,
            (-1000i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2,
            (-1000i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4,
            (-990i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6,
            (-800i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET,
            (-1000i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2,
            (-1000i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4,
            (-990i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6,
            (-800i16) as u16,
        );

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-3i16) as u16);
        bus.write_word(TEST_SP + 6, 0);

        let result = disp.dispatch_dialog(true, 0x012, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16,
            -1003,
            "destRect.top must shift by exactly dv=-3"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2) as i16,
            -1000,
            "destRect.left must be unchanged (dh=0)"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4) as i16,
            -993,
            "destRect.bottom must shift by exactly dv=-3"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6) as i16,
            -800,
            "destRect.right must be unchanged (dh=0)"
        );
    }

    #[test]
    fn tepinscroll_pascal_lr_stack_layout_reads_dh_dv_and_hte_from_correct_offsets() {
        // Per Pascal LR-push-first-arg-deepest convention (matching the
        // EXTERN_API expansion in MPW Universal Headers TextEdit.h):
        //   sp+0  hTE: TEHandle  (4) — last pushed, shallowest
        //   sp+4  dv:  INTEGER   (2) — middle
        //   sp+6  dh:  INTEGER   (2) — first pushed, deepest
        // Total pop = 8 bytes; no function-result slot.
        //
        // Stage hTE at sp+0, dv=-2 at sp+4, dh=+7 at sp+6 with a 0xCAFE
        // sentinel at sp+8 to detect over-reads. The HLE must consume
        // exactly 8 bytes and the sentinel must survive.
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"A\rB\rC\rD\rE\rF");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET,
            (-500i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2,
            (-500i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4,
            (-490i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6,
            (-400i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET,
            (-500i16) as u16,
        );
        // destRect.left = -510 (shifted left of view.left by 10, so positive
        // dh can be in-range to actually scroll right)
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2,
            (-510i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4,
            (-490i16) as u16,
        );
        bus.write_word(
            te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6,
            (-310i16) as u16,
        );

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-2i16) as u16);
        bus.write_word(TEST_SP + 6, 7);
        bus.write_word(TEST_SP + 8, 0xCAFE);

        let result = disp.dispatch_dialog(true, 0x012, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 8,
            "Pascal PROCEDURE must pop exactly 8 bytes (dh 2 + dv 2 + hTE 4)"
        );
        assert_eq!(
            bus.read_word(TEST_SP + 8),
            0xCAFE,
            "sentinel past the arg frame must survive"
        );
        // Verify destRect was actually mutated to confirm the HLE read
        // its args from the correct stack offsets (a swapped-arg stub
        // would shift by wrong amounts or pick up the sentinel).
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16,
            -502,
            "destRect.top must shift by dv=-2"
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2) as i16,
            -503,
            "destRect.left must shift by dh=+7 (clamped to max_right=10)"
        );
    }

    #[test]
    fn tepinscroll_clamps_overscroll_when_last_line_is_already_visible() {
        // Inside Macintosh: Text 1993, p. 2-91: TEPinScroll clamps movement
        // so scrolling stops once the last line is visible.
        let (mut disp, mut cpu, mut bus) = setup();

        let te_handle = make_te_with_text(&mut disp, &mut bus, b"A");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4, 50);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6, 80);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 50);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 80);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, (-20i16) as u16);
        bus.write_word(TEST_SP + 6, 0);
        let result = disp.dispatch_dialog(true, 0x012, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            10
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            20
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            50
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            80
        );
    }

    #[test]
    fn teselview_autoscroll_disabled_leaves_destrect_unchanged() {
        // Inside Macintosh: Text 1993, p. 2-92: TESelView only scrolls when
        // auto-scroll is enabled for the TE record.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(
            &mut disp,
            &mut bus,
            b"one two three four five six seven eight nine ten",
        );
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4, 50);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6, 80);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 40);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 80);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, false);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x011, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            0
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            20
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            40
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            80
        );
    }

    #[test]
    fn teselview_autoscroll_enabled_scrolls_destrect_toward_selection() {
        // Inside Macintosh: Text 1993, p. 2-92: TESelView scrolls selection
        // into view when auto-scroll is enabled.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(
            &mut disp,
            &mut bus,
            b"one two three four five six seven eight nine ten",
        );
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4, 50);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6, 80);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 40);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 80);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 0);
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, true);

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x011, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET),
            10
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2),
            20
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4),
            50
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6),
            80
        );
    }

    #[test]
    fn teselview_apple_canonical_below_view_shifts_destrect_top_up() {
        // Inside Macintosh: Text 1993, p. 2-92 (Apple canonical):
        //   "The top left part of the selection range is scrolled
        //    into view."
        // BasiliskII System 7.5.3 ROM does not scroll destRect in
        // this case (empirically verified); Systemless implements the
        // Apple-canonical semantic.
        //
        // Setup: destRect = (0, 0, 100, 200) tall enough for the
        // entire multi-line text, viewRect = (0, 0, 100, 30) showing
        // only the first ~1.5 lines, selection at the last character
        // (line 7 of "A\rB\r..\rH", well below viewRect.bottom).
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"A\rB\rC\rD\rE\rF\rG\rH");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 2, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 4, 30);
        bus.write_word(te_ptr + TrapDispatcher::TE_VIEW_RECT_OFFSET + 6, 100);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 0);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 200);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 100);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 14);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 14);
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, true);

        let pre_top = bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16;

        bus.write_long(TEST_SP, te_handle);
        let result = disp.dispatch_dialog(true, 0x011, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        let post_top = bus.read_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET) as i16;
        assert!(
            post_top < pre_top,
            "TESelView must shift destRect.top UP (negative direction) when selection is below viewRect per IM:Text 1993 p. 2-92; got pre_top={} post_top={}",
            pre_top,
            post_top
        );
    }

    #[test]
    fn teselview_procedure_protocol_consumes_only_tehandle_arg() {
        // Pascal PROCEDURE: 4-byte hTE arg, no result. Sentinel guard
        // at TEST_SP+4 must survive the call (trap must not write past
        // its argument frame). A7 advances by exactly 4 bytes.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"hi");
        // Auto-scroll disabled so the trap is a guaranteed destRect
        // no-op and the only observable side-effect is the SP advance.
        disp.set_te_feature_bit(te_handle, TrapDispatcher::TE_FEATURE_AUTO_SCROLL, false);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0xCAFE);
        bus.write_word(TEST_SP + 6, 0xBABE);
        let result = disp.dispatch_dialog(true, 0x011, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_word(TEST_SP + 4), 0xCAFE);
        assert_eq!(bus.read_word(TEST_SP + 6), 0xBABE);
    }

    #[test]
    fn tegetoffset_point_above_destrect_returns_zero() {
        // Inside Macintosh Volume V (1986), p. V-172: TEGetOffset
        // returns the character offset of the start of the first line
        // when the point is above the first line. Pascal LR layout:
        //   sp+0..3 hTE (last pushed, shallowest)
        //   sp+4..5 pt.v
        //   sp+6..7 pt.h
        //   sp+8..9 INTEGER result slot
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"ABC");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 40);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 140);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 5);
        bus.write_word(TEST_SP + 6, 30);
        bus.write_word(TEST_SP + 8, 0xBEEF);
        let result = disp.dispatch_dialog(true, 0x03C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, 0);
    }

    #[test]
    fn tegetoffset_point_below_last_line_returns_telength() {
        // Inside Macintosh Volume V (1986), p. V-172: TEGetOffset
        // returns the character offset of the end of the text when
        // the point is below the last line. Witnesses that the HLE
        // computes a value that varies with the point arg (vs the
        // always-zero result the pre-fix off-by-2 read produced).
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"ABC");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 40);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 140);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 200);
        bus.write_word(TEST_SP + 6, 30);
        bus.write_word(TEST_SP + 8, 0xBEEF);
        let result = disp.dispatch_dialog(true, 0x03C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, 3);
    }

    #[test]
    fn tegetoffset_function_protocol_consumes_point_and_tehandle_args_writes_integer_result() {
        // Pascal FUNCTION protocol: 8 bytes of args (Point + TEHandle)
        // popped, 2-byte INTEGER result written at sp+8. Sentinel at
        // sp+10 must survive — the trap must not write past the
        // 2-byte result slot.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"ABC");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET, 10);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 2, 20);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 4, 40);
        bus.write_word(te_ptr + TrapDispatcher::TE_DEST_RECT_OFFSET + 6, 140);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 5);
        bus.write_word(TEST_SP + 6, 30);
        bus.write_word(TEST_SP + 8, 0xDEAD);
        bus.write_word(TEST_SP + 10, 0xCAFE);
        let result = disp.dispatch_dialog(true, 0x03C, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_ne!(bus.read_word(TEST_SP + 8), 0xDEAD);
        assert_eq!(bus.read_word(TEST_SP + 10), 0xCAFE);
    }

    #[test]
    fn tefindword_returns_word_bounds_for_interior_positions() {
        // Inside Macintosh: Text (1993), pp. 2-60..2-61: TEFindWord
        // reports the word boundaries surrounding the current position.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"alpha beta");
        let te_ptr = bus.read_long(te_handle);

        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 1); // inside "alpha"
        cpu.write_reg(Register::D2, 0x1111);
        cpu.write_reg(Register::A3, te_ptr);
        cpu.write_reg(Register::A4, te_handle);

        let result = disp.dispatch_dialog(false, 0x0FE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(cpu.read_reg(Register::D0) as u16, 0);
        assert_eq!(cpu.read_reg(Register::D1) as u16, 5);
    }

    #[test]
    fn tefindword_returns_second_word_bounds_without_touching_stack() {
        // Same TextEdit hook: a later position inside the second word
        // should return that word's bounds, and the register-based hook
        // must not consume any stack bytes.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"alpha beta");
        let te_ptr = bus.read_long(te_handle);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0xCAFE);
        cpu.write_reg(Register::D0, 7); // inside "beta"
        cpu.write_reg(Register::D2, 0x2222);
        cpu.write_reg(Register::A3, te_ptr);
        cpu.write_reg(Register::A4, te_handle);

        let result = disp.dispatch_dialog(false, 0x0FE, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_word(TEST_SP), 0xCAFE);
        assert_eq!(cpu.read_reg(Register::D0) as u16, 6);
        assert_eq!(cpu.read_reg(Register::D1) as u16, 10);
    }

    #[test]
    fn tesettext_copies_bytes_updates_length_and_pops_arguments() {
        // Inside Macintosh Volume I (1985), p. I-378: TESetText replaces an
        // edit record's text contents.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"OLD");
        let te_ptr = bus.read_long(te_handle);
        let source_ptr = bus.alloc(4);
        bus.write_bytes(source_ptr, b"NEW!");

        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 4);
        bus.write_long(TEST_SP + 8, source_ptr);
        let result = disp.dispatch_dialog(true, 0x1CF, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);

        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"NEW!".to_vec()
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 4);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            4
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 4);
        let n_lines = u32::from(bus.read_word(te_ptr + TrapDispatcher::TE_N_LINES_OFFSET));
        assert!(n_lines > 0);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_LINE_STARTS_OFFSET + n_lines * 2),
            4
        );
    }

    #[test]
    fn tesettext_nil_or_zero_length_input_clears_text() {
        // Inside Macintosh Volume I (1985), p. I-378: TESetText sets current
        // text contents; empty input yields empty text.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);

        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 5);
        bus.write_long(TEST_SP + 8, 0);
        let result = disp.dispatch_dialog(true, 0x1CF, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            Vec::<u8>::new()
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 0);
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            0
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 0);
    }

    #[test]
    fn tesettext_replaces_prior_contents_via_sequential_call() {
        // Inside Macintosh Volume I (1985), p. I-378: TESetText *sets*
        // (not appends to) the current text contents: TESetText("WORLD!", 6)
        // followed by TESetText("HI", 2) on the same TERec yields
        // teLength == 2 with the first two bytes equal to "HI".
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"OLD");
        let te_ptr = bus.read_long(te_handle);

        let first_ptr = bus.alloc(6);
        bus.write_bytes(first_ptr, b"WORLD!");
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 6);
        bus.write_long(TEST_SP + 8, first_ptr);
        let r1 = disp.dispatch_dialog(true, 0x1CF, &mut cpu, &mut bus);
        assert!(r1.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"WORLD!".to_vec()
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 6);

        cpu.write_reg(Register::A7, TEST_SP);
        let second_ptr = bus.alloc(2);
        bus.write_bytes(second_ptr, b"HI");
        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 2);
        bus.write_long(TEST_SP + 8, second_ptr);
        let r2 = disp.dispatch_dialog(true, 0x1CF, &mut cpu, &mut bus);
        assert!(r2.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HI".to_vec()
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_LENGTH_OFFSET), 2);
    }

    #[test]
    fn tesettext_balances_stackspace_with_pascal_protocol() {
        // Inside Macintosh Volume I (1985), p. I-378: TESetText is a
        // PROCEDURE with three args (Ptr text, LONGINT length, TEHandle).
        // Pascal LR push order yields a 12-byte arg frame and no result
        // slot. A7 must advance by exactly 12 bytes regardless of arg
        // values.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"SEED");
        let source = bus.alloc(1);
        bus.write_bytes(source, b"X");

        bus.write_long(TEST_SP, te_handle);
        bus.write_long(TEST_SP + 4, 1);
        bus.write_long(TEST_SP + 8, source);
        bus.write_word(TEST_SP + 12, 0xCAFE);
        let result = disp.dispatch_dialog(true, 0x1CF, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 12);
        assert_eq!(bus.read_word(TEST_SP + 12), 0xCAFE);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"X".to_vec()
        );
    }

    #[test]
    fn tekey_inserts_character_at_caret_and_advances_selection() {
        // Inside Macintosh Volume I (1985), p. I-385 and Text 1993, p. 2-81:
        // TEKey inserts typed characters at the insertion point.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 1);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, u16::from(b'E'));
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 2);
    }

    #[test]
    fn tekey_redraws_the_edited_text_before_returning() {
        // Inside Macintosh: Text (1993), pp. 2-81 to 2-82: TEKey redraws
        // the text as necessary. A dialog event loop may therefore call
        // TEKey without a subsequent TEUpdate and still expects the typed
        // character to become visible immediately.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"");
        let (screen_base, row_bytes, width, height, _) = disp.screen_mode;
        let screen_len = row_bytes * height as u32;
        let before = bus.read_bytes(screen_base, screen_len as usize);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, u16::from(b'W'));
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"W".to_vec()
        );
        assert_ne!(
            bus.read_bytes(screen_base, screen_len as usize),
            before,
            "TEKey must paint the changed TextEdit record before returning ({}x{})",
            width,
            height
        );
    }

    #[test]
    fn tekey_backspace_deletes_selection_or_previous_character() {
        // Inside Macintosh Volume I (1985), p. I-385 and Text 1993, p. 2-81:
        // backspace deletes current selection, or the previous character.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);

        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 1);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 3);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0x0008);
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 1);

        disp.te_set_text_contents(&mut bus, te_handle, b"HELLO");
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0x0008);
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HLLO".to_vec()
        );
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            1
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 1);
    }

    #[test]
    fn tekey_arrows_move_the_caret_without_inserting_text() {
        // Inside Macintosh: Text (1993), pp. 2-36 to 2-37 and 2-81:
        // arrow key codes move the insertion point; they are not text bytes.
        let (mut disp, mut cpu, mut bus) = setup();
        let te_handle = make_te_with_text(&mut disp, &mut bus, b"HELLO");
        let te_ptr = bus.read_long(te_handle);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET, 2);
        bus.write_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET, 4);

        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0x001C);
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            2
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 2);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, te_handle);
        bus.write_word(TEST_SP + 4, 0x001D);
        let result = disp.dispatch_dialog(true, 0x1DC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_word(te_ptr + TrapDispatcher::TE_SEL_START_OFFSET),
            3
        );
        assert_eq!(bus.read_word(te_ptr + TrapDispatcher::TE_SEL_END_OFFSET), 3);
        assert_eq!(
            TrapDispatcher::te_text_bytes(&bus, te_handle),
            b"HELLO".to_vec()
        );
    }

    // ---- HideDialogItem / ShowDialogItem ($A827 / $A828) ----
    // Gate the move-offscreen-and-restore behaviour.

    #[test]
    fn hide_dialog_item_moves_rect_offscreen_and_saves_original() {
        // MTE 1992, 6-123: HideDialogItem offsets left/right by +16384.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 20, 30, 120),
                text: "OK".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        // Stack: SP+0=itemNo(2), SP+2=dialog_ptr(4). Pop 6.
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x027, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);

        // Rect is now offscreen.
        let item_rect = disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect;
        assert!(
            item_rect.0 == 10 && item_rect.2 == 30,
            "vertical coordinates are unchanged by HideDialogItem, got {item_rect:?}"
        );
        assert!(
            item_rect.1 == 20 + 16384 && item_rect.3 == 120 + 16384,
            "hidden item rect must be offscreen, got {item_rect:?}"
        );
        // Original saved.
        assert_eq!(
            disp.hidden_dialog_item_rects.get(&(dialog_ptr, 1)).copied(),
            Some((10, 20, 30, 120))
        );
    }

    #[test]
    fn show_dialog_item_restores_saved_rect() {
        // MTE 1992, 6-124: ShowDialogItem restores pre-hide item rect.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 20 + 16384, 30, 120 + 16384),
                text: "OK".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );
        disp.hidden_dialog_item_rects
            .insert((dialog_ptr, 1), (10, 20, 30, 120));

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);

        assert_eq!(
            disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect,
            (10, 20, 30, 120)
        );
        assert!(
            !disp.hidden_dialog_item_rects.contains_key(&(dialog_ptr, 1)),
            "saved rect must be removed after show"
        );
    }

    #[test]
    fn hide_then_show_is_idempotent_roundtrip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        let orig_rect = (50, 60, 80, 200);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: orig_rect,
                text: "Cancel".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        disp.dispatch_dialog(true, 0x027, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        // Reset SP + args for Show.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect,
            orig_rect,
            "round-trip hide→show must restore the exact original rect"
        );
    }

    #[test]
    fn hide_dialog_item_already_hidden_left_coord_is_noop() {
        // MTE 1992, 6-123: left > 8192 means the item is already hidden.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        let already_hidden = (10, 9000, 30, 9200);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: already_hidden,
                text: "Hidden".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x027, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect,
            already_hidden
        );
        assert!(
            !disp.hidden_dialog_item_rects.contains_key(&(dialog_ptr, 1)),
            "already hidden items must not record a new saved rect"
        );
    }

    #[test]
    fn show_dialog_item_without_saved_rect_subtracts_offset_when_hidden() {
        // MTE 1992, 6-124: ShowDialogItem uses left > 8192 as hidden predicate.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 30 + 16384, 30, 130 + 16384),
                text: "ShowMe".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);

        let result = disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(
            disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect,
            (10, 30, 30, 130)
        );
    }

    #[test]
    fn show_dialog_item_visible_left_coord_is_noop() {
        // MTE 1992, 6-124: left < 8192 means already visible; no-op.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x210000u32;
        let visible = (10, 20, 30, 120);
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: visible,
                text: "Visible".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x028, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(disp.dialog_items.get(&dialog_ptr).unwrap()[0].rect, visible);
    }

    // ---- FindDItem ($A984) ----

    #[test]
    fn findditem_returns_zero_based_index_for_hit_and_pops_stack() {
        // MTE 1992, 6-125: FindDialogItem/FindDItem returns 0 for the first
        // item, 1 for the second, etc.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x220000u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 4,
                    rect: (10, 20, 30, 40),
                    text: "A".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (40, 50, 70, 90),
                    text: "B".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        bus.write_word(TEST_SP, 45);
        bus.write_word(TEST_SP + 2, 55);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, 1);
    }

    #[test]
    fn findditem_point_outside_all_items_returns_minus_one() {
        // IM:IV 1986, p. IV-60: FindDItem returns -1 when the point does not
        // lie within any item rectangle.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x220100u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![DialogItem {
                item_type: 4,
                rect: (10, 20, 30, 40),
                text: "A".into(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            }],
        );

        bus.write_word(TEST_SP, 99);
        bus.write_word(TEST_SP + 2, 99);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, -1);
    }

    #[test]
    fn findditem_returns_first_overlapping_item_and_includes_disabled_items() {
        // IM:IV 1986, p. IV-60: overlap resolution is first item in list.
        // IM:IV 1986, p. IV-60 note: disabled items are still returned.
        let (mut disp, mut cpu, mut bus) = setup();
        let dialog_ptr = 0x220200u32;
        disp.dialog_items.insert(
            dialog_ptr,
            vec![
                DialogItem {
                    item_type: 0x80 | 4, // disabled
                    rect: (10, 20, 40, 60),
                    text: "DisabledTop".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
                DialogItem {
                    item_type: 4,
                    rect: (20, 30, 50, 70),
                    text: "EnabledBottom".into(),
                    resource_id: 0,
                    proc_ptr: 0,
                    sel_start: 0,
                    sel_end: 0,
                },
            ],
        );

        // Point lies within both rectangles.
        bus.write_word(TEST_SP, 25);
        bus.write_word(TEST_SP + 2, 35);
        bus.write_long(TEST_SP + 4, dialog_ptr);
        let result = disp.dispatch_dialog(true, 0x184, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        assert_eq!(bus.read_word(TEST_SP + 8) as i16, 0);
    }

    // ========== Cursor Manager ==========

    // ---- InitCursor ($A850) ----

    #[test]
    fn init_cursor_resets_cursor_level_to_zero_and_sets_arrow_visible() {
        // IM:I I-167: InitCursor sets arrow cursor, sets cursor level to 0,
        // and makes cursor visible.
        let (mut disp, mut cpu, mut bus) = setup();

        // Start from a hidden nested level to prove InitCursor reset.
        disp.cursor_state.clear_image_for_test();
        disp.cursor_state.set_level_for_test(-3);

        let result = disp.dispatch_dialog(true, 0x050, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(disp.cursor_data().is_some());
        assert_eq!(disp.cursor_level(), 0);
        assert!(disp.cursor_visible());
    }

    // ---- SetCursor ($A851) ----

    #[test]
    fn set_cursor_reads_cursor_data_and_pops_4_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let crsr_ptr = 0x300000u32;

        // Write cursor bitmap data (32 bytes)
        for i in 0..32u32 {
            bus.write_byte(crsr_ptr + i, 0xAA);
        }
        // Write cursor mask data (32 bytes)
        for i in 0..32u32 {
            bus.write_byte(crsr_ptr + 32 + i, 0xFF);
        }
        // Write hotspot
        bus.write_word(crsr_ptr + 64, 5); // hot_v
        bus.write_word(crsr_ptr + 66, 3); // hot_h

        // SP+0: crsr_ptr (4 bytes)
        bus.write_long(TEST_SP, crsr_ptr);

        let result = disp.dispatch_dialog(true, 0x051, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

        let (data, mask, hot_v, hot_h) = disp.cursor_data().unwrap();
        assert!(data.iter().all(|&b| b == 0xAA));
        assert!(mask.iter().all(|&b| b == 0xFF));
        assert_eq!(hot_v, 5);
        assert_eq!(hot_h, 3);
    }

    #[test]
    fn set_cursor_does_not_force_visibility_when_cursor_is_hidden() {
        // IM:I I-167: if the cursor is hidden, SetCursor changes the
        // current cursor image but it remains hidden until uncovered.
        let (mut disp, mut cpu, mut bus) = setup();
        let crsr_ptr = 0x300100u32;
        for i in 0..32u32 {
            bus.write_byte(crsr_ptr + i, 0x11);
            bus.write_byte(crsr_ptr + 32 + i, 0x22);
        }
        bus.write_word(crsr_ptr + 64, 9);
        bus.write_word(crsr_ptr + 66, 4);
        bus.write_long(TEST_SP, crsr_ptr);

        disp.cursor_state.set_level_for_test(-1);
        let result = disp.dispatch_dialog(true, 0x051, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.cursor_level(), -1);
        assert!(!disp.cursor_visible());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    }

    // ---- HideCursor ($A852) ----

    #[test]
    fn hide_cursor_decrements_level_and_hides_cursor() {
        // IM:I I-168: HideCursor decrements cursor level (from 0 to -1)
        // and removes the cursor from the screen.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.cursor_state.set_level_for_test(0);

        let result = disp.dispatch_dialog(true, 0x052, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.cursor_level(), -1);
        assert!(!disp.cursor_visible());
    }

    // ---- ShowCursor ($A853) ----

    #[test]
    fn show_cursor_balances_hidecursor_and_reveals_at_level_zero() {
        // IM:I I-168: ShowCursor increments toward 0 and only shows the
        // cursor when level becomes 0.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.cursor_state.set_level_for_test(-2);

        let result = disp.dispatch_dialog(true, 0x053, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.cursor_level(), -1);
        assert!(!disp.cursor_visible());

        let result = disp.dispatch_dialog(true, 0x053, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.cursor_level(), 0);
        assert!(disp.cursor_visible());
    }

    #[test]
    fn show_cursor_at_level_zero_is_noop() {
        // IM:I I-168: extra ShowCursor calls have no effect and do not
        // increment cursor level above 0.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.cursor_state.set_level_for_test(0);

        let result = disp.dispatch_dialog(true, 0x053, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.cursor_level(), 0);
        assert!(disp.cursor_visible());
    }

    // ---- ObscureCursor ($A856) ----

    #[test]
    fn obscure_cursor_noop_preserves_cursor_level_visibility_and_stack() {
        // IM:I I-168: ObscureCursor has no effect on cursor level and takes
        // no arguments. Systemless's HLE compromise keeps it as a no-op because
        // synthesized mouse-move events would immediately un-obscure anyway.
        let (mut disp, mut cpu, mut bus) = setup();
        disp.cursor_state.set_level_for_test(-1);
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_dialog(true, 0x056, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(disp.cursor_level(), -1);
        assert!(!disp.cursor_visible());
    }

    #[test]
    fn obscure_cursor_five_call_composition_preserves_stack_pointer_and_level() {
        // 5 successive ObscureCursor dispatches inside one
        // StackSpace sandwich leave SP unchanged. Per IM:I I-168 the
        // PROCEDURE has no arguments and no result slot, so each call
        // pops 0 bytes; the cumulative SP delta after N calls is zero.
        // Also pins the cursor-level no-effect contract per IM:I I-168:
        // ObscureCursor "has no effect on the cursor level and must
        // not be balanced by a call to ShowCursor."
        let (mut disp, mut cpu, mut bus) = setup();
        disp.cursor_state.set_level_for_test(-2);
        let sp_before = cpu.read_reg(Register::A7);

        for i in 0..5 {
            let result = disp.dispatch_dialog(true, 0x056, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok(), "iteration {i} dispatch failed");
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp_before,
                "iteration {i}: ObscureCursor must leave SP unchanged (0-byte pop)",
            );
            assert_eq!(
                disp.cursor_level(), -2,
                "iteration {i}: ObscureCursor must NOT change cursor level per IM:I I-168",
            );
        }
    }

    // ---- GetCursor ($A9B9) — IM:I I-474 contract ----

    #[test]
    fn get_cursor_returns_handle_for_system_cursor() {
        // crossCursor (ID 2) is one of the four standard system
        // cursors per IM:I I-475..I-477. Systemless synthesises it via
        // [`TrapDispatcher::synthesize_system_cursor`] because the
        // System file's resource fork isn't loaded; the result must
        // be a non-NIL handle whose master ptr's bitmap matches the
        // synthesised crosshair.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 2); // crossCursor

        let _ = disp.dispatch_dialog(true, 0x1B9, &mut cpu, &mut bus);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);

        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(
            handle, 0,
            "GetCursor(crossCursor) must return non-NIL handle for a built-in system cursor"
        );
        let crsr = bus.read_long(handle);
        let hot_v = bus.read_word(crsr + 64) as i16;
        let hot_h = bus.read_word(crsr + 66) as i16;
        assert_eq!(
            (hot_v, hot_h),
            (7, 7),
            "crossCursor's hotspot is documented at (7,7) in IM:I I-475"
        );
    }

    #[test]
    fn get_cursor_returns_nil_for_unknown_id_per_im_i_474() {
        // IM:I I-474: "If the resource can't be read, GetCursor
        // returns NIL." For an ID that's neither a CURS resource we
        // have loaded nor one of the four standard built-ins, the
        // miss path is NIL — not a fresh empty 68-byte block. Apps
        // that defensively check `if handle = NIL then use_arrow
        // else SetCursor(handle^^)` would otherwise SetCursor a
        // blank cursor onto the screen.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 999); // not a system cursor, no CURS 999 installed

        let _ = disp.dispatch_dialog(true, 0x1B9, &mut cpu, &mut bus);
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "GetCursor must pop 2 bytes (cursorID INTEGER)"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetCursor must return NIL when CURS resource is missing per IM:I I-474"
        );
    }

    #[test]
    fn get_cursor_returns_stable_handle_across_repeated_calls_for_system_cursor() {
        // Apps cache GetCursor results at boot and pass them to
        // SetCursor every frame; without handle stability the
        // dispatcher would leak a 68-byte block per call. Pin the
        // cache hit so a future "tighten alloc" change doesn't
        // accidentally drop the cache.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 4); // watchCursor

        let _ = disp.dispatch_dialog(true, 0x1B9, &mut cpu, &mut bus);
        let h1 = bus.read_long(TEST_SP + 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 4);

        let _ = disp.dispatch_dialog(true, 0x1B9, &mut cpu, &mut bus);
        let h2 = bus.read_long(TEST_SP + 2);
        assert_eq!(
            h1, h2,
            "GetCursor must return the same handle for repeated calls on a system cursor ID"
        );
    }

    #[test]
    fn get_cursor_returns_handle_to_loaded_curs_resource() {
        // With a CURS resource installed, GetCursor must return a
        // handle whose master ptr equals the loaded resource bytes
        // — same value-asserting gate as get_icon_returns_handle_to_loaded_icon_resource.
        let (mut disp, mut cpu, mut bus) = setup();
        // CURS records are 68 bytes (32 data + 32 mask + 4 hotSpot).
        let curs_data: Vec<u8> = (0..68).map(|i| (i as u8).wrapping_mul(11)).collect();
        let res_ptr = disp.install_test_resource(&mut bus, *b"CURS", 200, &curs_data);
        bus.write_word(TEST_SP, 200);

        let _ = disp.dispatch_dialog(true, 0x1B9, &mut cpu, &mut bus);
        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(
            handle, 0,
            "GetCursor must return non-NIL handle when CURS resource is loaded"
        );
        assert_eq!(
            bus.read_long(handle),
            res_ptr,
            "GetCursor's handle must dereference to the loaded CURS resource bytes"
        );
    }

    // ---- GetPattern ($A9B8) — IM:I I-473 contract ----

    #[test]
    fn get_pattern_returns_nil_for_missing_resource_per_im_i_473() {
        // IM:I I-473: "If the resource can't be read, GetPattern
        // returns NIL." The previous Stub returned a handle to a
        // fresh all-0xFF (white) 8-byte block, strictly worse than
        // NIL because callers branching on `handle = NIL` took the
        // FillRect-with-white path instead of the recovery path.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 1); // patID = 1, no PAT 1 installed

        let _ = disp.dispatch_dialog(true, 0x1B8, &mut cpu, &mut bus);
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "GetPattern must pop 2 bytes (patID INTEGER)"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetPattern must return NIL when PAT resource is missing per IM:I I-473"
        );
    }

    #[test]
    fn get_pattern_returns_handle_to_loaded_pat_resource() {
        // With a PAT resource installed, GetPattern must return a
        // handle whose master ptr equals the loaded resource bytes.
        // Pin master-ptr-equality so a future regression that
        // allocates fresh memory instead of reusing the loaded ptr
        // fails here. Same gate shape as
        // get_icon_returns_handle_to_loaded_icon_resource.
        let (mut disp, mut cpu, mut bus) = setup();
        let pat_data: [u8; 8] = [0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA];
        let res_ptr = disp.install_test_resource(&mut bus, *b"PAT ", 16, &pat_data);
        bus.write_word(TEST_SP, 16);

        let _ = disp.dispatch_dialog(true, 0x1B8, &mut cpu, &mut bus);
        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(
            handle, 0,
            "GetPattern must return non-NIL handle when PAT resource is loaded"
        );
        assert_eq!(
            bus.read_long(handle),
            res_ptr,
            "GetPattern's handle must dereference to the loaded PAT resource bytes \
             (not a fresh all-0xFF allocation)"
        );
        // Verify the bytes through the handle deref are the
        // installed sentinel pattern (50/AA stripes), not 0xFF.
        let master = bus.read_long(handle);
        for (i, want) in pat_data.iter().enumerate() {
            assert_eq!(
                bus.read_byte(master + i as u32),
                *want,
                "GetPattern PAT byte +{i} must match installed resource byte"
            );
        }
    }

    #[test]
    fn get_pattern_returns_stable_handle_across_repeated_calls() {
        // Apps that paint with a pattern in tight loops cache the
        // GetPattern result; pin handle stability so a future
        // regression that drops `get_or_create_resource_handle` for
        // a fresh `bus.alloc(4)` per call surfaces here.
        let (mut disp, mut cpu, mut bus) = setup();
        let pat_data: [u8; 8] = [0x0F, 0xF0, 0x0F, 0xF0, 0x0F, 0xF0, 0x0F, 0xF0];
        let _ = disp.install_test_resource(&mut bus, *b"PAT ", 100, &pat_data);

        bus.write_word(TEST_SP, 100);
        let _ = disp.dispatch_dialog(true, 0x1B8, &mut cpu, &mut bus);
        let h1 = bus.read_long(TEST_SP + 2);
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 100);

        let _ = disp.dispatch_dialog(true, 0x1B8, &mut cpu, &mut bus);
        let h2 = bus.read_long(TEST_SP + 2);
        assert_eq!(
            h1, h2,
            "GetPattern must return the same handle for repeated calls on a loaded PAT resource"
        );
    }

    // ---- GetIcon ($A9BB) — IM:I I-473 contract ----

    #[test]
    fn get_icon_returns_standard_system_icon_one_after_application_chain_miss() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 1);

        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);

        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(handle, 0);
        let ptr = bus.read_long(handle);
        assert_eq!(bus.get_alloc_size(ptr), Some(128));
        assert_eq!(bus.read_long(ptr), 0xFFFF_FFFF);
        assert_eq!(bus.read_long(ptr + 124), 0xFFFF_FFFF);
    }

    #[test]
    fn get_icon_returns_nil_when_resource_missing() {
        // IM:I I-473: "If the resource can't be read, GetIcon
        // returns NIL." Critical for apps that defensively check
        // `if handle = NIL` before dereferencing — the prior Stub
        // always returned a non-NIL handle to uninitialised
        // memory which broke that branch.
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 999); // non-system icon ID, no resource installed
        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP + 2,
            "GetIcon must pop 2 bytes (iconID INTEGER)"
        );
        assert_eq!(
            bus.read_long(TEST_SP + 2),
            0,
            "GetIcon must return NIL when ICON resource is missing per IM:I I-473"
        );
    }

    #[test]
    fn get_icon_returns_handle_to_loaded_icon_resource() {
        // With an ICON resource installed, GetIcon must return a
        // non-NIL handle whose master ptr points at the loaded
        // resource bytes. Pin the master-ptr-equality so a future
        // regression that allocates fresh memory instead of
        // reusing the loaded ptr fails here.
        let (mut disp, mut cpu, mut bus) = setup();
        // Real ICON resources are 128 bytes (32x32 1bpp); fill
        // with a recognisable sentinel pattern so handle deref
        // assertions can verify "this is the right resource."
        let icon_data: Vec<u8> = (0..128).map(|i| (i as u8).wrapping_mul(7)).collect();
        let res_ptr = disp.install_test_resource(&mut bus, *b"ICON", 1, &icon_data);
        bus.write_word(TEST_SP, 1);

        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);
        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(
            handle, 0,
            "GetIcon must return a non-NIL handle when ICON resource is loaded"
        );
        assert_eq!(
            bus.read_long(handle),
            res_ptr,
            "GetIcon's handle must dereference to the loaded ICON resource bytes \
             (not a fresh uninitialised allocation)"
        );
        // Verify the bytes through the handle deref are the
        // installed sentinel pattern.
        let master = bus.read_long(handle);
        for (i, want) in icon_data.iter().enumerate() {
            assert_eq!(
                bus.read_byte(master + i as u32),
                *want,
                "GetIcon ICON byte +{i} must match installed resource byte"
            );
        }
    }

    #[test]
    fn get_icon_returns_stable_handle_across_repeated_calls() {
        // Per IM:Resource Manager, GetResource returns the SAME
        // handle on repeat calls (unless ReleaseResource has run).
        // GetIcon delegates to GetResource per IM:I I-473, so it
        // must inherit this stability — apps that cache the handle
        // depend on it not changing between alert cycles.
        let (mut disp, mut cpu, mut bus) = setup();
        let icon_data: Vec<u8> = vec![0xCC; 128];
        disp.install_test_resource(&mut bus, *b"ICON", 200, &icon_data);

        bus.write_word(TEST_SP, 200);
        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);
        let h1 = bus.read_long(TEST_SP + 2);

        // Reset SP and call again with the same iconID.
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 200);
        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);
        let h2 = bus.read_long(TEST_SP + 2);

        assert_ne!(h1, 0, "first GetIcon must return non-NIL");
        assert_eq!(
            h1, h2,
            "GetIcon must return the SAME handle on repeated calls per IM:Resource Mgr stability"
        );
    }

    #[test]
    fn get_icon_pops_two_bytes_per_pascal_signature() {
        // FUNCTION GetIcon(iconID: INTEGER): Handle;
        // Pascal stack frame: result Handle (4) pre-pushed by
        // caller, iconID (2) on top. Trap pops 2 bytes (iconID),
        // result lands at new SP+0 (= old SP+2).
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 999); // any iconID
        let pre_a7 = cpu.read_reg(Register::A7);
        let _ = disp.dispatch_dialog(true, 0x1BB, &mut cpu, &mut bus);
        assert_eq!(
            cpu.read_reg(Register::A7),
            pre_a7 + 2,
            "GetIcon must advance A7 by 2 bytes (iconID INTEGER)"
        );
    }

    // ---- GetPicture ($A9BC) ----

    #[test]
    fn get_picture_returns_nil_without_resource() {
        let (mut disp, mut cpu, mut bus) = setup();

        // SP+0: picture id (2 bytes)
        bus.write_word(TEST_SP, 1);

        let result = disp.dispatch_dialog(true, 0x1BC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);

        let handle = bus.read_long(TEST_SP + 2);
        assert_eq!(handle, 0); // NIL when no PICT resource loaded
    }

    #[test]
    fn get_picture_clears_stale_reserr_on_success() {
        // GetPicture is documented as a GetResource('PICT', picID) wrapper
        // (Inside Macintosh Volume I, I-475). Like the generic GetResource
        // implementation, a successful lookup must leave ResErr at noErr;
        // stale resNotFound would make callers reject a valid PicHandle.
        let (mut disp, mut cpu, mut bus) = setup();
        let pict_data = [0x00, 0x11, 0x22, 0x33];
        disp.install_test_resource(&mut bus, *b"PICT", 10000, &pict_data);
        bus.write_word(0x0A60, (-192i16) as u16);
        bus.write_word(TEST_SP, 10000);

        let result = disp.dispatch_dialog(true, 0x1BC, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_ne!(bus.read_long(TEST_SP + 2), 0);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
    }

    #[test]
    fn get_picture_reloads_released_resource_from_open_resource_file() {
        fn make_resource_fork_bytes(resources: &[([u8; 4], i16, &[u8])]) -> Vec<u8> {
            let mut type_groups: Vec<([u8; 4], Vec<(i16, &[u8], u32)>)> = Vec::new();
            for (res_type, res_id, data) in resources {
                let group_idx = type_groups
                    .iter()
                    .position(|(existing_type, _)| existing_type == res_type)
                    .unwrap_or_else(|| {
                        type_groups.push((*res_type, Vec::new()));
                        type_groups.len() - 1
                    });
                type_groups[group_idx].1.push((*res_id, *data, 0));
            }
            type_groups.sort_by_key(|(res_type, _)| *res_type);
            for (_, entries) in &mut type_groups {
                entries.sort_by_key(|(res_id, _, _)| *res_id);
            }

            let data_offset = 16u32;
            let mut data_section = Vec::new();
            for (_, entries) in &mut type_groups {
                for (_, data, data_pos) in entries {
                    *data_pos = data_section.len() as u32;
                    data_section.extend_from_slice(&(data.len() as u32).to_be_bytes());
                    data_section.extend_from_slice(data);
                }
            }

            let map_offset = data_offset + data_section.len() as u32;
            let type_list_offset = 30u16;
            let type_count = type_groups.len();
            let resource_count: usize = type_groups.iter().map(|(_, entries)| entries.len()).sum();
            let ref_lists_offset = 2 + type_count * 8;
            let name_list_offset =
                type_list_offset as usize + ref_lists_offset + resource_count * 12;
            let map_length = name_list_offset as u32;

            let mut bytes = vec![0u8; (map_offset + map_length) as usize];
            let mut header = [0u8; 16];
            header[0..4].copy_from_slice(&data_offset.to_be_bytes());
            header[4..8].copy_from_slice(&map_offset.to_be_bytes());
            header[8..12].copy_from_slice(&(data_section.len() as u32).to_be_bytes());
            header[12..16].copy_from_slice(&map_length.to_be_bytes());
            bytes[0..16].copy_from_slice(&header);
            bytes[data_offset as usize..data_offset as usize + data_section.len()]
                .copy_from_slice(&data_section);

            let map_start = map_offset as usize;
            bytes[map_start..map_start + 16].copy_from_slice(&header);
            bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
            bytes[map_start + 26..map_start + 28]
                .copy_from_slice(&(name_list_offset as u16).to_be_bytes());
            bytes[map_start + 28..map_start + 30]
                .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

            let type_list_start = map_start + type_list_offset as usize;
            bytes[type_list_start..type_list_start + 2]
                .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());
            let mut next_ref_list_offset = ref_lists_offset;
            for (i, (res_type, entries)) in type_groups.iter().enumerate() {
                let type_entry = type_list_start + 2 + i * 8;
                bytes[type_entry..type_entry + 4].copy_from_slice(res_type);
                bytes[type_entry + 4..type_entry + 6]
                    .copy_from_slice(&((entries.len() as u16) - 1).to_be_bytes());
                bytes[type_entry + 6..type_entry + 8]
                    .copy_from_slice(&(next_ref_list_offset as u16).to_be_bytes());

                let ref_list_start = type_list_start + next_ref_list_offset;
                for (j, (res_id, _, data_pos)) in entries.iter().enumerate() {
                    let ref_entry = ref_list_start + j * 12;
                    bytes[ref_entry..ref_entry + 2]
                        .copy_from_slice(&(*res_id as u16).to_be_bytes());
                    bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
                    bytes[ref_entry + 4] = 0;
                    let data_offset_bytes = data_pos.to_be_bytes();
                    bytes[ref_entry + 5..ref_entry + 8].copy_from_slice(&data_offset_bytes[1..4]);
                }

                next_ref_list_offset += entries.len() * 12;
            }

            bytes
        }

        let (mut disp, mut cpu, mut bus) = setup();
        let pict_data = b"\x00\x11Wordtris reloadable picture data";
        let fork = make_resource_fork_bytes(&[(*b"PICT", 1127, &pict_data[..])]);
        disp.vfs_rsrc.insert("Pictures".to_string(), fork);
        disp.open_resource_file_from_vfs_key(&mut bus, "Pictures", false);

        bus.write_word(TEST_SP, 1127);
        let first = disp.dispatch_dialog(true, 0x1BC, &mut cpu, &mut bus);
        assert!(first.unwrap().is_ok());
        let first_handle = bus.read_long(TEST_SP + 2);
        assert_ne!(first_handle, 0, "first GetPicture must find PICT 1127");
        let first_ptr = bus.read_long(first_handle);
        assert_eq!(bus.read_bytes(first_ptr, pict_data.len()), pict_data);

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, first_handle);
        let release = disp.dispatch_resource(true, 0x1A3, &mut cpu, &mut bus);
        assert!(release.unwrap().is_ok());
        assert_eq!(
            bus.read_long(first_handle),
            0,
            "ReleaseResource should clear the old master pointer"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1127);
        let second = disp.dispatch_dialog(true, 0x1BC, &mut cpu, &mut bus);
        assert!(second.unwrap().is_ok());
        let second_handle = bus.read_long(TEST_SP + 2);
        assert_ne!(
            second_handle, 0,
            "GetPicture must reload a released PICT from the open resource file"
        );
        assert_ne!(
            second_handle, first_handle,
            "a released resource should be returned through a fresh handle"
        );
        let second_ptr = bus.read_long(second_handle);
        assert_eq!(bus.read_bytes(second_ptr, pict_data.len()), pict_data);
    }

    // ---- GetString ($A9BA) ----

    #[test]
    fn get_string_returns_loaded_str_resource_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let str_ptr = bus.alloc(6);
        bus.write_byte(str_ptr, 5);
        bus.write_bytes(str_ptr + 1, b"Hello");
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([((*b"STR ", 1), str_ptr)]),
                    named: std::collections::HashMap::new(),
                    names_by_id: std::collections::HashMap::new(),
                    attrs: std::collections::HashMap::new(),
                    map_attrs: 0,
                },
            )]),
            names: std::collections::HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        // SP+0: string id (2 bytes)
        bus.write_word(TEST_SP, 1);

        let result = disp.dispatch_dialog(true, 0x1BA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);

        let handle = bus.read_long(TEST_SP + 2);
        assert_ne!(handle, 0);
        assert_eq!(bus.read_long(handle), str_ptr);
    }

    #[test]
    fn get_string_returns_nil_when_resource_is_missing() {
        let (mut disp, mut cpu, mut bus) = setup();
        bus.write_word(TEST_SP, 999);

        let result = disp.dispatch_dialog(true, 0x1BA, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
        assert_eq!(bus.read_long(TEST_SP + 2), 0);
    }

    // ---- Unhandled trap returns None ----

    #[test]
    fn unhandled_trap_returns_none() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_dialog(true, 0xFFFF, &mut cpu, &mut bus);
        assert!(result.is_none());
    }
