//! Typed Menu Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcMenuDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) last_resource_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) current_resource_refnum: i16,
    pub(super) resource_policy: &'a SharedProcessResourcePolicy,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) current_menu_list: &'a mut u32,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) current_gworld: &'a mut u32,
    pub(super) current_gdevice: &'a mut u32,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) input: PpcInputSnapshot,
}

pub(super) fn dispatch_menu_import(context: PpcMenuDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcMenuDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        last_resource_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        resource_policy,
        toolbox_startup,
        current_menu_list,
        gworlds,
        screen_clut,
        current_gworld,
        current_gdevice,
        event_queue,
        input,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitMenus => {
            toolbox_startup.menus_initialized = true;
            let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            // Inside Macintosh Volume V (1986), pp. V-228--V-230: the first
            // InitMenus call allocates the stable DynamicMenuList whose handle
            // is published in the MenuList low-memory global. Later calls do
            // not replace that manager-owned handle.
            if *current_menu_list == 0 {
                *current_menu_list = ppc_alloc_menu_list_handle_with_allocator(
                    &[],
                    Some(&mut allocator),
                    None,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                *last_mem_error = if *current_menu_list == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    ppc_set_current_menu_list(memory, *current_menu_list);
                    PPC_NO_ERR
                };
            } else {
                // Macintosh Toolbox Essentials (1992), pp. 3-103--3-104:
                // InitMenus restores the standard MBDF and an empty menu list.
                // Reinitialization reuses the existing MenuList handle.
                let result = ppc_replace_menu_list_definition_with_allocator(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    *current_menu_list,
                    &PpcMenuListDefinition::default(),
                );
                *last_mem_error = result;
            }
            // InitMenus creates the process MenuCInfo table and automatically
            // adds entries from the current resource chain's `'mctb'` 0.
            // Inside Macintosh Volume V (1986), pp. V-242--V-244; Macintosh
            // Toolbox Essentials (1992), pp. 3-154--3-156.
            let _ = ppc_ensure_menu_color_table_handle_with_allocator(
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            ppc_load_menu_color_resource_with_allocator(
                0,
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
            );
            if *current_menu_list != 0 && !toolbox_startup.host_menu_bar_hidden {
                let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
                let _ = ppc_draw_menu_bar_with_colors(
                    memory,
                    gworlds,
                    *current_menu_list,
                    screen_clut,
                    MenuColorTable::new(&menu_color_bytes),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NewMenu => {
            let menu_proc = ppc_menu_definition_handle(
                0,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
            );
            let menu = if menu_proc == 0 {
                0
            } else {
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                ppc_alloc_new_menu(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    menu_proc,
                    cpu.gpr[3] as u16 as i16,
                    cpu.gpr[4],
                )
            };
            *last_mem_error = if menu == 0 {
                PPC_MEM_FULL_ERR
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(menu))
        }
        PpcImportDispatcherTarget::DisposeMenu => {
            let menu_handle = cpu.gpr[3];
            for resource in vfs_resources
                .iter_mut()
                .filter(|resource| resource.handle == menu_handle)
            {
                resource.handle = 0;
            }
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let _ = allocator.dispose_handle(
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                menu_handle,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetMenu => {
            let menu_handle = ppc_load_menu_resource(
                cpu.gpr[3] as u16 as i16,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
            );
            if menu_handle != 0 {
                // A custom definition owns the initial dimensions of a newly
                // created MenuRecord. Macintosh Toolbox Essentials (1992),
                // pp. 3-148--3-151.
                if let Some(action) = ppc_dispatch_native_menu_definition_with_return(
                    cpu,
                    Some(process_memory_manager),
                    memory,
                    heap_cursor,
                    heap_limit,
                    vfs_resources,
                    toolbox_startup,
                    MenuDefinitionInvocation::size(menu_handle),
                    cpu.lr,
                    PpcNativeReturnGpr3::Set(menu_handle),
                ) {
                    return Some(action);
                }
                ppc_calc_menu_size_with_resources(
                    memory,
                    menu_handle,
                    vfs_resources,
                    current_resource_refnum,
                );
            }
            Some(PpcImportAction::Return(menu_handle))
        }
        PpcImportDispatcherTarget::GetItemCmd => {
            ppc_get_item_cmd(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetItemCmd => {
            if crate::trap::dispatch::trace_input_enabled() {
                eprintln!(
                    "[INPUT] PPC SetItemCmd menu=${:08X} item={} command=${:02X}",
                    cpu.gpr[3], cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u8
                );
            }
            ppc_set_item_cmd(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetItemMark => {
            ppc_get_item_mark(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CountMItems => Some(PpcImportAction::Return(u32::from(
            ppc_count_menu_items(memory, cpu.gpr[3]),
        ))),
        PpcImportDispatcherTarget::GetMenuItemText => {
            ppc_get_menu_item_text(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetMenuItemText => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_set_menu_item_text_with_allocator(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DeleteMenuItem => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_delete_menu_item_with_allocator(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CalcMenuSize => Some(ppc_dispatch_calc_menu_size(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            vfs_resources,
            current_resource_refnum,
            toolbox_startup,
        )),
        PpcImportDispatcherTarget::PopUpMenuSelect => ppc_step_menu_tracking(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            gworlds,
            screen_clut,
            toolbox_startup,
            current_gworld,
            current_gdevice,
            input,
            vfs_resources,
            current_resource_refnum,
        ),
        PpcImportDispatcherTarget::InsertMenu => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_insert_menu_with_allocator(
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DeleteMenu => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_delete_menu_with_allocator(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                *current_menu_list,
                cpu.gpr[3] as u16 as i16,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AppendMenu => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_insert_menu_items_with_allocator(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                i16::MAX,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InsertMenuItem => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_insert_menu_items_with_allocator(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5] as u16 as i16,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AppendResMenu => {
            // AppendResMenu
            // Appends the alphabetized names of matching resources.
            // PROCEDURE AppendResMenu(theMenu: MenuHandle; theType: ResType);
            // Macintosh Toolbox Essentials (1992), pp. 3-101--3-102.
            let result = ppc_insert_resource_menu(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                resource_policy,
                last_resource_error,
                cpu.gpr[3],
                cpu.gpr[4],
                i16::MAX,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InsertResMenu => {
            // InsertResMenu
            // Inserts alphabetized matching resource names after one item.
            // PROCEDURE InsertResMenu(theMenu: MenuHandle; theType: ResType;
            //                         afterItem: Integer);
            // Macintosh Toolbox Essentials (1992), pp. 3-103--3-104.
            let result = ppc_insert_resource_menu(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                resource_policy,
                last_resource_error,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5] as u16 as i16,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EnableMenuItem => {
            ppc_set_menu_item_enabled(memory, handles, cpu.gpr[3], cpu.gpr[4] as u16 as i16, true);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DisableMenuItem => {
            ppc_set_menu_item_enabled(memory, handles, cpu.gpr[3], cpu.gpr[4] as u16 as i16, false);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetItemMark => {
            ppc_set_item_mark(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CheckItem => {
            ppc_check_menu_item(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetMenuBar => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            Some(PpcImportAction::Return(ppc_get_menu_bar_with_allocator(
                *current_menu_list,
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            )))
        }
        PpcImportDispatcherTarget::GetNewMBar => {
            let result_handle = ppc_get_new_mbar(
                cpu.gpr[3] as u16 as i16,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
            );
            if result_handle == 0 {
                return Some(PpcImportAction::Return(0));
            }
            let menu_handles = ppc_menu_list_definition(memory, result_handle)
                .map(|menu_list| menu_list.handles().collect())
                .unwrap_or_default();
            if toolbox_startup
                .execution
                .calls()
                .begin_menu_bar_build(
                    MenuBarBuild::new(result_handle, menu_handles),
                    MenuBarCallOrigin::PowerPc {
                        return_address: cpu.lr,
                    },
                )
                .is_none()
            {
                return Some(PpcImportAction::Return(0));
            }
            Some(ppc_continue_menu_bar_build(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                vfs_resources,
                current_resource_refnum,
            ))
        }
        PpcImportDispatcherTarget::ClearMenuBar => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            // Macintosh Toolbox Essentials (1992), p. 3-110: ClearMenuBar
            // removes every menu from the current list without disposing the
            // menu records themselves. Preserve the current list Handle when
            // one exists, matching the manager-owned MenuList identity.
            if *current_menu_list != 0 {
                let mut menu_list =
                    ppc_menu_list_definition(memory, *current_menu_list).unwrap_or_default();
                menu_list.clear_entries();
                let result = ppc_replace_menu_list_definition_with_allocator(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    *current_menu_list,
                    &menu_list,
                );
                *last_mem_error = result;
                if *last_mem_error == PPC_NO_ERR {
                    let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
                }
            }
            // ClearMenuBar also deletes every entry from the application
            // MenuCInfo table without disposing its stable handle. Macintosh
            // Toolbox Essentials (1992), p. 3-110.
            ppc_clear_menu_color_table_with_allocator(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetMenuBar => {
            let source = cpu.gpr[3];
            let Some(menu_list) = ppc_menu_list_definition(memory, source) else {
                *last_mem_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::ReturnPreserve);
            };
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let installation =
                install_menu_list_copy(*current_menu_list, &menu_list, |request| match request {
                    MenuListInstallRequest::Allocate { bytes } => {
                        let handle = allocator.allocate_handle_with_bytes(
                            memory,
                            heap_cursor,
                            last_mem_error,
                            handles,
                            bytes,
                        );
                        (handle != 0).then_some(handle).ok_or(PPC_MEM_FULL_ERR)
                    }
                    MenuListInstallRequest::Replace { handle, bytes } => {
                        let result = ppc_replace_menu_bytes_with_allocator(
                            Some(&mut allocator),
                            memory,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            handles,
                            handle,
                            bytes,
                        );
                        (result == PPC_NO_ERR).then_some(handle).ok_or(result)
                    }
                });
            match installation {
                Ok(installation) => {
                    *current_menu_list = installation.handle;
                    if installation.allocated {
                        ppc_set_current_menu_list(memory, *current_menu_list);
                    }
                    *last_mem_error = PPC_NO_ERR;
                }
                Err(error) => *last_mem_error = error,
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetMenuHandle => Some(PpcImportAction::Return(
            ppc_get_menu_handle(memory, *current_menu_list, cpu.gpr[3] as u16 as i16),
        )),
        PpcImportDispatcherTarget::DrawMenuBar => {
            // An explicit draw satisfies any earlier deferred request.
            event_queue.take_menu_bar_invalidation();
            toolbox_startup.menu_bar_draw_count =
                toolbox_startup.menu_bar_draw_count.saturating_add(1);
            if !toolbox_startup.host_menu_bar_hidden {
                let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
                let _ = ppc_draw_menu_bar_with_colors(
                    memory,
                    gworlds,
                    *current_menu_list,
                    screen_clut,
                    MenuColorTable::new(&menu_color_bytes),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InvalMenuBar => {
            // InvalMenuBar
            // Marks the menu bar for one redraw during the next Toolbox
            // Event Manager scan; repeated calls coalesce.
            // PROCEDURE InvalMenuBar;
            // Macintosh Toolbox Essentials (1992), pp. 3-93 and 3-114.
            event_queue.invalidate_menu_bar();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FlashMenuBar => {
            // FlashMenuBar
            // Inverts the requested regular menu title, or the entire menu
            // bar when the ID is zero or does not identify a regular title.
            // PROCEDURE FlashMenuBar (menuID: INTEGER);
            // Macintosh Toolbox Essentials (1992), pp. 3-141--3-142.
            let requested_menu_id = cpu.gpr[3] as u16 as i16;
            let requested_is_regular = requested_menu_id != 0
                && ppc_regular_menu_contains_id(memory, *current_menu_list, requested_menu_id);
            let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
            let menu_colors = MenuColorTable::new(&menu_color_bytes);
            if requested_is_regular {
                let selected_menu_id = memory.read_u16_be(PPC_THE_MENU_ADDR).unwrap_or(0) as i16;
                let selected_root_menu_id =
                    ppc_root_menu_id_for_selection(memory, *current_menu_list, selected_menu_id);
                ppc_set_menu_title_highlight_with_colors(
                    memory,
                    gworlds,
                    *current_menu_list,
                    if selected_root_menu_id == requested_menu_id {
                        0
                    } else {
                        requested_menu_id
                    },
                    screen_clut,
                    menu_colors,
                    toolbox_startup.host_menu_bar_hidden,
                );
            } else if !toolbox_startup.host_menu_bar_hidden {
                let _ = ppc_flash_entire_menu_bar_with_colors(
                    memory,
                    gworlds,
                    screen_clut,
                    menu_colors,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HMGetHelpMenuHandle => {
            // Systemless does not expose Balloon Help. Match the established
            // 68k Pack14 fallback: clear the output MenuHandle and report that
            // the Help Manager has not been initialized.
            if cpu.gpr[3] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[3], 0);
            }
            Some(PpcImportAction::Return(ppc_i16_result(
                PPC_HM_HELP_MANAGER_NOT_INITED,
            )))
        }
        PpcImportDispatcherTarget::HMGetBalloons => {
            // HMGetBalloons returns whether Balloon Help is enabled.
            // Inside Macintosh VI (1991), chapter 11, pp. 11-65–11-66.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::HiliteMenu => {
            // HiliteMenu first restores the currently highlighted title, then
            // highlights the requested title; zero or an unknown menu ID
            // leaves every title normal. Macintosh Toolbox Essentials
            // (1992), p. 3-119.
            let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
            ppc_set_menu_title_highlight_with_colors(
                memory,
                gworlds,
                *current_menu_list,
                cpu.gpr[3] as u16 as i16,
                screen_clut,
                MenuColorTable::new(&menu_color_bytes),
                toolbox_startup.host_menu_bar_hidden,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MenuNoop => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::MenuKey => {
            let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
            let menu_colors = MenuColorTable::new(&menu_color_bytes);
            let selection = ppc_menu_key(memory, *current_menu_list, cpu.gpr[3] as u8);
            let result = selection.map_or(0, MenuKeySelection::packed_result);
            let root_menu_id = selection
                .and_then(|selection| selection.owner_handle)
                .and_then(|owner_handle| memory.read_u32_be(owner_handle))
                .filter(|menu| *menu != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .map_or(0, |menu_id| menu_id as i16);
            ppc_set_menu_command_highlight_with_colors(
                memory,
                gworlds,
                *current_menu_list,
                result,
                Some(root_menu_id),
                screen_clut,
                menu_colors,
                toolbox_startup.host_menu_bar_hidden,
            );
            if crate::trap::dispatch::trace_input_enabled() {
                eprintln!(
                    "[INPUT] PPC MenuKey menu_list=${:08X} key=${:02X} -> ${result:08X}",
                    *current_menu_list, cpu.gpr[3] as u8
                );
            }
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::MenuEvent => {
            // Menus.h: MenuEvent examines a classic EventRecord and returns
            // the MenuKey-style packed result for command-key keyboard
            // events, or zero when the event has no menu equivalent.
            let event = cpu.gpr[3];
            let what = memory.read_u16_be(event).unwrap_or(0);
            let message = memory.read_u32_be(event + 2).unwrap_or(0);
            let modifiers = memory.read_u16_be(event + 14).unwrap_or(0);
            let result = if matches!(what, 3 | 5) && modifiers & 0x0100 != 0 {
                ppc_menu_key(memory, *current_menu_list, message as u8)
                    .map_or(0, MenuKeySelection::packed_result)
            } else {
                0
            };
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::MenuChoice => {
            // MenuChoice is a parameterless C function that returns the
            // standard MDEF's packed MenuDisable low-memory value unchanged.
            // Macintosh Toolbox Essentials (1992), pp. 3-118--3-119.
            Some(PpcImportAction::Return(
                memory
                    .read_u32_be(crate::memory::globals::addr::MENU_DISABLE)
                    .unwrap_or(0),
            ))
        }
        PpcImportDispatcherTarget::GetMBarHeight => Some(PpcImportAction::Return(u32::from(
            memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20),
        ))),
        PpcImportDispatcherTarget::SetMBarHeight => {
            let _ = memory.write_u16_be(PPC_MBAR_HEIGHT_ADDR, cpu.gpr[3] as u16);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MenuSelect => ppc_step_menu_tracking(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            gworlds,
            screen_clut,
            toolbox_startup,
            current_gworld,
            current_gdevice,
            input,
            vfs_resources,
            current_resource_refnum,
        ),
        _ => None,
    }
}
