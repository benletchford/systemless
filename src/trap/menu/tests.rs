use super::super::test_helpers::{setup, setup_with_port, MockCpu, TEST_SP};
use super::super::TrapDispatcher;
use super::{
    count_menu_items_from_memory, menu_list_from_memory, parse_appendmenu_items,
    parse_menu_resource, test_tracked_menu_state, Menu, MenuItem, MC_ENTRY_SIZE,
    MENU_KEY_REDUCED_ICON, MENU_KEY_SMALL_ICON, MENU_ROW_HEIGHT,
};
use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::menu_manager::{menu_choice_value, MenuDefinitionInvocation, TrackedMenuPaneView};
use crate::ui_theme::UiThemeId;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::span::{Attributes, Record};
use tracing::subscriber::Interest;
use tracing::{Event, Id, Level, Metadata, Subscriber};

struct WarnCounter {
    warnings: Arc<AtomicUsize>,
}

impl Subscriber for WarnCounter {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }

    fn new_span(&self, _attrs: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        if *event.metadata().level() == Level::WARN {
            self.warnings.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

fn write_pstring(bus: &mut crate::memory::MacMemoryBus, ptr: u32, s: &str) {
    let bytes = s.as_bytes();
    bus.write_byte(ptr, bytes.len().min(255) as u8);
    for (i, b) in bytes.iter().take(255).enumerate() {
        bus.write_byte(ptr + 1 + i as u32, *b);
    }
}

fn screen_pixel_is_set(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> bool {
    let byte = bus.read_byte(base + (y as u32 * row_bytes) + ((x as u32) / 8));
    byte & (0x80u8 >> ((x as u8) & 7)) != 0
}

fn title_region_pixels(
    bus: &MacMemoryBus,
    base: u32,
    row_bytes: u32,
    left: i16,
    right: i16,
) -> Vec<bool> {
    let mut pixels = Vec::new();
    for y in 1i16..19 {
        for x in left..right {
            pixels.push(screen_pixel_is_set(bus, base, row_bytes, x, y));
        }
    }
    pixels
}

fn changed_pixel_count(before: &[bool], after: &[bool]) -> usize {
    before
        .iter()
        .zip(after.iter())
        .filter(|(lhs, rhs)| lhs != rhs)
        .count()
}

fn clear_1bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32) {
    for offset in 0..(row_bytes * height) {
        bus.write_byte(base + offset, 0);
    }
}

fn screen_pixel_index(bus: &MacMemoryBus, base: u32, row_bytes: u32, x: i16, y: i16) -> u8 {
    bus.read_byte(base + (y as u32 * row_bytes) + x as u32)
}

fn packed_4bpp_screen_pixel_index(
    bus: &MacMemoryBus,
    base: u32,
    row_bytes: u32,
    x: i16,
    y: i16,
) -> u8 {
    let packed = bus.read_byte(base + (y as u32 * row_bytes) + (x as u32 / 2));
    if x & 1 == 0 {
        packed >> 4
    } else {
        packed & 0x0F
    }
}

fn set_packed_4bpp_screen_pixel_index(
    bus: &mut MacMemoryBus,
    screen: (u32, u32, i16, i16),
    x: i16,
    y: i16,
    pixel_index: u8,
) {
    let (base, row_bytes, width, height) = screen;
    super::super::TrapDispatcher::fb_set_pixel_index(
        bus,
        base,
        row_bytes,
        4,
        width,
        height,
        x,
        y,
        pixel_index,
    );
}

fn setup_4bpp_menu_screen(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut MacMemoryBus,
    width: u16,
    height: u16,
) -> (u32, u32) {
    let row_bytes = u32::from(width).div_ceil(2);
    let base = bus.alloc(row_bytes * u32::from(height));
    disp.set_screen_mode_for_test(base, row_bytes, width, height, 4);
    for offset in 0..(row_bytes * u32::from(height)) {
        bus.write_byte(base + offset, 0);
    }
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, base);

    let gdevice_handle = disp.ensure_main_gdevice(bus);
    bus.write_long(0x08A4, gdevice_handle); // MainDevice
    bus.write_long(0x0CC8, gdevice_handle); // TheGDevice
    (base, row_bytes)
}

fn make_8bpp_current_gdevice(bus: &mut MacMemoryBus) -> u32 {
    let mut foreign = super::super::TrapDispatcher::new();
    let base = bus.alloc(16 * 16);
    foreign.set_screen_mode_for_test(base, 16, 16, 16, 8);
    foreign.ensure_main_gdevice(bus)
}

fn remap_main_device_mono_indexes(bus: &mut MacMemoryBus, white: u8, black: u8) {
    let gdevice_handle = bus.read_long(0x08A4); // MainDevice
    let gdevice = bus.read_long(gdevice_handle);
    let pixmap_handle = bus.read_long(gdevice + 22);
    let pixmap = bus.read_long(pixmap_handle);
    let ctab_handle = bus.read_long(pixmap + 42);
    let ctab = bus.read_long(ctab_handle);
    let count = bus.read_word(ctab + 6) as u32 + 1;
    assert!(u32::from(white.max(black)) < count);

    for ordinal in 0..count {
        let entry = ctab + 8 + ordinal * 8;
        bus.write_word(entry, ordinal as u16);
        for component in 0..3 {
            bus.write_word(entry + 2 + component * 2, 0x8000);
        }
    }
    for (index, component) in [(white, 0xFFFF), (black, 0)] {
        let entry = ctab + 8 + u32::from(index) * 8;
        for offset in [2, 4, 6] {
            bus.write_word(entry + offset, component);
        }
    }
}

fn clear_8bpp_screen(bus: &mut MacMemoryBus, base: u32, row_bytes: u32, height: u32, fill: u8) {
    for offset in 0..(row_bytes * height) {
        bus.write_byte(base + offset, fill);
    }
}

fn setup_8bpp_menu_screen(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut MacMemoryBus,
    width: u16,
    height: u16,
) -> (u32, u32) {
    let row_bytes = u32::from(width);
    let base = bus.alloc(row_bytes * u32::from(height));
    disp.set_screen_mode_for_test(base, row_bytes, width, height, 8);
    clear_8bpp_screen(bus, base, row_bytes, u32::from(height), 0xEE);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, base);

    let gdevice_handle = disp.ensure_main_gdevice(bus);
    bus.write_long(0x08A4, gdevice_handle); // MainDevice
    bus.write_long(0x0CC8, gdevice_handle); // TheGDevice
    (base, row_bytes)
}

fn menu_icon_source_with_left_stripe() -> [u8; 128] {
    let mut icon = [0u8; 128];
    for row in 0..32 {
        icon[row * 4] = 0x30;
    }
    icon
}

fn sicn_source_with_left_stripe() -> [u8; 64] {
    let mut sicn = [0u8; 64];
    for row in 0..16 {
        sicn[row * 2] = 0x30;
        sicn[32 + row * 2] = 0xFF;
        sicn[32 + row * 2 + 1] = 0xFF;
    }
    sicn
}

fn sicn_source_with_right_stripe() -> [u8; 64] {
    let mut sicn = [0u8; 64];
    for row in 0..16 {
        sicn[row * 2 + 1] = 0x03;
        sicn[32 + row * 2] = 0xFF;
        sicn[32 + row * 2 + 1] = 0xFF;
    }
    sicn
}

fn write_be_word(data: &mut [u8], offset: usize, value: u16) {
    data[offset] = (value >> 8) as u8;
    data[offset + 1] = value as u8;
}

fn cicn_source_with_left_stripe(width: u16, height: u16) -> Vec<u8> {
    let row_bytes = u32::from(width).div_ceil(8);
    let mask_size = row_bytes * u32::from(height);
    let bmap_size = row_bytes * u32::from(height);
    let ctab_size = 16u32;
    let pixel_size = row_bytes * u32::from(height);
    let bmap_offset = 82 + mask_size as usize;
    let ctab_offset = bmap_offset + bmap_size as usize;
    let pixel_offset = ctab_offset + ctab_size as usize;
    let mut data = vec![0u8; pixel_offset + pixel_size as usize];

    write_be_word(&mut data, 4, row_bytes as u16);
    write_be_word(&mut data, 10, height);
    write_be_word(&mut data, 12, width);
    write_be_word(&mut data, 32, 1);

    write_be_word(&mut data, 54, row_bytes as u16);
    write_be_word(&mut data, 60, height);
    write_be_word(&mut data, 62, width);

    write_be_word(&mut data, 68, row_bytes as u16);
    write_be_word(&mut data, 74, height);
    write_be_word(&mut data, 76, width);

    write_be_word(&mut data, ctab_offset + 6, 0);

    for row in 0..usize::from(height) {
        let mask_row = 82 + row * row_bytes as usize;
        let bmap_row = bmap_offset + row * row_bytes as usize;
        let pixel_row = pixel_offset + row * row_bytes as usize;
        for col in 0..row_bytes as usize {
            data[mask_row + col] = 0xFF;
        }
        data[bmap_row] = 0x30;
        data[pixel_row] = 0x30;
    }
    data
}

fn cicn_source_with_solid_4bpp_color(source_index: u8, rgb: [u16; 3]) -> Vec<u8> {
    let width = 16u16;
    let height = 16u16;
    let pm_row_bytes = 8usize;
    let mask_row_bytes = 2usize;
    let mask_size = mask_row_bytes * usize::from(height);
    let bmap_size = mask_size;
    let ctab_offset = 82 + mask_size + bmap_size;
    let pixel_offset = ctab_offset + 16;
    let mut data = vec![0u8; pixel_offset + pm_row_bytes * usize::from(height)];

    write_be_word(&mut data, 4, 0x8000 | pm_row_bytes as u16);
    write_be_word(&mut data, 10, height);
    write_be_word(&mut data, 12, width);
    write_be_word(&mut data, 32, 4);
    write_be_word(&mut data, 34, 1);
    write_be_word(&mut data, 36, 4);

    write_be_word(&mut data, 54, mask_row_bytes as u16);
    write_be_word(&mut data, 60, height);
    write_be_word(&mut data, 62, width);
    write_be_word(&mut data, 68, mask_row_bytes as u16);
    write_be_word(&mut data, 74, height);
    write_be_word(&mut data, 76, width);

    write_be_word(&mut data, ctab_offset + 6, 0);
    write_be_word(&mut data, ctab_offset + 8, u16::from(source_index));
    write_be_word(&mut data, ctab_offset + 10, rgb[0]);
    write_be_word(&mut data, ctab_offset + 12, rgb[1]);
    write_be_word(&mut data, ctab_offset + 14, rgb[2]);

    let packed = (source_index << 4) | source_index;
    for row in 0..usize::from(height) {
        data[82 + row * mask_row_bytes] = 0xFF;
        data[82 + row * mask_row_bytes + 1] = 0xFF;
        data[pixel_offset + row * pm_row_bytes..pixel_offset + (row + 1) * pm_row_bytes]
            .fill(packed);
    }
    data
}

fn seed_menu_resource(bus: &mut crate::memory::MacMemoryBus, menu_id: i16, title: &str) -> u32 {
    let menu_res_ptr = bus.alloc(256);
    bus.write_word(menu_res_ptr, menu_id as u16);
    bus.write_word(menu_res_ptr + 2, 0);
    bus.write_word(menu_res_ptr + 4, 0);
    bus.write_long(menu_res_ptr + 6, 0);
    bus.write_long(menu_res_ptr + 10, 0xFFFF_FFFF);
    write_pstring(bus, menu_res_ptr + 14, title);
    bus.write_byte(menu_res_ptr + 15 + title.len() as u32, 0);
    menu_res_ptr
}

fn seed_mbar_resource(bus: &mut crate::memory::MacMemoryBus, menu_ids: &[i16]) -> u32 {
    let mbar_ptr = bus.alloc((2 + 2 * menu_ids.len()) as u32);
    bus.write_word(mbar_ptr, menu_ids.len() as u16);
    for (idx, menu_id) in menu_ids.iter().enumerate() {
        bus.write_word(mbar_ptr + 2 + (idx as u32) * 2, *menu_id as u16);
    }
    mbar_ptr
}

fn new_menu_with_title(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_id: i16,
    title_ptr: u32,
    title: &str,
) -> u32 {
    write_pstring(bus, title_ptr, title);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, menu_id as u16);
    assert!(
        disp.dispatch_menu(true, 0x131, cpu, bus).unwrap().is_ok(),
        "NewMenu should succeed"
    );
    bus.read_long(cpu.read_reg(Register::A7))
}

fn append_menu_data(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    data_ptr: u32,
    data: &str,
) {
    write_pstring(bus, data_ptr, data);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x133, cpu, bus).unwrap().is_ok(),
        "AppendMenu should succeed"
    );
}

fn set_menu_item_style(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    item: i16,
    style: u8,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, style as u16);
    bus.write_word(TEST_SP + 2, item as u16);
    bus.write_long(TEST_SP + 4, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x142, cpu, bus).unwrap().is_ok(),
        "SetItemStyle should succeed"
    );
}

fn calc_menu_size_for_test(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
) -> (i16, i16) {
    let menu_ptr = bus.read_long(menu_handle);
    bus.write_word(menu_ptr + 4, 0);
    bus.write_word(menu_ptr + 6, 0);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x148, cpu, bus).unwrap().is_ok(),
        "CalcMenuSize should succeed"
    );
    (
        bus.read_word(menu_ptr + 2) as i16,
        bus.read_word(menu_ptr + 4) as i16,
    )
}

fn write_rgb(bus: &mut crate::memory::MacMemoryBus, ptr: u32, rgb: (u16, u16, u16)) {
    bus.write_word(ptr, rgb.0);
    bus.write_word(ptr + 2, rgb.1);
    bus.write_word(ptr + 4, rgb.2);
}

fn write_mc_entry(
    bus: &mut crate::memory::MacMemoryBus,
    ptr: u32,
    menu_id: i16,
    item: i16,
    seed: u16,
) {
    bus.write_word(ptr, menu_id as u16);
    bus.write_word(ptr + 2, item as u16);
    write_rgb(
        bus,
        ptr + 4,
        (seed, seed.wrapping_add(1), seed.wrapping_add(2)),
    );
    write_rgb(
        bus,
        ptr + 10,
        (
            seed.wrapping_add(3),
            seed.wrapping_add(4),
            seed.wrapping_add(5),
        ),
    );
    write_rgb(
        bus,
        ptr + 16,
        (
            seed.wrapping_add(6),
            seed.wrapping_add(7),
            seed.wrapping_add(8),
        ),
    );
    write_rgb(
        bus,
        ptr + 22,
        (
            seed.wrapping_add(9),
            seed.wrapping_add(10),
            seed.wrapping_add(11),
        ),
    );
    bus.write_word(ptr + 28, 0);
}

fn write_mc_entry_colors(
    bus: &mut crate::memory::MacMemoryBus,
    ptr: u32,
    menu_id: i16,
    item: i16,
    rgb1: (u16, u16, u16),
    rgb2: (u16, u16, u16),
    rgb3: (u16, u16, u16),
    rgb4: (u16, u16, u16),
) {
    bus.write_word(ptr, menu_id as u16);
    bus.write_word(ptr + 2, item as u16);
    write_rgb(bus, ptr + 4, rgb1);
    write_rgb(bus, ptr + 10, rgb2);
    write_rgb(bus, ptr + 16, rgb3);
    write_rgb(bus, ptr + 22, rgb4);
    bus.write_word(ptr + 28, 0);
}

fn write_rgb_bytes(data: &mut [u8], offset: usize, rgb: (u16, u16, u16)) {
    write_be_word(data, offset, rgb.0);
    write_be_word(data, offset + 2, rgb.1);
    write_be_word(data, offset + 4, rgb.2);
}

fn compiled_mctb_resource(entries: &[(i16, i16, u16)]) -> Vec<u8> {
    let mut data = vec![0u8; 2 + entries.len() * MC_ENTRY_SIZE];
    write_be_word(&mut data, 0, entries.len() as u16);
    for (idx, &(menu_id, item, seed)) in entries.iter().enumerate() {
        let base = 2 + idx * MC_ENTRY_SIZE;
        write_be_word(&mut data, base, menu_id as u16);
        write_be_word(&mut data, base + 2, item as u16);
        write_rgb_bytes(
            &mut data,
            base + 4,
            (seed, seed.wrapping_add(1), seed.wrapping_add(2)),
        );
        write_rgb_bytes(
            &mut data,
            base + 10,
            (
                seed.wrapping_add(3),
                seed.wrapping_add(4),
                seed.wrapping_add(5),
            ),
        );
        write_rgb_bytes(
            &mut data,
            base + 16,
            (
                seed.wrapping_add(6),
                seed.wrapping_add(7),
                seed.wrapping_add(8),
            ),
        );
        write_rgb_bytes(
            &mut data,
            base + 22,
            (
                seed.wrapping_add(9),
                seed.wrapping_add(10),
                seed.wrapping_add(11),
            ),
        );
        write_be_word(&mut data, base + 28, seed.wrapping_add(12));
    }
    data
}

fn install_mctb_resource(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut crate::memory::MacMemoryBus,
    resource_id: i16,
    entries: &[(i16, i16, u16)],
) {
    let data = compiled_mctb_resource(entries);
    disp.install_test_resource(bus, *b"mctb", resource_id, &data);
}

fn set_mc_entries_for_test(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    entries: &[(i16, i16, u16)],
) {
    let ptr = bus.alloc((entries.len() * MC_ENTRY_SIZE) as u32);
    for (idx, &(menu_id, item, seed)) in entries.iter().enumerate() {
        write_mc_entry(bus, ptr + (idx * MC_ENTRY_SIZE) as u32, menu_id, item, seed);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, ptr);
    bus.write_word(TEST_SP + 4, entries.len() as u16);
    assert!(
        disp.dispatch_menu(true, 0x265, cpu, bus).unwrap().is_ok(),
        "SetMCEntries should succeed"
    );
}

fn get_mc_entry_ptr_for_test(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_id: i16,
    item: i16,
) -> u32 {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, item as u16);
    bus.write_word(TEST_SP + 2, menu_id as u16);
    bus.write_long(TEST_SP + 4, 0xDEADBEEF);
    assert!(
        disp.dispatch_menu(true, 0x264, cpu, bus).unwrap().is_ok(),
        "GetMCEntry should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 4,
        "GetMCEntry should pop only the menuID/menuItem arguments"
    );
    bus.read_long(TEST_SP + 4)
}

fn insert_menu_item_data(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    data_ptr: u32,
    data: &str,
    after_item: i16,
) {
    write_pstring(bus, data_ptr, data);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, after_item as u16);
    bus.write_long(TEST_SP + 2, data_ptr);
    bus.write_long(TEST_SP + 6, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x026, cpu, bus).unwrap().is_ok(),
        "InsertMenuItem should succeed"
    );
}

fn insert_menu(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
) {
    insert_menu_before_id(disp, cpu, bus, menu_handle, 0);
}

fn insert_menu_before(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    before_id: i16,
) {
    insert_menu_before_id(disp, cpu, bus, menu_handle, before_id);
}

fn insert_menu_before_id(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    before_id: i16,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, before_id as u16);
    bus.write_long(TEST_SP + 2, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x135, cpu, bus).unwrap().is_ok(),
        "InsertMenu should succeed"
    );
}

fn get_mhandle_for_id(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_id: i16,
) -> u32 {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, menu_id as u16);
    assert!(
        disp.dispatch_menu(true, 0x149, cpu, bus).unwrap().is_ok(),
        "GetMHandle should succeed"
    );
    bus.read_long(cpu.read_reg(Register::A7))
}

fn delete_menu_by_id(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_id: i16,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, menu_id as u16);
    assert!(
        disp.dispatch_menu(true, 0x136, cpu, bus).unwrap().is_ok(),
        "DeleteMenu should succeed"
    );
}

fn dispose_menu_by_handle(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x132, cpu, bus).unwrap().is_ok(),
        "DisposeMenu should succeed"
    );
}

fn menu_key_result(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    key: u8,
) -> u32 {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, key as u16);
    assert!(
        disp.dispatch_menu(true, 0x13E, cpu, bus).unwrap().is_ok(),
        "MenuKey should succeed"
    );
    bus.read_long(cpu.read_reg(Register::A7))
}

fn menu_key_result_and_stack(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    key: u8,
) -> (u32, u32) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, key as u16);
    bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);
    assert!(
        disp.dispatch_menu(true, 0x13E, cpu, bus).unwrap().is_ok(),
        "MenuKey should succeed"
    );
    (bus.read_long(TEST_SP + 2), cpu.read_reg(Register::A7))
}

fn set_item_cmd(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    item: i16,
    cmd: u8,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, cmd as u16);
    bus.write_word(TEST_SP + 2, item as u16);
    bus.write_long(TEST_SP + 4, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x04F, cpu, bus).unwrap().is_ok(),
        "SetItemCmd should succeed"
    );
}

fn set_item_mark(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    item: i16,
    mark: u8,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, mark as u16);
    bus.write_word(TEST_SP + 2, item as u16);
    bus.write_long(TEST_SP + 4, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x144, cpu, bus).unwrap().is_ok(),
        "SetItemMark should succeed"
    );
}

fn get_item_cmd(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    item: i16,
) -> u8 {
    let out_ptr = 0x306700u32;
    bus.write_word(out_ptr, 0xFFFF);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, out_ptr);
    bus.write_word(TEST_SP + 4, item as u16);
    bus.write_long(TEST_SP + 6, menu_handle);
    assert!(
        disp.dispatch_menu(true, 0x04E, cpu, bus).unwrap().is_ok(),
        "GetItemCmd should succeed"
    );
    (bus.read_word(out_ptr) & 0xFF) as u8
}

#[test]
fn packed_two_bit_menu_highlight_preserves_neighboring_pixels() {
    let mut bus = MacMemoryBus::new(8 * 1024 * 1024);
    let base = bus.alloc(1);
    bus.write_byte(base, 0b00_01_10_11);

    super::super::TrapDispatcher::hilite_packed_menu_pixel(
        &mut bus,
        (base, 1, 4, 1),
        2,
        0,
        0,
        (0, 3),
    );
    assert_eq!(bus.read_byte(base), 0b11_01_10_11);

    super::super::TrapDispatcher::hilite_packed_menu_pixel(
        &mut bus,
        (base, 1, 4, 1),
        2,
        3,
        0,
        (0, 3),
    );
    assert_eq!(bus.read_byte(base), 0b11_01_10_00);
}

// IM:I I-353: AddResMenu appends named resources of the requested type
// as enabled plain items, and skips names beginning with '.' or '%'.
#[test]
fn addresmenu_appends_named_resources_as_enabled_plain_items_and_skips_hidden_names() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 330, 0x306800, "Apple");

    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306900, "Existing");

    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::new(),
                named: std::collections::HashMap::from([
                    ((*b"DRVR", ".HiddenDA".to_string()), (101, 0)),
                    ((*b"DRVR", "%MetaDA".to_string()), (102, 0)),
                    ((*b"DRVR", "Calculator".to_string()), (103, 0)),
                    ((*b"DRVR", "Chooser".to_string()), (104, 0)),
                    ((*b"MENU", "NotDriver".to_string()), (105, 0)),
                ]),
                names_by_id: std::collections::HashMap::new(),
                attrs: std::collections::HashMap::new(),
                map_attrs: 0,
            },
        )]),
        names: std::collections::HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, u32::from_be_bytes(*b"DRVR"));
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x14D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "AddResMenu should succeed"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

    let menu = disp
        .menus
        .iter()
        .find(|m| m.handle == handle)
        .expect("menu should remain registered");
    assert_eq!(menu.items[0].text, "Existing");
    assert_eq!(menu.items.len(), 3, "two visible DRVR names should append");
    assert!(
        menu.items.iter().any(|it| it.text == "Calculator"),
        "visible DRVR names should be appended"
    );
    assert!(
        menu.items.iter().any(|it| it.text == "Chooser"),
        "visible DRVR names should be appended"
    );
    assert!(
        !menu.items.iter().any(|it| it.text == ".HiddenDA"),
        "leading '.' names must be skipped"
    );
    assert!(
        !menu.items.iter().any(|it| it.text == "%MetaDA"),
        "leading '%' names must be skipped"
    );
    for item in menu
        .items
        .iter()
        .filter(|it| it.text == "Calculator" || it.text == "Chooser")
    {
        assert!(item.enabled, "new AddResMenu items should be enabled");
        assert_eq!(item.icon, 0, "new AddResMenu items should have no icon");
        assert_eq!(item.mark, 0, "new AddResMenu items should have no mark");
        assert_eq!(item.style, 0, "new AddResMenu items should be plain style");
    }
}

#[test]
fn addresmenu_prioritizes_fond_names_loads_resources_and_exposes_builtin_fonts() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 331, 0x306A00, "Font");
    let fond_ptr = bus.alloc(1);
    let font_ptr = bus.alloc(1);
    bus.write_byte(fond_ptr, 1);
    bus.write_byte(font_ptr, 2);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::new(),
                named: std::collections::HashMap::from([
                    ((*b"FONT", "Custom Face".to_string()), (7, font_ptr)),
                    ((*b"FOND", "Custom Face".to_string()), (8, fond_ptr)),
                ]),
                names_by_id: std::collections::HashMap::new(),
                attrs: std::collections::HashMap::new(),
                map_attrs: 0,
            },
        )]),
        names: std::collections::HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });
    disp.policy.set_res_load(false);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, u32::from_be_bytes(*b"FONT"));
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x14D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "AddResMenu should succeed"
    );
    assert!(
        disp.policy.res_load(),
        "AppendResMenu must restore SetResLoad(TRUE)"
    );
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::RES_LOAD),
        0x0100
    );

    let menu = disp
        .menus
        .iter()
        .find(|menu| menu.handle == handle)
        .expect("font menu should remain registered");
    for expected in ["Chicago", "Geneva", "Monaco", "New York", "Palatino"] {
        assert!(
            menu.items.iter().any(|item| item.text == expected),
            "built-in family {expected} should appear in the Font menu"
        );
    }
    assert!(
        !menu.items.iter().any(|item| item.text == "Application"),
        "the applFont selector is not a user-facing family"
    );
    assert_eq!(
        menu.items
            .iter()
            .filter(|item| item.text == "Custom Face")
            .count(),
        1,
        "the FOND name must suppress the later FONT duplicate"
    );
    for (resource_type, id, ptr) in [(*b"FOND", 8, fond_ptr), (*b"FONT", 7, font_ptr)] {
        let resource_handle = disp.resource_handles_by_key[&(0, resource_type, id)];
        assert_eq!(bus.read_long(resource_handle), ptr);
    }
}

// IM:I I-360: SetItemStyle takes one Style value, one item index,
// and one MenuHandle argument.
#[test]
fn setitemstyle_consumes_menu_item_and_style_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 340, 0x306A10, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A20, "Open");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0005);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemStyle should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 8,
        "SetItemStyle should consume style, item, and menu arguments"
    );
}

// IM:I I-360: SetItemStyle changes the style of the addressed item.
#[test]
fn setitemstyle_updates_target_menu_item_style() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 341, 0x306A30, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A40, "Paste");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0006);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemStyle should succeed"
    );

    let menu = disp
        .menus
        .iter()
        .find(|m| m.handle == handle)
        .expect("menu should exist");
    assert_eq!(
        menu.items[0].style, 0x06,
        "SetItemStyle should store the requested style byte on the target item"
    );
}

// IM:I I-360: GetItemStyle takes VAR chStyle, item, and menu arguments.
#[test]
fn getitemstyle_consumes_menu_item_and_stylevar_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 342, 0x306A50, "View");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A60, "Status");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0003);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemStyle should succeed"
    );

    let out_ptr = 0x306A70u32;
    bus.write_word(out_ptr, 0xFFFF);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, out_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x141, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "GetItemStyle should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "GetItemStyle should consume chStyle pointer, item, and menu arguments"
    );
}

// IM:I I-360: GetItemStyle returns the current item style in chStyle.
#[test]
fn getitemstyle_writes_current_item_style_to_output_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 343, 0x306A80, "Window");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306A90, "Zoom");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0009);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x142, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemStyle should succeed"
    );

    let out_ptr = 0x306AA0u32;
    bus.write_word(out_ptr, 0xFFFF);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, out_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x141, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "GetItemStyle should succeed"
    );
    assert_eq!(
        bus.read_word(out_ptr),
        0x0009,
        "GetItemStyle should write the current style value to the chStyle output pointer"
    );
}

// IM:I I-361: SetMenuFlash takes one INTEGER count parameter.
#[test]
fn setmenuflash_consumes_count_argument() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 7);
    assert!(
        disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMenuFlash should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "SetMenuFlash should consume one INTEGER argument"
    );
}

// IM:I I-361 assembly note: SetMenuFlash stores the count in MenuFlash.
#[test]
fn setmenuflash_writes_count_to_menuflash_global() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MENU_FLASH, 3);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    assert!(
        disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMenuFlash should succeed"
    );
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::MENU_FLASH),
        1,
        "SetMenuFlash should update the MenuFlash low-memory word"
    );
}

// IM:I I-353: InsertResMenu takes afterItem, resource type, and menu
// handle arguments.
#[test]
fn insertresmenu_consumes_menu_type_and_afteritem_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 344, 0x306AB0, "Apple");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DRVR"));
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x151, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "InsertResMenu should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "InsertResMenu should consume afterItem, type, and menu arguments"
    );
}

// IM:I I-353: InsertResMenu follows afterItem placement and skips names
// beginning with '.' and '%'.
#[test]
fn insertresmenu_inserts_visible_resource_names_at_requested_position() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 345, 0x306AC0, "Apple");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x306AD0,
        "Existing;Tail",
    );

    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::new(),
                named: std::collections::HashMap::from([
                    ((*b"DRVR", ".HiddenDA".to_string()), (101, 0)),
                    ((*b"DRVR", "%MetaDA".to_string()), (102, 0)),
                    ((*b"DRVR", "Alpha".to_string()), (103, 0)),
                    ((*b"DRVR", "Beta".to_string()), (104, 0)),
                ]),
                names_by_id: std::collections::HashMap::new(),
                attrs: std::collections::HashMap::new(),
                map_attrs: 0,
            },
        )]),
        names: std::collections::HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"DRVR"));
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x151, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "InsertResMenu should succeed"
    );

    let menu = disp
        .menus
        .iter()
        .find(|m| m.handle == handle)
        .expect("menu should remain registered");
    let names: Vec<&str> = menu.items.iter().map(|it| it.text.as_str()).collect();
    assert_eq!(
        names,
        vec!["Existing", "Alpha", "Beta", "Tail"],
        "InsertResMenu should insert visible names after afterItem position"
    );
    assert!(
        !menu.items.iter().any(|it| it.text == ".HiddenDA"),
        "InsertResMenu should skip names starting with '.'"
    );
    assert!(
        !menu.items.iter().any(|it| it.text == "%MetaDA"),
        "InsertResMenu should skip names starting with '%'"
    );
}

// IM:I I-359: SetItemIcon stores the icon number for the specified item.
#[test]
fn setitemicon_sets_requested_icon_number_for_target_item() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 331, 0x306A00, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306B00, "Open");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 7);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, handle);
    assert!(
        disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemIcon should succeed"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);

    let menu = disp
        .menus
        .iter()
        .find(|m| m.handle == handle)
        .expect("menu should exist");
    assert_eq!(menu.items[0].icon, 7, "SetItemIcon should store icon byte");

    let icon_ptr = 0x306C00u32;
    bus.write_word(icon_ptr, 0xFFFF);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, icon_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x13F, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "GetItemIcon should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "GetItemIcon should pop 10 bytes from stack"
    );
    assert_eq!(bus.read_word(icon_ptr), 7);
}

// IM:I I-360: GetItemIcon returns the item's icon number (1..255),
// or 0 if no icon is associated with that item.
#[test]
fn getitemicon_returns_zero_when_item_has_no_icon() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 332, 0x306D80, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306D90, "Paste");

    let icon_ptr = 0x306DA0u32;
    bus.write_word(icon_ptr, 0xFFFF);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, icon_ptr);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, handle);
    assert!(
        disp.dispatch_menu(true, 0x13F, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "GetItemIcon should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "GetItemIcon should pop one pointer, one item index, and one menu handle"
    );
    assert_eq!(
        bus.read_word(icon_ptr),
        0,
        "GetItemIcon should write 0 when no icon is associated with the item"
    );
}

// IM:I I-361: CalcMenuSize recalculates menu dimensions and stores them
// in the menu record's menuWidth/menuHeight fields.
#[test]
fn calcmenusize_writes_recalculated_menuwidth_and_menuheight_fields() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 333, 0x306DB0, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306DC0, "Open");

    let menu_ptr = bus.read_long(handle);
    bus.write_word(menu_ptr + 2, 0);
    bus.write_word(menu_ptr + 4, 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(
        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "CalcMenuSize should succeed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 4,
        "CalcMenuSize should pop one MenuHandle argument"
    );

    let width = bus.read_word(menu_ptr + 2) as i16;
    let height = bus.read_word(menu_ptr + 4) as i16;
    assert_eq!(
        width,
        super::super::TrapDispatcher::fb_measure_string("Open", 0, 12) + 32,
        "68k CalcMenuSize should use the shared Mac OS 8.1 width policy"
    );
    assert_eq!(height, 16);
}

#[test]
fn calcmenusize_arms_custom_mdef_size_message_with_pascal_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 335, 0x306DD0, "Custom");
    let menu_ptr = bus.read_long(handle);
    let mdef_ptr = bus.alloc(2);
    let mdef_handle = bus.alloc(4);
    bus.write_word(mdef_ptr, 0x4E75);
    bus.write_long(mdef_handle, mdef_ptr);
    bus.write_long(menu_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));

    let return_pc = 0x0012_3456;
    cpu.write_reg(Register::PC, return_pc);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x148, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    let trampoline = cpu.read_reg(Register::PC);
    assert_ne!(trampoline, 0);
    assert_eq!(cpu.read_reg(Register::PC), trampoline);
    assert_eq!(cpu.read_reg(Register::A7), trampoline - 4);
    assert_eq!(bus.read_word(trampoline + 50), 80);
    assert_eq!(bus.read_long(trampoline - 4), return_pc);
    assert_eq!(bus.read_word(trampoline + 6), 2);
    assert_eq!(bus.read_long(trampoline + 10), handle);
    assert_eq!(bus.read_long(trampoline + 16), trampoline + 60);
    assert_eq!(bus.read_long(trampoline + 22), 0);
    assert_eq!(bus.read_long(trampoline + 28), trampoline + 68);
    assert_eq!(bus.read_long(trampoline + 34), mdef_ptr);
    assert!(disp.guest_calls.is_empty());
}

#[test]
fn calcmenusize_calls_the_m68k_record_from_a_fat_mdef_descriptor() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 338, 0x306DE0, "Fat");
    let menu_ptr = bus.read_long(handle);
    let descriptor = bus.alloc(
        crate::guest_procedure::ROUTINE_DESCRIPTOR_HEADER_SIZE
            + 2 * crate::guest_procedure::ROUTINE_RECORD_SIZE,
    );
    let powerpc_entry = bus.alloc(4);
    let m68k_entry = bus.alloc(2);
    let mdef_handle = bus.alloc(4);
    bus.write_word(
        descriptor,
        crate::guest_procedure::ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
    );
    bus.write_byte(
        descriptor + 2,
        crate::guest_procedure::ROUTINE_DESCRIPTOR_VERSION,
    );
    bus.write_word(descriptor + 10, 1);
    let powerpc_record = descriptor + crate::guest_procedure::ROUTINE_DESCRIPTOR_HEADER_SIZE;
    bus.write_byte(
        powerpc_record + crate::guest_procedure::ROUTINE_RECORD_ISA_OFFSET,
        crate::guest_procedure::ROUTINE_RECORD_POWERPC_ISA,
    );
    bus.write_long(
        powerpc_record + crate::guest_procedure::ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        powerpc_entry,
    );
    let m68k_record = powerpc_record + crate::guest_procedure::ROUTINE_RECORD_SIZE;
    bus.write_byte(
        m68k_record + crate::guest_procedure::ROUTINE_RECORD_ISA_OFFSET,
        crate::guest_procedure::ROUTINE_RECORD_M68K_ISA,
    );
    bus.write_long(
        m68k_record + crate::guest_procedure::ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        m68k_entry,
    );
    bus.write_word(m68k_entry, 0x4E75);
    bus.write_long(mdef_handle, descriptor);
    bus.write_long(menu_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (descriptor, *b"MDEF", 256));

    let return_pc = 0x0012_3456;
    cpu.write_reg(Register::PC, return_pc);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x148, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    let trampoline = cpu.read_reg(Register::PC);
    assert_ne!(trampoline, 0);
    assert_eq!(cpu.read_reg(Register::PC), trampoline);
    assert_eq!(bus.read_long(trampoline + 34), m68k_entry);
    assert_ne!(bus.read_long(trampoline + 34), powerpc_entry);
}

#[test]
fn calcmenusize_does_not_enter_a_powerpc_only_mdef_descriptor_as_m68k() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 339, 0x306DE8, "PPC");
    let menu_ptr = bus.read_long(handle);
    let descriptor = bus.alloc(
        crate::guest_procedure::ROUTINE_DESCRIPTOR_HEADER_SIZE
            + crate::guest_procedure::ROUTINE_RECORD_SIZE,
    );
    let powerpc_entry = bus.alloc(4);
    let mdef_handle = bus.alloc(4);
    bus.write_word(
        descriptor,
        crate::guest_procedure::ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
    );
    bus.write_byte(
        descriptor + 2,
        crate::guest_procedure::ROUTINE_DESCRIPTOR_VERSION,
    );
    bus.write_word(descriptor + 10, 0);
    let record = descriptor + crate::guest_procedure::ROUTINE_DESCRIPTOR_HEADER_SIZE;
    bus.write_byte(
        record + crate::guest_procedure::ROUTINE_RECORD_ISA_OFFSET,
        crate::guest_procedure::ROUTINE_RECORD_POWERPC_ISA,
    );
    bus.write_long(
        record + crate::guest_procedure::ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        powerpc_entry,
    );
    bus.write_long(mdef_handle, descriptor);
    bus.write_long(menu_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (descriptor, *b"MDEF", 256));

    let return_pc = 0x0012_3456;
    cpu.write_reg(Register::PC, return_pc);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x148, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert_eq!(cpu.read_reg(Register::PC), return_pc);
}

#[test]
fn classic_mdef_refuses_invalid_stack_without_publishing_a_callback() {
    for (sp, protected) in [
        (64, false),
        (TEST_SP - 1, false),
        (0xff00_0000, false),
        (TEST_SP, true),
    ] {
        let (mut disp, mut cpu, mut bus) = setup();
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 335, 0x306DD0, "Custom");
        let menu_ptr = bus.read_long(handle);
        let mdef_ptr = bus.alloc(2);
        let mdef_handle = bus.alloc(4);
        bus.write_word(mdef_ptr, 0x4e75);
        bus.write_long(mdef_handle, mdef_ptr);
        bus.write_long(menu_ptr + 6, mdef_handle);
        disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));
        let snapshot_start = TEST_SP - 160;
        let before = bus.read_bytes(snapshot_start, 160);
        if protected {
            bus.protect_readonly_code(TEST_SP - 100, 1);
        }
        cpu.write_reg(Register::PC, 0x123456);
        cpu.write_reg(Register::A7, sp);
        assert!(!disp.arm_menu_definition_to(
            &mut cpu,
            &mut bus,
            MenuDefinitionInvocation::size(handle),
            0x123454
        ));
        assert_eq!(cpu.read_reg(Register::PC), 0x123456);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.guest_calls.is_empty());
        assert_eq!(bus.read_bytes(snapshot_start, 160), before);
    }
}

#[test]
fn custom_mdef_adapter_marshals_shared_choose_invocation_scratch() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 336, 0x306DF0, "Custom");
    let menu_ptr = bus.read_long(handle);
    let mdef_ptr = bus.alloc(2);
    let mdef_handle = bus.alloc(4);
    bus.write_word(mdef_ptr, 0x4E75);
    bus.write_long(mdef_handle, mdef_ptr);
    bus.write_long(menu_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));

    cpu.write_reg(Register::PC, 0x0012_3456);
    cpu.write_reg(Register::A7, TEST_SP);
    let invocation = MenuDefinitionInvocation {
        message: crate::menu_manager::MenuDefinitionMessage::Choose,
        menu_handle: handle,
        menu_rect: (20, 30, 120, 180),
        hit_point: 0x0050_0060,
        which_item: 4,
    };
    assert!(disp.arm_menu_definition(&mut cpu, &mut bus, invocation));

    let trampoline = cpu.read_reg(Register::PC);
    assert_eq!(bus.read_word(trampoline + 6), 1);
    assert_eq!(bus.read_long(trampoline + 10), handle);
    assert_eq!(bus.read_long(trampoline + 16), trampoline + 60);
    assert_eq!(bus.read_long(trampoline + 22), 0x0050_0060);
    assert_eq!(bus.read_long(trampoline + 28), trampoline + 68);
    assert_eq!(
        bus.read_bytes(trampoline + 60, 10),
        invocation.scratch_bytes()
    );
}

#[test]
fn classic_tracking_retains_original_return_for_both_menu_entry_forms() {
    use crate::guest_call::{MenuTrackingCall, MenuTrackingOrigin};
    use crate::memory::globals::addr;
    use crate::menu_manager::{MenuTrackingRequest, PopupMenuRequest};
    for opcode in [0xa93d, 0xad3d, 0xa80b, 0xac0b] {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        disp.menu_bar_hidden = false;
        bus.write_word(addr::MBAR_HEIGHT, 20);
        bus.write_word(addr::MENU_FLASH, 0);
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 337, 0x306E40, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306E80, "One;Two");
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        disp.draw_menu_bar_to_fb(&mut bus);
        let auto_pop = opcode & 0x0400 != 0;
        let popup = opcode & !0x0400 == 0xa80b;
        let trap_pc = 0x0012_3600;
        let parameters = TEST_SP + if auto_pop { 4 } else { 0 };
        let return_pc = trap_pc + if auto_pop { 0x100 } else { 2 };
        if auto_pop {
            bus.write_long(TEST_SP, return_pc);
        }
        let request = if popup {
            bus.write_word(parameters, 1);
            bus.write_word(parameters + 2, 30);
            bus.write_word(parameters + 4, 30);
            bus.write_long(parameters + 6, handle);
            MenuTrackingRequest::PopUp(PopupMenuRequest {
                menu_handle: handle,
                anchor: (30, 30),
                requested_item: 1,
            })
        } else {
            let initial_point = (10u32 << 16) | 12;
            bus.write_long(parameters, initial_point);
            MenuTrackingRequest::MenuSelect { initial_point }
        };
        bus.write_byte(addr::MB_STATE, 0);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch(opcode, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            disp.menu_tracking.context().call,
            Some(MenuTrackingCall {
                request,
                origin: MenuTrackingOrigin::M68k {
                    stack_pointer: parameters,
                    return_address: return_pc
                },
            }),
            "opcode {opcode:04x}"
        );
        let state = disp
            .menu_tracking
            .as_ref()
            .expect("standard menu remains live until release");
        bus.write_word(addr::MOUSE_LOC2, (state.popup_top + 8) as u16);
        bus.write_word(addr::MOUSE_LOC2 + 2, (state.popup_left + 8) as u16);
        bus.write_byte(addr::MB_STATE, 0x80);
        assert!(disp.resume_menu_tracking(&mut cpu, &mut bus).is_some());
        let result_slot = parameters + if popup { 10 } else { 4 };
        assert_eq!(
            bus.read_long(result_slot),
            (337u32 << 16) | 1,
            "opcode {opcode:04x}"
        );
        assert_eq!(cpu.read_reg(Register::A7), result_slot);
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert!(disp.menu_tracking.is_none());
        assert_eq!(disp.menu_tracking.context().call, None);
        assert!(disp.guest_calls.is_empty());
    }
}

#[test]
fn nested_classic_menu_select_preserves_outer_tracking_and_return() {
    for (nested_point, nested_result) in [(u32::MAX, 0), ((10u32 << 16) | 12, (337u32 << 16) | 1)] {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        disp.menu_bar_hidden = false;
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 337, 0x306E40, "Custom");
        let menu_ptr = bus.read_long(handle);
        bus.write_word(menu_ptr + 2, 80);
        bus.write_word(menu_ptr + 4, 32);
        let mdef_ptr = bus.alloc(96);
        let mdef_handle = bus.alloc(4);
        bus.write_word(mdef_ptr, 0x4E75);
        bus.write_long(mdef_handle, mdef_ptr);
        bus.write_long(menu_ptr + 6, mdef_handle);
        disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
        disp.draw_menu_bar_to_fb(&mut bus);
        let original_port = *disp.current_port;

        let mut cpu = crate::cpu::M68kCpu::new();
        let marker = bus.alloc(8);
        bus.write_long(marker + 4, u32::MAX);
        let code = [
            0x206f,
            4, // whichItem pointer
            0x2039,
            (marker >> 16) as u16,
            marker as u16,
            0x6620, // only the first callback enters the nested MenuSelect
            0x23fc,
            0,
            1,
            (marker >> 16) as u16,
            marker as u16,
            0x2f08, // retain outer whichItem pointer
            0x598f, // reserve nested result
            0x2f3c,
            (nested_point >> 16) as u16,
            nested_point as u16,
            0xa93d,
            0x201f, // consume nested result
            0x23c0,
            ((marker + 4) >> 16) as u16,
            (marker + 4) as u16,
            0x205f,
            0x30bc,
            1,
            0x4e74,
            18,
        ];
        for (index, word) in code.into_iter().enumerate() {
            bus.write_word(mdef_ptr + index as u32 * 2, word);
        }
        let trap_pc = 0x0012_3600;
        bus.write_word(trap_pc, 0xa93d);
        bus.write_long(TEST_SP, (10u32 << 16) | 12);
        bus.write_word(crate::memory::globals::addr::MENU_FLASH, 0);
        bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 28);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 20);
        cpu.write_reg(Register::PC, trap_pc);
        cpu.write_reg(Register::A7, TEST_SP);
        for _ in 0..1024 {
            match cpu.step(&mut bus) {
                crate::cpu::StepResult::Ok => {}
                crate::cpu::StepResult::Aline(trap) => {
                    disp.dispatch(trap, &mut cpu, &mut bus).unwrap();
                }
                _ => panic!("unexpected instruction during nested menu tracking"),
            }
            if cpu.read_reg(Register::PC) == trap_pc + 2
                && cpu.read_reg(Register::A7) == TEST_SP + 4
            {
                break;
            }
        }
        assert_eq!(
            bus.read_long(marker + 4),
            nested_result,
            "nested selection result"
        );
        assert_eq!(cpu.read_reg(Register::PC), trap_pc + 2);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
        assert_eq!(bus.read_long(TEST_SP + 4), (337u32 << 16) | 1);
        assert_eq!(*disp.current_port, original_port);
        assert!(disp.guest_calls.is_empty());
        assert!(disp.menu_tracking.is_none());
    }
}

#[test]
fn menuselect_retains_custom_mdef_draw_and_choose_until_release() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 337, 0x306E40, "Custom");
    let menu_ptr = bus.read_long(handle);
    bus.write_word(menu_ptr + 2, 80);
    bus.write_word(menu_ptr + 4, 32);
    let mdef_ptr = bus.alloc(2);
    let mdef_handle = bus.alloc(4);
    bus.write_word(mdef_ptr, 0x4E75);
    bus.write_long(mdef_handle, mdef_ptr);
    bus.write_long(menu_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    disp.draw_menu_bar_to_fb(&mut bus);
    let original_port = *disp.current_port;

    let trap_pc = 0x0012_3400;
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, 12);
    bus.write_byte(crate::memory::globals::addr::MB_STATE, 0);
    bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 28);
    bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 20);

    assert!(disp
        .step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let trampoline = cpu.read_reg(Register::PC);
    assert_eq!(bus.read_word(trampoline + 6), 0);
    assert_eq!(bus.read_long(trampoline - 4), trampoline + 52);
    assert!(disp.is_menu_definition_callback_pending());
    assert_eq!(disp.guest_calls.len(), 1);
    assert_eq!(*disp.current_port, disp.window_manager_cport);

    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    assert!(disp
        .step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(bus.read_word(trampoline + 6), 1);
    assert_eq!(bus.read_long(trampoline + 22), 0x001C_0014);
    assert_eq!(bus.read_word(trampoline + 68), 0);

    bus.write_word(trampoline + 68, 2);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    assert!(disp
        .step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menu_tracking.as_ref().unwrap().flash_remaining, 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    disp.menu_tracking
        .with_tracking_mut(|tracking| {
            tracking.flash_deadline = tracking.flash_tick.unwrap_or(0);
        })
        .unwrap();

    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(trampoline + 6), 1);
    let (top, left, _, _) = disp.active_menu_definition().unwrap().menu_rect();
    assert_eq!(
        bus.read_long(trampoline + 22),
        (u32::from((top - 1) as u16) << 16) | u32::from(left as u16)
    );
    assert_eq!(bus.read_word(trampoline + 68), 2);

    bus.write_word(trampoline + 68, 0);
    disp.menu_tracking
        .with_tracking_mut(|tracking| {
            tracking.flash_remaining = 1;
            tracking.flash_deadline = tracking.flash_tick.unwrap_or(0);
        })
        .unwrap();
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP + 4), (337u32 << 16) | 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(disp.menu_tracking, None);
    assert_eq!(disp.menu_tracking.context().definition, None);
    assert!(disp.guest_calls.is_empty());
    assert_eq!(*disp.current_port, original_port);
}

#[test]
fn popup_menu_select_runs_custom_popup_draw_and_choose_sequence() {
    for (top, left, requested_item) in [
        (40i16, 30i16, 4i16),
        (-40, -30, -1),
        (i16::MIN, i16::MAX, 0),
    ] {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 338, 0x306E80, "Custom");
        let menu_ptr = bus.read_long(handle);
        let mdef_ptr = bus.alloc(2);
        let mdef_handle = bus.alloc(4);
        bus.write_word(mdef_ptr, 0x4E75);
        bus.write_long(mdef_handle, mdef_ptr);
        bus.write_long(menu_ptr + 6, mdef_handle);
        disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));

        let trap_pc = 0x0012_3500;
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        bus.write_word(TEST_SP, requested_item as u16);
        bus.write_word(TEST_SP + 2, left as u16);
        bus.write_word(TEST_SP + 4, top as u16);
        bus.write_long(TEST_SP + 6, handle);
        bus.write_byte(crate::memory::globals::addr::MB_STATE, 0);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 52);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 45);

        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let trampoline = cpu.read_reg(Register::PC);
        assert_eq!(bus.read_word(trampoline + 6), 3);
        assert_eq!(
            bus.read_long(trampoline + 22),
            (u32::from(top as u16) << 16) | u32::from(left as u16)
        );
        assert_eq!(bus.read_word(trampoline + 68), requested_item as u16);

        for (offset, value) in [(0, 40i16), (2, 30), (4, 72), (6, 110), (8, 0)] {
            bus.write_word(trampoline + 60 + offset, value as u16);
        }
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(trampoline + 6), 0);
        assert_eq!(
            bus.read_bytes(trampoline + 60, 8),
            [0, 40, 0, 30, 0, 72, 0, 110]
        );

        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(trampoline + 6), 1);
        assert_eq!(bus.read_long(trampoline + 22), 0x0034_002D);

        bus.write_word(trampoline + 68, 2);
        bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.menu_tracking.as_ref().unwrap().flash_remaining, 6);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        disp.menu_tracking
            .with_tracking_mut(|tracking| {
                tracking.flash_deadline = tracking.flash_tick.unwrap_or(0);
            })
            .unwrap();

        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(trampoline + 6), 1);
        assert_eq!(bus.read_long(trampoline + 22), 0x0027_001E);
        assert_eq!(bus.read_word(trampoline + 68), 2);

        bus.write_word(trampoline + 68, 0);
        disp.menu_tracking
            .with_tracking_mut(|tracking| {
                tracking.flash_remaining = 1;
                tracking.flash_deadline = tracking.flash_tick.unwrap_or(0);
            })
            .unwrap();
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(
            Register::A7,
            if disp.guest_calls.depth() == 0 {
                TEST_SP
            } else {
                TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            },
        );
        disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(TEST_SP + 10), (338u32 << 16) | 2);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(disp.menu_tracking, None);
        assert_eq!(disp.menu_tracking.context().definition, None);
    }
}

#[test]
fn calcmenusize_caps_oversized_menu_height_from_profile_geometry() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_screen_mode_for_test(0x0040_0000, 100, 800, 600, 1);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 334, 0x306DE0, "File");
    let description = (0..40).map(|_| "A").collect::<Vec<_>>().join(";");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x306E00,
        &description,
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(bus.read_long(handle) + 4), 560);
}

#[test]
fn calcmenusize_recomputes_dimensions_after_menu_contents_change() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 334, 0x306DD0, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x306DE0, "Open");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(
        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "initial CalcMenuSize should succeed"
    );
    let menu_ptr = bus.read_long(handle);
    let width_before = bus.read_word(menu_ptr + 2) as i16;
    let height_before = bus.read_word(menu_ptr + 4) as i16;

    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x306DF0,
        "This is a much longer menu item label",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(
        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "recalc CalcMenuSize should succeed"
    );
    let width_after = bus.read_word(menu_ptr + 2) as i16;
    let height_after = bus.read_word(menu_ptr + 4) as i16;
    assert!(
        width_after > width_before,
        "adding a longer item should increase menuWidth after CalcMenuSize"
    );
    assert!(
        height_after > height_before,
        "adding an item should increase menuHeight after CalcMenuSize"
    );
}

fn calcmenusize_results_for_theme(theme_id: UiThemeId) -> (i16, i16, u16, u32, u16) {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_ui_theme_id(theme_id);
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 335, 0x306E00, "View");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x306E40,
        "Open/O;!XMarked;(-;Normal Icon;Color Icon;Small Icon",
    );
    disp.menus[0].items[3].icon = 7;
    disp.install_test_resource(
        &mut bus,
        *b"ICON",
        263,
        &menu_icon_source_with_left_stripe(),
    );
    disp.menus[0].items[4].icon = 8;
    disp.install_test_resource(
        &mut bus,
        *b"cicn",
        264,
        &cicn_source_with_left_stripe(24, 20),
    );
    disp.menus[0].items[5].icon = 9;
    disp.menus[0].items[5].key_equiv = MENU_KEY_SMALL_ICON;
    disp.install_test_resource(&mut bus, *b"SICN", 265, &sicn_source_with_left_stripe());

    let menu_ptr = bus.read_long(handle);
    bus.write_word(menu_ptr + 2, 0);
    bus.write_word(menu_ptr + 4, 0);
    bus.write_word(menu_ptr + 6, 0x1357);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    bus.write_word(TEST_SP + 4, 0xCAFE);

    disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    (
        bus.read_word(menu_ptr + 2) as i16,
        bus.read_word(menu_ptr + 4) as i16,
        bus.read_word(menu_ptr + 6),
        cpu.read_reg(Register::A7),
        bus.read_word(TEST_SP + 4),
    )
}

#[test]
fn systemless_theme_does_not_change_calcmenusize_menu_record_fields() {
    // IM:I I-361: CalcMenuSize recalculates a changed menu and stores the
    // horizontal and vertical dimensions in the menu record. Theme chrome
    // must not alter those guest-visible Menu Manager fields or protocol.
    let classic = calcmenusize_results_for_theme(UiThemeId::ClassicSystem7);
    let themed = calcmenusize_results_for_theme(UiThemeId::SystemlessDefault);

    assert!(
        classic.0 >= 100,
        "classic CalcMenuSize should retain the standard minimum menu width"
    );
    assert_eq!(
        classic.1, 108,
        "classic CalcMenuSize should sum standard, normal ICON, cicn, and SICN row heights"
    );
    assert_eq!(
            classic,
            (classic.0, 108, 0x1357, TEST_SP + 4, 0xCAFE),
            "CalcMenuSize should write menuWidth/menuHeight, preserve adjacent fields, and pop one MenuHandle"
        );
    assert_eq!(
        themed, classic,
        "systemless-default must not change CalcMenuSize menu record geometry or stack protocol"
    );
}

// IM:I I-193: PinRect returns the point unchanged when it is already in
// the rectangle; result packs v in high word and h in low word.
#[test]
fn pinrect_inside_point_returns_original_coordinates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let rect_ptr = 0x306D00u32;
    bus.write_word(rect_ptr, 10); // top
    bus.write_word(rect_ptr + 2, 20); // left
    bus.write_word(rect_ptr + 4, 40); // bottom
    bus.write_word(rect_ptr + 6, 80); // right

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 15); // v
    bus.write_word(TEST_SP + 2, 25); // h
    bus.write_long(TEST_SP + 4, rect_ptr);
    bus.write_long(TEST_SP + 8, 0xDEADBEEF);
    assert!(
        disp.dispatch_menu(true, 0x14E, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "PinRect should succeed"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(bus.read_long(TEST_SP + 8), ((15u32) << 16) | 25u32);
}

// IM:I I-193: PinRect clamps out-of-bounds coordinates to the nearest
// interior pixel of the rectangle.
#[test]
fn pinrect_outside_point_clamps_to_nearest_interior_pixel() {
    let (mut disp, mut cpu, mut bus) = setup();
    let rect_ptr = 0x306E00u32;
    bus.write_word(rect_ptr, 10); // top
    bus.write_word(rect_ptr + 2, 20); // left
    bus.write_word(rect_ptr + 4, 40); // bottom
    bus.write_word(rect_ptr + 6, 80); // right

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 100); // v (below)
    bus.write_word(TEST_SP + 2, (-5i16) as u16); // h (left)
    bus.write_long(TEST_SP + 4, rect_ptr);
    bus.write_long(TEST_SP + 8, 0xDEADBEEF);
    assert!(
        disp.dispatch_menu(true, 0x14E, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "PinRect should succeed"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(
        bus.read_long(TEST_SP + 8),
        ((39u32) << 16) | 20u32,
        "v clamps to bottom-1 and h clamps to left"
    );
}

// IM:I I-475: DeltaPoint subtracts the coordinates of ptB from those
// of ptA. The low-order horizontal-difference word must not bleed
// into the high-order vertical-difference word — each word is an
// independent signed 16-bit subtraction packed into one LONGINT.
// Guard against any future "fix" that changes the cast such that
// dh's negative sign-extension corrupts dv.
#[test]
fn deltapoint_negative_delta_packs_signed_words_without_sign_bleed() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A7, TEST_SP);
    // ptB.v = 5, ptB.h = 10
    bus.write_word(TEST_SP, 5);
    bus.write_word(TEST_SP + 2, 10);
    // ptA.v = 0, ptA.h = 0
    bus.write_word(TEST_SP + 4, 0);
    bus.write_word(TEST_SP + 6, 0);
    bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
    assert!(
        disp.dispatch_menu(true, 0x14F, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DeltaPoint should succeed"
    );
    // dv = 0 - 5 = -5 → high word 0xFFFB
    // dh = 0 - 10 = -10 → low word 0xFFF6
    // result = 0xFFFB_FFF6 (no bleed: high word stays 0xFFFB).
    assert_eq!(bus.read_long(TEST_SP + 8), 0xFFFB_FFF6u32);
}

// IM:I I-475: DeltaPoint follows the Pascal FUNCTION calling
// convention with two 4-byte Point arguments and a 4-byte LONGINT
// result. The trap pops the 8 argument bytes (A7 += 8) and writes
// the result at the former SP+8. Guard against future stack-frame
// changes (8-byte pop must not creep up or down).
#[test]
fn deltapoint_consumes_eight_arg_bytes_and_writes_function_result_slot() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A7, TEST_SP);
    // ptB.v = 30, ptB.h = 25
    bus.write_word(TEST_SP, 30);
    bus.write_word(TEST_SP + 2, 25);
    // ptA.v = 50, ptA.h = 40
    bus.write_word(TEST_SP + 4, 50);
    bus.write_word(TEST_SP + 6, 40);
    // Pre-poison the result slot and a sentinel beyond it to guard
    // against any future "fix" that writes past the LONGINT slot.
    bus.write_long(TEST_SP + 8, 0xDEAD_BEEF);
    bus.write_long(TEST_SP + 12, 0xCAFE_BABE);
    assert!(
        disp.dispatch_menu(true, 0x14F, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DeltaPoint should succeed"
    );
    // A7 advanced by 8: SP+8 is the new top, which holds the result.
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
    assert_eq!(bus.read_long(TEST_SP + 8), ((20u32) << 16) | 15u32);
    // The sentinel past the result slot is preserved (no over-write).
    assert_eq!(bus.read_long(TEST_SP + 12), 0xCAFE_BABEu32);
}

// parse_appendmenu_items handles the IM:I I-358 meta-character set
// across ';'-separated items.
#[test]
fn parse_appendmenu_items_splits_semicolons_and_parses_metachars() {
    let raw = b"New/N;Open/O;(-;Quit/Q";
    let items = parse_appendmenu_items(raw);
    assert_eq!(items.len(), 4, "expected 4 items, got {:?}", items);

    assert_eq!(items[0].text, "New");
    assert_eq!(items[0].key_equiv, b'N');
    assert!(items[0].enabled);

    assert_eq!(items[1].text, "Open");
    assert_eq!(items[1].key_equiv, b'O');
    assert!(items[1].enabled);

    // "(-" — '(' disables the item, leaving "-" as the text
    // (the conventional separator-line marker).
    assert_eq!(items[2].text, "-");
    assert!(!items[2].enabled, "'(' must disable the item");

    assert_eq!(items[3].text, "Quit");
    assert_eq!(items[3].key_equiv, b'Q');
}

#[test]
fn parse_appendmenu_items_handles_marks_icons_and_styles() {
    let raw = b"BoldItalic<B<I;Marked!\x12\rIconItem^5";
    let items = parse_appendmenu_items(raw);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].text, "BoldItalic");
    assert_eq!(items[0].style, 0x03);
    assert_eq!(items[1].text, "Marked");
    assert_eq!(items[1].mark, 0x12);
    assert_eq!(items[2].text, "IconItem");
    assert_eq!(items[2].icon, 5);
}

#[test]
fn parse_appendmenu_items_handles_single_item_no_separator() {
    let raw = b"Solo";
    let items = parse_appendmenu_items(raw);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text, "Solo");
}

#[test]
fn popup_menu_item_title_prefers_live_menu_items() {
    let (mut disp, _cpu, bus) = setup();
    disp.menus.push(Menu {
        id: 1008,
        title: "Squadies1".to_string(),
        items: vec![
            MenuItem {
                text: "Brix".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
            MenuItem {
                text: "Ryan".to_string(),
                icon: 0,
                key_equiv: 0,
                mark: 0,
                style: 0,
                enabled: true,
            },
        ],
        enabled: true,
        handle: 0x1234,
        in_menu_bar: false,
        hierarchical: false,
        visible_in_menu_bar: false,
    });

    assert_eq!(
        disp.popup_menu_item_title(&bus, 1008, 2).as_deref(),
        Some("Ryan")
    );
}

// 0x130 — InitMenus: initializes the live MenuCInfo handle if needed.
#[test]
fn initmenus_procedure_call_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let result = disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitMenus should be handled");
    assert!(result.unwrap().is_ok(), "InitMenus should succeed");
    assert_ne!(
        bus.read_long(crate::memory::globals::addr::MENU_C_INFO),
        0,
        "InitMenus should seed the live MenuCInfo handle"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp_before);
}

// IM:V 1986 p. V-234 and MTE 1992 p. 3-156: InitMenus attempts to load
// 'mctb' resource 0 into the application's menu color information table.
#[test]
fn initmenus_autoloads_mctb_zero_resource_into_menucinfo() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_mctb_resource(&mut disp, &mut bus, 0, &[(0, 0, 0x1100), (601, 0, 0x1200)]);

    let result = disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitMenus should be handled");
    assert!(result.unwrap().is_ok(), "InitMenus should succeed");

    let bar_entry = get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0);
    assert_ne!(
        bar_entry, 0,
        "InitMenus should autoload the mctb=0 menu-bar entry"
    );
    assert_eq!(
        bus.read_word(bar_entry + 4),
        0x1100,
        "compiled mctb RGB1 should be copied into the live MCEntry"
    );
    assert_eq!(
        bus.read_word(bar_entry + 28),
        0x110C,
        "compiled mctb entries should preserve their reserved word"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 601, 0),
        0,
        "InitMenus should autoload all declared mctb=0 entries"
    );
}

// 0x131 — NewMenu: pops 6 bytes, writes handle at SP+6.
#[test]
fn test_new_menu() {
    let (mut disp, mut cpu, mut bus) = setup();
    let result = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewMenu should be handled");
    assert!(result.unwrap().is_ok(), "NewMenu should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 6, "NewMenu should pop 6 bytes from stack");
    let handle = bus.read_long(sp);
    assert_ne!(handle, 0, "NewMenu should write a non-zero handle");
}

// IM:I I-352 and MTE 1992 p. 3-105: NewMenu creates a menu record
// with the requested menu ID and title in menuData.
#[test]
fn newmenu_sets_menuid_and_copies_title_into_menu_data() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 230i16;
    let title = "Tools";
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B100, title);
    assert_ne!(handle, 0, "NewMenu should return a non-NIL handle");

    let menu_ptr = bus.read_long(handle);
    assert_ne!(menu_ptr, 0, "NewMenu handle should dereference to MenuInfo");
    assert_eq!(
        bus.read_word(menu_ptr) as i16,
        menu_id,
        "menuID field should match NewMenu(menuID)"
    );
    assert_eq!(
        bus.read_byte(menu_ptr + 14),
        title.len() as u8,
        "menuData title length should match input Str255 length"
    );
    assert_eq!(
        bus.read_bytes(menu_ptr + 15, title.len()),
        title.as_bytes().to_vec(),
        "menuData title bytes should match input Str255 bytes"
    );
    assert_eq!(
        bus.read_byte(menu_ptr + 15 + title.len() as u32),
        0,
        "menuData should terminate items with an empty item string"
    );
}

#[test]
fn newmenu_allocates_the_complete_maximum_length_title_record() {
    let (mut disp, mut cpu, mut bus) = setup();
    let title = "A".repeat(255);
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 231, 0x30B200, &title);
    let menu_ptr = bus.read_long(handle);

    assert_eq!(bus.get_alloc_size(menu_ptr), Some(271));
    assert_eq!(bus.read_byte(menu_ptr + 14), 255);
    assert_eq!(bus.read_byte(menu_ptr + 270), 0);
}

#[test]
fn newmenu_installs_callable_standard_mdef_handle() {
    // Inside Macintosh Volume I, I-352: NewMenu stores a handle to the
    // standard menu definition procedure in MenuInfo.menuProc.
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 232, 0x30B300, "Window");
    let menu_ptr = bus.read_long(handle);
    let mdef_handle = bus.read_long(menu_ptr + 6);

    assert_ne!(
        mdef_handle, 0,
        "NewMenu must not leave the menuProc field NIL"
    );
    let mdef_ptr = bus.read_long(mdef_handle);
    assert_ne!(mdef_ptr, 0, "standard MDEF handle must be loaded");
    assert_eq!(
        bus.read_word(mdef_ptr),
        0x205F,
        "standard MDEF shim should begin by recovering the JSR return address"
    );
}

// MTE 1992 p. 3-105: NewMenu does not insert into the current menu list;
// InsertMenu is required before GetMHandle can find the menu by ID.
#[test]
fn newmenu_requires_insertmenu_before_getmhandle_finds_menu() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 231i16;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B200, "Tools");

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
        0,
        "GetMHandle should return NIL before InsertMenu adds NewMenu to current list"
    );

    insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
        handle,
        "GetMHandle should return the NewMenu handle after InsertMenu"
    );
}

// IM:V 1986 pp. V-228–V-230: applications may inspect the MenuList
// low-memory global directly. Keep its DynamicMenuList records in sync
// with InsertMenu and DeleteMenu, not just the host-side menu model.
#[test]
fn insert_and_delete_menu_sync_the_guest_dynamic_menu_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    assert!(
        disp.dispatch_menu(true, 0x130, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "InitMenus should succeed"
    );

    let menu_list = bus.read_long(crate::memory::globals::addr::MENU_LIST);
    assert_ne!(menu_list, 0, "InitMenus should install a MenuList handle");
    let empty_list = bus.read_long(menu_list);
    assert_ne!(empty_list, 0, "MenuList handle should dereference");
    assert_eq!(bus.read_word(empty_list), 0, "lastMenu should start empty");
    assert_eq!(
        bus.read_word(empty_list + 6),
        0,
        "lastHMenu should start empty"
    );

    let menu_id = 231i16;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B200, "Home");
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    let inserted_list = bus.read_long(menu_list);
    assert_eq!(
        bus.read_word(inserted_list),
        6,
        "one regular MenuRec should occupy six bytes"
    );
    assert_eq!(
        bus.read_long(inserted_list + 6),
        handle,
        "MenuRec should expose the inserted menu handle"
    );
    assert_eq!(
        bus.read_word(inserted_list + 12),
        0,
        "lastHMenu should follow the regular MenuRec"
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, menu_id);
    let deleted_list = bus.read_long(menu_list);
    assert_eq!(
        bus.read_word(deleted_list),
        0,
        "DeleteMenu should remove the regular MenuRec"
    );
    assert_eq!(
        bus.read_word(deleted_list + 6),
        0,
        "lastHMenu should return to the empty-list offset"
    );
}

#[test]
fn insert_and_delete_menu_use_shared_order_and_hierarchical_precedence() {
    let (mut disp, mut cpu, mut bus) = setup();
    let first = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 100, 0x30B300, "First");
    let last = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x30B340, "Last");
    let middle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 200, 0x30B380, "Middle");
    let colliding_submenu =
        new_menu_with_title(&mut disp, &mut cpu, &mut bus, 200, 0x30B3C0, "Submenu");

    insert_menu(&mut disp, &mut cpu, &mut bus, first);
    insert_menu(&mut disp, &mut cpu, &mut bus, last);
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, middle, 300);
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, colliding_submenu, -1);

    let menu_list_handle = bus.read_long(crate::memory::globals::addr::MENU_LIST);
    let inserted = super::menu_list_from_memory(&bus, menu_list_handle).unwrap();
    assert_eq!(
        inserted.regular_handles().collect::<Vec<_>>(),
        vec![first, middle, last],
        "InsertMenu must place a regular menu before the requested ID"
    );
    assert_eq!(
        inserted.hierarchical_handles().collect::<Vec<_>>(),
        vec![colliding_submenu]
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 200),
        colliding_submenu,
        "GetMHandle must resolve a colliding hierarchical ID first"
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, 200);
    let after_submenu = super::menu_list_from_memory(&bus, menu_list_handle).unwrap();
    assert!(after_submenu.hierarchical.is_empty());
    assert_eq!(
        after_submenu.regular_handles().collect::<Vec<_>>(),
        vec![first, middle, last],
        "DeleteMenu must remove a colliding hierarchical ID before a regular one"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 200),
        middle
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, 200);
    let after_regular = super::menu_list_from_memory(&bus, menu_list_handle).unwrap();
    assert_eq!(
        after_regular.regular_handles().collect::<Vec<_>>(),
        vec![first, last]
    );
}

#[test]
fn submenu_resolution_uses_only_the_current_hierarchical_partition() {
    // MTE 1992 pp. 3-53--3-55: MenuSelect resolves the ID stored by a
    // hierarchical item only in the submenu portion of the current list.
    let (mut disp, mut cpu, mut bus) = setup();
    let root = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x30B500, "Root");
    append_menu_data(&mut disp, &mut cpu, &mut bus, root, 0x30B540, "Child");
    set_item_cmd(&mut disp, &mut cpu, &mut bus, root, 1, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, root, 1, 138);
    let child = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 138, 0x30B580, "Child");
    append_menu_data(&mut disp, &mut cpu, &mut bus, child, 0x30B5C0, "Choice");
    insert_menu(&mut disp, &mut cpu, &mut bus, root);

    let root_index = disp
        .menus
        .iter()
        .position(|menu| menu.handle == root)
        .unwrap();
    assert_eq!(
        disp.submenu_menu_index_for_parent_item(&bus, root_index, 1),
        None,
        "a detached menu record must not resolve as a submenu"
    );

    insert_menu(&mut disp, &mut cpu, &mut bus, child);
    let root_index = disp
        .menus
        .iter()
        .position(|menu| menu.handle == root)
        .unwrap();
    assert_eq!(
        disp.submenu_menu_index_for_parent_item(&bus, root_index, 1),
        None,
        "a regular-partition ID match must not resolve as a submenu"
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, 138);
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, child, -1);
    let root_index = disp
        .menus
        .iter()
        .position(|menu| menu.handle == root)
        .unwrap();
    let child_index = disp
        .menus
        .iter()
        .position(|menu| menu.handle == child)
        .unwrap();
    assert_eq!(
        disp.submenu_menu_index_for_parent_item(&bus, root_index, 1),
        Some(child_index),
        "the matching hierarchical-partition handle must resolve"
    );

    let root_ptr = bus.read_long(root);
    let first_item = root_ptr + 15 + u32::from(bus.read_byte(root_ptr + 14));
    let command = first_item + 2 + u32::from(bus.read_byte(first_item));
    bus.write_byte(command, b'C');
    assert!(disp.menus[root_index].items[0].key_equiv == 0x1B);
    assert_eq!(
        disp.submenu_menu_index_for_parent_item(&bus, root_index, 1),
        None,
        "live guest item bytes must override the stale presentation cache"
    );
    bus.write_byte(command, 0x1B);
    assert_eq!(
        disp.submenu_menu_index_for_parent_item(&bus, root_index, 1),
        Some(child_index)
    );
}

#[test]
fn insert_menu_uses_live_guest_list_order_instead_of_the_presentation_cache() {
    let (mut disp, mut cpu, mut bus) = setup();
    let first = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 100, 0x30B400, "First");
    let second = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 200, 0x30B440, "Second");
    let inserted = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 150, 0x30B480, "Inserted");
    insert_menu(&mut disp, &mut cpu, &mut bus, first);
    insert_menu(&mut disp, &mut cpu, &mut bus, second);

    let menu_list_handle = bus.read_long(crate::memory::globals::addr::MENU_LIST);
    let menu_list_ptr = bus.read_long(menu_list_handle);
    bus.write_long(menu_list_ptr + 6, second);
    bus.write_long(menu_list_ptr + 12, first);
    bus.write_word(menu_list_ptr + 2, 120);
    bus.write_word(menu_list_ptr + 10, 40);
    bus.write_word(menu_list_ptr + 16, 80);
    assert_eq!(
        disp.current_menu_title_regions_with_indices(&bus)
            .into_iter()
            .map(|(index, region)| (disp.menus[index].handle, region.left, region.right))
            .collect::<Vec<_>>(),
        vec![(second, 40, 80), (first, 80, 120)],
        "68k title geometry must come from the live guest MenuList"
    );
    assert_eq!(disp.current_menu_title_hit_test(&bus, 39), None);
    assert_eq!(
        disp.current_menu_title_hit_test(&bus, 40)
            .map(|index| disp.menus[index].handle),
        Some(second)
    );
    assert_eq!(
        disp.current_menu_title_hit_test(&bus, 80)
            .map(|index| disp.menus[index].handle),
        Some(first)
    );
    assert_eq!(disp.current_menu_title_hit_test(&bus, 120), None);
    assert_eq!(
        disp.guest_menu_snapshot(&bus)
            .menus
            .iter()
            .map(|menu| menu.id)
            .collect::<Vec<_>>(),
        vec![200, 100],
        "frontend snapshots must follow direct guest MenuList ordering changes"
    );

    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, inserted, 100);
    let live = super::menu_list_from_memory(&bus, menu_list_handle).unwrap();
    assert_eq!(
        live.regular_handles().collect::<Vec<_>>(),
        vec![second, inserted, first],
        "InsertMenu must preserve direct guest MenuList ordering changes"
    );
    assert_eq!(
        disp.menus
            .iter()
            .filter(|menu| menu.in_menu_bar)
            .map(|menu| menu.handle)
            .collect::<Vec<_>>(),
        vec![second, inserted, first],
        "the 68k presentation cache must follow the guest list"
    );
}

// IM:I I-352: if the MENU resource can't be read, GetMenu returns NIL.
#[test]
fn test_get_menu_returns_nil_when_resource_missing() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(0x0A60, 0);
    bus.write_word(TEST_SP, 999); // menu_id = 999 (missing)
    let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetMenu should be handled");
    assert!(result.unwrap().is_ok(), "GetMenu should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 2, "GetMenu should pop 2 bytes from stack");
    assert_eq!(
        bus.read_long(sp),
        0,
        "GetMenu must return NIL when MENU resource is unavailable"
    );
    assert_eq!(
        bus.read_word(0x0A60) as i16,
        -192,
        "GetMenu miss must set ResErr to resNotFound"
    );
}

#[test]
fn getmenu_hit_clears_stale_reserror() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
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
    bus.write_word(0x0A60, (-192i16) as u16);
    bus.write_word(TEST_SP, 128);

    let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetMenu should be handled");
    assert!(result.unwrap().is_ok(), "GetMenu should succeed");

    let sp = cpu.read_reg(Register::A7);
    assert_ne!(bus.read_long(sp), 0, "GetMenu hit should return a handle");
    assert_eq!(
        bus.read_word(0x0A60),
        0,
        "GetMenu hit must clear stale resource errors"
    );
}

#[test]
fn getmenu_replaces_standard_mdef_id_placeholder_with_callable_handle() {
    // Inside Macintosh Volume I, I-127 and I-352: an on-disk MENU stores
    // its MDEF resource ID followed by a zero word. GetMenu replaces that
    // four-byte placeholder with the loaded procedure handle.
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
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
    bus.write_word(TEST_SP, 128);

    disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let menu_handle = bus.read_long(TEST_SP + 2);
    let live_menu_ptr = bus.read_long(menu_handle);
    let mdef_handle = bus.read_long(live_menu_ptr + 6);
    assert_ne!(
        mdef_handle, 0,
        "GetMenu must replace the standard MDEF ID placeholder"
    );
    let mdef_ptr = bus.read_long(mdef_handle);
    assert_ne!(mdef_ptr, 0, "standard MDEF handle must be loaded");
    assert_eq!(
        bus.read_word(mdef_ptr),
        0x205F,
        "standard MDEF shim should be directly callable by 68k code"
    );
}

#[test]
fn getmenu_resolves_custom_mdef_from_the_resource_chain() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_ptr = seed_menu_resource(&mut bus, 129, "Custom");
    let mdef_ptr = bus.alloc(2);
    bus.write_word(menu_ptr + 6, 256);
    bus.write_word(menu_ptr + 8, 0);
    bus.write_word(mdef_ptr, 0x4E75); // RTS: sufficient callable test body.
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([
                    ((*b"MENU", 129), menu_ptr),
                    ((*b"MDEF", 256), mdef_ptr),
                ]),
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
    bus.write_word(TEST_SP, 129);

    disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    // While the MDEF callback is active A7 points at its synthetic return
    // slot; the GetMenu result already occupies the caller's result slot.
    let menu_handle = bus.read_long(TEST_SP + 2);
    let live_menu_ptr = bus.read_long(menu_handle);
    let mdef_handle = bus.read_long(live_menu_ptr + 6);
    assert_ne!(
        mdef_handle,
        u32::from(256u16) << 16,
        "GetMenu must replace the raw ID-plus-zero placeholder"
    );
    assert_eq!(
        bus.read_long(mdef_handle),
        mdef_ptr,
        "custom MDEF handle should dereference to the loaded resource"
    );
    let trampoline = cpu.read_reg(Register::PC);
    assert_ne!(trampoline, 0);
    assert_eq!(
        bus.read_word(trampoline + 6),
        2,
        "GetMenu must size a newly created custom menu through mSizeMsg"
    );
    assert_eq!(bus.read_long(trampoline + 10), menu_handle);
}

#[test]
fn getmenu_returns_resource_backed_handle_reused_until_release() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_ptr = seed_menu_resource(&mut bus, 128, "Game");

    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", 128), menu_ptr)]),
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

    bus.write_word(TEST_SP, 128);
    let first = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(first.is_some(), "GetMenu should be handled");
    assert!(first.unwrap().is_ok(), "GetMenu should succeed");
    let first_handle = bus.read_long(cpu.read_reg(Register::A7));
    assert_ne!(first_handle, 0, "GetMenu should return a handle on hit");
    assert_eq!(
        disp.loaded_handles.get(&first_handle).copied(),
        Some((bus.read_long(first_handle), *b"MENU", 128)),
        "GetMenu must return a Resource Manager-backed MENU handle"
    );
    assert_eq!(
        disp.resource_handle_files.get(&first_handle).copied(),
        Some(0),
        "GetMenu-backed MENU handle should retain its resource-file owner"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 128);
    let second = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(second.is_some(), "second GetMenu should be handled");
    assert!(second.unwrap().is_ok(), "second GetMenu should succeed");
    assert_eq!(
        bus.read_long(cpu.read_reg(Register::A7)),
        first_handle,
        "repeated GetMenu should reuse the loaded MENU resource handle"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, first_handle);
    let release = disp.dispatch_resource(true, 0x1A3, &mut cpu, &mut bus);
    assert!(release.is_some(), "ReleaseResource should be handled");
    assert!(release.unwrap().is_ok(), "ReleaseResource should succeed");
    assert_eq!(
        bus.read_long(first_handle),
        0,
        "ReleaseResource should nil the released menu handle"
    );
    assert!(
        !disp.loaded_handles.contains_key(&first_handle),
        "ReleaseResource should invalidate the old MENU handle identity"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 128);
    let third = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(third.is_some(), "third GetMenu should be handled");
    assert!(third.unwrap().is_ok(), "third GetMenu should succeed");
    let third_handle = bus.read_long(cpu.read_reg(Register::A7));
    assert_ne!(third_handle, 0, "GetMenu after release should reload");
    assert_ne!(
        third_handle, first_handle,
        "GetMenu after ReleaseResource should allocate a fresh handle"
    );
    assert_eq!(
        disp.resource_handle_files.get(&third_handle).copied(),
        Some(0),
        "reloaded MENU handle should be resource-backed"
    );
}

// IM:V 1986 p. V-234 and MTE 1992 p. 3-156: successful GetMenu also
// attempts to load an 'mctb' resource with the same resource ID.
#[test]
fn getmenu_autoloads_matching_mctb_resource_into_menucinfo() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 181i16;
    let menu_ptr = seed_menu_resource(&mut bus, menu_id, "Color");
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", menu_id), menu_ptr)]),
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
    install_mctb_resource(
        &mut disp,
        &mut bus,
        menu_id,
        &[(menu_id, 0, 0x2100), (menu_id, 2, 0x2200)],
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, menu_id as u16);
    let result = disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetMenu should be handled");
    assert!(result.unwrap().is_ok(), "GetMenu should succeed");
    assert_ne!(
        bus.read_long(TEST_SP + 2),
        0,
        "GetMenu should return the loaded MENU handle"
    );

    let title_entry = get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 0);
    assert_ne!(
        title_entry, 0,
        "GetMenu should autoload the matching mctb title entry"
    );
    assert_eq!(
        bus.read_word(title_entry + 10),
        0x2103,
        "compiled mctb RGB2 should be converted into the live MCEntry"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
        0,
        "GetMenu should autoload matching mctb item entries"
    );
}

// IM:I I-354: GetNewMBar returns NIL when the MBAR resource can't be read.
#[test]
fn getnewmbar_missing_resource_returns_nil_and_pops_menuid_word() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(TEST_SP, 128);
    bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);

    let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetNewMBar should be handled");
    assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "GetNewMBar should consume one INTEGER argument"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 2),
        0,
        "GetNewMBar must return NIL when MBAR cannot be read"
    );
}

#[test]
fn getnewmbar_rejects_a_truncated_declared_menu_sequence() {
    let (mut disp, mut cpu, mut bus) = setup();
    let mbar_ptr = bus.alloc(4);
    bus.write_bytes(mbar_ptr, &[0, 2, 0, 128]);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MBAR", 902), mbar_ptr)]),
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
    bus.write_word(TEST_SP, 902);
    bus.write_long(TEST_SP + 2, 0xDEAD_BEEF);

    assert!(disp
        .dispatch_menu(true, 0x1C0, &mut cpu, &mut bus)
        .is_some_and(|result| result.is_ok()));
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert_eq!(bus.read_long(TEST_SP + 2), 0);
}

// IM:I I-354: GetNewMBar creates and returns a menu list handle, and
// SetMenuBar installs that list as the current menu list.
#[test]
fn getnewmbar_present_resource_returns_handle_and_setmenubar_installs_it() {
    let (mut disp, mut cpu, mut bus) = setup();
    let baseline = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x306000, "Old");
    insert_menu(&mut disp, &mut cpu, &mut bus, baseline);
    assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300), 0);

    let file_menu_ptr = seed_menu_resource(&mut bus, 128, "File");
    let edit_menu_ptr = seed_menu_resource(&mut bus, 129, "Edit");
    let mbar_ptr = seed_mbar_resource(&mut bus, &[128, 129]);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([
                    ((*b"MBAR", 900), mbar_ptr),
                    ((*b"MENU", 128), file_menu_ptr),
                    ((*b"MENU", 129), edit_menu_ptr),
                ]),
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

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 900);
    let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetNewMBar should be handled");
    assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "GetNewMBar should pop menuBarID"
    );
    let mbar_handle = bus.read_long(TEST_SP + 2);
    assert_ne!(mbar_handle, 0, "GetNewMBar should return a non-NIL handle");
    let menu_list = menu_list_from_memory(&bus, mbar_handle)
        .expect("returned handle should contain a DynamicMenuList");
    assert_eq!(menu_list.mb_res_id, 900);
    assert_eq!(menu_list.regular.len(), 2);
    assert!(menu_list.regular.iter().all(|entry| entry.handle != 0));
    let file_handle = menu_list.regular[0].handle;
    assert_eq!(
        disp.loaded_handles.get(&file_handle).copied(),
        Some((bus.read_long(file_handle), *b"MENU", 128)),
        "GetNewMBar must obtain each menu through the Resource Manager",
    );
    assert_eq!(
        disp.resource_handle_files.get(&file_handle).copied(),
        Some(0),
        "GetNewMBar menu handles should retain their resource-file owner",
    );
    assert_ne!(
        bus.read_long(bus.read_long(file_handle) + 6),
        0,
        "GetNewMBar should resolve the standard MDEF through GetMenu",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 128);
    disp.dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        bus.read_long(cpu.read_reg(Register::A7)),
        file_handle,
        "GetMenu should reuse the MENU resource loaded by GetNewMBar",
    );

    // IM:I I-354: GetNewMBar only creates the list; SetMenuBar installs it.
    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
        0,
        "GetNewMBar alone should not replace the current menu list"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
        0,
        "new MBAR menus should not be current until SetMenuBar"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, mbar_handle);
    let set_result = disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus);
    assert!(set_result.is_some(), "SetMenuBar should be handled");
    assert!(set_result.unwrap().is_ok(), "SetMenuBar should succeed");
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
        0,
        "SetMenuBar should replace current list with the list from GetNewMBar"
    );
    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
        0,
        "SetMenuBar should install File menu from MBAR"
    );
    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129),
        0,
        "SetMenuBar should install Edit menu from MBAR"
    );
}

#[test]
fn nested_classic_getnewmbar_preserves_plain_and_auto_pop_returns() {
    for opcode in [0xa9c0, 0xadc0] {
        let (mut disp, _, mut bus) = setup();
        let mut cpu = crate::cpu::M68kCpu::new();
        let first_menu = seed_menu_resource(&mut bus, 601, "First");
        let second_menu = seed_menu_resource(&mut bus, 602, "Second");
        for menu in [first_menu, second_menu] {
            bus.write_word(menu + 6, 256);
            bus.write_word(menu + 8, 0);
        }
        let mdef = bus.alloc(96);
        bus.write_word(mdef, 0x4E75);
        let mbar = seed_mbar_resource(&mut bus, &[601, 602]);
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([
                        ((*b"MBAR", 900), mbar),
                        ((*b"MENU", 601), first_menu),
                        ((*b"MENU", 602), second_menu),
                        ((*b"MDEF", 256), mdef),
                    ]),
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
        let inner = seed_mbar_resource(&mut bus, &[]);
        disp.with_resource_file_mut_for_test(0, |file| {
            file.loaded.insert((*b"MBAR", 901), inner);
        })
        .unwrap();
        let marker = bus.alloc(4);
        let code = [
            0x206f,
            16,     // menu handle from the Pascal MDEF frame
            0x2050, // dereference the live menu record
            0x2f08, // save the outer record
            0x598f, // reserve nested Handle result
            0x3f3c,
            901,
            0xa9c0, // nested GetNewMBar
            0x201f, // consume nested result
            0x23c0,
            (marker >> 16) as u16,
            marker as u16,
            0x205f, // restore outer record
            0x317c,
            123,
            2,
            0x317c,
            45,
            4,
            0x4e74,
            18,
        ];
        for (index, word) in code.into_iter().enumerate() {
            bus.write_word(mdef + index as u32 * 2, word);
        }
        let trap_pc = 0x0012_3600;
        let return_pc = if opcode == 0xadc0 {
            trap_pc + 0x100
        } else {
            trap_pc + 2
        };
        let parameters = if opcode == 0xadc0 {
            TEST_SP + 4
        } else {
            TEST_SP
        };
        bus.write_word(trap_pc, opcode);
        if opcode == 0xadc0 {
            bus.write_long(TEST_SP, return_pc);
        }
        bus.write_word(parameters, 900);
        cpu.write_reg(Register::PC, trap_pc);
        cpu.write_reg(Register::A7, TEST_SP);
        for _ in 0..256 {
            match cpu.step(&mut bus) {
                crate::cpu::StepResult::Ok => {}
                crate::cpu::StepResult::Aline(trap) => {
                    disp.dispatch(trap, &mut cpu, &mut bus).unwrap()
                }
                _ => panic!("unexpected instruction while building menus"),
            }
            if cpu.read_reg(Register::PC) == return_pc {
                break;
            }
        }
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), parameters + 2);
        let inner_handle = bus.read_long(marker);
        assert_ne!(inner_handle, 0);
        assert_eq!(
            menu_list_from_memory(&bus, inner_handle)
                .unwrap()
                .regular_handles()
                .count(),
            0
        );
        let result = bus.read_long(parameters + 2);
        let menus = menu_list_from_memory(&bus, result)
            .unwrap()
            .regular_handles()
            .collect::<Vec<_>>();
        assert_eq!(menus.len(), 2);
        for handle in menus {
            let record = bus.read_long(handle);
            assert_eq!(bus.read_word(record + 2), 123);
            assert_eq!(bus.read_word(record + 4), 45);
        }
        assert!(disp.guest_calls.is_empty());
        assert!(!disp.preserve_auto_pop_pc_once);
    }
}

#[test]
fn menu_build_completion_does_not_reenter_a_newly_installed_trap_patch() {
    for opcode in [0xa9c0, 0xadc0] {
        let (mut disp, _, mut bus) = setup();
        let mut cpu = crate::cpu::M68kCpu::new();
        let first_menu = seed_menu_resource(&mut bus, 601, "First");
        let second_menu = seed_menu_resource(&mut bus, 602, "Second");
        for menu in [first_menu, second_menu] {
            bus.write_word(menu + 6, 256);
            bus.write_word(menu + 8, 0);
        }
        let mdef = bus.alloc(96);
        bus.write_word(mdef, 0x4E75);
        let mbar = seed_mbar_resource(&mut bus, &[601]);
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([(
                0,
                crate::trap::dispatch::ResourceFileMap {
                    loaded: std::collections::HashMap::from([
                        ((*b"MBAR", 900), mbar),
                        ((*b"MENU", 601), first_menu),
                        ((*b"MENU", 602), second_menu),
                        ((*b"MDEF", 256), mdef),
                    ]),
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
        let inner = seed_mbar_resource(&mut bus, &[]);
        disp.with_resource_file_mut_for_test(0, |file| {
            file.loaded.insert((*b"MBAR", 901), inner);
        })
        .unwrap();
        let marker = bus.alloc(4);
        let code = [
            0x206f,
            16,     // menu handle from the Pascal MDEF frame
            0x2050, // dereference the live menu record
            0x2f08, // save the outer record
            0x598f, // reserve nested Handle result
            0x3f3c,
            901,
            0xa9c0, // nested GetNewMBar
            0x201f, // consume nested result
            0x23c0,
            (marker >> 16) as u16,
            marker as u16,
            0x205f, // restore outer record
            0x317c,
            123,
            2,
            0x317c,
            45,
            4,
            0x4e74,
            18,
        ];
        for (index, word) in code.into_iter().enumerate() {
            bus.write_word(mdef + index as u32 * 2, word);
        }
        let trap_pc = 0x0012_3600;
        let return_pc = if opcode == 0xadc0 {
            trap_pc + 0x100
        } else {
            trap_pc + 2
        };
        let parameters = if opcode == 0xadc0 {
            TEST_SP + 4
        } else {
            TEST_SP
        };
        bus.write_word(trap_pc, opcode);
        if opcode == 0xadc0 {
            bus.write_long(TEST_SP, return_pc);
        }
        bus.write_word(parameters, 900);
        cpu.write_reg(Register::PC, trap_pc);
        cpu.write_reg(Register::A7, TEST_SP);
        let patch = bus.alloc(4);
        bus.write_word(patch, 0x4e72);
        bus.write_word(patch + 2, 0x2700);
        let mut wrapper = None;
        let mut patched = false;
        for _ in 0..256 {
            match cpu.step(&mut bus) {
                crate::cpu::StepResult::Ok => {}
                crate::cpu::StepResult::Aline(trap) => {
                    disp.dispatch(trap, &mut cpu, &mut bus).unwrap()
                }
                crate::cpu::StepResult::Stopped => break,
                _ => panic!("unexpected instruction while building menus"),
            }
            if wrapper.is_none() {
                wrapper = Some(cpu.read_reg(Register::PC));
            }
            if !patched && cpu.read_reg(Register::PC) == wrapper.unwrap() + 48 {
                assert!(disp.install_trap_address(&mut bus, 0xa9c0, patch).is_ok());
                patched = true;
            }
            if cpu.read_reg(Register::PC) == return_pc {
                break;
            }
        }
        assert!(patched);
        assert_eq!(
            cpu.read_reg(Register::PC),
            return_pc,
            "completion must not re-enter the patched GetNewMBar"
        );
        assert_eq!(cpu.read_reg(Register::A7), parameters + 2);
        let inner_handle = bus.read_long(marker);
        assert_ne!(inner_handle, 0);
        assert_eq!(
            menu_list_from_memory(&bus, inner_handle)
                .unwrap()
                .regular_handles()
                .count(),
            0
        );
        let result = bus.read_long(parameters + 2);
        let menus = menu_list_from_memory(&bus, result)
            .unwrap()
            .regular_handles()
            .collect::<Vec<_>>();
        assert_eq!(menus.len(), 1);
        for handle in menus {
            let record = bus.read_long(handle);
            assert_eq!(bus.read_word(record + 2), 123);
            assert_eq!(bus.read_word(record + 4), 45);
        }
        assert!(disp.guest_calls.is_empty());
        assert!(!disp.preserve_auto_pop_pc_once);
        cpu.write_reg(Register::PC, trap_pc + 2);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch(0xa9c0, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            cpu.read_reg(Register::PC),
            patch,
            "a fresh guest entry must still honor the patch"
        );
    }
}

#[test]
fn getnewmbar_sizes_each_custom_menu_before_returning_the_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let first_menu = seed_menu_resource(&mut bus, 601, "First");
    let second_menu = seed_menu_resource(&mut bus, 602, "Second");
    for menu in [first_menu, second_menu] {
        bus.write_word(menu + 6, 256);
        bus.write_word(menu + 8, 0);
    }
    let mdef = bus.alloc(2);
    bus.write_word(mdef, 0x4E75);
    let mbar = seed_mbar_resource(&mut bus, &[601, 602]);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([
                    ((*b"MBAR", 900), mbar),
                    ((*b"MENU", 601), first_menu),
                    ((*b"MENU", 602), second_menu),
                    ((*b"MDEF", 256), mdef),
                ]),
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
    let trap_pc = 0x0012_3600;
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    bus.write_word(TEST_SP, 900);

    disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let trampoline = cpu.read_reg(Register::PC);
    assert_eq!(bus.read_word(trampoline + 6), 2);
    let first_handle = bus.read_long(trampoline + 10);
    assert_eq!(bus.read_word(bus.read_long(first_handle)), 601);
    assert!(!disp.is_tracking_refire(0xA9C0));
    bus.write_word(bus.read_long(first_handle) + 2, 101);
    bus.write_word(bus.read_long(first_handle) + 4, 41);

    cpu.write_reg(Register::PC, trampoline + 54);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.dispatch(0xa9c0, &mut cpu, &mut bus).unwrap();
    assert_eq!(bus.read_word(trampoline + 6), 2);
    let second_handle = bus.read_long(trampoline + 10);
    assert_eq!(bus.read_word(bus.read_long(second_handle)), 602);
    bus.write_word(bus.read_long(second_handle) + 2, 102);
    bus.write_word(bus.read_long(second_handle) + 4, 42);

    cpu.write_reg(Register::PC, trampoline + 54);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.dispatch(0xa9c0, &mut cpu, &mut bus).unwrap();

    let list_handle = bus.read_long(TEST_SP + 2);
    assert_ne!(list_handle, 0);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 2);
    assert!(!disp.guest_calls.has_menu_bar_builds());
    assert!(
        disp.guest_calls.is_empty(),
        "completed GetNewMBar must retire every MDEF frame"
    );
    assert_eq!(bus.read_word(bus.read_long(first_handle) + 2), 101);
    assert_eq!(bus.read_word(bus.read_long(second_handle) + 4), 42);
    assert_eq!(
        super::menu_list_from_memory(&bus, list_handle)
            .unwrap()
            .regular_handles()
            .collect::<Vec<_>>(),
        vec![first_handle, second_handle]
    );
}

#[test]
fn getnewmbar_preserves_complete_resource_backed_menu_records() {
    let (mut disp, mut cpu, mut bus) = setup();
    let seed_ptr = seed_menu_resource(&mut bus, 640, "Long");
    let menu_ptr = bus.alloc(320);
    let seed_bytes = bus.read_bytes(seed_ptr, 256);
    bus.write_bytes(menu_ptr, &seed_bytes);
    bus.write_byte(menu_ptr + 300, 0xA5);
    let mbar_ptr = seed_mbar_resource(&mut bus, &[640]);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([
                    ((*b"MBAR", 940), mbar_ptr),
                    ((*b"MENU", 640), menu_ptr),
                ]),
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

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 940);
    disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let list_handle = bus.read_long(cpu.read_reg(Register::A7));
    let list = menu_list_from_memory(&bus, list_handle).expect("GetNewMBar menu list");
    let menu_handle = list.regular[0].handle;
    assert_eq!(bus.read_long(menu_handle), menu_ptr);
    assert_eq!(bus.get_alloc_size(menu_ptr), Some(320));
    assert_eq!(bus.read_byte(menu_ptr + 300), 0xA5);
}

// IM:V 1986 p. V-244: GetNewMBar clears the current menu color
// information table, loads the requested menus, and leaves the new
// MenuCInfo state in place even though the previous MenuList is restored.
#[test]
fn getnewmbar_rebuilds_menucinfo_from_menu_mctb_resources() {
    let (mut disp, mut cpu, mut bus) = setup();
    let baseline = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x306080, "Old");
    insert_menu(&mut disp, &mut cpu, &mut bus, baseline);
    set_mc_entries_for_test(&mut disp, &mut cpu, &mut bus, &[(999, 1, 0x3100)]);
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 999, 1),
        0,
        "precondition: old MenuCInfo entry should exist before GetNewMBar"
    );

    let file_menu_ptr = seed_menu_resource(&mut bus, 601, "File");
    let edit_menu_ptr = seed_menu_resource(&mut bus, 602, "Edit");
    let mbar_ptr = seed_mbar_resource(&mut bus, &[601, 602]);
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([
                    ((*b"MBAR", 901), mbar_ptr),
                    ((*b"MENU", 601), file_menu_ptr),
                    ((*b"MENU", 602), edit_menu_ptr),
                ]),
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
    install_mctb_resource(&mut disp, &mut bus, 601, &[(601, 0, 0x3200)]);
    install_mctb_resource(&mut disp, &mut bus, 602, &[(602, 2, 0x3300)]);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 901);
    let result = disp.dispatch_menu(true, 0x1C0, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetNewMBar should be handled");
    assert!(result.unwrap().is_ok(), "GetNewMBar should succeed");
    assert_ne!(
        bus.read_long(TEST_SP + 2),
        0,
        "GetNewMBar should return a new menu-list handle"
    );

    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 300),
        0,
        "GetNewMBar should restore the previous current MenuList"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 601),
        0,
        "GetNewMBar should not install the new MenuList until SetMenuBar"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 999, 1),
        0,
        "GetNewMBar should clear the previous MenuCInfo table"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 601, 0),
        0,
        "GetNewMBar should load mctb entries for MBAR menu 601"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 602, 2),
        0,
        "GetNewMBar should load mctb entries for MBAR menu 602"
    );
}

// IM:I I-358 + I-352 + I-355:
// AppendMenu must work before InsertMenu, and MenuKey should only find
// items once the menu is actually in the current menu list.
#[test]
fn getmenu_appendmenu_then_insertmenu_enables_menukey_shortcut() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Seed MENU resource ID 180 with title "File" and no items.
    let menu_res_ptr = bus.alloc(64);
    bus.write_word(menu_res_ptr, 180);
    bus.write_word(menu_res_ptr + 2, 0);
    bus.write_word(menu_res_ptr + 4, 0);
    bus.write_long(menu_res_ptr + 6, 0);
    bus.write_long(menu_res_ptr + 10, 0xFFFF_FFFF);
    write_pstring(&mut bus, menu_res_ptr + 14, "File");
    bus.write_byte(menu_res_ptr + 19, 0); // empty items terminator
    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", 180), menu_res_ptr)]),
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

    // GetMenu(180) should return a non-NIL handle.
    bus.write_word(TEST_SP, 180);
    assert!(disp
        .dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let menu_handle = bus.read_long(cpu.read_reg(Register::A7));
    assert_ne!(menu_handle, 0, "GetMenu should return a handle on hit");

    // Menu isn't in current list yet; MenuKey('O') must return 0.
    assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'), 0);

    // Append before insertion must persist through InsertMenu.
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu_handle,
        0x306000,
        "Open/O",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, menu_handle);
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
        (180u32 << 16) | 1,
        "MenuKey should resolve appended shortcut after InsertMenu"
    );
}

// IM:I I-355: MenuKey scans menus from right to left when shortcuts collide.
#[test]
fn menukey_duplicate_shortcuts_prefer_rightmost_menu() {
    let (mut disp, mut cpu, mut bus) = setup();
    let left = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 200, 0x306100, "Left");
    append_menu_data(&mut disp, &mut cpu, &mut bus, left, 0x306200, "LeftCmd/X");
    insert_menu(&mut disp, &mut cpu, &mut bus, left);

    let right = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 201, 0x306300, "Right");
    append_menu_data(&mut disp, &mut cpu, &mut bus, right, 0x306400, "RightCmd/X");
    insert_menu(&mut disp, &mut cpu, &mut bus, right);

    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
        (201u32 << 16) | 1,
        "MenuKey should choose the rightmost menu's matching item"
    );
}

// IM:I I-355: only enabled items in the current menu list are eligible.
#[test]
fn menukey_ignores_uninserted_menu_items() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 210, 0x306500, "Ghost");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306600, "Hidden/G");
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'G'),
        0,
        "MenuKey should ignore items from menus not yet inserted"
    );
}

// IM:I I-352 and I-355: InsertMenu adds a menu to the current menu
// list, and MenuKey searches that list. A beforeID of -1 omits the title
// from the menu bar but does not make its command equivalents unavailable.
#[test]
fn menukey_searches_installed_command_only_menu_without_drawing_title() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    disp.menu_bar_hidden = false;
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let visible = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 230, 0x306800, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, visible, 0x306840, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, visible);

    let commands = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 231, 0x306880, "Commands");
    append_menu_data(&mut disp, &mut cpu, &mut bus, commands, 0x3068C0, "Pause/P");
    insert_menu_before(&mut disp, &mut cpu, &mut bus, commands, -1);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(disp.menu_title_regions().len(), 1);
    let menu_bar_before = bus.read_bytes(base, row_bytes as usize * 20);

    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'),
        (231u32 << 16) | 1
    );
    assert_eq!(
        bus.read_bytes(base, row_bytes as usize * 20),
        menu_bar_before,
        "a command-only match must not paint a hidden menu title"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, commands);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'), 0);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, commands);
    disp.dispatch_menu(true, 0x139, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, commands);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(menu_key_result(&mut disp, &mut cpu, &mut bus, b'P'), 0);
}

#[test]
fn menukey_prefers_regular_partition_and_highlights_hierarchical_owner() {
    // IM:V V-235 and V-245: regular menus are searched before the
    // hierarchical portion, and a submenu match highlights its owning
    // regular title.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    disp.menu_bar_hidden = false;
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let root = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 230, 0x306900, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        root,
        0x306940,
        "Regular/X;Recent",
    );
    set_item_cmd(&mut disp, &mut cpu, &mut bus, root, 2, 0x1b);
    set_item_mark(&mut disp, &mut cpu, &mut bus, root, 2, 231);
    insert_menu(&mut disp, &mut cpu, &mut bus, root);

    let child = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 231, 0x306980, "Recent");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        child,
        0x3069C0,
        "Nested/H;Shadow/X",
    );
    insert_menu_before(&mut disp, &mut cpu, &mut bus, child, -1);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
        (230u32 << 16) | 1,
        "a hierarchical shortcut must not beat a regular-menu match"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (left, right) = disp.menu_title_regions()[0];
    let before = title_region_pixels(&bus, base, row_bytes, left, right);
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'H'),
        (231u32 << 16) | 1
    );
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::THE_MENU),
        231,
        "TheMenu must retain the selected submenu ID rather than its root title ID"
    );
    let after = title_region_pixels(&bus, base, row_bytes, left, right);
    assert!(
        changed_pixel_count(&before, &after) > 0,
        "a hierarchical shortcut must highlight its owning regular title"
    );
}

// MTE 1992 p. 3-138: GetItemCmd returns 0 if the item has no
// keyboard equivalent, submenu marker, script-code marker, or icon marker.
#[test]
fn getitemcmd_returns_zero_when_no_keyboard_equivalent() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 220, 0x306710, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306720, "Open");

    assert_eq!(
        get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
        0,
        "GetItemCmd should return 0 when keyboard-equivalent field is clear"
    );
}

// MTE 1992 pp. 3-138 to 3-139: SetItemCmd writes cmdChar into the
// item's keyboard-equivalent field.
#[test]
fn setitemcmd_sets_keyboard_equivalent_field() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 221, 0x306730, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306740, "Open");

    set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1, b'O');
    assert_eq!(
        get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
        b'O',
        "GetItemCmd should read back cmdChar written by SetItemCmd"
    );

    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
        (221u32 << 16) | 1,
        "MenuKey should honor command key installed via SetItemCmd"
    );
}

// MTE 1992 p. 3-138: cmdChar=$1B marks the item as hierarchical.
#[test]
fn setitemcmd_sets_submenu_marker_for_hierarchical_items() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 222, 0x306750, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306760, "Open");

    set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1, 0x1B);
    assert_eq!(
        get_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1),
        0x1B,
        "GetItemCmd should return $1B after SetItemCmd installs submenu marker"
    );
}

#[test]
fn checkitem_persists_mark_across_later_menu_mutations() {
    // IM:I I-355 and I-358: CheckItem changes the mark stored in the
    // guest MenuInfo item record, not merely transient drawing state.
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 223, 0x306770, "Speed");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306780, "Moderate");
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0100);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, menu);

    disp.dispatch_menu(true, 0x145, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 1, b'M');

    let item = disp
        .menus
        .iter()
        .find(|candidate| candidate.handle == menu)
        .and_then(|candidate| candidate.items.first())
        .unwrap();
    assert_eq!(item.mark, 0x12);
}

// 0x133 — AppendMenu: pops 8 bytes.
#[test]
fn test_append_menu() {
    let (mut disp, mut cpu, mut bus) = setup();
    let result = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
    assert!(result.is_some(), "AppendMenu should be handled");
    assert!(result.unwrap().is_ok(), "AppendMenu should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 8, "AppendMenu should pop 8 bytes from stack");
}

// AppendMenu must serialise parsed items into the MENU record at
// offset 15+title_len so the guest-memory item count (CountMItems /
// CalcMenuSize path) matches the Rust-side items list.
#[test]
fn test_appendmenu_serialises_items_to_guest_memory() {
    let (mut disp, mut cpu, mut bus) = setup();

    // NewMenu(129, "File") — registers menu + allocates menu record
    let title_ptr = 0x302000u32;
    bus.write_byte(title_ptr, 4);
    bus.write_byte(title_ptr + 1, b'F');
    bus.write_byte(title_ptr + 2, b'i');
    bus.write_byte(title_ptr + 3, b'l');
    bus.write_byte(title_ptr + 4, b'e');
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 129);
    let r = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());
    let sp_after_new = cpu.read_reg(Register::A7);
    let handle = bus.read_long(sp_after_new);
    assert_ne!(handle, 0);

    // AppendMenu(handle, "New/N;Open/O;Quit/Q")
    let data_ptr = 0x302100u32;
    let data = b"New/N;Open/O;Quit/Q";
    bus.write_byte(data_ptr, data.len() as u8);
    for (i, b) in data.iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr); // SP+0 text ptr
    bus.write_long(TEST_SP + 4, handle); // SP+4 menu handle
    let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());

    // Rust-side self.menus should have 3 items.
    assert_eq!(disp.menus.len(), 1);
    assert_eq!(disp.menus[0].items.len(), 3);

    // CountMItems (0x150) reads from guest memory; must see 3 too.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    let r = disp.dispatch_menu(true, 0x150, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());
    let sp_count = cpu.read_reg(Register::A7);
    let count = bus.read_word(sp_count);
    assert_eq!(count, 3, "CountMItems must see AppendMenu'd items");

    // Guest memory layout sanity: at menu_ptr + 15 + 4 ("File")
    // we expect item 0's pstring = "New" (len 3).
    let menu_ptr = bus.read_long(handle);
    assert_eq!(bus.read_byte(menu_ptr + 19), 3);
    assert_eq!(bus.read_byte(menu_ptr + 20), b'N');
    assert_eq!(bus.read_byte(menu_ptr + 21), b'e');
    assert_eq!(bus.read_byte(menu_ptr + 22), b'w');
}

#[test]
fn count_menu_items_rejects_an_unterminated_live_record() {
    let (_disp, _cpu, mut bus) = setup();
    let menu = bus.alloc(24);
    let handle = bus.alloc(4);
    bus.write_long(handle, menu);
    bus.write_long(menu + 10, u32::MAX);
    bus.write_byte(menu + 14, 0);
    bus.write_byte(menu + 15, 4);
    bus.write_bytes(menu + 16, b"Open");
    bus.write_bytes(menu + 20, &[0, b'O', 0, 0]);

    assert_eq!(count_menu_items_from_memory(&bus, handle), 0);
}

#[test]
fn long_menu_mutations_preserve_trailing_item_and_refresh_guest_edits() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 447;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x303800, "Long");

    let mut data = vec!["A/A"; 39];
    data.push("Tail/Z");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x303900,
        &data.join(";"),
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    let first_long_ptr = bus.read_long(handle);
    assert!(
        bus.get_alloc_size(first_long_ptr).unwrap() > 256,
        "AppendMenu must grow MENU records beyond the legacy 256-byte buffer"
    );
    assert_eq!(count_menu_items_from_memory(&bus, handle), 40);
    assert_eq!(
        get_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 40),
        b'Z'
    );

    let replacement = "Expanded first item that forces another complete record resize";
    write_pstring(&mut bus, 0x303A00, replacement);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x303A00);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, handle);
    assert!(disp
        .dispatch_menu(true, 0x147, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    set_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 1, b'X');

    for trap in [0x13A, 0x139] {
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, handle);
        assert!(disp
            .step_menu_fixture(true, trap, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
    }
    let (height, width) = calc_menu_size_for_test(&mut disp, &mut cpu, &mut bus, handle);
    assert!(height > 0 && width > 0);

    let resized_ptr = bus.read_long(handle);
    assert!(bus.get_alloc_size(resized_ptr).unwrap() > 256);
    let parsed = parse_menu_resource(&bus, resized_ptr, handle);
    assert_eq!(parsed.items.len(), 40);
    assert_eq!(parsed.items[39].text, "Tail");
    assert_eq!(parsed.items[39].key_equiv, b'Z');

    // MenuInfo belongs to the guest. A direct, same-size edit to the last
    // item must become authoritative before rendering or frontend export.
    let mut tail_ptr = resized_ptr + 15 + bus.read_byte(resized_ptr + 14) as u32;
    for _ in 0..39 {
        tail_ptr += 5 + bus.read_byte(tail_ptr) as u32;
    }
    assert_eq!(bus.read_byte(tail_ptr), 4);
    for (index, byte) in b"Last".iter().enumerate() {
        bus.write_byte(tail_ptr + 1 + index as u32, *byte);
    }
    bus.write_byte(tail_ptr + 6, b'Q');

    cpu.write_reg(Register::A7, TEST_SP);
    assert!(disp
        .dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menus[0].items[39].text, "Last");
    assert_eq!(disp.menus[0].items[39].key_equiv, b'Q');

    let snapshot = disp.guest_menu_snapshot(&bus);
    assert_eq!(snapshot.menus[0].items[39].text, "Last");
    assert_eq!(snapshot.menus[0].items[39].key_equivalent, Some('q'));
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'Q'),
        ((menu_id as u32) << 16) | 40
    );
}

#[test]
fn resource_backed_long_menu_keeps_its_complete_record_when_mutated() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 448;
    let resource_ptr = bus.alloc(263);
    bus.write_word(resource_ptr, menu_id as u16);
    bus.write_long(resource_ptr + 10, 0xFFFF_FFFF);
    write_pstring(&mut bus, resource_ptr + 14, "Long");
    let mut offset = resource_ptr + 19;
    for _ in 0..39 {
        bus.write_byte(offset, 1);
        bus.write_byte(offset + 1, b'A');
        bus.write_byte(offset + 3, b'A');
        offset += 6;
    }
    bus.write_byte(offset, 4);
    bus.write_bytes(offset + 1, b"Tail");
    bus.write_byte(offset + 6, b'Z');
    bus.write_byte(offset + 9, 0);
    assert_eq!(offset + 10, resource_ptr + 263);

    disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
        files: std::collections::HashMap::from([(
            0,
            crate::trap::dispatch::ResourceFileMap {
                loaded: std::collections::HashMap::from([((*b"MENU", menu_id), resource_ptr)]),
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
    bus.write_word(TEST_SP, menu_id as u16);
    assert!(disp
        .dispatch_menu(true, 0x1BF, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));
    assert_eq!(bus.get_alloc_size(bus.read_long(handle)), Some(263));

    set_item_cmd(&mut disp, &mut cpu, &mut bus, handle, 1, b'X');
    let parsed = parse_menu_resource(&bus, bus.read_long(handle), handle);
    assert_eq!(parsed.items.len(), 40);
    assert_eq!(parsed.items[39].text, "Tail");
    assert_eq!(parsed.items[39].key_equiv, b'Z');
}

#[test]
fn appendmenu_tracks_raw_menu_handle_before_appending_items() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = bus.alloc(4);
    let menu_ptr = bus.alloc(32);
    bus.write_long(handle, menu_ptr);
    bus.write_word(menu_ptr, 4000);
    bus.write_word(menu_ptr + 2, 0);
    bus.write_word(menu_ptr + 4, 0);
    bus.write_long(menu_ptr + 6, 0);
    bus.write_long(menu_ptr + 10, 0xFFFF_FFFF);
    write_pstring(&mut bus, menu_ptr + 14, "Sections");
    bus.write_byte(menu_ptr + 15 + "Sections".len() as u32, 0);

    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x302200,
        "Graphics;Sound;Controls",
    );

    assert_eq!(disp.menus.len(), 1);
    assert_eq!(disp.menus[0].id, 4000);
    assert_eq!(disp.menus[0].items.len(), 3);
    assert_eq!(
        disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
        Some("Graphics")
    );
    assert_eq!(
        bus.get_alloc_size(bus.read_long(handle)),
        Some(256),
        "AppendMenu should grow small raw MENU handles before serialising items"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    let r = disp.dispatch_menu(true, 0x150, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());
    let sp_count = cpu.read_reg(Register::A7);
    assert_eq!(bus.read_word(sp_count), 3);
}

#[test]
fn appendmenu_accepts_a0_menu_handle_when_stack_handle_is_nil() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = bus.alloc(4);
    let menu_ptr = bus.alloc(32);
    let data_ptr = 0x302300;
    bus.write_long(handle, menu_ptr);
    bus.write_word(menu_ptr, 4000);
    bus.write_word(menu_ptr + 2, 0);
    bus.write_word(menu_ptr + 4, 0);
    bus.write_long(menu_ptr + 6, 0);
    bus.write_long(menu_ptr + 10, 0xFFFF_FFFF);
    write_pstring(&mut bus, menu_ptr + 14, "Sections");
    bus.write_byte(menu_ptr + 15 + "Sections".len() as u32, 0);
    write_pstring(&mut bus, data_ptr, "Graphics");

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, 0);

    let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 8,
        "AppendMenu should still pop the stack data/handle slots"
    );
    assert_eq!(
        disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
        Some("Graphics")
    );
}

#[test]
fn appendmenu_accepts_a0_menu_ptr_when_stack_handle_is_nil() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302500, "Sections");
    let menu_ptr = bus.read_long(handle);
    write_pstring(&mut bus, 0x302540, "Graphics");

    cpu.write_reg(Register::A0, menu_ptr);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x302540);
    bus.write_long(TEST_SP + 4, 0);

    let r = disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 8,
        "AppendMenu should still pop the stack data/handle slots"
    );
    assert_eq!(
        disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
        Some("Graphics")
    );
}

#[test]
fn setitem_accepts_a0_menu_handle_when_stack_handle_is_nil() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302300, "Sections");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x302340, " ");
    write_pstring(&mut bus, 0x302380, "Graphics");

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x302380);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, 0);

    let r = disp.dispatch_menu(true, 0x147, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "SetItem should still pop text, item, and menu argument slots"
    );
    assert_eq!(
        disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
        Some("Graphics")
    );
}

#[test]
fn setitem_accepts_a0_menu_ptr_when_stack_handle_is_nil() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 4000, 0x302600, "Sections");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x302640, " ");
    let menu_ptr = bus.read_long(handle);
    write_pstring(&mut bus, 0x302680, "Graphics");

    cpu.write_reg(Register::A0, menu_ptr);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x302680);
    bus.write_word(TEST_SP + 4, 1);
    bus.write_long(TEST_SP + 6, 0);

    let r = disp.dispatch_menu(true, 0x147, &mut cpu, &mut bus);
    assert!(r.is_some() && r.unwrap().is_ok());

    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 10,
        "SetItem should still pop text, item, and menu argument slots"
    );
    assert_eq!(
        disp.popup_menu_item_title(&bus, 4000, 1).as_deref(),
        Some("Graphics")
    );
}

// InsertMenuItem (0x026) must sync the guest-memory MENU record so
// CountMItems reflects the inserted item.
#[test]
fn test_insertmenuitem_serialises_to_guest_memory() {
    let (mut disp, mut cpu, mut bus) = setup();

    // NewMenu(129, "File")
    let title_ptr = 0x303000u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"File".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 129);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));
    assert_ne!(handle, 0);

    // InsertMenuItem(handle, "Copy", afterItem=0)
    let text_ptr = 0x303100u32;
    bus.write_byte(text_ptr, 4);
    for (i, b) in b"Copy".iter().enumerate() {
        bus.write_byte(text_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0); // afterItem = 0 (insert at head)
    bus.write_long(TEST_SP + 2, text_ptr);
    bus.write_long(TEST_SP + 6, handle);
    assert!(disp
        .dispatch_menu(true, 0x026, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert_eq!(disp.menus[0].items.len(), 1);
    assert_eq!(disp.menus[0].items[0].text, "Copy");

    // CountMItems (0x150) must read 1 from guest memory.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 1);
}

// MTE 1992 p. 3-126: afterItem=0 inserts before the first menu item.
#[test]
fn insertmenuitem_afteritem_zero_inserts_before_first_item() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 150, 0x30A100, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A110, "B;C");
    insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A120, "A", 0);

    assert_eq!(disp.menus[0].items[0].text, "A");
    assert_eq!(disp.menus[0].items[1].text, "B");
    assert_eq!(disp.menus[0].items[2].text, "C");
}

// MTE 1992 p. 3-126: afterItem=n inserts after item n; values >= last item
// append at the end.
#[test]
fn insertmenuitem_afteritem_index_and_past_end_place_items_per_spec() {
    let (mut disp, mut cpu, mut bus) = setup();
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 151, 0x30A200, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A210, "A;B;C");

    insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A220, "X", 2);
    insert_menu_item_data(&mut disp, &mut cpu, &mut bus, handle, 0x30A230, "Tail", 99);

    let texts: Vec<String> = disp.menus[0].items.iter().map(|i| i.text.clone()).collect();
    assert_eq!(texts, vec!["A", "B", "X", "C", "Tail"]);
}

// MTE 1992 p. 3-126: when itemString contains multiple items separated by
// ';', InsertMenuItem inserts them in reverse order, while honoring the same
// metacharacter parsing rules as AppendMenu.
#[test]
fn insertmenuitem_multiple_items_reverse_order_and_metacharacters() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 152;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30A300, "Edit");

    insert_menu_item_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30A310,
        "Paste/V;Copy/C;Cut/X;(-;Undo/Z",
        0,
    );

    let menu = &disp.menus[0];
    assert_eq!(menu.items.len(), 5);
    assert_eq!(menu.items[0].text, "Undo");
    assert_eq!(menu.items[0].key_equiv, b'Z');
    assert_eq!(menu.items[1].text, "-");
    assert!(!menu.items[1].enabled, "'(' must disable inserted item");
    assert_eq!(menu.items[2].text, "Cut");
    assert_eq!(menu.items[2].key_equiv, b'X');
    assert_eq!(menu.items[3].text, "Copy");
    assert_eq!(menu.items[3].key_equiv, b'C');
    assert_eq!(menu.items[4].text, "Paste");
    assert_eq!(menu.items[4].key_equiv, b'V');

    let menu_ptr = bus.read_long(handle);
    let flags = bus.read_long(menu_ptr + 10);
    assert_eq!(
        flags & (1 << 2),
        0,
        "inserted disabled item must clear item-2 bit"
    );

    insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
        ((menu_id as u32) << 16) | 4,
        "Copy command should map to item 4 in reverse-ordered insertion result"
    );
}

// SetItem (0x147) must sync the guest-memory MENU record's item text
// after mutation — for the same reason AppendMenu and InsertMenuItem do.
#[test]
fn test_setitem_syncs_guest_memory_text() {
    let (mut disp, mut cpu, mut bus) = setup();

    // NewMenu(129, "File")
    let title_ptr = 0x304000u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"File".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 129);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    // AppendMenu(handle, "Old")
    let data_ptr = 0x304100u32;
    bus.write_byte(data_ptr, 3);
    for (i, b) in b"Old".iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    // SetItem(handle, 1, "New")
    let new_ptr = 0x304200u32;
    bus.write_byte(new_ptr, 3);
    for (i, b) in b"New".iter().enumerate() {
        bus.write_byte(new_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, new_ptr); // SP+0 text ptr
    bus.write_word(TEST_SP + 4, 1); // SP+4 item index
    bus.write_long(TEST_SP + 6, handle); // SP+6 menu handle
    assert!(disp
        .dispatch_menu(true, 0x147, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    // Rust side updated.
    assert_eq!(disp.menus[0].items[0].text, "New");
    // Guest memory: at menu_ptr + 15 + 4 ("File") we expect the
    // first item's pstring = "New" (len 3).
    let menu_ptr = bus.read_long(handle);
    assert_eq!(bus.read_byte(menu_ptr + 19), 3);
    assert_eq!(bus.read_byte(menu_ptr + 20), b'N');
    assert_eq!(bus.read_byte(menu_ptr + 21), b'e');
    assert_eq!(bus.read_byte(menu_ptr + 22), b'w');
}

// DeleteMenuItem must sync the guest-memory MENU record so CountMItems
// doesn't still see the deleted item via the guest-memory path.
#[test]
fn test_deletemenuitem_syncs_guest_memory() {
    let (mut disp, mut cpu, mut bus) = setup();

    // NewMenu(130, "Edit")
    let title_ptr = 0x305000u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"Edit".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 130);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    // AppendMenu(handle, "A;B;C") — 3 items.
    let data_ptr = 0x305100u32;
    let data = b"A;B;C";
    bus.write_byte(data_ptr, data.len() as u8);
    for (i, b) in data.iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menus[0].items.len(), 3);

    // DeleteMenuItem(handle, 2) — remove "B".
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert_eq!(disp.menus[0].items.len(), 2);
    assert_eq!(disp.menus[0].items[0].text, "A");
    assert_eq!(disp.menus[0].items[1].text, "C");

    // CountMItems (0x150) via guest memory must read 2.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 2);
}

// IM:V 1986 p. V-244: DelMenuItem also removes the deleted item's
// color entry from the application's menu color information table.
#[test]
fn deletemenuitem_removes_exact_menu_color_item_entry() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 521i16;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x305180, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x3051C0, "A;B;C");
    set_mc_entries_for_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        &[
            (menu_id, 0, 0x1100),
            (menu_id, 1, 0x1200),
            (menu_id, 2, 0x1300),
            (menu_id, 3, 0x1400),
        ],
    );

    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
        0,
        "precondition: item-2 MenuCInfo entry should exist"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, handle);
    assert!(
        disp.dispatch_menu(true, 0x152, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DeleteMenuItem should succeed"
    );

    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 2),
        0,
        "DeleteMenuItem should remove the deleted item's MenuCInfo entry"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 0),
        0,
        "DeleteMenuItem should preserve the menu-title MenuCInfo entry"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 1),
        0,
        "DeleteMenuItem should preserve other item entries"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, menu_id, 3),
        0,
        "DeleteMenuItem should not renumber unrelated MenuCInfo entries"
    );
}

#[test]
fn disableitem_item_gt_31_is_noop() {
    let (mut disp, mut cpu, mut bus) = setup();

    // IM:TB Essentials 1992 p.3-131: items with number >31 cannot be
    // individually disabled by DisableItem.
    let title_ptr = 0x305200u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"Long".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 140);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    let mut items = String::new();
    for i in 1..=32 {
        if i > 1 {
            items.push(';');
        }
        items.push_str(&format!("I{i:02}"));
    }
    let data_ptr = 0x305300u32;
    bus.write_byte(data_ptr, items.len() as u8);
    for (i, b) in items.as_bytes().iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert!(disp.menus[0].items[31].enabled, "item 32 starts enabled");
    let menu_ptr = bus.read_long(handle);
    let flags_before = bus.read_long(menu_ptr + 10);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 32);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert!(
        disp.menus[0].items[31].enabled,
        "DisableItem(item>31) must not change item state"
    );
    assert_eq!(
        bus.read_long(menu_ptr + 10),
        flags_before,
        "DisableItem(item>31) must leave enableFlags unchanged"
    );
}

#[test]
fn disableitem_zero_disables_menu_title_and_menukey_shortcuts() {
    // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 3-131:
    // DisableItem(menu, 0) disables the whole menu.
    // Inside Macintosh Volume I (1985), p. I-355:
    // MenuKey returns 0 when no enabled item in the current menu list
    // matches the key equivalent.
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 144, 0x305A00, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x305B00,
        "Open/O;Save/S",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
        (144u32 << 16) | 1,
        "MenuKey should resolve Open shortcut before whole-menu disable"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
        (144u32 << 16) | 2,
        "MenuKey should resolve Save shortcut before whole-menu disable"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, menu);
    assert!(disp
        .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    let menu_state = disp
        .menus
        .iter()
        .find(|m| m.handle == menu)
        .expect("menu should remain tracked after disable");
    assert!(
        !menu_state.enabled,
        "DisableItem(item=0) should disable the menu title"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'O'),
        0,
        "MenuKey should return 0 for whole-menu-disabled Open shortcut"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
        0,
        "MenuKey should return 0 for whole-menu-disabled Save shortcut"
    );
}

#[test]
fn enableitem_preserves_guest_enableflags_changes_before_mutating_item() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 145, 0x305C00, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x305D00,
        "Open/O;Save/S",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, menu);
    assert!(disp
        .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    // MenuInfo is guest-owned memory. A menu definition procedure may
    // restore bit 0 directly before asking the Menu Manager to adjust a
    // particular item's bit.
    let menu_ptr = bus.read_long(menu);
    bus.write_long(menu_ptr + 10, bus.read_long(menu_ptr + 10) | 1);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, menu);
    assert!(disp
        .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    let state = disp.menus.iter().find(|m| m.handle == menu).unwrap();
    assert!(
        state.enabled,
        "guest-restored menu bit must survive EnableItem"
    );
    assert!(
        state.items[1].enabled,
        "the requested item should also be enabled"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'S'),
        (145u32 << 16) | 2,
        "MenuKey must observe the guest-restored menu enable bit"
    );
}

#[test]
fn enableitem_item_gt_31_is_noop() {
    let (mut disp, mut cpu, mut bus) = setup();

    // IM:TB Essentials 1992 p.3-131: items with number >31 cannot be
    // individually enabled by EnableItem.
    let title_ptr = 0x305400u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"Long".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 141);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    let mut items = String::new();
    for i in 1..=32 {
        if i > 1 {
            items.push(';');
        }
        if i == 32 {
            items.push('(');
        }
        items.push_str(&format!("I{i:02}"));
    }
    let data_ptr = 0x305500u32;
    bus.write_byte(data_ptr, items.len() as u8);
    for (i, b) in items.as_bytes().iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert!(
        disp.menus[0].items[31].enabled,
        "MenuInfo has no individual disable bit after item 31"
    );
    let menu_ptr = bus.read_long(handle);
    let flags_before = bus.read_long(menu_ptr + 10);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 32);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert!(
        disp.menus[0].items[31].enabled,
        "EnableItem(item>31) must remain a no-op"
    );
    assert_eq!(
        bus.read_long(menu_ptr + 10),
        flags_before,
        "EnableItem(item>31) must leave enableFlags unchanged"
    );
}

#[test]
fn enableitem_zero_reenables_menu_title_but_preserves_preexisting_disabled_items() {
    let (mut disp, mut cpu, mut bus) = setup();

    // IM:TB Essentials 1992 p.3-131: enabling a whole menu with item=0
    // preserves any items that were previously individually disabled.
    let title_ptr = 0x305600u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"Edit".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 142);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    let data_ptr = 0x305700u32;
    let data = b"Cut/X;Copy/C;Paste/V";
    bus.write_byte(data_ptr, data.len() as u8);
    for (i, b) in data.iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
        (142u32 << 16) | 1,
        "MenuKey should resolve item 1 before whole-menu disable"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
        (142u32 << 16) | 2,
        "MenuKey should resolve item 2 before individual disable"
    );

    // Disable Copy (item 2) individually.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(!disp.menus[0].items[1].enabled);

    // Disable and re-enable whole menu.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(!disp.menus[0].enabled, "item=0 should disable menu title");
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
        0,
        "MenuKey should return 0 while whole menu is disabled"
    );
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
        0,
        "MenuKey should return 0 for individually disabled item while whole menu is disabled"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x139, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert!(disp.menus[0].enabled, "menu title should be re-enabled");
    assert!(disp.menus[0].items[0].enabled, "item 1 stays enabled");
    assert!(
        !disp.menus[0].items[1].enabled,
        "individually disabled item must remain disabled"
    );
    assert!(disp.menus[0].items[2].enabled, "item 3 stays enabled");
    assert_eq!(
        menu_key_result(&mut disp, &mut cpu, &mut bus, b'X'),
        (142u32 << 16) | 1,
        "MenuKey should resolve non-individually-disabled item after EnableItem(item=0)"
    );
    assert_eq!(
            menu_key_result(&mut disp, &mut cpu, &mut bus, b'C'),
            0,
            "MenuKey should keep preexisting individually disabled item unavailable after EnableItem(item=0)"
        );

    let menu_ptr = bus.read_long(handle);
    let flags = bus.read_long(menu_ptr + 10);
    assert_eq!(flags & 1, 1, "menu-enabled bit should be set");
    assert_eq!(flags & (1 << 2), 0, "item 2 bit should remain cleared");
}

#[test]
fn deletemenuitem_zero_or_oob_is_noop() {
    let (mut disp, mut cpu, mut bus) = setup();

    // IM:TB Essentials 1992 p.3-127: item=0 or item>last is a no-op.
    let title_ptr = 0x305800u32;
    bus.write_byte(title_ptr, 4);
    for (i, b) in b"Edit".iter().enumerate() {
        bus.write_byte(title_ptr + 1 + i as u32, *b);
    }
    bus.write_long(TEST_SP, title_ptr);
    bus.write_word(TEST_SP + 4, 143);
    assert!(disp
        .dispatch_menu(true, 0x131, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    let handle = bus.read_long(cpu.read_reg(Register::A7));

    let data_ptr = 0x305900u32;
    let data = b"A;B;C";
    bus.write_byte(data_ptr, data.len() as u8);
    for (i, b) in data.iter().enumerate() {
        bus.write_byte(data_ptr + 1 + i as u32, *b);
    }
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, data_ptr);
    bus.write_long(TEST_SP + 4, handle);
    assert!(disp
        .dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menus[0].items.len(), 3);

    // item=0 -> no-op
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menus[0].items.len(), 3);
    assert_eq!(disp.menus[0].items[0].text, "A");
    assert_eq!(disp.menus[0].items[1].text, "B");
    assert_eq!(disp.menus[0].items[2].text, "C");

    // item>last -> no-op
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 4);
    bus.write_long(TEST_SP + 2, handle);
    assert!(disp
        .dispatch_menu(true, 0x152, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(disp.menus[0].items.len(), 3);
    assert_eq!(disp.menus[0].items[0].text, "A");
    assert_eq!(disp.menus[0].items[1].text, "B");
    assert_eq!(disp.menus[0].items[2].text, "C");

    // Guest-memory count should also remain 3.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, handle);
    assert!(disp
        .dispatch_menu(true, 0x150, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(bus.read_word(cpu.read_reg(Register::A7)), 3);
}

// 0x135 — InsertMenu: pops 6 bytes, adds menu to menus vec.
#[test]
fn test_insert_menu() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Allocate a menu record at 0x300000
    let menu_ptr = 0x300000u32;
    // Write menu ID
    bus.write_word(menu_ptr, 1); // menu_id = 1
                                 // Write title "File" as a Pascal string at offset 14 of the menu record
    bus.write_byte(menu_ptr + 14, 4); // length = 4
    bus.write_byte(menu_ptr + 15, b'F');
    bus.write_byte(menu_ptr + 16, b'i');
    bus.write_byte(menu_ptr + 17, b'l');
    bus.write_byte(menu_ptr + 18, b'e');

    // Create a handle pointing to this menu record
    let handle_addr = 0x300100u32;
    bus.write_long(handle_addr, menu_ptr);

    // Push stack: SP+0: before_id(2), SP+2: menu_handle(4)
    bus.write_word(TEST_SP, 0); // before_id = 0
    bus.write_long(TEST_SP + 2, handle_addr); // menu_handle

    let result = disp.dispatch_menu(true, 0x135, &mut cpu, &mut bus);
    assert!(result.is_some(), "InsertMenu should be handled");
    assert!(result.unwrap().is_ok(), "InsertMenu should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 6, "InsertMenu should pop 6 bytes from stack");
    assert_eq!(disp.menus.len(), 1, "InsertMenu should add one menu");
    assert_eq!(
        disp.menus[0].title, "File",
        "InsertMenu should add 'File' to menus"
    );
}

// NewMenu+AppendMenu+InsertMenu must NOT produce duplicate entries in
// self.menus — InsertMenu only adds a menu the bar tracker doesn't
// already know about.
#[test]
fn test_newmenu_then_insertmenu_does_not_duplicate() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Step 1: NewMenu(129, "File")
    let title_ptr = 0x301000u32;
    bus.write_byte(title_ptr, 4);
    bus.write_byte(title_ptr + 1, b'F');
    bus.write_byte(title_ptr + 2, b'i');
    bus.write_byte(title_ptr + 3, b'l');
    bus.write_byte(title_ptr + 4, b'e');
    bus.write_long(TEST_SP, title_ptr); // SP+0 titlePtr
    bus.write_word(TEST_SP + 4, 129); // SP+4 menuID
    let result = disp.dispatch_menu(true, 0x131, &mut cpu, &mut bus);
    assert!(result.is_some() && result.unwrap().is_ok());
    let sp_after_new = cpu.read_reg(Register::A7);
    let handle = bus.read_long(sp_after_new);
    assert_ne!(handle, 0);
    assert_eq!(
        disp.menus.len(),
        1,
        "NewMenu should register the menu exactly once"
    );

    // Step 2: InsertMenu(handle, 0) — without a MENU resource
    // nothing should change in self.menus count.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0); // before_id
    bus.write_long(TEST_SP + 2, handle);
    let result = disp.dispatch_menu(true, 0x135, &mut cpu, &mut bus);
    assert!(result.is_some() && result.unwrap().is_ok());
    assert_eq!(
        disp.menus.len(),
        1,
        "InsertMenu must not duplicate the entry NewMenu already registered"
    );
    assert_eq!(disp.menus[0].title, "File");
    assert_eq!(disp.menus[0].handle, handle);
    assert_eq!(disp.last_inserted_menu_id, Some(129));
}

// InvalMenuBar ($A81D).
// IM:MTE 1992 p. 3-93: PROCEDURE InvalMenuBar. Parameterless
// Tool-bit PROCEDURE; A7 unchanged across the call.
#[test]
fn invalmenubar_procedure_call_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let result = disp.dispatch_menu(true, 0x01D, &mut cpu, &mut bus);
    assert!(result.is_some(), "InvalMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "InvalMenuBar should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InvalMenuBar is a parameterless procedure and must preserve A7"
    );

    // 5-call composition catches per-call drift that a single-call
    // check might mask.
    let sp_before_five = cpu.read_reg(Register::A7);
    for _ in 0..5 {
        let result = disp.dispatch_menu(true, 0x01D, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before_five,
        "InvalMenuBar must preserve A7 across a 5-call composition"
    );
}

#[test]
fn invalmenubar_defers_and_coalesces_one_draw_until_a_toolbox_event_scan() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_sent_open_app_event_for_test(true);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let (base, row_bytes, width, height, depth) = disp.get_screen_params();
    TrapDispatcher::fb_set_pixel(
        &mut bus, base, row_bytes, depth, width, height, 100, 5, true,
    );
    let dirty =
        TrapDispatcher::fb_get_pixel_index(&bus, base, row_bytes, depth, width, height, 100, 5);

    assert!(disp
        .dispatch_menu(true, 0x01D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(disp
        .dispatch_menu(true, 0x01D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(disp.event_queue.menu_bar_is_invalid());
    assert_eq!(
        TrapDispatcher::fb_get_pixel_index(&bus, base, row_bytes, depth, width, height, 100, 5,),
        dirty,
        "InvalMenuBar must not draw synchronously"
    );

    cpu.write_reg(Register::D0, 0);
    cpu.write_reg(Register::A0, 0x0020_0000);
    assert!(disp
        .dispatch_event(false, 0x030, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(
        disp.event_queue.menu_bar_is_invalid(),
        "low-level OS event scans must not consume Toolbox redraw work"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0x0020_0000);
    bus.write_word(TEST_SP + 4, 0);
    assert!(disp
        .dispatch_toolbox(true, 0x171, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(!disp.event_queue.menu_bar_is_invalid());
    assert_ne!(
        TrapDispatcher::fb_get_pixel_index(&bus, base, row_bytes, depth, width, height, 100, 5,),
        dirty,
        "the next Toolbox event scan must perform DrawMenuBar"
    );

    TrapDispatcher::fb_set_pixel(
        &mut bus, base, row_bytes, depth, width, height, 100, 5, true,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    assert!(disp
        .dispatch_toolbox(true, 0x171, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(
        TrapDispatcher::fb_get_pixel_index(&bus, base, row_bytes, depth, width, height, 100, 5,),
        dirty,
        "the deferred redraw must be consumed exactly once"
    );

    assert!(disp
        .dispatch_menu(true, 0x01D, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(disp
        .dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert!(
        !disp.event_queue.menu_bar_is_invalid(),
        "an explicit DrawMenuBar must satisfy the deferred request"
    );
}

// InitProcMenu ($A808).
// IM:V V-244: PROCEDURE InitProcMenu(mbResID: INTEGER). Tool-bit
// Pascal PROCEDURE; caller pushes a 2-byte mbResID, trap pops 2
// bytes, no FUNCTION result slot, A7 unchanged after the pop.
#[test]
fn initprocmenu_consumes_mbresid_and_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Caller-pushed mbResID + a trailing sentinel that the trap
    // must not touch.
    let sp_start = cpu.read_reg(Register::A7);
    let sentinel_addr = sp_start - 4;
    bus.write_long(sentinel_addr, 0xCAFE_BEEF);
    let sp_with_arg = sp_start - 6;
    bus.write_word(sp_with_arg, 0x0000); // mbResID = 0 (Apple-reserved default)
    cpu.write_reg(Register::A7, sp_with_arg);

    let result = disp.dispatch_menu(true, 0x008, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitProcMenu should be handled");
    assert!(result.unwrap().is_ok(), "InitProcMenu should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_with_arg + 2,
        "InitProcMenu must pop exactly 2 bytes (one INTEGER mbResID)"
    );
    assert_eq!(
        bus.read_long(sentinel_addr),
        0xCAFE_BEEF,
        "InitProcMenu must not write a FUNCTION result slot"
    );

    // 5-call composition: push 5 × 2-byte mbResID=0, dispatch 5
    // times in sequence, expect A7 to return to the pre-composition
    // value. Catches cumulative pop-size drift (e.g. pop-0 → +10,
    // pop-4 → −10) that a single-call test might mask.
    let sp_pre_five = cpu.read_reg(Register::A7);
    let sp_after_pushes = sp_pre_five - 10;
    for i in 0..5 {
        bus.write_word(sp_pre_five - 2 * (i + 1) as u32, 0x0000);
    }
    cpu.write_reg(Register::A7, sp_after_pushes);
    for _ in 0..5 {
        let result = disp.dispatch_menu(true, 0x008, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre_five,
        "InitProcMenu must pop 2 bytes per call across a 5-call composition"
    );
}

// 0x137 — DrawMenuBar: no stack params, calls draw_menu_bar_to_fb.
#[test]
fn test_draw_menu_bar() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
    assert!(result.is_some(), "DrawMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
}

#[test]
fn draw_menu_bar_releases_initial_kiosk_policy() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::InitialKiosk);

    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);

    assert!(result.is_some(), "DrawMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
    assert_eq!(
        disp.menu_bar_policy,
        crate::runner::MenuBarPolicy::GuestControlled
    );
    assert!(!disp.menu_bar_hidden);
}

#[test]
fn redraw_chrome_reconciles_menu_membership_before_painting() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x302000, "File");
    let game = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 301, 0x302020, "Game");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, game);

    disp.menus
        .iter_mut()
        .find(|menu| menu.handle == file)
        .unwrap()
        .visible_in_menu_bar = false;

    disp.redraw_chrome(&mut bus);

    assert!(
        disp.menus
            .iter()
            .find(|menu| menu.handle == file)
            .unwrap()
            .visible_in_menu_bar
    );
}

#[test]
fn draw_menu_bar_preserves_force_hidden_policy() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);

    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);

    assert!(result.is_some(), "DrawMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");
    assert_eq!(
        disp.menu_bar_policy,
        crate::runner::MenuBarPolicy::ForceHidden
    );
    assert!(disp.menu_bar_hidden);
}

#[test]
fn drawmenubar_uninserted_newmenu_title_is_not_hit_testable() {
    // IM:I I-352 and I-354: NewMenu creates a menu record, but only
    // InsertMenu places that menu in the current menu list that
    // DrawMenuBar renders.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let _ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x302000, "Ghost");

    cpu.write_reg(Register::A7, TEST_SP);
    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    assert!(
        disp.menu_title_regions().is_empty(),
        "DrawMenuBar should have no title regions when no menus are inserted"
    );
    assert!(
        disp.menu_title_hit_test(20).is_none(),
        "Uninserted NewMenu title must not be menu-bar hit-testable"
    );
}

#[test]
fn drawmenubar_skips_uninserted_menus_and_tracks_inserted_title_regions() {
    // IM:I I-354 / Macintosh Toolbox Essentials 1992 p.3-113:
    // DrawMenuBar draws according to the current menu list.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let _ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 300, 0x302000, "Ghost");
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302100, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x302200, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);

    cpu.write_reg(Register::A7, TEST_SP);
    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    let regions = disp.menu_title_regions();
    assert_eq!(
        regions.len(),
        2,
        "Only inserted menus should receive menu-bar title regions"
    );

    let file_mid = (regions[0].0 + regions[0].1) / 2;
    let file_idx = disp.menu_title_hit_test(file_mid).expect("file hit");
    assert_eq!(disp.menus[file_idx].id, 128);

    let edit_mid = (regions[1].0 + regions[1].1) / 2;
    let edit_idx = disp.menu_title_hit_test(edit_mid).expect("edit hit");
    assert_eq!(disp.menus[edit_idx].id, 129);
}

#[test]
fn drawmenubar_uses_retro_computer_art_for_the_system_mark_without_layout_drift() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (screen_base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let system = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302300, "\u{14}");
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x302400, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, system);
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    // The mark cell is pinned to the Chicago 12 mark advance the
    // System 7.5.3 menu bar lays out with, not measured from the
    // loaded font's mark glyph — Systemless substitutes its own
    // artwork for that glyph, so measuring it would drift the whole
    // bar whenever the font catalogue changed.
    let system_title_width = super::super::TrapDispatcher::menu_title_advance("\u{14}");
    assert_eq!(system_title_width, 11, "pinned system mark cell width");
    let regions = disp.menu_title_regions();
    assert_eq!(regions[0], (11, 18 + system_title_width + 6));
    assert_eq!(regions[1].0, 18 + system_title_width + 6);

    disp.draw_menu_bar_to_fb(&mut bus);

    const PIXELS: [[u8; 10]; 12] = [
        [0, 1, 1, 1, 1, 1, 1, 1, 1, 0],
        [1, 2, 2, 2, 2, 2, 2, 2, 2, 1],
        [1, 2, 3, 3, 3, 3, 3, 3, 2, 1],
        [1, 2, 3, 4, 3, 3, 3, 3, 2, 1],
        [1, 2, 3, 1, 3, 3, 1, 3, 2, 1],
        [1, 2, 3, 3, 3, 3, 3, 3, 2, 1],
        [1, 2, 3, 1, 3, 3, 1, 3, 2, 1],
        [1, 2, 3, 3, 1, 1, 3, 3, 2, 1],
        [1, 2, 2, 2, 2, 2, 2, 2, 2, 1],
        [0, 1, 1, 1, 1, 1, 1, 1, 1, 0],
        [0, 0, 0, 1, 2, 2, 1, 0, 0, 0],
        [0, 0, 1, 1, 1, 1, 1, 1, 0, 0],
    ];
    const PALETTE: [[u16; 3]; 4] = [
        [0x2222, 0x2222, 0x2222],
        [0xCCCC, 0xBBBB, 0x8888],
        [0x2222, 0x9999, 0xCCCC],
        [0xDDDD, 0xFFFF, 0xFFFF],
    ];
    let palette_indices = PALETTE.map(|rgb| {
        super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, rgb)
            .expect("8bpp test screen should expose a device color table")
    });
    let background =
        super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF, 0xFFFF, 0xFFFF])
            .unwrap();

    for (row, pixels) in PIXELS.into_iter().enumerate() {
        for (col, palette_index) in pixels.into_iter().enumerate() {
            let expected = if palette_index == 0 {
                background
            } else {
                palette_indices[usize::from(palette_index - 1)]
            };
            assert_eq!(
                screen_pixel_index(
                    &bus,
                    screen_base,
                    row_bytes,
                    18 + col as i16,
                    3 + row as i16,
                ),
                expected,
                "system menu mark pixel ({col}, {row})"
            );
        }
    }

    assert_eq!(disp.menu_title_regions(), regions);
    let file_midpoint = (regions[1].0 + regions[1].1) / 2;
    assert_eq!(disp.menu_title_hit_test(file_midpoint), Some(1));
}

#[test]
fn drawmenubar_4bpp_keeps_the_color_system_mark_through_title_reversals() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let system = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302380, "\u{14}");
    insert_menu(&mut disp, &mut cpu, &mut bus, system);
    disp.draw_menu_bar_to_fb(&mut bus);

    let palette_indices = crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE.map(|rgb| {
        super::super::TrapDispatcher::fb_main_screen_pixel_index_for_rgb(&bus, rgb)
            .expect("4bpp MainDevice should expose its color table")
    });
    let white = super::super::TrapDispatcher::fb_main_screen_pixel_index_for_rgb(&bus, [0xFFFF; 3])
        .unwrap();
    let black =
        super::super::TrapDispatcher::fb_main_screen_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
    let assert_mark_art = |bus: &MacMemoryBus, transparent_index: u8| {
        for (row, pixels) in crate::ui_art::RETRO_COMPUTER_MENU_MARK_PIXELS
            .into_iter()
            .enumerate()
        {
            for (col, palette_index) in pixels.into_iter().enumerate() {
                let expected = if palette_index == 0 {
                    transparent_index
                } else {
                    palette_indices[usize::from(palette_index - 1)]
                };
                assert_eq!(
                    packed_4bpp_screen_pixel_index(
                        bus,
                        base,
                        row_bytes,
                        18 + col as i16,
                        3 + row as i16,
                    ),
                    expected,
                    "packed system-menu mark pixel ({col}, {row})"
                );
            }
        }
    };
    assert_mark_art(&bus, white);
    let title_cell_pixel = (disp.menu_title_regions()[0].0 - 2, 1);
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            title_cell_pixel.0,
            title_cell_pixel.1,
        ),
        white
    );
    let unrelated_pixel = (title_cell_pixel.0 + 1, title_cell_pixel.1 + 1);
    set_packed_4bpp_screen_pixel_index(
        &mut bus,
        (base, row_bytes, 128, 64),
        unrelated_pixel.0,
        unrelated_pixel.1,
        8,
    );
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    disp.highlight_menu_title(&mut bus, 0);

    assert_mark_art(&bus, black);
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            title_cell_pixel.0,
            title_cell_pixel.1,
        ),
        black,
        "title selection should reverse only the transparent mark-cell background"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, unrelated_pixel.0, unrelated_pixel.1,),
        8,
        "title selection should preserve an unrelated packed color index"
    );

    disp.highlight_menu_title(&mut bus, 0);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "two packed title reversals should restore the full menu bar exactly"
    );

    disp.flash_menu_bar(&mut bus, 128);
    assert_mark_art(&bus, black);
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, unrelated_pixel.0, unrelated_pixel.1,),
        8,
        "FlashMenuBar(menuID) should preserve an unrelated packed color index"
    );
    disp.flash_menu_bar(&mut bus, 128);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "two packed FlashMenuBar(menuID) reversals should restore the full menu bar"
    );

    let mut kept_art_pixel = None;
    let mut knocked_art_pixel = None;
    for (row, pixels) in crate::ui_art::RETRO_COMPUTER_MENU_MARK_PIXELS
        .into_iter()
        .enumerate()
    {
        for (col, palette_index) in pixels.into_iter().enumerate() {
            if palette_index == 0 {
                continue;
            }
            let expected = palette_indices[usize::from(palette_index - 1)];
            if expected == black || expected == white {
                continue;
            }
            let point = (18 + col as i16, 3 + row as i16, expected);
            if (point.0 + point.1) % 2 == 0 {
                kept_art_pixel.get_or_insert(point);
            } else {
                knocked_art_pixel.get_or_insert(point);
            }
        }
    }
    let kept_art_pixel = kept_art_pixel.expect("colored art pixel on an enabled pattern bit");
    let knocked_art_pixel = knocked_art_pixel.expect("colored art pixel on a disabled pattern bit");

    disp.menus[0].enabled = false;
    disp.draw_menu_bar_to_fb(&mut bus);
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            knocked_art_pixel.0,
            knocked_art_pixel.1,
        ),
        white,
        "a disabled color mark should knock alternate art pixels into its normal background"
    );
    let disabled_before = bus.read_bytes(base, (row_bytes * 64) as usize);

    disp.highlight_menu_title(&mut bus, 0);
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, kept_art_pixel.0, kept_art_pixel.1,),
        kept_art_pixel.2,
        "selected disabled mark should retain art on enabled pattern bits"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            knocked_art_pixel.0,
            knocked_art_pixel.1,
        ),
        black,
        "selected disabled mark should knock alternate art pixels into the reversed background"
    );
    disp.highlight_menu_title(&mut bus, 0);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        disabled_before,
        "two disabled-title reversals should restore the dimmed mark exactly"
    );

    disp.flash_menu_bar(&mut bus, 128);
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            knocked_art_pixel.0,
            knocked_art_pixel.1,
        ),
        black,
        "FlashMenuBar(menuID) should keep a disabled mark dimmed on the reversed background"
    );
    disp.flash_menu_bar(&mut bus, 128);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        disabled_before,
        "two disabled FlashMenuBar(menuID) reversals should restore the dimmed mark"
    );
}

#[test]
fn drawmenubar_keeps_the_retro_computer_mark_legible_in_monochrome() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 16;
    let screen_base = bus.alloc(row_bytes * 64);
    disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 64, 1);
    clear_1bpp_screen(&mut bus, screen_base, row_bytes, 64);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let system = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302300, "\u{14}");
    insert_menu(&mut disp, &mut cpu, &mut bus, system);

    disp.draw_menu_bar_to_fb(&mut bus);

    for &(x, y) in &[
        (1, 0),
        (0, 1),
        (9, 1),
        (3, 4),
        (6, 4),
        (3, 6),
        (6, 6),
        (4, 7),
        (5, 7),
        (2, 11),
        (7, 11),
    ] {
        assert!(
            screen_pixel_is_set(&bus, screen_base, row_bytes, 18 + x, 3 + y),
            "dark mark pixel ({x}, {y})"
        );
    }
    for &(x, y) in &[(0, 0), (1, 1), (3, 3), (5, 10)] {
        assert!(
            !screen_pixel_is_set(&bus, screen_base, row_bytes, 18 + x, 3 + y),
            "light mark pixel ({x}, {y})"
        );
    }
}

// MTE 1992 p. 3-131 / HIG 1992 p. 54: DisableItem(menu, 0) leaves the
// title visible but dimmed. On a colour screen the definition procedure
// greys it through GetGray (IM:V 1986 p. V-142), so every glyph pixel
// lands on the intermediate shade — System 7.5.3 under BasiliskII draws
// Sid Meier's Civilization's disabled City title in solid grey, not as
// stippled black.
#[test]
fn drawmenubar_8bpp_dims_a_disabled_title_to_solid_gray() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 643, 0x303300, "File");
    let city = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 644, 0x303340, "City");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, city);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, city);
    assert!(
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DisableItem(menu, 0) should disable the whole City title"
    );

    disp.draw_menu_bar_to_fb(&mut bus);

    let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
    let gray =
        super::super::TrapDispatcher::fb_gray_pixel_index_between(&bus, [0xFFFF; 3], [0, 0, 0])
            .expect("8bpp test screen should express an intermediate shade");
    let regions = disp.menu_title_regions();
    let cell = |(left, right): (i16, i16)| -> Vec<u8> {
        (1..19)
            .flat_map(|y| (left..right).map(move |x| (x, y)))
            .map(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y))
            .collect()
    };

    let enabled = cell(regions[0]);
    assert!(
        enabled.iter().any(|&pixel| pixel == black),
        "the enabled File title should draw in full black"
    );

    let dimmed = cell(regions[1]);
    assert!(
        dimmed.iter().any(|&pixel| pixel == gray),
        "the disabled City title should draw in the intermediate grey shade"
    );
    assert!(
        !dimmed.iter().any(|&pixel| pixel == black),
        "no part of a dimmed title should stay full black"
    );
}

#[test]
fn drawmenubar_tracks_full_top_menubar_title_order_and_hits() {
    // The visible Mac menu bar is part of the rendered surface, not
    // only the pull-down menu body. This pins the common top-bar layout
    // shape; exact pixel rendering follows BasiliskII.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    for (id, title, ptr) in [
        (128, "Apple", 0x302300),
        (129, "File", 0x302360),
        (130, "Edit", 0x3023C0),
        (131, "Special", 0x302420),
    ] {
        let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, id, ptr, title);
        insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    }

    cpu.write_reg(Register::A7, TEST_SP);
    let result = disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus);
    assert!(result.is_some(), "DrawMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "DrawMenuBar should succeed");

    let regions = disp.menu_title_regions();
    assert_eq!(
        regions.len(),
        4,
        "Apple/File/Edit/Special should all receive title hit regions"
    );
    for pair in regions.windows(2) {
        assert!(
            pair[0].1 <= pair[1].0,
            "top menu-bar title regions should be left-to-right and non-overlapping"
        );
    }

    for ((left, right), expected_id) in regions.iter().copied().zip([128, 129, 130, 131]) {
        let hit_h = (left + right) / 2;
        let menu_idx = disp
            .menu_title_hit_test(hit_h)
            .expect("title midpoint should hit an inserted menu");
        assert_eq!(
            disp.menus[menu_idx].id, expected_id,
            "menu title midpoint should hit the expected top-bar menu"
        );
    }
}

#[test]
fn drawmenubar_systemless_theme_routes_chrome_through_provider() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_row_bytes = 64;
    let classic_base = classic_bus.alloc(classic_row_bytes * 342);
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    classic.menu_bar_hidden = false;
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    classic_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic
        .dispatch_menu(true, 0x137, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .unwrap();

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    let themed_row_bytes = 64;
    let themed_base = themed_bus.alloc(themed_row_bytes * 342);
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    themed.menu_bar_hidden = false;
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    themed_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    themed_cpu.write_reg(Register::A7, TEST_SP);
    themed
        .dispatch_menu(true, 0x137, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();

    assert!(
        !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 5, 0),
        "classic System 7 menu bar chrome should leave the top edge white"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 5, 0),
        "systemless-default menu bar provider should own the top edge"
    );
}

#[test]
fn drawmenubar_systemless_theme_routes_title_states_through_provider() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_row_bytes = 64;
    let classic_base = classic_bus.alloc(classic_row_bytes * 342);
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    classic.menu_bar_hidden = false;
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    classic_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let classic_file = new_menu_with_title(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        621,
        0x302E00,
        "File",
    );
    let classic_edit = new_menu_with_title(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        622,
        0x302E40,
        "Edit",
    );
    insert_menu(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        classic_file,
    );
    insert_menu(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        classic_edit,
    );
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_word(TEST_SP, 0);
    classic_bus.write_long(TEST_SP + 2, classic_edit);
    classic
        .dispatch_menu(true, 0x13A, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .unwrap();
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic
        .dispatch_menu(true, 0x137, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .unwrap();

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    let themed_row_bytes = 64;
    let themed_base = themed_bus.alloc(themed_row_bytes * 342);
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    themed.menu_bar_hidden = false;
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    themed_bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let themed_file = new_menu_with_title(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        621,
        0x302E00,
        "File",
    );
    let themed_edit = new_menu_with_title(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        622,
        0x302E40,
        "Edit",
    );
    insert_menu(&mut themed, &mut themed_cpu, &mut themed_bus, themed_file);
    insert_menu(&mut themed, &mut themed_cpu, &mut themed_bus, themed_edit);
    themed_cpu.write_reg(Register::A7, TEST_SP);
    themed_bus.write_word(TEST_SP, 0);
    themed_bus.write_long(TEST_SP + 2, themed_edit);
    themed
        .dispatch_menu(true, 0x13A, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();
    themed_cpu.write_reg(Register::A7, TEST_SP);
    themed
        .dispatch_menu(true, 0x137, &mut themed_cpu, &mut themed_bus)
        .unwrap()
        .unwrap();

    let classic_regions = classic.menu_title_regions();
    let themed_regions = themed.menu_title_regions();
    assert_eq!(
        themed_regions, classic_regions,
        "systemless-default menu-title chrome must preserve title hit regions"
    );
    let (file_left, _file_right) = themed_regions[0];
    let (edit_left, _edit_right) = themed_regions[1];

    // HIG 1992 p. 54: unavailable menu titles remain visible but dimmed.
    // The provider leaves resting enabled titles undecorated, frames
    // disabled titles, and keeps text placement and title geometry on the
    // existing Menu Manager path.
    assert!(
        !screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            file_left + 4,
            17
        ),
        "systemless-default should leave an enabled menu title undecorated at rest"
    );
    assert!(
        !screen_pixel_is_set(
            &classic_bus,
            classic_base,
            classic_row_bytes,
            file_left + 4,
            17
        ),
        "classic System 7 path should leave an enabled menu title undecorated at rest"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, edit_left + 2, 3),
        "systemless-default should draw provider chrome for a disabled menu title"
    );
    assert!(
        !screen_pixel_is_set(
            &classic_bus,
            classic_base,
            classic_row_bytes,
            edit_left + 2,
            3
        ),
        "classic System 7 path should not draw the systemless disabled-title frame"
    );
}

#[test]
fn highlight_menu_title_systemless_theme_routes_highlight_state_through_provider() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    disp.menu_bar_hidden = false;
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 623, 0x302E80, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let (left, right) = regions[0];
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, left + 4, 17),
        "precondition: enabled systemless title should be undecorated before highlighting"
    );
    let title_pixel = (2..16)
        .flat_map(|y| ((left + 7)..(right - 7)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
        .expect("precondition: menu title text should draw before highlighting");

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        disp.menu_title_regions(),
        regions,
        "systemless highlighted title chrome must preserve title hit regions"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, left + 1, 5),
        "systemless-default should fill highlighted title chrome through the provider"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, left + 4, 17),
        "provider-highlighted title chrome should fill the resting decoration area"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        "highlighted systemless title should redraw the menu text in the highlighted foreground"
    );
}

#[test]
fn draw_menu_dropdown_systemless_theme_routes_chrome_through_provider() {
    let rect = (20, 20, 56, 120);

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_row_bytes = 64;
    let classic_base = classic_bus.alloc(classic_row_bytes * 342);
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    let classic_menu = new_menu_with_title(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        610,
        0x302300,
        "File",
    );
    append_menu_data(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        classic_menu,
        0x302340,
        "Open/O;Close/W",
    );
    classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    let themed_row_bytes = 64;
    let themed_base = themed_bus.alloc(themed_row_bytes * 342);
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    let themed_menu = new_menu_with_title(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        610,
        0x302300,
        "File",
    );
    append_menu_data(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        themed_menu,
        0x302340,
        "Open/O;Close/W",
    );
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

    assert!(
        screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 120, 22),
        "classic dropdown chrome should draw its right-edge drop shadow"
    );
    assert!(
        !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 120, 22),
        "systemless-default dropdown provider should not draw the classic shadow"
    );
}

#[test]
fn draw_menu_dropdown_systemless_theme_routes_item_states_through_provider() {
    let rect = (20, 20, 100, 140);

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_row_bytes = 64;
    let classic_base = classic_bus.alloc(classic_row_bytes * 342);
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    let classic_menu = new_menu_with_title(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        611,
        0x302400,
        "File",
    );
    append_menu_data(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        classic_menu,
        0x302440,
        "Open/O;Pick;-;(Dim/D",
    );
    classic.menus[0].items[0].mark = 0x12;
    classic.menus[0].items[1].icon = 7;
    classic
        .menu_tracking
        .set(Some(test_tracked_menu_state(classic_menu, rect, 1)));
    classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    let themed_row_bytes = 64;
    let themed_base = themed_bus.alloc(themed_row_bytes * 342);
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    let themed_menu = new_menu_with_title(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        611,
        0x302400,
        "File",
    );
    append_menu_data(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        themed_menu,
        0x302440,
        "Open/O;Pick;-;(Dim/D",
    );
    themed.menus[0].items[0].mark = 0x12;
    themed.menus[0].items[1].icon = 7;
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);
    let highlighted_text_pixel = ((rect.0 + 3)..(rect.0 + MENU_ROW_HEIGHT - 2))
        .flat_map(|y| ((rect.1 + 15)..(rect.1 + 60)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, *x, *y))
        .expect("precondition: unselected menu-item text should draw");
    themed
        .menu_tracking
        .set(Some(test_tracked_menu_state(themed_menu, rect, 1)));
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

    assert!(
        screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 23, 23),
        "classic selected rows should reverse their full background"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 23, 23),
        "systemless-default provider should own highlighted menu-item row chrome"
    );
    assert!(
        screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 100, 26),
        "classic selected rows should remain filled away from the provider rail"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 100, 26),
        "systemless-default should fill the complete highlighted row"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 100, 99),
        "systemless-default should preserve the menu pane's bottom border"
    );
    assert!(
        !screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            highlighted_text_pixel.0,
            highlighted_text_pixel.1,
        ),
        "systemless-default should redraw highlighted item text in a contrasting foreground"
    );
    assert!(
        !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 36, 43),
        "classic dropdown path should not draw systemless menu-item icon chrome"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 36, 43),
        "systemless-default provider should receive and render menu-item icon state"
    );
    assert!(
        !screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, 29, 79),
        "classic separator remains dotted at this odd x-coordinate"
    );
    assert!(
        screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, 29, 79),
        "systemless-default provider should own separator menu-item row chrome"
    );
}

#[test]
fn draw_menu_dropdown_systemless_theme_preserves_mark_and_command_indicators() {
    let rect = (20, 20, 56, 140);
    let row_top = rect.0 + 1;
    let row_bottom = row_top + 16;

    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let classic_row_bytes = 64;
    let classic_base = classic_bus.alloc(classic_row_bytes * 342);
    classic.set_screen_mode_for_test(classic_base, classic_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut classic_bus, classic_base, classic_row_bytes, 342);
    let classic_menu = new_menu_with_title(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        618,
        0x302B00,
        "File",
    );
    append_menu_data(
        &mut classic,
        &mut classic_cpu,
        &mut classic_bus,
        classic_menu,
        0x302B40,
        "Open/1",
    );
    classic.menus[0].items[0].mark = 0x12;
    classic.draw_menu_dropdown(&mut classic_bus, 0, rect);

    let (mut themed, mut themed_cpu, mut themed_bus) = setup_with_port();
    let themed_row_bytes = 64;
    let themed_base = themed_bus.alloc(themed_row_bytes * 342);
    themed.set_ui_theme_id(UiThemeId::SystemlessDefault);
    themed.set_screen_mode_for_test(themed_base, themed_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    let themed_menu = new_menu_with_title(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        618,
        0x302B00,
        "File",
    );
    append_menu_data(
        &mut themed,
        &mut themed_cpu,
        &mut themed_bus,
        themed_menu,
        0x302B40,
        "Open/1",
    );
    themed.menus[0].items[0].mark = 0x12;
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

    let provider_mark_pixel = |x: i16, y: i16| {
        let provider_left = rect.1 + 1;
        let provider_top = row_top;
        x >= provider_left + 5
            && x < provider_left + 13
            && y >= provider_top + 3
            && y < provider_top + 13
    };
    let mark_pixel = (row_top..row_bottom)
        .flat_map(|y| ((rect.1 + 4)..(rect.1 + 16)).map(move |x| (x, y)))
        .find(|(x, y)| {
            screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
                && !provider_mark_pixel(*x, *y)
        })
        .expect("classic mark glyph should have a pixel outside the provider mark indicator");

    let provider_command_pixel = |x: i16, y: i16| {
        let provider_left = rect.1 + 1;
        let provider_top = row_top;
        let provider_width = (rect.3 - 1) - provider_left;
        x >= provider_left + provider_width - 15
            && x < provider_left + provider_width - 13
            && y >= provider_top + 4
            && y < provider_top + 12
    };
    let command_pixel = (row_top..row_bottom)
        .flat_map(|y| ((rect.3 - 28)..(rect.3 - 4)).map(move |x| (x, y)))
        .find(|(x, y)| {
            screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y)
                && !provider_command_pixel(*x, *y)
        })
        .expect(
            "classic command-key glyph should have a pixel outside the provider command marker",
        );
    let command_left = rect.3 - 25;
    let command_symbol_pixel = (row_top..row_bottom)
        .flat_map(|y| (command_left..(command_left + 7)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y))
        .expect("classic command-key equivalent should draw the Command symbol itself");
    let label_pixel = (row_top..row_bottom)
        .flat_map(|y| ((rect.1 + 15)..(rect.1 + 60)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&classic_bus, classic_base, classic_row_bytes, *x, *y))
        .expect("classic item label should draw");

    assert!(
        screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            mark_pixel.0,
            mark_pixel.1
        ),
        "systemless-default row chrome should preserve the menu item's mark glyph"
    );
    assert!(
        screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            command_pixel.0,
            command_pixel.1
        ),
        "systemless-default row chrome should preserve the full command-key equivalent"
    );
    assert!(
        screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            command_symbol_pixel.0,
            command_symbol_pixel.1
        ),
        "systemless-default row chrome should preserve the Command symbol itself"
    );

    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    themed
        .menu_tracking
        .set(Some(test_tracked_menu_state(themed_menu, rect, 1)));
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

    assert!(
        !screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            mark_pixel.0,
            mark_pixel.1
        ),
        "highlighted systemless-default should reverse the menu item's mark glyph"
    );
    assert!(
        !screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            command_pixel.0,
            command_pixel.1
        ),
        "highlighted systemless-default should reverse the full command-key equivalent"
    );
    assert!(
        !screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            command_symbol_pixel.0,
            command_symbol_pixel.1
        ),
        "highlighted systemless-default should reverse the Command symbol itself"
    );

    themed.redraw_chrome(&mut themed_bus);

    for (pixel, label) in [
        (mark_pixel, "checkmark"),
        (label_pixel, "item label"),
        (command_symbol_pixel, "Command symbol"),
        (command_pixel, "command key"),
    ] {
        assert!(
            !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, pixel.0, pixel.1),
            "final themed chrome composition should preserve the reversed highlighted {label}"
        );
    }

    clear_1bpp_screen(&mut themed_bus, themed_base, themed_row_bytes, 342);
    themed.menu_tracking.set(None);
    themed.dialog_tracking = Some(super::super::dispatch::DialogTrackingState {
        active_popup: Some(super::super::dispatch::DialogPopupTrackingState {
            item_no: 1,
            ctrl_handle: 0,
            ctrl_ptr: 0,
            active_menu: 0,
            highlighted_item: 1,
            saved_pixels: Default::default(),
            dropdown_rect: rect,
        }),
        ..Default::default()
    });
    themed.draw_menu_dropdown(&mut themed_bus, 0, rect);

    assert!(
        screen_pixel_is_set(
            &themed_bus,
            themed_base,
            themed_row_bytes,
            rect.1 + 2,
            row_top + 1
        ),
        "dialog popup tracking should route the highlighted row through themed chrome"
    );

    themed.redraw_chrome(&mut themed_bus);

    for (pixel, label) in [
        (mark_pixel, "popup checkmark"),
        (label_pixel, "popup item label"),
        (command_symbol_pixel, "popup Command symbol"),
        (command_pixel, "popup command key"),
    ] {
        assert!(
            !screen_pixel_is_set(&themed_bus, themed_base, themed_row_bytes, pixel.0, pixel.1),
            "final themed popup composition should preserve the reversed highlighted {label}"
        );
    }
}

#[test]
fn draw_menu_dropdown_applies_setitemstyle_pixels_with_classic_style_metrics() {
    let rect = (20, 20, 160, 180);
    let menu_data = "Bold;Italic;Underline;Outline;Shadow;Condense;Extend";
    let style_cases = [
        ("bold", "Bold", 0x01u8),
        ("italic", "Italic", 0x02u8),
        ("underline", "Underline", 0x04u8),
        ("outline", "Outline", 0x08u8),
        ("shadow", "Shadow", 0x10u8),
        ("condense", "Condense", 0x20u8),
        ("extend", "Extend", 0x40u8),
    ];

    let (mut plain, mut plain_cpu, mut plain_bus) = setup_with_port();
    let plain_row_bytes = 64;
    let plain_base = plain_bus.alloc(plain_row_bytes * 342);
    plain.set_screen_mode_for_test(plain_base, plain_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut plain_bus, plain_base, plain_row_bytes, 342);
    let plain_menu = new_menu_with_title(
        &mut plain,
        &mut plain_cpu,
        &mut plain_bus,
        619,
        0x302C00,
        "Style",
    );
    append_menu_data(
        &mut plain,
        &mut plain_cpu,
        &mut plain_bus,
        plain_menu,
        0x302C40,
        menu_data,
    );
    let plain_size =
        calc_menu_size_for_test(&mut plain, &mut plain_cpu, &mut plain_bus, plain_menu);
    plain.draw_menu_dropdown(&mut plain_bus, 0, rect);

    let (mut styled, mut styled_cpu, mut styled_bus) = setup_with_port();
    let styled_row_bytes = 64;
    let styled_base = styled_bus.alloc(styled_row_bytes * 342);
    styled.set_screen_mode_for_test(styled_base, styled_row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut styled_bus, styled_base, styled_row_bytes, 342);
    let styled_menu = new_menu_with_title(
        &mut styled,
        &mut styled_cpu,
        &mut styled_bus,
        619,
        0x302C00,
        "Style",
    );
    append_menu_data(
        &mut styled,
        &mut styled_cpu,
        &mut styled_bus,
        styled_menu,
        0x302C40,
        menu_data,
    );
    for (idx, (_label, _text, style)) in style_cases.iter().enumerate() {
        set_menu_item_style(
            &mut styled,
            &mut styled_cpu,
            &mut styled_bus,
            styled_menu,
            idx as i16 + 1,
            *style,
        );
    }
    let styled_size =
        calc_menu_size_for_test(&mut styled, &mut styled_cpu, &mut styled_bus, styled_menu);
    styled.draw_menu_dropdown(&mut styled_bus, 0, rect);

    assert_eq!(
        styled_size.0, plain_size.0,
        "SetItemStyle should not change CalcMenuSize width"
    );
    assert_eq!(
        styled_size.1,
        plain_size.1
            + (super::standard_menu_row_height(
                None,
                false,
                super::QuickDrawTextStyle::from_bits(super::QuickDrawTextStyle::SHADOW_BIT),
            ) - super::MENU_ROW_HEIGHT),
        "System 7.5.3's standard MDEF grows the shadow-styled item row"
    );

    let mut row_top = rect.0 + 1;
    for (idx, (label, text, style)) in style_cases.iter().enumerate() {
        let styled_height = styled.menu_item_height(&styled_bus, &styled.menus[0].items[idx]);
        let plain_height = plain.menu_item_height(&plain_bus, &plain.menus[0].items[idx]);
        let expected_height = if super::QuickDrawTextStyle::from_bits(*style).shadow() {
            plain_height.max(super::standard_menu_row_height(
                None,
                false,
                super::QuickDrawTextStyle::from_bits(super::QuickDrawTextStyle::SHADOW_BIT),
            ))
        } else {
            plain_height
        };
        assert_eq!(
            styled_height, expected_height,
            "{label} style should use the classic MDEF row height"
        );
        assert_eq!(
            styled.menu_item_icon_width(&styled_bus, &styled.menus[0].items[idx]),
            plain.menu_item_icon_width(&plain_bus, &plain.menus[0].items[idx]),
            "{label} style should not change mark/icon/command geometry"
        );

        let row_bottom = row_top + styled_height;
        let text_left = rect.1 + 18;
        let text_right =
            text_left + super::super::TrapDispatcher::fb_measure_string(text, 0, 12) + 8;
        let styled_only_pixel = ((row_top - 1)..(row_bottom + 2))
            .flat_map(|y| ((text_left - 2)..(text_right + 4)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&styled_bus, styled_base, styled_row_bytes, *x, *y)
                    && !screen_pixel_is_set(&plain_bus, plain_base, plain_row_bytes, *x, *y)
            });
        let plain_only_pixel = ((row_top - 1)..(row_bottom + 2))
            .flat_map(|y| ((text_left - 2)..(text_right + 4)).map(move |x| (x, y)))
            .find(|(x, y)| {
                screen_pixel_is_set(&plain_bus, plain_base, plain_row_bytes, *x, *y)
                    && !screen_pixel_is_set(&styled_bus, styled_base, styled_row_bytes, *x, *y)
            });
        assert!(
            styled_only_pixel.is_some() || plain_only_pixel.is_some(),
            "{label} SetItemStyle bit should produce a visible menu-text pixel difference"
        );
        row_top = row_bottom;
    }
}

// MTE 1992 p. 3-131 / HIG 1992 p. 54: an unavailable item stays visible
// but dimmed, and MTE 1992 p. 3-30 says a colour screen shows dividers
// and dimmed content as grey lines and glyphs (a black-and-white screen
// gets the 50% grey pattern instead). System 7.5.3 under BasiliskII draws
// Absolute Solitaire's all-disabled Edit menu that way — every row
// legible in solid grey — so a dimmed row must not collapse to bare
// background.
#[test]
fn draw_menu_dropdown_8bpp_dims_disabled_items_and_dividers_to_solid_gray() {
    let rect = (20, 20, 70, 140);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 640, 0x303000, "Edit");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x303040,
        "Undo/Z;-;Copy",
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, menu);
    assert!(
        disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DisableItem should disable the first item"
    );

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let background =
        super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
    let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
    let gray =
        super::super::TrapDispatcher::fb_gray_pixel_index_between(&bus, [0xFFFF; 3], [0, 0, 0])
            .expect("8bpp test screen should express an intermediate shade");

    let row_pixels = |top: i16, height: i16| -> Vec<u8> {
        (top..top + height)
            .flat_map(|y| ((rect.1 + 1)..(rect.3 - 1)).map(move |x| (x, y)))
            .map(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y))
            .collect()
    };

    // The detached test rectangle's top edge is menu chrome, not part of
    // the disabled row's ink. Inspect only the row interior.
    let disabled = row_pixels(rect.0 + 1, MENU_ROW_HEIGHT - 1);
    assert!(
        disabled.iter().any(|&pixel| pixel == gray),
        "the disabled item's text should draw in the intermediate grey shade"
    );
    assert!(
        !disabled.iter().any(|&pixel| pixel == black),
        "no part of a dimmed item should stay full black"
    );

    // The divider is a solid grey line one pixel above the row midpoint.
    let divider_top = rect.0 + MENU_ROW_HEIGHT;
    let divider_y = divider_top + super::STANDARD_MENU_SEPARATOR_HEIGHT / 2 - 1;
    for x in (rect.1 + 1)..(rect.3 - 1) {
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, x, divider_y),
            gray,
            "divider pixel at x={x}"
        );
    }
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, divider_y - 2),
        background,
        "the divider should be a single line, not a filled row"
    );

    let enabled = row_pixels(
        divider_top + super::STANDARD_MENU_SEPARATOR_HEIGHT,
        MENU_ROW_HEIGHT,
    );
    assert!(
        enabled.iter().any(|&pixel| pixel == black),
        "an enabled item's text should still draw in full black"
    );
}

// Disabling item zero dims the title and every item without preventing
// the menu from being displayed. Macintosh Toolbox Essentials (1992),
// pp. 3-6--3-7 and 3-58--3-59.
#[test]
fn draw_menu_dropdown_8bpp_dims_every_item_in_a_disabled_menu() {
    let rect = (20, 20, 52, 140);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 645, 0x303680, "View");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x3036C0,
        "Zoom;Actual Size",
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, menu);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
    let gray =
        super::super::TrapDispatcher::fb_gray_pixel_index_between(&bus, [0xFFFF; 3], [0, 0, 0])
            .expect("8bpp test screen should express an intermediate shade");
    for row in 0..2 {
        let pixels = ((rect.0 + row * MENU_ROW_HEIGHT + 1)
            ..(rect.0 + (row + 1) * MENU_ROW_HEIGHT - 1))
            .flat_map(|y| ((rect.1 + 1)..(rect.3 - 1)).map(move |x| (x, y)))
            .map(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y))
            .collect::<Vec<_>>();
        assert!(
            pixels.iter().any(|pixel| *pixel == gray),
            "disabled menu row {} did not retain dimmed ink",
            row + 1,
        );
        assert!(
            !pixels.iter().any(|pixel| *pixel == black),
            "disabled menu row {} retained enabled black ink",
            row + 1,
        );
    }
}

// MTE 1992 p. 3-30: a black-and-white screen has no intermediate shade,
// so the definition procedure dims with the 50% grey pattern and draws
// dividers as dotted lines. Imaging With QuickDraw 1994 p. 3-9 fixes the
// pattern phase, so the surviving pixels are the ones where x + y is even.
#[test]
fn draw_menu_dropdown_1bpp_dims_disabled_items_with_the_gray_pattern() {
    let rect = (20, 16, 60, 120);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 16u32;
    let base = bus.alloc(row_bytes * 96);
    disp.set_screen_mode_for_test(base, row_bytes, 128, 96, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 96);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, base);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 641, 0x303100, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303140, "(Undo;-");

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let dimmed_row: Vec<(i16, i16)> = (rect.0..(rect.0 + MENU_ROW_HEIGHT))
        .flat_map(|y| ((rect.1 + 1)..(rect.3 - 1)).map(move |x| (x, y)))
        .collect();
    assert!(
        dimmed_row
            .iter()
            .any(|&(x, y)| screen_pixel_is_set(&bus, base, row_bytes, x, y)),
        "the dimmed item should still leave legible ink on a 1-bit screen"
    );
    assert!(
        dimmed_row
            .iter()
            .filter(|&&(x, y)| screen_pixel_is_set(&bus, base, row_bytes, x, y))
            .all(|&(x, y)| (x + y) % 2 == 0),
        "dimmed ink should survive only where the 50% grey pattern is on"
    );

    // A trailing divider is not laid out at all, so the box is one row
    // tall and its bottom frame sits directly under the dimmed item.
    assert_eq!(
        disp.menu_items_height(&bus, &disp.menus[0].items),
        MENU_ROW_HEIGHT,
        "a trailing divider should not claim a row"
    );
}

// Applications author the Apple menu as an About command plus a divider
// so AppendResMenu has something to append below (MTE 1992 pp. 3-97 to
// 3-98). With no Apple Menu Items to append, System 7.5.3 under
// BasiliskII draws Absolute Solitaire's Apple menu exactly one item tall
// rather than leaving a dangling divider.
#[test]
fn menu_layout_gives_a_trailing_divider_no_row() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 642, 0x303200, "\u{14}");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x303240,
        "About Systemless;-",
    );

    assert_eq!(disp.menus[0].items.len(), 2, "both items stay in the menu");
    assert_eq!(
        disp.menu_items_height(&bus, &disp.menus[0].items),
        MENU_ROW_HEIGHT,
        "only the About command claims a row"
    );

    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303280, "Note Pad");
    assert_eq!(
        disp.menu_items_height(&bus, &disp.menus[0].items),
        MENU_ROW_HEIGHT * 2 + super::STANDARD_MENU_SEPARATOR_HEIGHT,
        "the divider claims its row again once an item follows it"
    );
}

#[test]
fn draw_menu_dropdown_8bpp_uses_menucinfo_background_and_item_name_color() {
    let rect = (20, 20, 38, 110);
    let menu_id = 620;
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let black = (0, 0, 0);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x302D00, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302D40, "Open");

    let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
    write_mc_entry_colors(
        &mut bus,
        entries_ptr + MC_ENTRY_SIZE as u32,
        menu_id,
        1,
        green,
        blue,
        green,
        red,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 2);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install title and item color entries"
    );

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 24, 22),
        35,
        "title RGB4 full red should fill the 8bpp dropdown background"
    );

    let text_left = rect.1 + 18;
    let text_top = rect.0 + 1;
    let text_bottom = text_top + 16;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    let has_blue_text_pixel = (text_top..text_bottom)
        .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
        .any(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y) == 210);
    assert!(
        has_blue_text_pixel,
        "item RGB2 full blue should draw the menu item name"
    );

    let has_title_default_green_text_pixel = (text_top..text_bottom)
        .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
        .any(|(x, y)| screen_pixel_index(&bus, base, row_bytes, x, y) == 185);
    assert!(
        !has_title_default_green_text_pixel,
        "item RGB2 should override the title RGB3 default item color"
    );
}

#[test]
fn draw_menu_dropdown_8bpp_uses_item_name_color_for_black_and_white_icons() {
    let rect = (20, 20, 70, 150);
    let menu_id = 621;
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let black = (0, 0, 0);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 100);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x302E00, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302E40, "Big;Small");
    disp.menus[0].items[0].icon = 7;
    disp.menus[0].items[1].icon = 8;
    disp.menus[0].items[1].key_equiv = MENU_KEY_SMALL_ICON;
    let icon = menu_icon_source_with_left_stripe();
    let sicn = sicn_source_with_left_stripe();
    disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);
    disp.install_test_resource(&mut bus, *b"SICN", 264, &sicn);

    let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
    write_mc_entry_colors(
        &mut bus,
        entries_ptr + MC_ENTRY_SIZE as u32,
        menu_id,
        1,
        green,
        blue,
        green,
        red,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 2);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install title and item color entries"
    );

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, rect.0 + 1),
        210,
        "normal black-and-white ICON pixels should use the item RGB2 name color"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, rect.0 + 35),
        185,
        "SICN pixels should fall back to the title RGB3 default item color"
    );
}

#[test]
fn draw_menu_dropdown_remaps_cicn_color_table_to_main_screen_device() {
    let rect = (20, 20, 38, 110);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 622, 0x302F00, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302F40, "Crop");
    disp.menus[0].items[0].icon = 7;
    let icon = cicn_source_with_solid_4bpp_color(3, [0, 0xFFFF, 0]);
    disp.install_test_resource(&mut bus, *b"cicn", 263, &icon);

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let mapped_green =
        super::super::TrapDispatcher::fb_main_screen_pixel_index_for_rgb(&bus, [0, 0xFFFF, 0])
            .expect("main screen device should represent green");
    assert_ne!(
        mapped_green, 3,
        "precondition: source icon index must differ from the device index"
    );
    assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
            mapped_green,
            "cicn pixels should be mapped through their own ColorTable instead of copied as raw indexes"
        );
}

#[test]
fn draw_menu_dropdown_4bpp_preserves_remapped_cicn_color_when_selected() {
    let rect = (20, 20, 38, 110);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 622, 0x302F00, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302F40, "Crop");
    disp.menus[0].items[0].icon = 7;
    let icon = cicn_source_with_solid_4bpp_color(3, [0, 0xFFFF, 0]);
    disp.install_test_resource(&mut bus, *b"cicn", 263, &icon);

    disp.draw_menu_dropdown(&mut bus, 0, rect);
    let icon_pixel = (rect.1 + 4, rect.0 + 2);
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, icon_pixel.0, icon_pixel.1,),
        8,
        "4bpp cicn color should map through MainDevice instead of the foreign 8bpp TheGDevice"
    );

    disp.menu_tracking
        .set(Some(test_tracked_menu_state(menu, rect, 1)));
    disp.draw_menu_dropdown(&mut bus, 0, rect);
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, icon_pixel.0, icon_pixel.1,),
        8,
        "selected color cicn artwork should keep its mapped color"
    );
}

#[test]
fn cached_menu_bar_finishes_pending_cpu_recolor_like_a_repaint() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 622, 0x302F00, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    let palette = std::array::from_fn(|i| disp.device_clut[i].map(|c| (c >> 8) as u8));
    bus.enable_outline_presentation(disp.screen_mode, palette, 4);
    disp.draw_menu_bar_to_fb(&mut bus);
    let body = base + row_bytes * 24;
    let len = (row_bytes * 40) as usize;
    bus.fill_bytes(body, len as u32, 255);
    super::super::TrapDispatcher::fb_draw_string_styled_index(
        &mut bus, base, row_bytes, 8, 160, 64, 10, 45, "A", 0, 12, 0, 0,
    );
    let original = bus.save_pixel_bytes(body, len);
    assert_ne!(
        original,
        crate::memory::SavedPixels::from(original.to_vec()),
        "the body must contain retained outline coverage"
    );

    // A frame can end halfway through a CPU-drawn indexed highlight.
    // Repainting the menu used to finish that pending color transaction.
    bus.begin_cpu_drawing();
    for offset in 0..len as u32 {
        bus.write_byte(body + offset, bus.read_byte(body + offset) ^ 1);
    }
    bus.end_cpu_drawing(false);
    disp.draw_menu_bar_to_fb(&mut bus);
    let cached = bus.save_pixel_bytes(body, len);
    disp.menu_bar_cache.borrow_mut().take();
    disp.draw_menu_bar_to_fb(&mut bus);
    assert_eq!(
        cached,
        bus.save_pixel_bytes(body, len),
        "a cache hit must commit the same retained body coverage as a repaint"
    );
    assert_ne!(
        cached,
        crate::memory::SavedPixels::from(cached.to_vec()),
        "recoloring must keep the body's antialiased glyph"
    );
}

#[test]
fn cached_menu_bar_restores_outline_coverage_and_tracks_live_inputs() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 622, 0x302F00, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    let main = disp.ensure_main_gdevice(&mut bus);
    bus.write_long(0x08A4, main);
    let gdevice = bus.read_long(main);
    let pixmap = bus.read_long(bus.read_long(gdevice + 22));
    let ctab = bus.read_long(bus.read_long(pixmap + 42));
    let palette = std::array::from_fn(|i| disp.device_clut[i].map(|c| (c >> 8) as u8));
    bus.enable_outline_presentation(disp.screen_mode, palette, 4);

    for step in 0..10 {
        match step {
            1 => disp.menus[0].title = "Changed".into(),
            2 => disp.menus[0].enabled = false,
            3 => {
                disp.menus[0].enabled = true;
                bus.write_word(crate::memory::globals::addr::THE_MENU, 622);
            }
            4 => {
                bus.write_word(crate::memory::globals::addr::THE_MENU, 0);
                bus.write_word(ctab + 8 + 2, 0);
                bus.write_word(ctab + 8 + 4, 0);
                bus.write_word(ctab + 8 + 6, 0);
            }
            5 => bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 24),
            6 => disp.set_ui_theme_id(crate::ui_theme::UiThemeId::SystemlessDefault),
            7 => bus.enable_outline_presentation(disp.screen_mode, palette, 2),
            8 => {
                disp.set_ui_theme_id(crate::ui_theme::UiThemeId::ClassicSystem7);
                let entry = bus.alloc(MC_ENTRY_SIZE as u32);
                write_mc_entry_colors(
                    &mut bus,
                    entry,
                    622,
                    0,
                    (0xFFFF, 0, 0),
                    (0xFFFF, 0xFFFF, 0xFFFF),
                    (0, 0, 0),
                    (0xFFFF, 0xFFFF, 0xFFFF),
                );
                cpu.write_reg(Register::A7, TEST_SP);
                bus.write_long(TEST_SP, entry);
                bus.write_word(TEST_SP + 4, 1);
                assert!(disp
                    .dispatch_menu(true, 0x265, &mut cpu, &mut bus)
                    .unwrap()
                    .is_ok());
            }
            9 => {
                // Guest edits can bypass SetMCEntries; read live bytes
                // instead of relying on a host-side generation counter.
                let table = bus.read_long(bus.read_long(crate::memory::globals::addr::MENU_C_INFO));
                bus.write_word(table + 4, 0);
                bus.write_word(table + 6, 0xFFFF);
            }
            _ => {}
        }
        // First render may hit an old key: compare it with an explicitly
        // uncached repaint, then overwrite the band and require a replay
        // to recover both guest bytes and retained antialiasing coverage.
        disp.draw_menu_bar_to_fb(&mut bus);
        let actual = bus.read_bytes(base, (row_bytes * 24) as usize);
        let actual_outline = bus.outline_presentation_rgb().unwrap().2;
        disp.menu_bar_cache.borrow_mut().take();
        disp.draw_menu_bar_to_fb(&mut bus);
        assert_eq!(
            actual,
            bus.read_bytes(base, (row_bytes * 24) as usize),
            "step {step}"
        );
        assert_eq!(
            actual_outline,
            bus.outline_presentation_rgb().unwrap().2,
            "step {step}"
        );
        let height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as u32;
        bus.fill_bytes(base, row_bytes * height, 17);
        let glyphs = bus.outline_presentation_rgb().unwrap().3;
        disp.draw_menu_bar_to_fb(&mut bus);
        assert_eq!(
            actual,
            bus.read_bytes(base, (row_bytes * 24) as usize),
            "replay {step}"
        );
        let replay = bus.outline_presentation_rgb().unwrap();
        assert_eq!(actual_outline, replay.2, "outline replay {step}");
        assert_eq!(glyphs, replay.3, "a cache hit must not repaint glyphs");
    }
}

#[test]
fn draw_menu_bar_8bpp_uses_menucinfo_bar_and_title_colors() {
    let file_id = 622;
    let edit_id = 623;
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x302F00, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, edit_id, 0x302F40, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);

    let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, green, white, green, red);
    write_mc_entry_colors(
        &mut bus,
        entries_ptr + MC_ENTRY_SIZE as u32,
        file_id,
        0,
        blue,
        red,
        green,
        white,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 2);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install menu bar and title color entries"
    );

    disp.draw_menu_bar_to_fb(&mut bus);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 5, 5),
        35,
        "menu bar entry RGB4 full red should fill the 8bpp menu bar"
    );

    let regions = disp.menu_title_regions();
    let file_has_blue_title_pixel = (1..19).any(|y| {
        (regions[0].0..regions[0].1).any(|x| screen_pixel_index(&bus, base, row_bytes, x, y) == 210)
    });
    assert!(
        file_has_blue_title_pixel,
        "menu title entry RGB1 full blue should draw that menu title"
    );

    let edit_has_green_title_pixel = (1..19).any(|y| {
        (regions[1].0..regions[1].1).any(|x| screen_pixel_index(&bus, base, row_bytes, x, y) == 185)
    });
    assert!(
        edit_has_green_title_pixel,
        "menu bar entry RGB1 full green should draw titles without their own entry"
    );
}

#[test]
fn menu_chrome_4bpp_uses_main_device_standard_colors() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    remap_main_device_mono_indexes(&mut bus, 5, 10);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 620, 0x302E00, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302E40, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    disp.draw_menu_bar_to_fb(&mut bus);

    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 90, 5),
        5,
        "plain menu-bar background should use MainDevice white"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 90, 19),
        10,
        "menu-bar bottom rule should use MainDevice black"
    );
    let mut rounded_corner_pixels = Vec::new();
    crate::menu_manager::for_each_standard_menu_bar_corner_pixel(128, |x, y| {
        rounded_corner_pixels.push((x, y));
    });
    for y in 0..5 {
        for x in (0..6).chain(122..128) {
            assert_eq!(
                packed_4bpp_screen_pixel_index(&bus, base, row_bytes, x, y),
                if rounded_corner_pixels.contains(&(x, y)) {
                    10
                } else {
                    5
                },
                "rounded corner mask pixel ({x}, {y})",
            );
        }
    }
    let regions = disp.menu_title_regions();
    assert!((1..19).any(|y| {
        (regions[0].0..regions[0].1)
            .any(|x| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, x, y) == 10)
    }));

    let rect = (20, 21, 38, 111);
    set_packed_4bpp_screen_pixel_index(&mut bus, (base, row_bytes, 128, 96), 20, 25, 7);
    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 22, 25),
        5,
        "plain dropdown background should use MainDevice white"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 21, 25),
        10,
        "dropdown frame should use MainDevice black"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 111, 25),
        10,
        "dropdown shadow should use MainDevice black"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 20, 25),
        7,
        "drawing a low-nibble frame edge must preserve its packed neighbour"
    );
    let text_left = rect.1 + 15;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    assert!(((rect.0 + 1)..(rect.0 + 17)).any(|y| {
        (text_left..text_right)
            .any(|x| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, x, y) == 10)
    }));
}

#[test]
fn menu_chrome_8bpp_uses_main_device_standard_colors() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    remap_main_device_mono_indexes(&mut bus, 17, 23);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 621, 0x302E80, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302EC0, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    disp.draw_menu_bar_to_fb(&mut bus);

    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 90, 5), 17);
    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 90, 19), 23);
    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 0, 0), 23);

    let rect = (20, 21, 38, 111);
    disp.draw_menu_dropdown(&mut bus, 0, rect);
    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 22, 25), 17);
    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 21, 25), 23);
    assert_eq!(screen_pixel_index(&bus, base, row_bytes, 111, 25), 23);
    let text_left = rect.1 + 15;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    assert!(((rect.0 + 1)..(rect.0 + 17)).any(|y| {
        (text_left..text_right).any(|x| screen_pixel_index(&bus, base, row_bytes, x, y) == 23)
    }));
}

#[test]
fn menu_dim_4bpp_uses_main_device_midpoint_color() {
    let rect = (20, 20, 38, 110);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    remap_main_device_mono_indexes(&mut bus, 5, 10);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 621, 0x302EE0, "Dim");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302F20, "Unavailable");
    disp.menus[0].items[0].enabled = false;
    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let text_left = rect.1 + 15;
    let text_right =
        text_left + super::super::TrapDispatcher::fb_measure_string("Unavailable", 0, 12);
    assert!(
        ((rect.0 + 1)..(rect.0 + 17)).any(|y| {
            (text_left..text_right)
                .any(|x| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, x, y) == 0)
        }),
        "disabled item should use MainDevice midpoint index 0 rather than the foreign active table"
    );
}

#[test]
fn draw_menu_dropdown_1bpp_highlight_preserves_classic_row_inversion() {
    let rect = (20, 20, 38, 120);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 16;
    let base = bus.alloc(row_bytes * 96);
    disp.set_screen_mode_for_test(base, row_bytes, 128, 96, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 96);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 622, 0x302E80, "Mono");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302EC0, "Open/O");
    disp.menus[0].items[0].mark = 0x12;
    disp.menus[0].items[0].icon = 7;
    let cicn = cicn_source_with_left_stripe(16, 16);
    disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let item_top = rect.0 + 1;
    let item_bottom = item_top + 16;
    let mark_pixel = (item_top..item_bottom)
        .flat_map(|y| ((rect.1 + 3)..(rect.1 + 15)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
        .expect("precondition: monochrome mark should draw");
    let text_left = rect.1 + 34;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    let name_pixel = (item_top..item_bottom)
        .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
        .expect("precondition: monochrome item name should draw");
    let command_pixel = (item_top..item_bottom)
        .flat_map(|y| ((rect.3 - 25)..(rect.3 - 1)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
        .expect("precondition: monochrome command key should draw");
    let icon_black_pixel = (rect.1 + 20, item_top);
    let icon_white_pixel = (rect.1 + 18, item_top);
    assert!(screen_pixel_is_set(
        &bus,
        base,
        row_bytes,
        icon_black_pixel.0,
        icon_black_pixel.1
    ));
    assert!(!screen_pixel_is_set(
        &bus,
        base,
        row_bytes,
        icon_white_pixel.0,
        icon_white_pixel.1
    ));
    let outside_edge = (rect.1, item_top + 4);
    let outside_before = screen_pixel_is_set(&bus, base, row_bytes, outside_edge.0, outside_edge.1);
    let before = bus.read_bytes(base, (row_bytes * 96) as usize);

    disp.menu_tracking
        .set(Some(test_tracked_menu_state(menu, rect, 1)));
    disp.draw_menu_dropdown(&mut bus, 0, rect);

    for (component, pixel) in [
        ("mark", mark_pixel),
        ("name", name_pixel),
        ("command key", command_pixel),
        ("black cicn bit", icon_black_pixel),
    ] {
        assert!(
            !screen_pixel_is_set(&bus, base, row_bytes, pixel.0, pixel.1),
            "selected monochrome {component} should be bit-inverted"
        );
    }
    assert!(
        screen_pixel_is_set(
            &bus,
            base,
            row_bytes,
            icon_white_pixel.0,
            icon_white_pixel.1
        ),
        "selected monochrome white cicn bit should be inverted to black"
    );
    assert_eq!(
        screen_pixel_is_set(&bus, base, row_bytes, outside_edge.0, outside_edge.1,),
        outside_before,
        "non-byte-aligned selected row must preserve the border bit outside its rectangle"
    );

    disp.menu_tracking
        .with_tracking_mut(|tracking| tracking.highlighted_item = 0)
        .unwrap();
    disp.draw_menu_dropdown(&mut bus, 0, rect);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 96) as usize),
        before,
        "redrawing the unselected 1bpp row should restore every framebuffer byte"
    );
}

#[test]
fn draw_menu_dropdown_4bpp_highlight_reverses_all_item_component_colors() {
    let rect = (20, 20, 38, 110);
    let menu_id = 623;
    let (clut, _) = super::super::TrapDispatcher::standard_mac_indexed_clut(4).unwrap();
    let rgb = |index: usize| (clut[index][0], clut[index][1], clut[index][2]);
    let white = rgb(0);
    let red = rgb(3);
    let blue = rgb(6);
    let green = rgb(8);
    let black = rgb(15);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x302F00, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302F40, "Open/O");
    disp.menus[0].items[0].mark = 0x12;

    let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
    write_mc_entry_colors(
        &mut bus,
        entries_ptr + MC_ENTRY_SIZE as u32,
        menu_id,
        1,
        green,
        blue,
        black,
        red,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 2);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install dropdown color entries"
    );

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let edge_x = rect.1 + 1;
    let edge_y = rect.0 + 5;
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, edge_x, edge_y),
        3,
        "precondition: dropdown background should resolve to 4bpp red"
    );
    let packed_neighbor = packed_4bpp_screen_pixel_index(&bus, base, row_bytes, edge_x - 1, edge_y);

    let mark_pixel = ((rect.0 + 1)..(rect.0 + 17))
        .flat_map(|y| ((rect.1 + 3)..(rect.1 + 15)).map(move |x| (x, y)))
        .find(|(x, y)| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, *x, *y) == 8)
        .expect("precondition: item RGB1 green mark should draw");
    let text_left = rect.1 + 15;
    let text_top = rect.0 + 1;
    let text_bottom = text_top + 16;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    let name_pixel = (text_top..text_bottom)
        .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
        .find(|(x, y)| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, *x, *y) == 6)
        .expect("precondition: item RGB2 blue text should draw");
    let command_pixel = (text_top..text_bottom)
        .flat_map(|y| ((rect.3 - 25)..(rect.3 - 1)).map(move |x| (x, y)))
        .find(|(x, y)| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, *x, *y) == 15)
        .expect("precondition: item RGB3 black command key should draw");
    let before = bus.read_bytes(base, (row_bytes * 96) as usize);

    disp.menu_tracking
        .set(Some(test_tracked_menu_state(menu, rect, 1)));
    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, edge_x, edge_y),
        6,
        "selected 4bpp row should use item RGB2 as its background"
    );
    for (component, pixel) in [
        ("mark", mark_pixel),
        ("name", name_pixel),
        ("command key", command_pixel),
    ] {
        assert_eq!(
            packed_4bpp_screen_pixel_index(&bus, base, row_bytes, pixel.0, pixel.1),
            3,
            "selected 4bpp {component} should use item RGB4/menu background"
        );
    }
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, edge_x - 1, edge_y),
        packed_neighbor,
        "redrawing a selected odd pixel must preserve the neighbouring high nibble"
    );

    disp.menu_tracking
        .with_tracking_mut(|tracking| tracking.highlighted_item = 0)
        .unwrap();
    disp.draw_menu_dropdown(&mut bus, 0, rect);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 96) as usize),
        before,
        "redrawing the unselected 4bpp row should restore every packed framebuffer byte"
    );
}

#[test]
fn draw_menu_dropdown_8bpp_highlight_reverses_all_item_component_colors() {
    let rect = (20, 20, 38, 110);
    let menu_id = 624;
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let black = (0, 0, 0);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 96);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x303000, "Color");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303040, "Open/O");
    disp.menus[0].items[0].mark = 0x12;

    let entries_ptr = bus.alloc((2 * MC_ENTRY_SIZE) as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, black, white, green, red);
    write_mc_entry_colors(
        &mut bus,
        entries_ptr + MC_ENTRY_SIZE as u32,
        menu_id,
        1,
        green,
        blue,
        black,
        red,
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 2);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install dropdown color entries"
    );

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    let bg_x = rect.1 + 1;
    let bg_y = rect.0 + 5;
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
        35,
        "precondition: dropdown background should be the MenuCInfo red"
    );
    let mark_pixel = ((rect.0 + 1)..(rect.0 + 17))
        .flat_map(|y| ((rect.1 + 3)..(rect.1 + 15)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 185)
        .expect("precondition: item RGB1 green mark should draw");
    let text_left = rect.1 + 15;
    let text_top = rect.0 + 1;
    let text_bottom = text_top + 16;
    let text_right = text_left + super::super::TrapDispatcher::fb_measure_string("Open", 0, 12);
    let name_pixel = (text_top..text_bottom)
        .flat_map(|y| (text_left..text_right).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
        .expect("precondition: item RGB2 blue text should draw");
    let command_pixel = (text_top..text_bottom)
        .flat_map(|y| ((rect.3 - 25)..(rect.3 - 1)).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 255)
        .expect("precondition: item RGB3 black command key should draw");
    let before = bus.read_bytes(base, (row_bytes * 96) as usize);

    disp.menu_tracking
        .set(Some(test_tracked_menu_state(menu, rect, 1)));
    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
        210,
        "selected 8bpp row should use item RGB2 as its background"
    );
    for (component, pixel) in [
        ("mark", mark_pixel),
        ("name", name_pixel),
        ("command key", command_pixel),
    ] {
        assert_eq!(
            screen_pixel_index(&bus, base, row_bytes, pixel.0, pixel.1),
            35,
            "selected 8bpp {component} should use item RGB4/menu background"
        );
    }

    disp.menu_tracking
        .with_tracking_mut(|tracking| tracking.highlighted_item = 0)
        .unwrap();
    disp.draw_menu_dropdown(&mut bus, 0, rect);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 96) as usize),
        before,
        "redrawing the unselected 8bpp row should restore every framebuffer byte"
    );
}

#[test]
fn draw_menu_bar_4bpp_highlight_reverses_title_and_background_colors() {
    let file_id = 625;
    let (clut, _) = super::super::TrapDispatcher::standard_mac_indexed_clut(4).unwrap();
    let rgb = |index: usize| (clut[index][0], clut[index][1], clut[index][2]);
    let white = rgb(0);
    let red = rgb(3);
    let blue = rgb(6);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x303080, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, blue, white, blue, red);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 1);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install menu bar color entry"
    );

    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let classic_left = regions[0].0 - 2;
    assert_eq!(
        classic_left & 1,
        1,
        "test edge should occupy the low nibble"
    );
    let edge_y = 5;
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, classic_left, edge_y),
        3,
        "precondition: menu bar should use its 4bpp red background"
    );
    let packed_neighbor =
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, classic_left - 1, edge_y);

    let unrelated_x = regions[0].1 + 1;
    set_packed_4bpp_screen_pixel_index(
        &mut bus,
        (base, row_bytes, 128, 64),
        unrelated_x,
        edge_y,
        8,
    );
    let blue_title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, *x, *y) == 6)
        .expect("precondition: menu-bar RGB1 blue title should draw");
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, classic_left, edge_y),
        6,
        "4bpp menu-title highlighting should replace RGB2 background with RGB1 title color"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, unrelated_x, edge_y),
        8,
        "4bpp menu-title highlighting should preserve unrelated indexed colors"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(
            &bus,
            base,
            row_bytes,
            blue_title_pixel.0,
            blue_title_pixel.1,
        ),
        3,
        "4bpp menu-title highlighting should replace RGB1 title pixels with RGB2 background"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, classic_left - 1, edge_y),
        packed_neighbor,
        "highlighting a low nibble must preserve its neighbour outside the title rect"
    );

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "highlighting the same packed title twice should restore every framebuffer byte"
    );
}

#[test]
fn draw_menu_bar_8bpp_highlight_reverses_title_and_background_colors() {
    let file_id = 625;
    let red = (0xFFFF, 0, 0);
    let blue = (0, 0, 0xFFFF);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x303100, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, blue, white, blue, red);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 1);
    assert!(
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMCEntries should install menu bar color entry"
    );

    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let bg_x = regions[0].0 + 1;
    let bg_y = 5;
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
        35,
        "precondition: menu title background should be the MenuCInfo red bar"
    );
    let unrelated_x = regions[0].1 - 2;
    let unrelated_y = 5;
    bus.write_byte(
        base + (unrelated_y as u32) * row_bytes + unrelated_x as u32,
        185,
    );
    let blue_title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
        .expect("precondition: menu-bar RGB1 blue title should draw");
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, bg_x, bg_y),
        210,
        "8bpp menu-title highlighting should replace RGB2 background with RGB1 title color"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, unrelated_x, unrelated_y),
        185,
        "8bpp menu-title highlighting should preserve unrelated indexed colors"
    );
    assert_eq!(
        screen_pixel_index(
            &bus,
            base,
            row_bytes,
            blue_title_pixel.0,
            blue_title_pixel.1
        ),
        35,
        "8bpp menu-title highlighting should replace RGB1 title pixels with RGB2 background"
    );

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "highlighting the same 8bpp title twice should restore every framebuffer byte"
    );
}

#[test]
fn draw_menu_bar_plain_8bpp_highlight_swaps_logical_black_white() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 626, 0x303200, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let classic_left = regions[0].0 - 2;
    let right = regions[0].1;
    let white = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
    let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, classic_left, 1),
        white,
        "precondition: plain menu-title background should be logical white"
    );
    let title_pixel = (1..19)
        .flat_map(|y| (classic_left..right).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == black)
        .expect("precondition: plain title should draw logical black text");

    disp.highlight_menu_title(&mut bus, 0);

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, classic_left, 1),
        black,
        "plain 8bpp menu-title highlight should invert white background to black"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        white,
        "plain 8bpp menu-title highlight should invert black title text to white"
    );
}

#[test]
fn open_menu_dropdown_uses_system7_attached_pulldown_chrome() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 96);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 627, 0x303300, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        file,
        0x303340,
        "New/N;Open/O;Close/W",
    );
    disp.menus[0].items[1].mark = 0x12;
    disp.menus[0].items[2].style = 0x01;
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let expected_left = regions[0].0;
    let expected_width = disp
        .standard_menu_width(&bus, &disp.menus[0].items)
        .max(regions[0].1 - regions[0].0 + 20);
    let expected_rect = (
        20,
        expected_left,
        20 + disp.menu_items_height(&bus, &disp.menus[0].items),
        expected_left + expected_width,
    );
    let white = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0xFFFF; 3]).unwrap();
    let black = super::super::TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0; 3]).unwrap();

    disp.open_menu_dropdown(&mut bus, 0);

    assert_eq!(
        disp.menu_tracking.as_ref().unwrap().dropdown_rect(),
        expected_rect,
        "attached pull-down display rect should use the shared standard MDEF width"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, expected_left + 1, expected_rect.0),
        white,
        "attached pull-down should not draw a separate top horizontal border"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, expected_left, expected_rect.0),
        black,
        "attached pull-down should still draw its left vertical border"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, expected_rect.3 - 1, expected_rect.0),
        black,
        "attached pull-down should still draw its right vertical border"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, expected_left, 1),
        black,
        "opening a pull-down should highlight the attached menu title"
    );
}

#[test]
fn open_menu_dropdown_uses_system7_plain_item_padding() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (_base, _row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 180, 120);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let style = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 628, 0x303500, "Style");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        style,
        0x303540,
        "Plain;Underline;-;Wide Underline",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, style);
    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let expected_left = regions[0].0;
    let expected_width = disp
        .standard_menu_width(&bus, &disp.menus[0].items)
        .max(regions[0].1 - regions[0].0 + 20);
    let wide_item_width =
        super::super::TrapDispatcher::fb_measure_string("Wide Underline", 0, 12) + 32;

    disp.open_menu_dropdown(&mut bus, 0);

    assert_eq!(
        expected_width, wide_item_width,
        "plain no-mark/no-command rows should use the Mac OS 8.1 standard width"
    );
    assert_eq!(
        disp.menu_tracking.as_ref().unwrap().dropdown_rect(),
        (
            20,
            expected_left,
            20 + disp.menu_items_height(&bus, &disp.menus[0].items),
            expected_left + expected_width,
        ),
        "live pull-down geometry should match the System 7 plain-row width reference"
    );
}

#[test]
fn script_code_item_does_not_reserve_icon_geometry() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 629, 0x303700, "Script");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x303740, "Localized");
    disp.menus[0].items[0].icon = 7;
    disp.menus[0].items[0].key_equiv = 0x1C;
    let cicn = cicn_source_with_left_stripe(24, 20);
    disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);

    assert_eq!(
        disp.menu_item_icon_width(&bus, &disp.menus[0].items[0]),
        0,
        "$1C makes the icon byte a script code even when a matching cicn exists",
    );
    assert_eq!(disp.menu_item_height(&bus, &disp.menus[0].items[0]), 16);
    assert_eq!(
        disp.cicn_menu_icon_resource_ptr(&disp.menus[0].items[0]),
        None,
    );
}

#[test]
fn open_menu_dropdown_uses_system7_icon_column_geometry_without_icon_resources() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (_base, _row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 240, 220);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 629, 0x303700, "Icons");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x303740,
        "Folder View/F;Document/D;Checked Icon/C;Disabled Icon/X;(-;No Icon/N",
    );
    for (idx, item) in disp.menus[0].items.iter_mut().take(4).enumerate() {
        item.icon = (idx + 1) as u8;
    }
    disp.menus[0].items[2].mark = 0x12;
    disp.menus[0].items[3].enabled = false;
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);
    disp.draw_menu_bar_to_fb(&mut bus);

    let regions = disp.menu_title_regions();
    let expected_left = regions[0].0;
    let expected_height = 34 + 34 + 34 + 34 + super::STANDARD_MENU_SEPARATOR_HEIGHT + 16;
    let expected_width = disp
        .standard_menu_width(&bus, &disp.menus[0].items)
        .max(regions[0].1 - regions[0].0 + 20);

    disp.open_menu_dropdown(&mut bus, 0);
    let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();

    assert_eq!(
        disp.menu_items_height(&bus, &disp.menus[0].items),
        expected_height,
        "icon-column menus should use 34px normal ICON rows and a standard separator row"
    );
    assert_eq!(
            rect,
            (
                20,
                expected_left,
                20 + expected_height,
                expected_left + expected_width,
            ),
            "normal icon-number rows without resources should still reserve System 7 icon-column pull-down geometry"
        );
    assert_eq!(
        disp.menu_item_icon_width(&bus, &disp.menus[0].items[0]),
        32,
        "normal icon-number rows should reserve a 32-pixel icon slot"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 33),
        1,
        "hit testing should include the full first 34px normal ICON row"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 128),
        0,
        "hit testing inside the fourth 34px normal ICON row should respect its disabled state"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 137),
        0,
        "hit testing in the separator row should not select an item"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 141),
        0,
        "hit testing should leave the full separator row unselectable"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 142),
        6,
        "hit testing below the icon-column separator should reach the following item"
    );
}

#[test]
fn draw_menu_dropdown_reduced_icon_resource_draws_app_icon_not_theme_placeholder() {
    let rect = (20, 20, 38, 140);
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 612, 0x302500, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302540, "Pick");
    disp.menus[0].items[0].icon = 7;
    disp.menus[0].items[0].key_equiv = MENU_KEY_REDUCED_ICON;
    let icon = menu_icon_source_with_left_stripe();
    disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);

    disp.draw_menu_dropdown(&mut bus, 0, rect);

    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 23, 22),
        "menu item should draw the app's reduced ICON resource in the icon slot"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 36, 28),
        "an explicit ICON resource should suppress systemless icon placeholder chrome"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 124, 28),
        "$1D reduced-icon selector should not be rendered as command-key chrome"
    );
}

#[test]
fn sicn_menu_icon_resource_draws_small_icon_without_row_growth_or_theme_placeholder() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 615, 0x302800, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302840, "Pick;Next");
    disp.menus[0].items[0].icon = 7;
    disp.menus[0].items[0].key_equiv = MENU_KEY_SMALL_ICON;
    let sicn = sicn_source_with_left_stripe();
    disp.install_test_resource(&mut bus, *b"SICN", 263, &sicn);
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    disp.open_menu_dropdown(&mut bus, 0);
    let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();

    assert_eq!(
        rect.2 - rect.0,
        16 + 16,
        "SICN menu items should keep standard 16px attached pull-down geometry"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
        "SICN resource should draw the app's small icon in the icon slot"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 16, rect.0 + 8),
        "an explicit SICN resource should suppress systemless icon placeholder chrome"
    );
    // Command-key chrome is right-aligned; sample the SICN item's
    // command zone near the right edge (re-baselined from the old
    // left+8 sample, which landed on the second item's menu text and
    // became a false tripwire after a system-font glyph
    // redraw — glyph appearance changed, menu logic did not).
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 8, rect.2 - 8),
        "$1E SICN selector should not be rendered as command-key chrome"
    );
}

#[test]
fn cicn_menu_icon_resource_precedes_sicn_and_suppresses_theme_placeholder() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_ui_theme_id(UiThemeId::SystemlessDefault);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 616, 0x302900, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302940, "Pick;Next");
    disp.menus[0].items[0].icon = 7;
    disp.menus[0].items[0].key_equiv = MENU_KEY_SMALL_ICON;
    let cicn = cicn_source_with_left_stripe(24, 20);
    let sicn = sicn_source_with_right_stripe();
    disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);
    disp.install_test_resource(&mut bus, *b"SICN", 263, &sicn);
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    disp.open_menu_dropdown(&mut bus, 0);
    let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();

    assert_eq!(
        rect.2 - rect.0,
        20 + 16,
        "cicn menu rows should grow to the cicn resource rectangle height in an attached pull-down"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
        "cicn resource should draw before the SICN fallback"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 17, rect.0 + 2),
        "SICN fallback pixels must not draw when a cicn resource exists"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 16, rect.0 + 8),
        "an explicit cicn resource should suppress systemless icon placeholder chrome"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 19),
        1,
        "hit testing should include the full cicn-height first row"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 20),
        2,
        "hit testing below the cicn-height row should reach the next item"
    );
}

#[test]
fn calcmenusize_uses_cicn_geometry_instead_of_icon_fallback_geometry() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 617, 0x302A00, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302A40, "Pick;Next");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 7);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, menu);
    assert!(
        disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemIcon should install a normal icon menu item"
    );

    let cicn = cicn_source_with_left_stripe(24, 20);
    let icon = menu_icon_source_with_left_stripe();
    disp.install_test_resource(&mut bus, *b"cicn", 263, &cicn);
    disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);

    let menu_ptr = bus.read_long(menu);
    bus.write_word(menu_ptr + 4, 0);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, menu);
    assert!(
        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "CalcMenuSize should succeed"
    );

    assert_eq!(
        bus.read_word(menu_ptr + 4) as i16,
        20 + 16,
        "CalcMenuSize should use cicn resource height instead of normal ICON's 32px row"
    );
}

#[test]
fn normal_icon_resource_expands_dropdown_geometry_and_hit_testing() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 613, 0x302600, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302640, "Pick;Next");
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 7);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, menu);
    assert!(
        disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemIcon should install a normal ICON menu item"
    );
    let icon = menu_icon_source_with_left_stripe();
    disp.install_test_resource(&mut bus, *b"ICON", 263, &icon);
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    disp.open_menu_dropdown(&mut bus, 0);
    let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();

    assert_eq!(
            rect.2 - rect.0,
            34 + 16,
            "normal ICON row should enlarge the live attached pull-down height around its 32px icon slot"
        );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 1),
        "normal ICON resource should draw at full 32x32 size in the first row"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 33),
        1,
        "hit testing near the bottom of the 34px normal ICON row should still hit item 1"
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, rect.1 + 5, rect.0 + 34),
        2,
        "hit testing below the normal ICON row should hit the following item"
    );
}

#[test]
fn retained_selection_uses_guest_menu_handle_after_cache_reordering() {
    let (mut disp, mut cpu, mut bus) = setup();
    let first = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 701, 0x302680, "First");
    append_menu_data(&mut disp, &mut cpu, &mut bus, first, 0x3026C0, "Wrong");
    let retained = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 702, 0x302700, "Retained");
    append_menu_data(&mut disp, &mut cpu, &mut bus, retained, 0x302740, "Right");

    disp.menu_tracking.set(Some(test_tracked_menu_state(
        retained,
        (20, 10, 38, 100),
        1,
    )));
    disp.menus.swap(0, 1);

    assert_eq!(
        disp.menu_tracking_selection_result(&bus),
        (702u32 << 16) | 1,
        "retained MenuSelect identity must survive presentation-cache reordering"
    );
}

#[test]
fn calcmenusize_accounts_for_normal_icon_row_height() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 614, 0x302700, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x302740, "Pick;Next");

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 7);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, menu);
    assert!(
        disp.dispatch_menu(true, 0x140, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetItemIcon should install a normal ICON menu item"
    );

    let menu_ptr = bus.read_long(menu);
    bus.write_word(menu_ptr + 4, 0);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, menu);
    assert!(
        disp.dispatch_menu(true, 0x148, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "CalcMenuSize should succeed"
    );

    assert_eq!(
        bus.read_word(menu_ptr + 4) as i16,
        34 + 16,
        "CalcMenuSize should write summed row heights including normal ICON rows"
    );
}

#[test]
fn popupmenuselect_nohit_path_preserves_stack_and_returns_zero() {
    // Inside Macintosh Volume V (1986), p. V-229:
    // PopUpMenuSelect(menu, top, left, popUpItem) returns the
    // selected item. IM:V V-241 and MTE 1992 p. 3-120 require the
    // pop-up menu to be in the MenuList; an uninserted menu returns
    // no selection without disturbing the caller stack.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    disp.input_state.set_mouse_button_for_test(false);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x302000, "Pop");

    let sp = TEST_SP;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0); // popUpItem
    bus.write_word(sp + 2, 0xFC18); // -1000
    bus.write_word(sp + 4, 0xFC18); // -1000
    bus.write_long(sp + 6, menu);
    bus.write_long(sp + 10, 0xDEAD_BEEF); // result placeholder

    let result = disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus);
    assert!(result.is_some(), "PopUpMenuSelect should be handled");
    assert!(result.unwrap().is_ok(), "PopUpMenuSelect should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp + 10,
        "PopUpMenuSelect should consume the 10-byte Pascal argument frame"
    );
    assert_eq!(
        bus.read_long(sp + 10),
        0,
        "PopUpMenuSelect no-hit path should return 0"
    );
    assert!(
        disp.menu_tracking.is_none(),
        "uninserted pop-up menus should not seed tracking"
    );
}

#[test]
fn popupmenuselect_with_mouse_button_up_returns_zero_immediately() {
    // IM: Toolbox Essentials 1992, p. 3-120:
    // "If the user releases the mouse button without choosing an item,
    // or if the mouse button was not down when PopUpMenuSelect was called,
    // PopUpMenuSelect returns 0."
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    disp.input_state.set_mouse_button_for_test(false);
    bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80); // button up
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 402, 0x302000, "Pop");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x302040,
        "First;Second;Third",
    );
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, menu, -1);

    let sp = TEST_SP;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // popUpItem
    bus.write_word(sp + 2, 100); // left
    bus.write_word(sp + 4, 50); // top
    bus.write_long(sp + 6, menu);
    bus.write_long(sp + 10, 0xDEAD_BEEF); // result placeholder

    let result = disp.step_menu_fixture(true, 0x00B, &mut cpu, &mut bus);
    assert!(result.is_some(), "PopUpMenuSelect should be handled");
    assert!(result.unwrap().is_ok(), "PopUpMenuSelect should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp + 10,
        "PopUpMenuSelect should consume the 10-byte argument frame"
    );
    assert_eq!(
        bus.read_long(sp + 10),
        0,
        "PopUpMenuSelect must return 0 when called with mouse button up"
    );
    assert!(
        disp.menu_tracking.is_none(),
        "PopUpMenuSelect must not start menu tracking when button is up"
    );
}

// IM:I I-354: DeleteMenu removes only the specified menu ID from the
// current menu list and leaves other inserted menus present.
#[test]
fn deletemenu_removes_only_target_menu_id_from_current_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let left_id = 240i16;
    let right_id = 241i16;
    let left = new_menu_with_title(&mut disp, &mut cpu, &mut bus, left_id, 0x30B300, "Left");
    let right = new_menu_with_title(&mut disp, &mut cpu, &mut bus, right_id, 0x30B400, "Right");
    insert_menu(&mut disp, &mut cpu, &mut bus, left);
    insert_menu(&mut disp, &mut cpu, &mut bus, right);

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, left_id),
        left
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, right_id),
        right
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, left_id);

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, left_id),
        0,
        "Deleted menu ID should no longer be in the current menu list"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, right_id),
        right,
        "Deleting one menu ID should not remove other inserted menus"
    );
}

// IM:V 1986 p. V-244: DeleteMenu removes all color entries for the
// deleted menu ID from the application's menu color information table.
#[test]
fn deletemenu_removes_all_menu_color_entries_for_menu_id() {
    let (mut disp, mut cpu, mut bus) = setup();
    let target_id = 511i16;
    let other_id = 512i16;
    let target = new_menu_with_title(&mut disp, &mut cpu, &mut bus, target_id, 0x30B4C0, "Target");
    let other = new_menu_with_title(&mut disp, &mut cpu, &mut bus, other_id, 0x30B500, "Other");
    insert_menu(&mut disp, &mut cpu, &mut bus, target);
    insert_menu(&mut disp, &mut cpu, &mut bus, other);
    set_mc_entries_for_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        &[
            (target_id, 0, 0x2100),
            (target_id, 1, 0x2200),
            (target_id, 2, 0x2300),
            (other_id, 1, 0x2400),
            (0, 0, 0x2500),
        ],
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, target_id);

    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 0),
        0,
        "DeleteMenu should remove the deleted menu title entry"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 1),
        0,
        "DeleteMenu should remove item entries for the deleted menu"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, target_id, 2),
        0,
        "DeleteMenu should remove every entry for the deleted menu ID"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, other_id, 1),
        0,
        "DeleteMenu should preserve other menus' MenuCInfo entries"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
        0,
        "DeleteMenu should preserve the default MenuCInfo entry"
    );
}

// MTE 1992 p. 3-105: DeleteMenu removes from the menu list but does not
// dispose the menu record memory (DisposeMenu is a separate call).
#[test]
fn deletemenu_does_not_dispose_menu_record_memory() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 242i16;
    let title = "Temp";
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B500, title);
    let menu_ptr = bus.read_long(handle);
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, menu_id);

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
        0
    );
    assert_eq!(
        bus.read_long(handle),
        menu_ptr,
        "DeleteMenu should not clear or retarget the caller-held MenuHandle"
    );
    assert_eq!(
        bus.read_word(menu_ptr) as i16,
        menu_id,
        "MenuInfo memory should still be readable after DeleteMenu"
    );
    assert_eq!(bus.read_byte(menu_ptr + 14), title.len() as u8);
}

// MTE 1992 p. 3-140: callers remove a menu from the current list before
// DisposeMenu releases a NewMenu-allocated record and handle.
#[test]
fn disposemenu_releases_newmenu_menuhandle_and_record_allocations() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu_id = 244i16;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B800, "Temp");
    let menu_ptr = bus.read_long(handle);
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
        handle,
        "precondition: inserted menu should be discoverable by GetMHandle"
    );
    assert!(
        bus.get_alloc_size(handle).is_some(),
        "precondition: NewMenu should allocate a MenuHandle master pointer"
    );
    assert!(
        bus.get_alloc_size(menu_ptr).is_some(),
        "precondition: NewMenu should allocate a menu record block"
    );

    delete_menu_by_id(&mut disp, &mut cpu, &mut bus, menu_id);
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, menu_id),
        0,
        "DeleteMenu should remove the handle before disposal"
    );
    assert!(
        bus.get_alloc_size(menu_ptr).is_some(),
        "DeleteMenu must leave the menu record allocated"
    );
    dispose_menu_by_handle(&mut disp, &mut cpu, &mut bus, handle);

    assert_eq!(
        bus.get_alloc_size(handle),
        None,
        "DisposeMenu should free the MenuHandle master-pointer allocation"
    );
    assert_eq!(
        bus.get_alloc_size(menu_ptr),
        None,
        "DisposeMenu should free the menu record allocation"
    );
}

// IM:I p. I-352: PROCEDURE DisposeMenu(theMenu: MenuHandle) takes one
// 4-byte argument.
#[test]
fn disposemenu_consumes_menuhandle_argument() {
    let (mut disp, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0);

    let result = disp.dispatch_menu(true, 0x132, &mut cpu, &mut bus);
    assert!(result.is_some(), "DisposeMenu should be handled");
    assert!(result.unwrap().is_ok(), "DisposeMenu should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 4,
        "DisposeMenu should pop one MenuHandle argument (4 bytes)"
    );
}

// IM:I I-356: HiliteMenu(menuID) takes one INTEGER argument; callers pass
// 0 to unhighlight menu titles. In HLE this closes active menu tracking.
#[test]
fn hilitemenu_pops_menuid_word_and_clears_active_tracking_state() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu_id = 243i16;
    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x30B600, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30B700,
        "Open/O;Close/W",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    disp.open_menu_dropdown(&mut bus, 0);
    assert!(
        disp.menu_tracking.is_some(),
        "precondition: dropdown tracking should be active"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    let result = disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus);
    assert!(result.is_some(), "HiliteMenu should be handled");
    assert!(result.unwrap().is_ok(), "HiliteMenu should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 2,
        "HiliteMenu should pop one menuID word"
    );
    assert!(
        disp.menu_tracking.is_none(),
        "HiliteMenu(0) should clear active menu tracking/highlight state"
    );
}

#[test]
fn hilitemenu_retains_one_live_title_and_drawmenubar_replays_it() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 160, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 243, 0x30B800, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x30B840, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    let child = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 244, 0x30B880, "Recent");
    insert_menu_before(&mut disp, &mut cpu, &mut bus, child, -1);
    disp.draw_menu_bar_to_fb(&mut bus);
    let normal = bus.read_bytes(base, (row_bytes * 64) as usize);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 243);
    disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(crate::memory::globals::addr::THE_MENU), 243);
    let highlighted = bus.read_bytes(base, (row_bytes * 64) as usize);
    assert_ne!(highlighted, normal, "HiliteMenu must highlight its title");

    disp.draw_menu_bar_to_fb(&mut bus);
    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        highlighted,
        "DrawMenuBar must replay the title retained in TheMenu"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 244);
    disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::THE_MENU),
        0,
        "a submenu ID has no title for HiliteMenu to highlight"
    );
    assert_eq!(bus.read_bytes(base, (row_bytes * 64) as usize), normal);
}

// Five HiliteMenu(0) dispatches in sequence preserve A7 cumulatively.
// Per IM:I I-356 and MPW Universal Headers Menus.h, HiliteMenu is a
// Tool-bit Pascal PROCEDURE that pops 2 bytes per call and writes no
// result slot, so five calls must advance A7 by exactly 10 bytes
// (5 × 2 bytes).
#[test]
fn hilitemenu_five_call_composition_advances_stack_by_ten_bytes() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    // Pre-push five 2-byte menuID=0 arguments.
    for i in 0..5u32 {
        bus.write_word(sp_before.wrapping_sub((i + 1) * 2), 0);
    }
    cpu.write_reg(Register::A7, sp_before.wrapping_sub(10));

    for _ in 0..5 {
        let result = disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus);
        assert!(result.is_some(), "HiliteMenu should be handled");
        assert!(result.unwrap().is_ok(), "HiliteMenu should succeed");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5 HiliteMenu(0) calls should pop 5×2 = 10 bytes cumulatively"
    );
}

// Five FlashMenuBar(0) dispatches in sequence preserve A7 cumulatively.
// Per IM:I I-361 and MPW Universal Headers Menus.h, FlashMenuBar is a
// Tool-bit Pascal PROCEDURE that pops 2 bytes per call and writes no
// result slot, so five calls must advance A7 by exactly 10 bytes
// (5 × 2 bytes).
#[test]
fn flashmenubar_five_call_composition_advances_stack_by_ten_bytes() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    for i in 0..5u32 {
        bus.write_word(sp_before.wrapping_sub((i + 1) * 2), 0);
    }
    cpu.write_reg(Register::A7, sp_before.wrapping_sub(10));

    for _ in 0..5 {
        let result = disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus);
        assert!(result.is_some(), "FlashMenuBar should be handled");
        assert!(result.unwrap().is_ok(), "FlashMenuBar should succeed");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5 FlashMenuBar(0) calls should pop 5×2 = 10 bytes cumulatively"
    );
}

#[test]
fn flashmenubar_zero_inverts_and_restores_top_menu_bar_strip() {
    // IM:I I-361: FlashMenuBar(0) inverts the entire menu bar; calling it
    // again blinks the menu bar back.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    disp.menu_bar_hidden = false;
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 701, 0x306000, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_is_set(&bus, base, row_bytes, *x, *y))
        .expect("precondition: menu title should draw before flashing");
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
        "precondition: menu-bar background should start white"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
        "precondition: DrawMenuBar should stamp the top corner mask"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
        "FlashMenuBar(0) should invert the menu-bar background"
    );
    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        "FlashMenuBar(0) should invert title glyph pixels too"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
        "FlashMenuBar(0) should preserve the top corner mask"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert!(
        !screen_pixel_is_set(&bus, base, row_bytes, 200, 5),
        "a second FlashMenuBar(0) call should restore the background"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        "a second FlashMenuBar(0) call should restore title glyph pixels"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, 0, 0),
        "a second FlashMenuBar(0) call should keep the top corner mask"
    );
}

#[test]
fn flashmenubar_zero_4bpp_swaps_bar_and_default_title_colors() {
    // IM:V V-252 to V-253: the standard MBDF flashes a colour menu bar
    // by reversing its bar-background and default-title colours. Packed
    // 4bpp pixels must be transformed independently.
    let (clut, _) = super::super::TrapDispatcher::standard_mac_indexed_clut(4).unwrap();
    let rgb = |index: usize| (clut[index][0], clut[index][1], clut[index][2]);
    let white = rgb(0);
    let red = rgb(3);
    let blue = rgb(6);
    let green = rgb(8);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    let offscreen_gdevice = make_8bpp_current_gdevice(&mut bus);
    bus.write_long(0x0CC8, offscreen_gdevice); // TheGDevice
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 702, 0x306080, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, blue, white, green, red);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 1);
    disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| packed_4bpp_screen_pixel_index(&bus, base, row_bytes, *x, *y) == 6)
        .expect("precondition: default RGB1 blue title should draw before flashing");
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 90, 5),
        3,
        "precondition: menu-bar RGB4 red should fill the packed bar"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 0, 0),
        15,
        "precondition: DrawMenuBar should stamp the packed top corner mask"
    );
    set_packed_4bpp_screen_pixel_index(&mut bus, (base, row_bytes, 128, 64), 100, 5, 8);
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 90, 5),
        6,
        "FlashMenuBar(0) should replace RGB4 background with default RGB1 title color"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, title_pixel.0, title_pixel.1,),
        3,
        "FlashMenuBar(0) should replace default RGB1 title pixels with RGB4 background"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 100, 5),
        8,
        "FlashMenuBar(0) should preserve unrelated packed color indices"
    );
    assert_eq!(
        packed_4bpp_screen_pixel_index(&bus, base, row_bytes, 0, 0),
        15,
        "FlashMenuBar(0) should preserve the packed top corner mask"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "a second packed FlashMenuBar(0) call should restore every framebuffer byte"
    );
}

#[test]
fn flashmenubar_zero_8bpp_swaps_bar_and_default_title_colors() {
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 703, 0x306100, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, 0, 0, blue, white, green, red);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 1);
    disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
        .expect("precondition: default RGB1 blue title should draw before flashing");
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 90, 5),
        35,
        "precondition: menu-bar RGB4 red should fill the 8bpp bar"
    );
    bus.write_byte(base + 5 * row_bytes + 100, 185);
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 90, 5),
        210,
        "FlashMenuBar(0) should replace RGB4 background with default RGB1 title color"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        35,
        "FlashMenuBar(0) should replace default RGB1 title pixels with RGB4 background"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 100, 5),
        185,
        "FlashMenuBar(0) should preserve unrelated indexed colors"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        bus.read_bytes(base, (row_bytes * 64) as usize),
        before,
        "a second 8bpp FlashMenuBar(0) call should restore every framebuffer byte"
    );
}

#[test]
fn flashmenubar_zero_ignores_title_only_background_colors() {
    let red = (0xFFFF, 0, 0);
    let green = (0, 0xFFFF, 0);
    let blue = (0, 0, 0xFFFF);
    let white = (0xFFFF, 0xFFFF, 0xFFFF);

    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let (base, row_bytes) = setup_8bpp_menu_screen(&mut disp, &mut bus, 128, 64);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let menu_id = 704;
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, menu_id, 0x306180, "File");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let entries_ptr = bus.alloc(MC_ENTRY_SIZE as u32);
    write_mc_entry_colors(&mut bus, entries_ptr, menu_id, 0, blue, red, green, white);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, entries_ptr);
    bus.write_word(TEST_SP + 4, 1);
    disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let title_pixel = (1..19)
        .flat_map(|y| (regions[0].0..regions[0].1).map(move |x| (x, y)))
        .find(|(x, y)| screen_pixel_index(&bus, base, row_bytes, *x, *y) == 210)
        .expect("precondition: title-only RGB1 blue should draw");
    let title_bg_pixel = (regions[0].0 - 2, 5);
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 90, 5),
        0,
        "without MCEntry(0,0), StandardMBDF should keep the full bar standard white"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, title_bg_pixel.0, title_bg_pixel.1,),
        35,
        "StandardMBDF should erase the individual title cell to its RGB2 color"
    );
    let before = bus.read_bytes(base, (row_bytes * 64) as usize);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, 90, 5),
        255,
        "FlashMenuBar(0) should use standard black when MCEntry(0,0) is absent"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, title_pixel.0, title_pixel.1),
        210,
        "whole-bar flash should preserve a per-title RGB1 color"
    );
    assert_eq!(
        screen_pixel_index(&bus, base, row_bytes, title_bg_pixel.0, title_bg_pixel.1,),
        35,
        "whole-bar flash should preserve the per-title RGB2 background"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_bytes(base, (row_bytes * 64) as usize), before);
}

// Five MenuChoice() Pascal FUNCTION dispatches in sequence preserve
// A7 cumulatively. Per IM:MTb 1992 p. 3-118 and MPW Universal Headers
// Menus.h, MenuChoice is a parameterless Tool-bit Pascal FUNCTION
// returning LongInt — the caller pre-pushes a 4-byte result slot,
// the trap writes [SP+0] without modifying A7, and the caller pops
// the slot. Wrapping each dispatch in a manual pre-push/post-pop
// pair checks that the trap itself leaves A7 unchanged across
// five successive calls.
#[test]
fn menuchoice_pascal_function_preserves_stack_across_five_calls() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    for _ in 0..5 {
        let slot = cpu.read_reg(Register::A7).wrapping_sub(4);
        bus.write_long(slot, 0xDEADBEEF);
        cpu.write_reg(Register::A7, slot);

        let result = disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus);
        assert!(result.is_some(), "MenuChoice should be handled");
        assert!(result.unwrap().is_ok(), "MenuChoice should succeed");

        assert_eq!(
            cpu.read_reg(Register::A7),
            slot,
            "MenuChoice must leave A7 unchanged across the trap dispatch"
        );

        let _result_val = bus.read_long(slot);
        cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5 MenuChoice() FUNCTION call sequences must net-balance A7"
    );
}

// Exercises the MenuChoice lowmem read path by seeding lowmem
// MenuDisable directly. Per IM:MTb 1992 p. 3-118..3-119,
// MenuChoice returns the packed (menuID, itemNumber) stored in
// MenuDisable after MenuSelect tracks a disabled item. This test seeds
// the lowmem word and checks that
// the trap writes the same LongInt into the result slot while
// still preserving A7 across the Pascal FUNCTION call sequence.
#[test]
fn menuchoice_reads_menu_disable_lowmem_value() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let expected = 0x11223344u32;
    let slot = sp_before.wrapping_sub(4);

    bus.write_long(crate::memory::globals::addr::MENU_DISABLE, expected);
    bus.write_long(slot, 0xDEADBEEF);
    cpu.write_reg(Register::A7, slot);

    let result = disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus);
    assert!(result.is_some(), "MenuChoice should be handled");
    assert!(result.unwrap().is_ok(), "MenuChoice should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        slot,
        "MenuChoice must leave A7 unchanged across the trap dispatch"
    );
    assert_eq!(
        bus.read_long(slot),
        expected,
        "MenuChoice should return the seeded MenuDisable value"
    );
    cpu.write_reg(Register::A7, slot.wrapping_add(4));
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "MenuChoice should net-balance the Pascal FUNCTION call sequence"
    );
}

// Menu Color Manager family — per IM:V 1986 pp. V-247..V-248 the
// family is six Tool-bit Pascal routines with the following stack
// disciplines:
//   AA60 DelMCEntries  — PROCEDURE pop-4 (2xINTEGER)
//   AA61 GetMCInfo     — FUNCTION  parameterless + 4-byte result slot
//   AA62 SetMCInfo     — PROCEDURE pop-4 (1xHandle)
//   AA63 DispMCInfo    — PROCEDURE pop-4 (1xHandle)
//   AA64 GetMCEntry    — FUNCTION  2xINTEGER + 4-byte result slot
//   AA65 SetMCEntries  — PROCEDURE pop-6 (1xINTEGER + 1xPtr)
// This test dispatches 5 successive calls of each trap with the
// appropriate pre-pushed Pascal arg frame and (for FUNCTIONs) result
// slot, asserting cumulative A7 net-balance across each family.
#[test]
fn menu_color_family_five_call_compositions_preserve_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();

    // AA60 DelMCEntries — PROCEDURE pop-4 (2xINTEGER each call).
    // 5 calls × 4 bytes = 20 bytes consumed total.
    let sp0 = cpu.read_reg(Register::A7);
    for i in 0..5u32 {
        bus.write_long(sp0.wrapping_sub((i + 1) * 4), 0);
    }
    cpu.write_reg(Register::A7, sp0.wrapping_sub(20));
    for _ in 0..5 {
        disp.dispatch_menu(true, 0x260, &mut cpu, &mut bus)
            .expect("DelMCEntries handled")
            .expect("DelMCEntries Ok");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp0,
        "5 DelMCEntries calls should pop 5×4 = 20 bytes cumulatively"
    );

    // AA61 GetMCInfo — FUNCTION parameterless + 4-byte result slot.
    // 5 calls, each with manual pre-push/post-pop of the slot.
    let sp1 = cpu.read_reg(Register::A7);
    for _ in 0..5 {
        let slot = cpu.read_reg(Register::A7).wrapping_sub(4);
        bus.write_long(slot, 0xDEADBEEF);
        cpu.write_reg(Register::A7, slot);
        disp.dispatch_menu(true, 0x261, &mut cpu, &mut bus)
            .expect("GetMCInfo handled")
            .expect("GetMCInfo Ok");
        assert_eq!(
            cpu.read_reg(Register::A7),
            slot,
            "GetMCInfo must leave A7 unchanged across the trap dispatch"
        );
        cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp1,
        "5 GetMCInfo() FUNCTION call sequences must net-balance A7"
    );

    // AA62 SetMCInfo — PROCEDURE pop-4 (1xHandle). 5 × 4 = 20 bytes.
    let sp2 = cpu.read_reg(Register::A7);
    for i in 0..5u32 {
        bus.write_long(sp2.wrapping_sub((i + 1) * 4), 0);
    }
    cpu.write_reg(Register::A7, sp2.wrapping_sub(20));
    for _ in 0..5 {
        disp.dispatch_menu(true, 0x262, &mut cpu, &mut bus)
            .expect("SetMCInfo handled")
            .expect("SetMCInfo Ok");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp2,
        "5 SetMCInfo calls should pop 5×4 = 20 bytes cumulatively"
    );

    // AA63 DispMCInfo — PROCEDURE pop-4 (1xHandle). 5 × 4 = 20 bytes.
    let sp3 = cpu.read_reg(Register::A7);
    for i in 0..5u32 {
        bus.write_long(sp3.wrapping_sub((i + 1) * 4), 0);
    }
    cpu.write_reg(Register::A7, sp3.wrapping_sub(20));
    for _ in 0..5 {
        disp.dispatch_menu(true, 0x263, &mut cpu, &mut bus)
            .expect("DispMCInfo handled")
            .expect("DispMCInfo Ok");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp3,
        "5 DispMCInfo calls should pop 5×4 = 20 bytes cumulatively"
    );

    // AA64 GetMCEntry — FUNCTION 2xINTEGER + 4-byte result slot.
    // Each call: pre-push 4-byte slot + 4 bytes args, dispatch
    // pops the 4-byte args (leaving slot at SP+0), manually pop slot.
    let sp4 = cpu.read_reg(Register::A7);
    for _ in 0..5 {
        let cur = cpu.read_reg(Register::A7);
        // Pre-push 4-byte slot.
        bus.write_long(cur.wrapping_sub(4), 0xDEADBEEF);
        // Pre-push 4 bytes of args (2xINTEGER both zero).
        bus.write_long(cur.wrapping_sub(8), 0);
        cpu.write_reg(Register::A7, cur.wrapping_sub(8));
        disp.dispatch_menu(true, 0x264, &mut cpu, &mut bus)
            .expect("GetMCEntry handled")
            .expect("GetMCEntry Ok");
        // After trap: args popped (SP advanced by 4); slot now at SP+0.
        assert_eq!(
            cpu.read_reg(Register::A7),
            cur.wrapping_sub(4),
            "GetMCEntry must pop 4 bytes of args, leaving slot at SP+0"
        );
        // Manually pop the result slot.
        cpu.write_reg(Register::A7, cpu.read_reg(Register::A7).wrapping_add(4));
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp4,
        "5 GetMCEntry() FUNCTION call sequences must net-balance A7"
    );

    // AA65 SetMCEntries — PROCEDURE pop-6 (1xINTEGER + 1xPtr).
    // 5 calls × 6 = 30 bytes consumed total.
    let sp5 = cpu.read_reg(Register::A7);
    for i in 0..5u32 {
        // Each frame is 6 bytes; layout is [SP+0: ptr(4), SP+4: int(2)].
        bus.write_long(sp5.wrapping_sub((i + 1) * 6), 0);
        bus.write_word(sp5.wrapping_sub((i + 1) * 6).wrapping_add(4), 0);
    }
    cpu.write_reg(Register::A7, sp5.wrapping_sub(30));
    for _ in 0..5 {
        disp.dispatch_menu(true, 0x265, &mut cpu, &mut bus)
            .expect("SetMCEntries handled")
            .expect("SetMCEntries Ok");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp5,
        "5 SetMCEntries calls should pop 5×6 = 30 bytes cumulatively"
    );
}

// IM:I I-354: ClearMenuBar empties the current menu list.
#[test]
fn clearmenubar_empties_current_menu_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 501, 0x30B640, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 502, 0x30B680, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);
    assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 501), 0);
    assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 502), 0);

    let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
    assert!(result.is_some(), "ClearMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 501),
        0,
        "ClearMenuBar should remove previously inserted menu ID 501"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 502),
        0,
        "ClearMenuBar should remove previously inserted menu ID 502"
    );
}

// IM:V 1986 p. V-244: ClearMenuBar clears the current menu list and
// the application's menu color information table.
#[test]
fn clearmenubar_clears_menu_color_information_table() {
    let (mut disp, mut cpu, mut bus) = setup();
    let file_id = 501i16;
    let edit_id = 502i16;
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, file_id, 0x30B6C0, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, edit_id, 0x30B700, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);
    set_mc_entries_for_test(
        &mut disp,
        &mut cpu,
        &mut bus,
        &[
            (0, 0, 0x3100),
            (file_id, 0, 0x3200),
            (file_id, 1, 0x3300),
            (edit_id, 1, 0x3400),
        ],
    );

    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 1),
        0,
        "precondition: file item MenuCInfo entry should exist"
    );
    assert_ne!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
        0,
        "precondition: default MenuCInfo entry should exist"
    );

    let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
    assert!(result.is_some(), "ClearMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");

    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, file_id),
        0,
        "ClearMenuBar should remove the File menu from the current list"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, edit_id),
        0,
        "ClearMenuBar should remove the Edit menu from the current list"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 0),
        0,
        "ClearMenuBar should clear menu title MenuCInfo entries"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, file_id, 1),
        0,
        "ClearMenuBar should clear menu item MenuCInfo entries"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, edit_id, 1),
        0,
        "ClearMenuBar should clear every menu's MenuCInfo entries"
    );
    assert_eq!(
        get_mc_entry_ptr_for_test(&mut disp, &mut cpu, &mut bus, 0, 0),
        0,
        "ClearMenuBar should clear the default MenuCInfo entry too"
    );
}

// IM:I I-354: ClearMenuBar is a parameterless procedure.
#[test]
fn clearmenubar_has_no_parameters_and_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let result = disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus);
    assert!(result.is_some(), "ClearMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "ClearMenuBar should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "ClearMenuBar should not pop stack arguments"
    );
}

// 0x149 — GetMHandle: pops 2 bytes, writes handle at new SP.
#[test]
fn test_get_mhandle() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(TEST_SP, 1); // menu_id = 1
    let result = disp.dispatch_menu(true, 0x149, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetMHandle should be handled");
    assert!(result.unwrap().is_ok(), "GetMHandle should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 2, "GetMHandle should pop 2 bytes from stack");
    let handle = bus.read_long(sp);
    assert_eq!(handle, 0, "GetMHandle should return 0 handle when no menus");
}

// IM:I I-354: GetMenuBar returns a Handle to a copy of the current menu
// list and takes no parameters.
#[test]
fn getmenubar_returns_dynamic_menu_list_copy_and_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 510, 0x30B740, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 511, 0x30B780, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0xDEAD_BEEF);
    let result = disp.dispatch_menu(true, 0x13B, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "GetMenuBar should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "GetMenuBar should preserve A7 (no parameters)"
    );

    let list_handle = bus.read_long(TEST_SP);
    assert_ne!(list_handle, 0, "GetMenuBar should return a non-NIL handle");
    let menu_list = menu_list_from_memory(&bus, list_handle)
        .expect("returned handle should contain a DynamicMenuList");
    assert_eq!(
        menu_list.regular_handles().collect::<Vec<_>>(),
        vec![file, edit],
        "snapshot should preserve current regular-menu order"
    );
}

// IM:I I-355: SetMenuBar takes one Handle argument and has no function
// result, so it consumes 4 bytes from the stack.
#[test]
fn setmenubar_consumes_menulist_handle_argument() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_long(TEST_SP, 0); // mbar_handle
    let result = disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetMenuBar should be handled");
    assert!(result.unwrap().is_ok(), "SetMenuBar should succeed");
    let sp = cpu.read_reg(Register::A7);
    assert_eq!(sp, TEST_SP + 4, "SetMenuBar should pop 4 bytes from stack");
}

// IM:I I-354..I-355: SetMenuBar copies the specified list to the
// current menu list, enabling restoration of a prior GetMenuBar snapshot.
#[test]
fn setmenubar_restores_current_menu_list_from_getmenubar_snapshot() {
    let (mut disp, mut cpu, mut bus) = setup();

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 128, 0x30B700, "File");
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x30B710, "Edit");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);
    let current_menu_list = bus.read_long(crate::memory::globals::addr::MENU_LIST);
    assert_ne!(current_menu_list, 0);

    // Snapshot the current menu list with GetMenuBar.
    cpu.write_reg(Register::A7, TEST_SP);
    assert!(
        disp.dispatch_menu(true, 0x13B, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "GetMenuBar should succeed"
    );
    let saved_menu_list = bus.read_long(TEST_SP);
    assert_ne!(
        saved_menu_list, 0,
        "GetMenuBar should return non-NIL handle"
    );

    // Replace current menu list with a different one.
    assert!(
        disp.dispatch_menu(true, 0x134, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "ClearMenuBar should succeed"
    );
    let tools = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 130, 0x30B720, "Tools");
    insert_menu(&mut disp, &mut cpu, &mut bus, tools);
    assert_eq!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128), 0);
    assert_eq!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129), 0);
    assert_ne!(get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 130), 0);

    // Restore the saved list with SetMenuBar.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, saved_menu_list);
    assert!(
        disp.dispatch_menu(true, 0x13C, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "SetMenuBar should succeed"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::MENU_LIST),
        current_menu_list,
        "SetMenuBar should preserve the manager-owned current-list Handle"
    );

    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 128),
        0,
        "SetMenuBar should restore menu ID 128 from saved list"
    );
    assert_ne!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 129),
        0,
        "SetMenuBar should restore menu ID 129 from saved list"
    );
    assert_eq!(
        get_mhandle_for_id(&mut disp, &mut cpu, &mut bus, 130),
        0,
        "SetMenuBar should replace current list with the saved snapshot"
    );

    bus.write_long(saved_menu_list, 0);
    assert_eq!(
        menu_list_from_memory(&bus, current_menu_list)
            .expect("current list must outlive the caller-owned source")
            .regular_handles()
            .collect::<Vec<_>>(),
        vec![file, edit],
        "SetMenuBar must copy the list before the caller disposes its Handle"
    );
}

// IM:I I-355 / MTE 3-119: MenuSelect returns 0 when no menu item is
// chosen; the Point argument is consumed and the result is a LongInt.
#[test]
fn menuselect_no_menu_hit_returns_zero_and_pops_startpt() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.enable_input_trace_capture();
    disp.input_state.set_mouse_position_for_test((40, 120));
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 40);
    bus.write_word(TEST_SP + 2, 120);
    bus.write_long(TEST_SP + 4, 0xDEAD_BEEF);

    let result = disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus);
    assert!(result.is_some(), "MenuSelect should be handled");
    assert!(result.unwrap().is_ok(), "MenuSelect should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 4,
        "MenuSelect should pop the Point argument on no-hit return"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 4),
        0,
        "MenuSelect should return 0 when no menu item is selected"
    );
    assert!(
        disp.menu_tracking.is_none(),
        "MenuSelect should not leave tracking state active on no-hit return"
    );
    let trace = disp.input_trace_text();
    assert!(trace.contains("A93D action=start start=(40,120)"));
    assert!(trace.contains("result=$00000000 outcome=no_menu_title"));
}

// IM:I I-355: MenuSelect enters tracking while a menu title is active
// and does not immediately return a final menuResult.
#[test]
fn menuselect_title_hit_enters_tracking_without_immediate_stack_pop() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.enable_input_trace_capture();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30B800, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30B840, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    assert!(
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DrawMenuBar should succeed before MenuSelect tracking"
    );
    let regions = disp.menu_title_regions();
    assert!(
        !regions.is_empty(),
        "menu title regions should be available"
    );
    let title_mid_h = (regions[0].0 + regions[0].1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);

    let result = disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus);
    assert!(result.is_some(), "MenuSelect should be handled");
    assert!(result.unwrap().is_ok(), "MenuSelect should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "initial title-hit MenuSelect call should defer stack pop while tracking"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 4),
        0xA5A5_A5A5,
        "MenuSelect should defer writing result while tracking is active"
    );
    assert!(
        disp.menu_tracking.is_some(),
        "MenuSelect should enter tracking mode when menu title is hit"
    );
    let trace = disp.input_trace_text();
    assert!(trace.contains("A93D action=start"));
    assert!(trace.contains("outcome=open_tracking"));
    assert!(trace.contains("A93D action=tracking_entered"));
    assert!(trace.contains("tracking=menu:active"));
}

// IM:I I-355: MenuSelect tracks until mouse-up, then returns the
// selected enabled menu item as menuID in the high word and item number
// in the low word after the menu flash completes.
#[test]
fn menuselect_enabled_item_selection_traces_update_release_and_finish() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.enable_input_trace_capture();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30B900, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30B940,
        "Open/O;Close/W",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    assert!(
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DrawMenuBar should succeed before MenuSelect tracking"
    );
    let regions = disp.menu_title_regions();
    let title_mid_h = (regions[0].0 + regions[0].1) / 2;

    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(disp.menu_tracking.is_some());

    let (dropdown_top, dropdown_left, _, _) = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((dropdown_top + 17, dropdown_left + 8));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item),
        Some(2)
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .map(|tracking| tracking.flash_result),
        Some(0x0208_0002)
    );

    for _ in 0..40 {
        if disp.menu_tracking.is_none() {
            break;
        }
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
    }

    assert!(
        disp.menu_tracking.is_none(),
        "MenuSelect should finish after flash phases"
    );
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 0x0208_0002);

    let trace = disp.input_trace_text();
    assert!(trace.contains("A93D action=tracking_update"));
    assert!(trace.contains("highlighted_item=2 result=pending outcome=enabled_item_highlighted"));
    assert!(trace.contains("A93D action=release"));
    assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=start_flash"));
    assert!(trace.contains("A93D action=finish"));
    assert!(trace.contains("tracking=menu:idle dialog:idle control:idle"));
    assert!(trace.contains("highlighted_item=2 result=$02080002 outcome=enabled_item_selected"));
}

// The standard MDEF continually records the raw row under the pointer in
// MenuDisable, including disabled rows that MenuSelect cannot return.
// MenuChoice exposes that packed menu ID and item number after the
// zero-result release. Inside Macintosh Volume V (1986), pp. V-235 and
// V-248--V-249; Macintosh Toolbox Essentials (1992), pp. 3-118--3-119.
#[test]
fn menuselect_disabled_item_updates_live_menuchoice_result() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30B980, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30B9C0,
        "Open/O;Close/W",
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 2);
    bus.write_long(TEST_SP + 2, handle);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let title = disp.menu_title_regions()[0];
    let title_mid_h = (title.0 + title.1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let (dropdown_top, dropdown_left, _, _) = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((dropdown_top + 17, dropdown_left + 8));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let expected = menu_choice_value(520, 2);
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::MENU_DISABLE),
        expected,
    );
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item),
        Some(0),
        "a disabled row must not become the MenuSelect highlight",
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP + 4), 0);
    assert!(disp.menu_tracking.is_none());

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0xDEAD_BEEF);
    disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP), expected);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// MenuSelect consults the live MenuInfo enable flags on every tracking
// slice instead of treating the presentation snapshot as authority.
// Macintosh Toolbox Essentials (1992), pp. 3-90--3-92 and 3-114--3-119.
#[test]
fn menuselect_observes_live_item_disable_during_tracking() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 523, 0x30BD00, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30BD40, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let title = disp.menu_title_regions()[0];
    let title_mid_h = (title.0 + title.1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let menu_ptr = bus.read_long(handle);
    let enable_flags = bus.read_long(menu_ptr + 10);
    assert_ne!(enable_flags & (1 << 1), 0, "item 1 should start enabled");
    bus.write_long(menu_ptr + 10, enable_flags & !(1 << 1));

    let (top, left, _, _) = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((top + 8, left + 8));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::MENU_DISABLE),
        menu_choice_value(523, 1),
    );
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item),
        Some(0),
        "the live-disabled item became highlighted",
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP + 4), 0);
    assert!(disp.menu_tracking.is_none());
}

// A disabled menu remains available for examination even though every
// row must behave as disabled. Macintosh Toolbox Essentials (1992),
// pp. 3-6--3-7 and 3-114--3-119.
#[test]
fn menuselect_disabled_menu_opens_without_selecting_an_item() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 522, 0x30BC00, "View");
    append_menu_data(&mut disp, &mut cpu, &mut bus, handle, 0x30BC40, "Zoom/Z");
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, handle);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (screen_base, row_bytes, _, screen_height, _) = disp.get_screen_params();
    let screen_len =
        usize::try_from(row_bytes.saturating_mul(u32::try_from(screen_height.max(0)).unwrap_or(0)))
            .unwrap();
    let screen_before = bus.read_bytes(screen_base, screen_len);
    let title = disp.menu_title_regions()[0];
    let title_mid_h = (title.0 + title.1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (top, left, _, _) = disp
        .menu_tracking
        .as_ref()
        .expect("disabled title should still open")
        .dropdown_rect();

    disp.input_state
        .set_mouse_position_for_test((top + 8, left + 8));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let expected = menu_choice_value(522, 1);
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::MENU_DISABLE),
        expected,
    );
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .map(|tracking| tracking.highlighted_item),
        Some(0),
    );

    disp.input_state.set_mouse_button_for_test(false);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP + 4), 0);
    assert!(disp.menu_tracking.is_none());
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::MENU_DISABLE),
        expected,
    );
    assert_eq!(
        bus.read_bytes(screen_base, screen_len),
        screen_before,
        "disabled menu tracking did not restore the classic framebuffer",
    );

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, 0xDEAD_BEEF);
    disp.dispatch_menu(true, 0x266, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP), expected);
}

#[test]
fn setmenuflash_zero_returns_release_selection_without_blinking() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 521, 0x30BA00, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30BA40,
        "Open/O;Save/S",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let title = disp.menu_title_regions()[0];
    let title_mid_h = (title.0 + title.1) / 2;

    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let (top, left, _, _) = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((top + 17, left + 8));
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x14A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    disp.input_state.set_mouse_button_for_test(false);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(disp.menu_tracking, None);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(bus.read_long(TEST_SP + 4), 0x0209_0002);
}

struct MenuKeyThemeSnapshot {
    lower_result: u32,
    lower_stack_after: u32,
    title_regions: Vec<(i16, i16)>,
    matched_title_changed_pixels: usize,
    other_title_changed_pixels: usize,
    disabled_rightmost_result: u32,
    disabled_rightmost_stack_after: u32,
    all_disabled_result: u32,
    all_disabled_stack_after: u32,
    uninserted_result: u32,
    uninserted_stack_after: u32,
}

fn menu_key_results_for_theme(theme_id: UiThemeId) -> MenuKeyThemeSnapshot {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(theme_id);
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 342);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 342, 1);
    disp.menu_bar_hidden = false;
    clear_1bpp_screen(&mut bus, base, row_bytes, 342);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 700, 0x30B700, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x30B740, "Open/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, file);

    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 701, 0x30B800, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, edit, 0x30B840, "Other/O");
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);

    let ghost = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 702, 0x30B900, "Ghost");
    append_menu_data(&mut disp, &mut cpu, &mut bus, ghost, 0x30B940, "Ghost/G");

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let title_regions = disp.menu_title_regions();
    let file_before = title_region_pixels(
        &bus,
        base,
        row_bytes,
        title_regions[0].0,
        title_regions[0].1,
    );
    let edit_before = title_region_pixels(
        &bus,
        base,
        row_bytes,
        title_regions[1].0,
        title_regions[1].1,
    );

    let (lower_result, lower_stack_after) =
        menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'o');

    let file_after = title_region_pixels(
        &bus,
        base,
        row_bytes,
        title_regions[0].0,
        title_regions[0].1,
    );
    let edit_after = title_region_pixels(
        &bus,
        base,
        row_bytes,
        title_regions[1].0,
        title_regions[1].1,
    );
    let other_title_changed_pixels = changed_pixel_count(&file_before, &file_after);
    let matched_title_changed_pixels = changed_pixel_count(&edit_before, &edit_after);

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, edit);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (disabled_rightmost_result, disabled_rightmost_stack_after) =
        menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'O');

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0);
    bus.write_long(TEST_SP + 2, file);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let (all_disabled_result, all_disabled_stack_after) =
        menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'O');
    let (uninserted_result, uninserted_stack_after) =
        menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'G');

    MenuKeyThemeSnapshot {
        lower_result,
        lower_stack_after,
        title_regions,
        matched_title_changed_pixels,
        other_title_changed_pixels,
        disabled_rightmost_result,
        disabled_rightmost_stack_after,
        all_disabled_result,
        all_disabled_stack_after,
        uninserted_result,
        uninserted_stack_after,
    }
}

#[test]
fn menukey_does_not_paint_titles_while_menu_bar_is_hidden() {
    for (menu_bar_hidden, fullscreen_locked) in [(true, false), (false, true)] {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let row_bytes = 512;
        let height = 342;
        let base = bus.alloc(row_bytes * height);
        disp.set_screen_mode_for_test(base, row_bytes, 512, height as u16, 8);
        clear_1bpp_screen(&mut bus, base, row_bytes, height);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 703, 0x30BA00, "File");
        append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x30BA40, "New/N");
        insert_menu(&mut disp, &mut cpu, &mut bus, file);
        assert!(disp.menus[0].visible_in_menu_bar);

        disp.menu_bar_hidden = menu_bar_hidden;
        disp.fullscreen_locked = fullscreen_locked;
        let before = bus.read_bytes(base, row_bytes as usize * 20);

        let (result, stack_after) = menu_key_result_and_stack(&mut disp, &mut cpu, &mut bus, b'N');
        assert_eq!(result, 0x02BF_0001);
        assert_eq!(stack_after, TEST_SP + 2);
        assert_eq!(
            bus.read_bytes(base, row_bytes as usize * 20),
            before,
            "MenuKey must not expose a title highlight while menu chrome is hidden"
        );

        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_word(TEST_SP, 0);
        disp.dispatch_menu(true, 0x138, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.draw_menu_bar_to_fb(&mut bus);
        assert_eq!(
            bus.read_bytes(base, row_bytes as usize * 20),
            before,
            "clearing and compositing hidden menu chrome must leave the framebuffer unchanged"
        );
    }
}

#[test]
fn systemless_theme_does_not_change_menukey_result_or_highlight_geometry() {
    // IM:I I-356 says MenuKey maps a Command-key character to the
    // same LongInt result as MenuSelect, scans duplicate shortcuts
    // right-to-left/top-to-bottom, ignores disabled/non-current items,
    // folds case via UpperText, and highlights the matching menu title.
    // Theme chrome must not change the result, stack, or title geometry.
    let classic = menu_key_results_for_theme(UiThemeId::ClassicSystem7);
    let themed = menu_key_results_for_theme(UiThemeId::SystemlessDefault);

    assert_eq!(classic.lower_result, 0x02BD_0001);
    assert_eq!(classic.lower_stack_after, TEST_SP + 2);
    assert_eq!(classic.disabled_rightmost_result, 0x02BC_0001);
    assert_eq!(classic.disabled_rightmost_stack_after, TEST_SP + 2);
    assert_eq!(classic.all_disabled_result, 0);
    assert_eq!(classic.all_disabled_stack_after, TEST_SP + 2);
    assert_eq!(classic.uninserted_result, 0);
    assert_eq!(classic.uninserted_stack_after, TEST_SP + 2);
    assert!(
        classic.matched_title_changed_pixels > classic.other_title_changed_pixels,
        "classic MenuKey should primarily change the matched rightmost title"
    );
    assert!(
        classic.matched_title_changed_pixels > 0,
        "classic MenuKey should highlight the matched rightmost title"
    );

    assert_eq!(themed.lower_result, classic.lower_result);
    assert_eq!(themed.lower_stack_after, classic.lower_stack_after);
    assert_eq!(
        themed.disabled_rightmost_result,
        classic.disabled_rightmost_result
    );
    assert_eq!(
        themed.disabled_rightmost_stack_after,
        classic.disabled_rightmost_stack_after
    );
    assert_eq!(themed.all_disabled_result, classic.all_disabled_result);
    assert_eq!(
        themed.all_disabled_stack_after,
        classic.all_disabled_stack_after
    );
    assert_eq!(themed.uninserted_result, classic.uninserted_result);
    assert_eq!(
        themed.uninserted_stack_after,
        classic.uninserted_stack_after
    );
    assert_eq!(
        themed.title_regions, classic.title_regions,
        "systemless-default must not change MenuKey title geometry"
    );
    assert_eq!(
        themed.other_title_changed_pixels, 0,
        "systemless-default should keep non-matched title chrome unchanged"
    );
    assert!(
        themed.matched_title_changed_pixels > 0,
        "systemless-default MenuKey should route the matched title highlight through the provider"
    );
}

struct PopUpMenuSelectThemeSnapshot {
    rect: (i16, i16, i16, i16),
    highlighted_item: i16,
    item_at_requested_point: i16,
    first_stack_after: u32,
    result: u32,
    final_stack_after: u32,
    tracking_finished: bool,
    clamped_rect: (i16, i16, i16, i16),
    clamped_highlighted_item: i16,
    uninserted_result: u32,
    uninserted_stack_after: u32,
    uninserted_tracking_finished: bool,
}

fn dispatch_popupmenuselect_start(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
    menu_handle: u32,
    top: i16,
    left: i16,
    popup_item: i16,
) {
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, popup_item as u16);
    bus.write_word(TEST_SP + 2, left as u16);
    bus.write_word(TEST_SP + 4, top as u16);
    bus.write_long(TEST_SP + 6, menu_handle);
    bus.write_long(TEST_SP + 10, 0xDEAD_BEEF);
    assert!(
        disp.step_menu_fixture(true, 0x00B, cpu, bus)
            .unwrap()
            .is_ok(),
        "PopUpMenuSelect should succeed"
    );
}

#[test]
fn popupmenuselect_faults_in_cicn_items_before_sizing_and_drawing() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 64;
    let base = bus.alloc(row_bytes * 160);
    disp.set_screen_mode_for_test(base, row_bytes, 512, 160, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 160);
    disp.menu_bar_hidden = false;
    disp.input_state.set_mouse_button_for_test(true);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 733, 0x30BE00, "Crops");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x30BE40, "Corn;Wheat");
    // Item icon 19 resolves to resource 275. Leave its key-equivalent
    // byte at zero; without the color icon this would be mis-sized as a
    // 34-pixel normal ICON row.
    disp.menus[0].items[0].icon = 19;
    let cicn = cicn_source_with_left_stripe(16, 16);
    disp.install_test_resource(&mut bus, *b"cicn", 275, &cicn);
    disp.with_resource_file_mut_for_test(0, |file| {
        file.loaded.insert((*b"cicn", 275), 0);
    })
    .unwrap();
    assert!(
        disp.find_loaded_resource_any(*b"cicn", 275).is_none(),
        "precondition: the color icon should be nonresident"
    );
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, menu, -1);

    dispatch_popupmenuselect_start(&mut disp, &mut cpu, &mut bus, menu, 50, 40, 0);

    let rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    assert_eq!(
        rect.2 - rect.0,
        16 + 16,
        "popup geometry should use the loaded 16-pixel cicn rows"
    );
    assert!(
        disp.find_loaded_resource_any(*b"cicn", 275).is_some(),
        "PopUpMenuSelect should materialize a nonresident item icon"
    );
    assert!(
        screen_pixel_is_set(&bus, base, row_bytes, rect.1 + 4, rect.0 + 2),
        "the popup should draw the application color icon"
    );
}

#[test]
fn popupmenuselect_tracks_oversized_content_from_shared_layout() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    let row_bytes = 100;
    let base = bus.alloc(row_bytes * 600);
    disp.set_screen_mode_for_test(base, row_bytes, 800, 600, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 600);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.menu_bar_hidden = false;
    disp.input_state.set_mouse_button_for_test(true);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 734, 0x30BF00, "Long");
    let description = (0..40).map(|_| "A").collect::<Vec<_>>().join(";");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x30BF40, &description);
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, menu, -1);

    dispatch_popupmenuselect_start(&mut disp, &mut cpu, &mut bus, menu, 100, 120, 20);

    let tracking = disp.menu_tracking.as_ref().expect("popup tracking active");
    assert_eq!(tracking.popup_top, 4);
    assert_eq!(tracking.popup_height, 576);
    assert_eq!(tracking.content_top, -204);
    assert_eq!(tracking.highlighted_item, 20);
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::TOP_MENU_ITEM) as i16,
        -204,
    );
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::AT_MENU_BOTTOM) as i16,
        436,
    );
    assert_eq!(
        disp.dropdown_item_at_point(&bus, tracking.popup_left + 4, 100),
        20
    );
    assert!(
        screen_pixel_is_set(
            &bus,
            base,
            row_bytes,
            tracking.popup_left + tracking.popup_width / 2,
            tracking.popup_top + 4,
        ),
        "the hidden upper content must replace the first visible row with an indicator"
    );

    let popup_left = tracking.popup_left;
    disp.update_menu_tracking_for_point(&mut bus, popup_left + 4, 28);
    assert_eq!(disp.menu_tracking.as_ref().unwrap().highlighted_item, 15);
    disp.update_menu_tracking_for_point(&mut bus, popup_left + 4, 12);
    assert_eq!(disp.menu_tracking.as_ref().unwrap().content_top, -204);
    assert_eq!(disp.menu_tracking.as_ref().unwrap().highlighted_item, 0);
    disp.update_menu_tracking_for_point(&mut bus, popup_left + 4, 12);
    assert_eq!(disp.menu_tracking.as_ref().unwrap().content_top, -188);
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::TOP_MENU_ITEM) as i16,
        -188,
    );
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::AT_MENU_BOTTOM) as i16,
        452,
    );
    disp.update_menu_tracking_for_point(&mut bus, popup_left + 4, 3);
    assert_eq!(disp.menu_tracking.as_ref().unwrap().content_top, -172);
    assert_eq!(
        bus.read_word(crate::memory::globals::addr::AT_MENU_BOTTOM) as i16,
        468,
    );
}

fn finish_popupmenuselect(
    disp: &mut super::super::TrapDispatcher,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> (u32, u32, bool) {
    disp.input_state.set_mouse_button_for_test(false);
    bus.write_byte(crate::memory::globals::addr::MB_STATE, 0x80);
    for _ in 0..80 {
        if disp.menu_tracking.is_none() {
            break;
        }
        disp.step_menu_fixture(true, 0x00B, cpu, bus)
            .unwrap()
            .unwrap();
    }
    (
        bus.read_long(TEST_SP + 10),
        cpu.read_reg(Register::A7),
        disp.menu_tracking.is_none(),
    )
}

fn popupmenuselect_theme_snapshot(theme_id: UiThemeId) -> PopUpMenuSelectThemeSnapshot {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(theme_id);
    let row_bytes = 30;
    let base = bus.alloc(row_bytes * 160);
    disp.set_screen_mode_for_test(base, row_bytes, 240, 160, 1);
    clear_1bpp_screen(&mut bus, base, row_bytes, 160);
    disp.menu_bar_hidden = false;
    disp.input_state.set_mouse_button_for_test(true);

    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 730, 0x30BB00, "Pop");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x30BB40,
        "One;Two;Three;Four",
    );
    insert_menu_before_id(&mut disp, &mut cpu, &mut bus, menu, -1);

    dispatch_popupmenuselect_start(&mut disp, &mut cpu, &mut bus, menu, 58, 30, 3);
    let tracking = disp.menu_tracking.as_ref().expect("popup tracking active");
    let rect = tracking.dropdown_rect();
    let highlighted_item = tracking.highlighted_item;
    let first_stack_after = cpu.read_reg(Register::A7);
    let item_at_requested_point = disp.dropdown_item_at_point(&bus, 35, 58);
    // PopUpMenuSelect re-evaluates the live mouse position on release.
    // Keep the synthetic release over the requested third item rather
    // than inheriting setup_with_port's default position at (0, 0).
    disp.input_state.set_mouse_position_for_test((58, 35));
    let (result, final_stack_after, tracking_finished) =
        finish_popupmenuselect(&mut disp, &mut cpu, &mut bus);

    let (mut clamp_disp, mut clamp_cpu, mut clamp_bus) = setup_with_port();
    clamp_disp.set_ui_theme_id(theme_id);
    let clamp_base = clamp_bus.alloc(row_bytes * 160);
    clamp_disp.set_screen_mode_for_test(clamp_base, row_bytes, 240, 160, 1);
    clear_1bpp_screen(&mut clamp_bus, clamp_base, row_bytes, 160);
    clamp_disp.menu_bar_hidden = false;
    clamp_disp.input_state.set_mouse_button_for_test(true);
    let clamp_menu = new_menu_with_title(
        &mut clamp_disp,
        &mut clamp_cpu,
        &mut clamp_bus,
        731,
        0x30BC00,
        "Pop",
    );
    append_menu_data(
        &mut clamp_disp,
        &mut clamp_cpu,
        &mut clamp_bus,
        clamp_menu,
        0x30BC40,
        "One;Two;Three;Four",
    );
    insert_menu_before_id(
        &mut clamp_disp,
        &mut clamp_cpu,
        &mut clamp_bus,
        clamp_menu,
        -1,
    );
    dispatch_popupmenuselect_start(
        &mut clamp_disp,
        &mut clamp_cpu,
        &mut clamp_bus,
        clamp_menu,
        150,
        220,
        4,
    );
    let clamp_tracking = clamp_disp
        .menu_tracking
        .as_ref()
        .expect("clamped popup tracking active");
    let clamped_rect = clamp_tracking.dropdown_rect();
    let clamped_highlighted_item = clamp_tracking.highlighted_item;

    let (mut miss_disp, mut miss_cpu, mut miss_bus) = setup_with_port();
    miss_disp.set_ui_theme_id(theme_id);
    miss_disp.menu_bar_hidden = false;
    miss_disp.input_state.set_mouse_button_for_test(false);
    let miss_menu = new_menu_with_title(
        &mut miss_disp,
        &mut miss_cpu,
        &mut miss_bus,
        732,
        0x30BD00,
        "Pop",
    );
    append_menu_data(
        &mut miss_disp,
        &mut miss_cpu,
        &mut miss_bus,
        miss_menu,
        0x30BD40,
        "One;Two",
    );
    dispatch_popupmenuselect_start(
        &mut miss_disp,
        &mut miss_cpu,
        &mut miss_bus,
        miss_menu,
        20,
        20,
        1,
    );

    PopUpMenuSelectThemeSnapshot {
        rect,
        highlighted_item,
        item_at_requested_point,
        first_stack_after,
        result,
        final_stack_after,
        tracking_finished,
        clamped_rect,
        clamped_highlighted_item,
        uninserted_result: miss_bus.read_long(TEST_SP + 10),
        uninserted_stack_after: miss_cpu.read_reg(Register::A7),
        uninserted_tracking_finished: miss_disp.menu_tracking.is_none(),
    }
}

#[test]
fn systemless_theme_does_not_change_popupmenuselect_geometry_and_result() {
    // MTE 1992 p. 3-120 says PopUpMenuSelect displays the requested
    // PopUpItem at Top/Left, tracks until mouse-up, and returns the
    // chosen menu ID/item LongInt. IM:V V-241 adds that the menu must
    // be inserted in the MenuList for the duration of the call.
    let classic = popupmenuselect_theme_snapshot(UiThemeId::ClassicSystem7);
    let themed = popupmenuselect_theme_snapshot(UiThemeId::SystemlessDefault);

    let width = crate::menu_manager::standard_menu_text_advance(b"Three") + 32;
    assert_eq!(classic.rect, (26, 30, 90, 30 + width));
    assert_eq!(classic.highlighted_item, 3);
    assert_eq!(classic.item_at_requested_point, 3);
    assert_eq!(classic.first_stack_after, TEST_SP);
    assert_eq!(classic.result, 0x02DA_0003);
    assert_eq!(classic.final_stack_after, TEST_SP + 10);
    assert!(classic.tracking_finished);
    assert_eq!(classic.clamped_rect, (95, 236 - width, 159, 236));
    assert_eq!(classic.clamped_highlighted_item, 4);
    assert_eq!(classic.uninserted_result, 0);
    assert_eq!(classic.uninserted_stack_after, TEST_SP + 10);
    assert!(classic.uninserted_tracking_finished);

    assert_eq!(
        themed.rect, classic.rect,
        "systemless-default must preserve PopUpMenuSelect popup geometry"
    );
    assert_eq!(themed.highlighted_item, classic.highlighted_item);
    assert_eq!(
        themed.item_at_requested_point,
        classic.item_at_requested_point
    );
    assert_eq!(themed.first_stack_after, classic.first_stack_after);
    assert_eq!(themed.result, classic.result);
    assert_eq!(themed.final_stack_after, classic.final_stack_after);
    assert_eq!(themed.tracking_finished, classic.tracking_finished);
    assert_eq!(themed.clamped_rect, classic.clamped_rect);
    assert_eq!(
        themed.clamped_highlighted_item,
        classic.clamped_highlighted_item
    );
    assert_eq!(themed.uninserted_result, classic.uninserted_result);
    assert_eq!(
        themed.uninserted_stack_after,
        classic.uninserted_stack_after
    );
    assert_eq!(
        themed.uninserted_tracking_finished,
        classic.uninserted_tracking_finished
    );
}

fn menuselect_enabled_item_result_for_theme(
    theme_id: UiThemeId,
) -> (u32, u32, (i16, i16, i16, i16)) {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.set_ui_theme_id(theme_id);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let handle = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 520, 0x30BA00, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        handle,
        0x30BA40,
        "Open/O;Close/W",
    );
    insert_menu(&mut disp, &mut cpu, &mut bus, handle);

    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.input_state.set_mouse_position_for_test((10, 15));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, 15);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let dropdown_rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    let (dropdown_top, dropdown_left, _, _) = dropdown_rect;
    disp.input_state
        .set_mouse_position_for_test((dropdown_top + 17, dropdown_left + 8));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.input_state.set_mouse_button_for_test(false);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    for _ in 0..40 {
        if disp.menu_tracking.is_none() {
            break;
        }
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
    }

    (
        bus.read_long(TEST_SP + 4),
        cpu.read_reg(Register::A7),
        dropdown_rect,
    )
}

#[test]
fn systemless_theme_does_not_change_menuselect_result_encoding() {
    // IM:I I-356: MenuSelect tracks until mouse-up and returns a LongInt
    // with menu ID in the high word and item number in the low word.
    // Theme chrome must not change that guest-visible encoding or metrics.
    let classic = menuselect_enabled_item_result_for_theme(UiThemeId::ClassicSystem7);
    let themed = menuselect_enabled_item_result_for_theme(UiThemeId::SystemlessDefault);

    assert_eq!(classic.0, 0x0208_0002);
    assert_eq!(classic.1, TEST_SP + 4);
    assert_eq!(
        themed, classic,
        "systemless-default must not change MenuSelect result encoding or dropdown geometry"
    );
}

// IM:V V-235..V-237: a hierarchical item has itemCmd=$1B and
// itemMark equal to the submenu menuID; MenuSelect returns the
// selected submenu's menuID/item packed as a LongInt.
#[test]
fn menuselect_tracks_hierarchical_submenu_item() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x310000, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        file,
        0x310040,
        "New Game;Resume Game;-;Practice Battle;Pause Game;Resign Game;-;Quit",
    );
    let practice = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 138, 0x310200, "Practice");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        practice,
        0x310240,
        "1: Cake Walk;(2: One-Gun;(3: Sucker Punch;(4: Airborne",
    );
    set_item_cmd(&mut disp, &mut cpu, &mut bus, file, 4, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, file, 4, 138);

    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu_before(&mut disp, &mut cpu, &mut bus, practice, -1);
    cpu.write_reg(Register::A7, TEST_SP);
    assert!(
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DrawMenuBar should succeed before MenuSelect tracking"
    );

    let regions = disp.menu_title_regions();
    let file_mid_h = (regions[0].0 + regions[0].1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, file_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, file_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xFFFF_FFFF);

    assert!(
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "MenuSelect should enter tracking on the File title"
    );
    let parent_rect = disp
        .menu_tracking
        .as_ref()
        .expect("MenuSelect should be tracking")
        .dropdown_rect();
    let parent_item_y = parent_rect.0 + disp.menu_rows(&bus, &disp.menus[0].items).offset(4) + 8;

    disp.input_state
        .set_mouse_position_for_test((parent_item_y, parent_rect.1 + 24));
    assert!(
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "MenuSelect should track the hierarchical parent item"
    );

    disp.input_state
        .set_mouse_position_for_test((parent_item_y, parent_rect.3 + 20));
    assert!(
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "MenuSelect should track into the submenu"
    );

    disp.input_state.set_mouse_button_for_test(false);
    for _ in 0..40 {
        assert!(
            disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
                .unwrap()
                .is_ok(),
            "MenuSelect should finish submenu selection"
        );
        if disp.menu_tracking.is_none() {
            break;
        }
    }

    assert!(
        disp.menu_tracking.is_none(),
        "MenuSelect should finish after flashing the submenu item"
    );
    assert_eq!(
        bus.read_long(TEST_SP + 4),
        (138u32 << 16) | 1,
        "MenuSelect should return the selected submenu menuID/item"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP + 4,
        "MenuSelect should pop the Point argument after submenu selection"
    );
}

#[test]
fn menuselect_routes_hierarchical_child_through_its_custom_mdef() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    setup_8bpp_menu_screen(&mut disp, &mut bus, 240, 120);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let original_port = *disp.current_port;

    let root = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 140, 0x310600, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        root,
        0x310640,
        "Custom child",
    );
    let child = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 141, 0x310800, "Child");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        child,
        0x310840,
        "Opaque definition data",
    );
    set_item_cmd(&mut disp, &mut cpu, &mut bus, root, 1, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, root, 1, 141);

    let child_ptr = bus.read_long(child);
    bus.write_word(child_ptr + 2, 72);
    bus.write_word(child_ptr + 4, 32);
    let mdef_ptr = bus.alloc(2);
    let mdef_handle = bus.alloc(4);
    bus.write_word(mdef_ptr, 0x4E75);
    bus.write_long(mdef_handle, mdef_ptr);
    bus.write_long(child_ptr + 6, mdef_handle);
    disp.insert_loaded_resource_handle_for_test(mdef_handle, (mdef_ptr, *b"MDEF", 256));

    insert_menu(&mut disp, &mut cpu, &mut bus, root);
    insert_menu_before(&mut disp, &mut cpu, &mut bus, child, -1);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let title = disp.menu_title_regions()[0];
    let title_mid_h = (title.0 + title.1) / 2;
    let trap_pc = 0x0012_3600;
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, title_mid_h as u16);
    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let root_rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((root_rect.0 + 8, root_rect.1 + 16));
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let trampoline = cpu.read_reg(Register::PC);
    let child_rect = disp.menu_tracking.as_ref().unwrap().submenus[0].dropdown_rect();
    assert_eq!(child_rect.3 - child_rect.1, 72);
    assert_eq!(child_rect.2 - child_rect.0, 32);
    assert_eq!(bus.read_word(trampoline + 6), 0);
    assert_eq!(bus.read_long(trampoline + 10), child);
    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .unwrap()
            .active_definition_pane(),
        Some(super::SharedMenuDefinitionPane::Submenu(0))
    );

    disp.input_state
        .set_mouse_position_for_test((10, title_mid_h));
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(trampoline + 6), 1);
    bus.write_word(trampoline + 68, 0);
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(disp.menu_tracking.as_ref().unwrap().submenus.is_empty());
    assert_eq!(*disp.current_port, original_port);

    disp.input_state
        .set_mouse_position_for_test((root_rect.0 + 8, root_rect.1 + 16));
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(trampoline + 6), 0);

    disp.input_state
        .set_mouse_position_for_test((child_rect.0 + 8, child_rect.1 + 8));
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(trampoline + 6), 1);
    assert_eq!(bus.read_long(trampoline + 10), child);

    bus.write_word(trampoline + 68, 2);
    disp.input_state.set_mouse_button_for_test(false);
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        disp.menu_tracking.as_ref().unwrap().flash_result,
        (141u32 << 16) | 2
    );

    disp.menu_tracking
        .with_tracking_mut(|tracking| {
            tracking.flash_remaining = 1;
            tracking.flash_deadline = tracking.flash_tick.unwrap_or(0);
        })
        .unwrap();
    cpu.write_reg(Register::PC, trap_pc + 2);
    cpu.write_reg(
        Register::A7,
        if disp.guest_calls.depth() == 0 {
            TEST_SP
        } else {
            TEST_SP - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
        },
    );
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_long(TEST_SP + 4), (141u32 << 16) | 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(disp.menu_tracking, None);
}

#[test]
fn menuselect_tracks_nested_hierarchical_submenu_item() {
    // MTE 1992, pp. 3-137 to 3-141: hierarchical menus may themselves
    // contain hierarchical items. Tracking must retain every open level
    // and return the deepest selected menu ID and item number.
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let game = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 129, 0x310000, "Game");
    append_menu_data(&mut disp, &mut cpu, &mut bus, game, 0x310040, "Options");
    let options = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 138, 0x310200, "Options");
    append_menu_data(&mut disp, &mut cpu, &mut bus, options, 0x310240, "Speed");
    let speed = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 139, 0x310400, "Speed");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        speed,
        0x310440,
        "Cycle;Fast;Moderate;Slow",
    );
    set_item_cmd(&mut disp, &mut cpu, &mut bus, game, 1, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, game, 1, 138);
    set_item_cmd(&mut disp, &mut cpu, &mut bus, options, 1, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, options, 1, 139);
    set_item_cmd(&mut disp, &mut cpu, &mut bus, speed, 1, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, speed, 1, 138);
    insert_menu(&mut disp, &mut cpu, &mut bus, game);
    insert_menu_before(&mut disp, &mut cpu, &mut bus, options, -1);
    insert_menu_before(&mut disp, &mut cpu, &mut bus, speed, -1);
    cpu.write_reg(Register::A7, TEST_SP);
    disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let regions = disp.menu_title_regions();
    let game_mid_h = (regions[0].0 + regions[0].1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, game_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, game_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xFFFF_FFFF);
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let root_rect = disp.menu_tracking.as_ref().unwrap().dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((root_rect.0 + 9, root_rect.1 + 24));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let options_rect = disp.menu_tracking.as_ref().unwrap().submenus[0].dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((options_rect.0 + 9, options_rect.1 + 24));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let speed_rect = disp.menu_tracking.as_ref().unwrap().submenus[1].dropdown_rect();
    disp.input_state
        .set_mouse_position_for_test((speed_rect.0 + 9, speed_rect.1 + 24));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        disp.menu_tracking.as_ref().unwrap().submenus.len(),
        2,
        "a circular submenu must not grow the retained hierarchy"
    );
    disp.input_state
        .set_mouse_position_for_test((speed_rect.0 + 1 + 16 + 8, speed_rect.1 + 24));
    disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.input_state.set_mouse_button_for_test(false);
    for _ in 0..40 {
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        if disp.menu_tracking.is_none() {
            break;
        }
    }

    assert_eq!(bus.read_long(TEST_SP + 4), (139u32 << 16) | 2);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
}

// IM:V V-239: InsertMenu(beforeID=-1) places a menu in the hierarchical
// portion of the current menu list, so later nonhierarchical menu-bar
// titles must still hit-test by their source menu record, not by compacted
// title-region index.
#[test]
fn menuselect_regular_menu_after_hierarchical_menu_still_opens() {
    let (mut disp, mut cpu, mut bus) = setup_with_port();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let file = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 540, 0x311000, "File");
    append_menu_data(&mut disp, &mut cpu, &mut bus, file, 0x311040, "Open");
    let submenu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 541, 0x311100, "Practice");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        submenu,
        0x311140,
        "1: Cake Walk",
    );
    let edit = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 542, 0x311200, "Edit");
    append_menu_data(&mut disp, &mut cpu, &mut bus, edit, 0x311240, "Copy");

    insert_menu(&mut disp, &mut cpu, &mut bus, file);
    insert_menu_before(&mut disp, &mut cpu, &mut bus, submenu, -1);
    insert_menu(&mut disp, &mut cpu, &mut bus, edit);

    cpu.write_reg(Register::A7, TEST_SP);
    assert!(
        disp.dispatch_menu(true, 0x137, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "DrawMenuBar should succeed before MenuSelect tracking"
    );

    let regions = disp.menu_title_regions();
    assert_eq!(
        regions.len(),
        2,
        "hierarchical menu must not create a menu-bar title"
    );
    let edit_mid_h = (regions[1].0 + regions[1].1) / 2;
    disp.input_state
        .set_mouse_position_for_test((10, edit_mid_h));
    disp.input_state.set_mouse_button_for_test(true);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 10);
    bus.write_word(TEST_SP + 2, edit_mid_h as u16);
    bus.write_long(TEST_SP + 4, 0xA5A5_A5A5);

    assert!(
        disp.step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
            .unwrap()
            .is_ok(),
        "MenuSelect should enter tracking on the Edit title"
    );

    assert_eq!(
        disp.menu_tracking
            .as_ref()
            .expect("MenuSelect should be tracking")
            .menu_handle,
        edit,
        "MenuSelect should open the regular menu after a hierarchical menu"
    );
}

// save_dropdown_pixels / restore_dropdown_pixels must not overflow
// when the rect has y < 0 or y >= screen_h. Mirrors the off-screen
// guards in save_dialog_pixels and save_rect_pixels.
#[test]
fn save_and_restore_dropdown_pixels_4bpp_round_trip_odd_nibble_bounds() {
    let (mut disp, _cpu, mut bus) = setup();
    let (base, row_bytes) = setup_4bpp_menu_screen(&mut disp, &mut bus, 16, 4);
    let byte_len = row_bytes * 4;
    for offset in 0..byte_len {
        bus.write_byte(
            base + offset,
            (offset as u8).wrapping_mul(7).wrapping_add(3),
        );
    }
    let before = bus.read_bytes(base, byte_len as usize);

    // The saved pixels span x=3 through the right-hand shadow at x=8.
    // Both edges share a byte with a pixel outside that span, so four
    // whole bytes per row must be retained for the two included rows.
    let rect = (0, 3, 1, 8);
    let saved = disp.save_dropdown_pixels(&bus, rect);
    assert_eq!(saved.len(), 2 * 4);

    for y in 0..2u32 {
        for byte_x in 1..5u32 {
            bus.write_byte(base + y * row_bytes + byte_x, 0xAA);
        }
    }
    disp.restore_dropdown_pixels(&mut bus, rect, &saved);

    assert_eq!(
        bus.read_bytes(base, byte_len as usize),
        before,
        "4bpp save/restore should preserve both boundary nibbles byte-for-byte"
    );
}

#[test]
fn save_and_restore_dropdown_pixels_2bpp_round_trip_odd_field_bounds() {
    let (mut disp, _cpu, mut bus) = setup();
    let row_bytes = 4u32;
    let height = 4u32;
    let base = bus.alloc(row_bytes * height);
    disp.screen_mode = (base, row_bytes, 12, height as u16, 2);
    for offset in 0..row_bytes * height {
        bus.write_byte(
            base + offset,
            (offset as u8).wrapping_mul(11).wrapping_add(5),
        );
    }
    let before = bus.read_bytes(base, (row_bytes * height) as usize);

    // The rectangle plus its one-pixel shadow spans x=3 through x=8.
    // At 2bpp that occupies the first three bytes of each included row;
    // the fourth byte is scanline padding and must remain outside the
    // snapshot.
    let rect = (0, 3, 1, 8);
    let saved = disp.save_dropdown_pixels(&bus, rect);
    assert_eq!(saved.len(), 2 * 3);

    for y in 0..2u32 {
        for byte_x in 0..3u32 {
            bus.write_byte(base + y * row_bytes + byte_x, 0xAA);
        }
    }
    disp.restore_dropdown_pixels(&mut bus, rect, &saved);

    assert_eq!(
        bus.read_bytes(base, (row_bytes * height) as usize),
        before,
        "2bpp save/restore should preserve boundary fields without touching row padding"
    );
}

#[test]
fn save_dropdown_pixels_handles_negative_top_without_overflow() {
    let (mut disp, _cpu, mut bus) = setup();
    let screen_base = bus.alloc((800 * 600) as u32);
    for i in 0..800u32 * 600 {
        bus.write_byte(screen_base + i, 0x77);
    }
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    // Rect straddling the screen top: top=-2 through bottom=4.
    let saved = disp.save_dropdown_pixels(&bus, (-2, 100, 4, 120));
    // save_right = 121; save_bottom = 5. Row width on-screen =
    // (121 - 100) = 21 bytes. Rows y=-2, -1 are skipped; rows
    // y=0..4 contribute 21 bytes each → 5 * 21 = 105 bytes.
    assert_eq!(saved.len(), 5 * 21);
    for &b in &saved {
        assert_eq!(b, 0x77, "on-screen bytes must come from framebuffer");
    }
}

#[test]
fn save_dropdown_pixels_handles_top_beyond_screen_height_without_overflow() {
    let (mut disp, _cpu, mut bus) = setup();
    let screen_base = bus.alloc((800 * 600) as u32);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    // Rect entirely below screen bottom.
    let saved = disp.save_dropdown_pixels(&bus, (600, 0, 610, 50));
    assert_eq!(
        saved.len(),
        0,
        "dropdown entirely off-screen must produce an empty buffer"
    );
}

#[test]
fn ploticon_pascal_procedure_preserves_stack_across_five_calls() {
    // IM:I 1985 p. I-473 (Toolbox Utilities — Routines That
    // Operate on Icons — PlotIcon): Pascal PROCEDURE PlotIcon
    // (theRect: Rect; theIcon: Handle). Each call pops 8 bytes
    // (theIcon at SP+0 + theRect ptr at SP+4) and writes no
    // FUNCTION result slot. This test dispatches 5 successive
    // PlotIcon calls each with distinct icon Handles and
    // destination Rects and asserts they net-balance A7.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);

    // Five distinct 128-byte icon allocations + handles to defeat
    // any stub that caches the last-seen Handle argument.
    let icons: [u32; 5] = std::array::from_fn(|_| {
        let icon_ptr = bus.alloc(128);
        let icon_handle = bus.alloc(4);
        bus.write_long(icon_handle, icon_ptr);
        icon_handle
    });

    // Five distinct 32x32 destination Rects at non-overlapping
    // positions; Rect record is 8 bytes (4 INTEGERs).
    let rects: [u32; 5] = std::array::from_fn(|i| {
        let rect_ptr = bus.alloc(8);
        bus.write_word(rect_ptr, 2);
        bus.write_word(rect_ptr + 2, (i as u16) * 40 + 2);
        bus.write_word(rect_ptr + 4, 34);
        bus.write_word(rect_ptr + 6, (i as u16) * 40 + 34);
        rect_ptr
    });

    for i in 0..5 {
        let sp = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A7, sp - 8);
        bus.write_long(sp - 8, icons[i]);
        bus.write_long(sp - 4, rects[i]);
        let result = disp.dispatch_menu(true, 0x14B, &mut cpu, &mut bus);
        assert!(
            result.unwrap().is_ok(),
            "PlotIcon dispatch should succeed (call {})",
            i
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "PlotIcon should pop 8 bytes per call (call {})",
            i
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "5 successive PlotIcon calls should net-balance A7"
    );
}

#[test]
fn ploticon_current_port_zero_short_circuits_before_touching_memory() {
    // The current-port-zero path is a defensive no-op. It should
    // pop the 8-byte Pascal frame without touching the icon or
    // rect pointers on the stack.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = cpu.read_reg(Register::A7);
    let warnings = Arc::new(AtomicUsize::new(0));
    let subscriber = WarnCounter {
        warnings: warnings.clone(),
    };

    cpu.write_reg(Register::A7, sp - 8);
    bus.write_long(sp - 8, 0x0050_0000);
    bus.write_long(sp - 4, 0x0060_0000);

    let _guard = tracing::subscriber::set_default(subscriber);
    let result = disp.dispatch_menu(true, 0x14B, &mut cpu, &mut bus);
    assert!(result.is_some(), "PlotIcon should be handled");
    assert!(
        result.unwrap().is_ok(),
        "PlotIcon should return cleanly when no current port is set"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp,
        "PlotIcon should still pop the 8-byte argument frame"
    );
    assert_eq!(
        warnings.load(Ordering::Relaxed),
        0,
        "PlotIcon should not touch icon/rect memory when current_port is NIL"
    );
}

#[test]
fn guest_menu_snapshot_exposes_only_the_inserted_menu_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let inserted = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 100, 0x306A80, "File");
    let inserted_ptr = bus.read_long(inserted);
    bus.write_bytes(inserted_ptr + 15, b"Gam\xC9");
    let item_ptr = 0x306A90;
    let item = b"New Level\xC9/N";
    bus.write_byte(item_ptr, item.len() as u8);
    bus.write_bytes(item_ptr + 1, item);
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, item_ptr);
    bus.write_long(TEST_SP + 4, inserted);
    disp.dispatch_menu(true, 0x133, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 0x0100);
    bus.write_word(TEST_SP + 2, 1);
    bus.write_long(TEST_SP + 4, inserted);
    disp.dispatch_menu(true, 0x145, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    insert_menu(&mut disp, &mut cpu, &mut bus, inserted);

    let system_menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 102, 0x306AA0, "\u{14}");
    insert_menu(&mut disp, &mut cpu, &mut bus, system_menu);

    let _detached = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 101, 0x306AB0, "Detached");

    let live_items = super::menu_items_from_memory(&bus, inserted).unwrap();
    assert_eq!(live_items.items[0].text, b"New Level\xC9");
    let internal_text = super::macroman_to_string(&live_items.items[0].text);
    assert_eq!(
        TrapDispatcher::fb_measure_string(&internal_text, 0, 12),
        crate::menu_manager::standard_menu_text_advance(&live_items.items[0].text),
        "menu measurement and drawing must resolve the same Mac Roman glyphs"
    );
    let title_bytes = bus.read_bytes(inserted_ptr + 15, 4);
    assert_eq!(title_bytes, b"Gam\xC9");
    assert_eq!(
        TrapDispatcher::fb_measure_string(&super::macroman_to_string(&title_bytes), 0, 12),
        crate::menu_manager::standard_menu_title_advance(&title_bytes),
        "menu-title measurement and drawing must resolve the same Mac Roman glyphs"
    );

    let snapshot = disp.guest_menu_snapshot(&bus);
    assert_eq!(snapshot.menus.len(), 2);
    assert_eq!(snapshot.menus[0].id, 100);
    assert_eq!(snapshot.menus[0].title, "Gam…");
    assert_eq!(snapshot.menus[0].items[0].number, 1);
    assert_eq!(snapshot.menus[0].items[0].text, "New Level…");
    assert_eq!(snapshot.menus[0].items[0].key_equivalent, Some('n'));
    assert!(snapshot.menus[0].items[0].checked);
    assert_eq!(snapshot.menus[1].title, "Systemless");
}

#[test]
fn native_selection_returns_through_menuselect_pascal_frame() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, -120, 0x306AC0, "Game");
    append_menu_data(&mut disp, &mut cpu, &mut bus, menu, 0x306AD0, "Pause");
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    assert!(disp.queue_native_menu_selection(&bus, -120, 1).is_some());
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP + 4, 0xDEAD_BEEF);
    let result = disp
        .step_menu_fixture(true, 0x13D, &mut cpu, &mut bus)
        .expect("MenuSelect handled");
    assert!(result.is_ok());
    assert_eq!(bus.read_long(TEST_SP + 4), 0xFF88_0001);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 4);
    assert!(disp.pending_native_menu_selection.is_none());
    assert!(disp.pending_native_menu_event.is_none());
    assert!(disp.pending_native_menu_event_tick.is_none());
}

#[test]
fn native_selection_rejects_disabled_and_hierarchical_parent_items() {
    let (mut disp, mut cpu, mut bus) = setup();
    let menu = new_menu_with_title(&mut disp, &mut cpu, &mut bus, 100, 0x306AE0, "File");
    append_menu_data(
        &mut disp,
        &mut cpu,
        &mut bus,
        menu,
        0x306AF0,
        "Disabled;Recent",
    );
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_word(TEST_SP, 1);
    bus.write_long(TEST_SP + 2, menu);
    disp.dispatch_menu(true, 0x13A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    set_item_cmd(&mut disp, &mut cpu, &mut bus, menu, 2, 0x1B);
    set_item_mark(&mut disp, &mut cpu, &mut bus, menu, 2, 7);
    insert_menu(&mut disp, &mut cpu, &mut bus, menu);

    assert_eq!(disp.queue_native_menu_selection(&bus, 100, 1), None);
    assert_eq!(disp.queue_native_menu_selection(&bus, 100, 2), None);
    assert_eq!(disp.queue_native_menu_selection(&bus, 999, 1), None);
}
