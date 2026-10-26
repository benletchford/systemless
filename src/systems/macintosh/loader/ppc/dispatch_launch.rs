//! Deferred native Process Manager application launches.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcPendingLaunch {
    pub(crate) path: String,
    pub(crate) ready: bool,
    wait_for_exit: bool,
}

pub(super) fn dispatch(context: &mut PpcDispatchContext<'_>) -> Option<PpcImportAction> {
    let target = context.binding.dispatcher_target.clone();
    if let Some(pending) = context.toolbox_startup.pending_launch.as_mut() {
        let exiting = matches!(target, PpcImportDispatcherTarget::ExitToShell);
        let yielding = matches!(
            target,
            PpcImportDispatcherTarget::GetNextEvent(_)
                | PpcImportDispatcherTarget::GetOSEvent
                | PpcImportDispatcherTarget::RunApplicationEventLoop
        );
        if pending.ready || exiting || (yielding && !pending.wait_for_exit) {
            pending.ready = true;
            return Some(PpcImportAction::Yield(0));
        }
    }
    if !matches!(
        target,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LaunchApplication
        )
    ) {
        return None;
    }
    let memory = &mut *context.memory;
    let pb = context.cpu.gpr[3];
    if pb == 0
        || memory.readable_bytes(pb, 44).is_none()
        || memory.read_u16_be(pb.wrapping_add(6)) != Some(0x4c43)
        || memory
            .read_u32_be(pb.wrapping_add(8))
            .is_none_or(|length| length < 32)
    {
        return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
    }
    let flags = memory.read_u16_be(pb.wrapping_add(14)).unwrap_or(0);
    let spec = memory.read_u32_be(pb.wrapping_add(16)).unwrap_or(0);
    let path = files::ppc_path_for_fsspec(memory, context.vfs_directories, spec);
    let result = path.and_then(|path| {
        if !context
            .vfs_resource_files
            .iter()
            .any(|record| record.path.eq_ignore_ascii_case(&path))
        {
            return Err(PPC_FNF_ERR);
        }
        Ok(path)
    });
    let path = match result {
        Ok(path) => path,
        Err(error) => {
            for offset in [20, 24, 28, 32, 36] {
                let _ = memory.write_u32_be(pb.wrapping_add(offset), 0);
            }
            return Some(PpcImportAction::Return(ppc_i16_result(error)));
        }
    };
    // Inside Macintosh: Processes (1994), pp. 2-9 and 2-34: return
    // the target SIZE values and process identity; available size is only
    // meaningful for memFullErr. The runner owns one foreground process slot.
    let size = [0, -1].into_iter().find_map(|id| {
        context
            .vfs_resources
            .iter()
            .find(|resource| {
                resource.path.eq_ignore_ascii_case(&path)
                    && resource.res_type == u32::from_be_bytes(*b"SIZE")
                    && resource.res_id == id
            })
            .and_then(|resource| ApplicationSizeResource::parse(&resource.data))
    });
    let psn = crate::process_manager::ProcessSerialNumber::CURRENT;
    let _ = memory.write_u32_be(pb + 20, psn.high);
    let _ = memory.write_u32_be(pb + 24, psn.low);
    let _ = memory.write_u32_be(pb + 28, size.map_or(0, |size| size.preferred_size));
    let _ = memory.write_u32_be(pb + 32, size.map_or(0, |size| size.minimum_size));
    if flags & 0x0800 != 0 {
        if let Some(index) = ppc_vfs_resource_file_index(context.vfs_resource_files, &path) {
            let _ = memory.write_u16_be(pb + 12, context.vfs_resource_files[index].finder_flags);
        }
    }
    let wait_for_exit = flags & 0x0200 != 0;
    let ready = flags & 0x4000 == 0 && !wait_for_exit;
    context.toolbox_startup.pending_launch = Some(PpcPendingLaunch {
        path,
        ready,
        wait_for_exit,
    });
    Some(if ready {
        PpcImportAction::Yield(0)
    } else {
        PpcImportAction::Return(0)
    })
}
