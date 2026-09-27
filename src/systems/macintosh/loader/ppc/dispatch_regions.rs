//! Typed QuickDraw Region Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcRegionDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) current_gworld: u32,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_region_import(
    context: PpcRegionDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcRegionDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        current_gworld,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::ClipRect => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_clip_rect(
                cpu,
                Some(&mut allocator),
                memory,
                current_gworld,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetClip | PpcImportDispatcherTarget::SetClip => {
            let clip_rgn = memory
                .read_u32_be(current_gworld.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
                .unwrap_or(0);
            let (source, destination) = if matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetClip
            ) {
                (clip_rgn, cpu.gpr[3])
            } else {
                (cpu.gpr[3], clip_rgn)
            };
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_copy_rgn(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                source,
                destination,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::BitMapToRegion => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_bitmap_to_region(
                    cpu,
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                ),
            )))
        }
        PpcImportDispatcherTarget::NewRgn => Some(PpcImportAction::Return(ppc_process_new_rgn(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
        ))),
        PpcImportDispatcherTarget::DisposeRgn => {
            let _ = ppc_dispose_process_native_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CopyRgn => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            *last_mem_error = ppc_copy_rgn(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpenRgn => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_open_rgn(
                Some(&mut allocator),
                memory,
                current_gworld,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                None,
                toolbox_startup,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CloseRgn => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_close_rgn(
                Some(&mut allocator),
                memory,
                cpu.gpr[3],
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                None,
                toolbox_startup,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SectRgn
        | PpcImportDispatcherTarget::UnionRgn
        | PpcImportDispatcherTarget::DiffRgn
        | PpcImportDispatcherTarget::XorRgn => {
            let operation = match binding.dispatcher_target {
                PpcImportDispatcherTarget::SectRgn => PpcRegionBooleanOp::Intersection,
                PpcImportDispatcherTarget::UnionRgn => PpcRegionBooleanOp::Union,
                PpcImportDispatcherTarget::DiffRgn => PpcRegionBooleanOp::Difference,
                PpcImportDispatcherTarget::XorRgn => PpcRegionBooleanOp::Xor,
                _ => unreachable!(),
            };
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            *last_mem_error = ppc_region_boolean_op(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                operation,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetEmptyRgn => {
            let _ = ppc_set_empty_rgn(memory, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetRectRgn => {
            let _ = ppc_set_rect_rgn(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                cpu.gpr[5] as u16 as i16,
                cpu.gpr[6] as u16 as i16,
                cpu.gpr[7] as u16 as i16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RectRgn => {
            let _ = ppc_rect_rgn(memory, cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OffsetRgn => {
            *last_mem_error = ppc_offset_rgn(
                memory,
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                cpu.gpr[5] as u16 as i16,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EmptyRgn => Some(PpcImportAction::Return(
            if ppc_empty_rgn(memory, cpu.gpr[3]) {
                1
            } else {
                0
            },
        )),
        PpcImportDispatcherTarget::PtInRgn => {
            let v = (cpu.gpr[3] >> 16) as u16 as i16;
            let h = cpu.gpr[3] as u16 as i16;
            Some(PpcImportAction::Return(u32::from(ppc_point_in_region(
                memory, cpu.gpr[4], v, h,
            ))))
        }
        PpcImportDispatcherTarget::RectInRgn => Some(PpcImportAction::Return(u32::from(
            ppc_rect_in_region(memory, cpu.gpr[3], cpu.gpr[4]),
        ))),
        _ => None,
    }
}
