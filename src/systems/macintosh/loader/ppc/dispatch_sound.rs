use super::*;

pub(super) struct PpcSoundDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) current_resource_refnum: i16,
    pub(super) handles: &'a [PpcHandleRecord],
    pub(super) files: &'a [PpcFileRecord],
    pub(super) vfs_files: &'a ProcessVfsFileRecords,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) sound: &'a mut PpcSoundState,
}

pub(super) fn dispatch_sound_import(
    context: PpcSoundDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcSoundDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        current_resource_refnum,
        handles,
        files,
        vfs_files,
        vfs_resources,
        sound,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::SysBeep => {
            sound.sys_beep_count = sound.sys_beep_count.saturating_add(1);
            sound.last_sys_beep_duration = cpu.gpr[3] as u16 as i16;
            let volume = sound.manager.sys_beep_volume();
            sound.manager.play_sys_beep(volume);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SndSoundManagerVersion => {
            if cpu.gpr[3] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[3], 4) {
                let _ = memory.write_u32_be(cpu.gpr[3], PPC_SOUND_MANAGER_VERSION);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::UnsignedFixedMulDiv => {
            // Inside Macintosh: Sound (1994), pp. 2-151--2-152. Unsigned
            // 16.16 values keep the same binary scale through (a*b)/c, so a
            // 64-bit intermediate avoids losing precision or overflowing.
            let divisor = cpu.gpr[5];
            let result = if divisor == 0 {
                u32::MAX
            } else {
                ((u64::from(cpu.gpr[3]) * u64::from(cpu.gpr[4])) / u64::from(divisor))
                    .min(u64::from(u32::MAX)) as u32
            };
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::GetSoundOutputInfo => {
            // Sound Manager 3.1 GetSoundOutputInfo accepts NIL for the
            // default output device. Inside Macintosh: Sound (1994),
            // pp. 5-22--5-25 defines siSampleRate's UnsignedFixed result.
            const SI_SAMPLE_RATE: u32 = u32::from_be_bytes(*b"srat");
            let result = if cpu.gpr[4] == SI_SAMPLE_RATE
                && cpu.gpr[5] != 0
                && ppc_memory_can_write_bytes(memory, cpu.gpr[5], 4)
            {
                let _ = memory.write_u32_be(cpu.gpr[5], crate::sound::OUTPUT_RATE << 16);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::GetSoundVol => {
            // Inside Macintosh Volume II (1985), pp. II-232--II-233:
            // GetSoundVol returns the low three bits of SdVolume as an Integer.
            if cpu.gpr[3] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[3], 2) {
                let level = memory
                    .read_u8(crate::memory::globals::addr::SD_VOLUME)
                    .unwrap_or(1)
                    & 7;
                let _ = memory.write_u16_be(cpu.gpr[3], u16::from(level));
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetSoundVol => {
            // SetSoundVol updates the actual speaker level without changing
            // the user's saved Control Panel preference.
            let level = (cpu.gpr[3] as u8).min(7);
            let _ = memory.write_u8(crate::memory::globals::addr::SD_VOLUME, level);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDefaultOutputVolume => {
            let volume_out_ptr = cpu.gpr[3];
            if volume_out_ptr == 0
                || !ppc_memory_can_write_bytes(memory, volume_out_ptr, 4)
                || memory
                    .write_u32_be(volume_out_ptr, sound.manager.default_output_volume())
                    .is_none()
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::SetDefaultOutputVolume => {
            sound.manager.set_default_output_volume(cpu.gpr[3]);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::SndNewChannel => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_new_channel(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                sound,
            ),
        ))),
        PpcImportDispatcherTarget::SndDisposeChannel => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_dispose_channel(cpu, sound)),
        )),
        PpcImportDispatcherTarget::SndPlay => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_play(cpu, memory, handles, sound),
        ))),
        PpcImportDispatcherTarget::SndChannelStatus => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_channel_status(cpu, memory, sound)),
        )),
        PpcImportDispatcherTarget::SndGetInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_get_info(cpu, memory, sound),
        ))),
        PpcImportDispatcherTarget::SndSetInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_set_info(cpu, memory, sound),
        ))),
        PpcImportDispatcherTarget::ParseSndHeader => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_parse_snd_header(cpu, memory, handles),
        ))),
        PpcImportDispatcherTarget::SndDoCommand => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_do_command(cpu, memory, sound),
        ))),
        PpcImportDispatcherTarget::SndDoImmediate => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_snd_do_immediate(cpu, memory, sound),
        ))),
        PpcImportDispatcherTarget::SndPlayDoubleBuffer => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_play_double_buffer(cpu, memory, sound)),
        )),
        PpcImportDispatcherTarget::SndStartFilePlay => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_start_file_play(
                cpu,
                files,
                vfs_files,
                vfs_resources,
                current_resource_refnum,
                sound,
            )),
        )),
        PpcImportDispatcherTarget::SndPauseFilePlay => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_pause_file_play(cpu, sound)),
        )),
        PpcImportDispatcherTarget::SndStopFilePlay => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_snd_stop_file_play(cpu, sound)),
        )),
        PpcImportDispatcherTarget::GetSoundHeaderOffset => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_get_sound_header_offset(cpu, memory)),
        )),
        PpcImportDispatcherTarget::SoundInputCompatibility(operation) => {
            Some(ppc_dispatch_sound_input_compatibility(operation, cpu, memory))
        }
        _ => None,
    }
}
