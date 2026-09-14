use super::*;
use crate::sound::PendingSoundCallback;

    #[test]
    fn hle_import_runner_handles_sound_manager_version() {
        let pef = synthetic_pef_with_import(b"SndSoundManagerVersion");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(PPC_HEAP_BASE, vec![0; 16]);
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
        assert_eq!(
            loaded.memory.read_u32_be(PPC_HEAP_BASE),
            Some(PPC_SOUND_MANAGER_VERSION)
        );
    }

    #[test]
    fn hle_import_runner_handles_unsigned_fixed_mul_div() {
        let pef = synthetic_pef_with_import(b"UnsignedFixedMulDiv");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.imports[0].library_name = "SoundLib".to_string();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(
            &loaded.imports[0].library_name,
            &loaded.imports[0].symbol_name,
        );
        loaded.cpu.gpr[3] = 0x0001_0000;
        loaded.cpu.gpr[4] = 0x5622_0000;
        loaded.cpu.gpr[5] = 0x5622_0000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x0001_0000);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = u32::MAX;
        loaded.cpu.gpr[4] = u32::MAX;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], u32::MAX);
    }

    #[test]
    fn hle_import_runner_gets_default_sound_output_sample_rate() {
        let pef = synthetic_pef_with_library_import(b"SoundLib", b"GetSoundOutputInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let rate_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(rate_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"srat");
        loaded.cpu.gpr[5] = rate_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(rate_ptr),
            Some(crate::sound::OUTPUT_RATE << 16)
        );
    }

    #[test]
    fn hle_import_runner_handles_get_default_output_volume() {
        let pef = synthetic_pef_with_import(b"GetDefaultOutputVolume");
        let mut loaded = load_pef_application(&pef).unwrap();
        let volume_out_ptr = PPC_HEAP_BASE;
        loaded
            .sound
            .manager
            .set_default_output_volume(0x0000_8000);
        loaded.memory.add_region(volume_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = volume_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(volume_out_ptr), Some(0x0000_8000));
    }

    #[test]
    fn hle_import_runner_handles_set_default_output_volume() {
        let pef = synthetic_pef_with_import(b"SetDefaultOutputVolume");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0x0000_4000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.sound.manager.default_output_volume(),
            0x0000_4000
        );
    }

    #[test]
    fn hle_import_runner_records_sys_beep_and_queues_pcm() {
        let pef = synthetic_pef_with_import(b"SysBeep");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0x0000_003c;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x0000_003c);
        assert_eq!(loaded.sound.sys_beep_count, 1);
        assert_eq!(loaded.sound.last_sys_beep_duration, 60);
        assert!(loaded.sound.immediate_commands.is_empty());
        assert!(loaded.sound.file_playbacks.is_empty());
        assert!(loaded.sound.decoded_file_playbacks.is_empty());
        assert!(loaded.sound.manager.pending_sound_callbacks.is_empty());
        assert!(loaded.sound.completion_invocations.is_empty());
        assert_eq!(loaded.sound.manager.channels.len(), 1);
        let beep_audio = loaded
            .sound
            .manager
            .mix_frame(crate::sound::OUTPUT_RATE as usize);
        assert!(
            beep_audio.iter().any(|sample| *sample != 0x80),
            "native SysBeep must enqueue non-silent PCM"
        );
        let beep_channel = loaded.sound.manager.channels[0].guest_ptr;
        assert_ne!(beep_channel, 0);
        assert!(loaded
            .sound
            .manager
            .idle_auto_dispose_channel_ptrs()
            .contains(&beep_channel));
        loaded.sound.manager.remove_channel(beep_channel);
        assert!(loaded.sound.manager.channels.is_empty());

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0x0000_ffff;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x0000_ffff);
        assert_eq!(loaded.sound.sys_beep_count, 2);
        assert_eq!(loaded.sound.last_sys_beep_duration, -1);
        assert_eq!(loaded.sound.start_count, 0);
        assert_eq!(loaded.sound.pause_count, 0);
        assert_eq!(loaded.sound.stop_count, 0);
    }

    #[test]
    fn hle_import_runner_handles_snd_new_channel() {
        let pef = synthetic_pef_with_import(b"SndNewChannel");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(channel_out_ptr, vec![0; 4]);
        loaded.cpu.gpr[3] = channel_out_ptr;
        loaded.cpu.gpr[4] = 5; // sampledSynth
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0x0123_4567;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let channel = loaded.memory.read_u32_be(channel_out_ptr).unwrap();
        assert_ne!(channel, 0);
        assert_eq!(loaded.memory.read_u32_be(channel), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 4), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 8), Some(0x0123_4567));
        assert_eq!(loaded.memory.read_u32_be(channel + 12), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 16), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 20), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 24), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 28), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 30), Some(128));
        assert_eq!(loaded.memory.read_u16_be(channel + 32), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 34), Some(0));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    }

    #[test]
    fn queued_callback_command_schedules_the_async_snd_play_completion() {
        let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
        let channel = PPC_DATA_BASE + 0x1000;
        let sound_handle = PPC_DATA_BASE + 0x1100;
        let sound_ptr = PPC_DATA_BASE + 0x1200;
        let command_ptr = PPC_DATA_BASE + 0x1300;
        let completion = PPC_CODE_BASE + 0x80;
        let samples = [0x80, 0x90, 0x70, 0x80];
        let mut resource = vec![0; 40];
        write_u16(&mut resource, 0, 2);
        write_u16(&mut resource, 4, 1);
        write_u16(&mut resource, 6, 0x8050);
        write_u32(&mut resource, 10, 14);
        write_u32(&mut resource, 18, samples.len() as u32);
        write_u32(&mut resource, 22, crate::sound::OUTPUT_RATE << 16);
        resource[34] = 0;
        resource[35] = 60;
        resource[36..40].copy_from_slice(&samples);
        loaded
            .memory
            .add_region(channel, vec![0; PPC_GUEST_SND_CHANNEL_SIZE as usize]);
        loaded.memory.add_region(sound_handle, vec![0; 4]);
        loaded.memory.add_region(sound_ptr, resource.clone());
        loaded.memory.add_region(command_ptr, vec![0; 8]);
        loaded.memory.write_u32_be(channel + 8, completion).unwrap();
        loaded.sound.manager.register_channel(
            channel,
            false,
            completion,
            CallbackTaskArchitecture::PowerPc,
        );
        loaded.memory.write_u32_be(sound_handle, sound_ptr).unwrap();
        test_handles!(loaded).push(PpcHandleRecord {
            handle: sound_handle,
            ptr: sound_ptr,
            size: resource.len() as u32,
            capacity: resource.len() as u32,
        });
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = sound_handle;
        loaded.cpu.gpr[5] = 1;

        assert_eq!(
            ppc_snd_play(
                &loaded.cpu,
                &mut loaded.memory,
                &test_handle_records!(loaded),
                &mut loaded.sound,
            ),
            PPC_NO_ERR
        );
        assert!(loaded.sound.file_playbacks.is_empty());
        assert_eq!(loaded.sound.manager.debug_buffer_cmd_count, 1);
        assert_eq!(loaded.sound.manager.debug_file_play_count, 0);

        loaded.memory.write_u16_be(command_ptr, 13).unwrap();
        loaded.memory.write_u16_be(command_ptr + 2, 0x5348).unwrap();
        loaded
            .memory
            .write_u32_be(command_ptr + 4, 0x0200_0000)
            .unwrap();
        loaded.cpu.gpr[4] = command_ptr;
        loaded.cpu.gpr[5] = 1;

        assert_eq!(
            ppc_snd_do_command(&loaded.cpu, &mut loaded.memory, &mut loaded.sound),
            PPC_NO_ERR
        );
        let command = PpcSndCommandRecord {
            channel,
            command: 13,
            param1: 0x5348,
            param2: 0x0200_0000,
        };
        assert_eq!(loaded.sound.queued_commands, vec![command]);

        // The command FIFO is serviced after the short buffer exhausts. It
        // must retain the native callback ABI and command record for the
        // PPC runner, rather than pretending SndPlay was a file playback.
        loaded.sound.manager.mix_frame(8);
        loaded.sound.manager.mix_frame(1);
        assert_eq!(loaded.sound.manager.pending_sound_callbacks.len(), 1);
        match &loaded.sound.manager.pending_sound_callbacks[0] {
            PendingSoundCallback::Command {
                architecture,
                callback_addr,
                chan_ptr,
                cmd,
            } => {
                assert_eq!(*architecture, CallbackTaskArchitecture::PowerPc);
                assert_eq!(*callback_addr, completion);
                assert_eq!(*chan_ptr, channel);
                assert_eq!(
                    PpcSndCommandRecord {
                        channel: *chan_ptr,
                        command: cmd.cmd,
                        param1: cmd.param1,
                        param2: cmd.param2,
                    },
                    command
                );
            }
            other => panic!("expected native PPC command callback, got {other:?}"),
        }
    }

    #[test]
    fn sndplay_preserves_multiple_resource_commands_and_fires_callback_once() {
        let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
        let channel = PPC_DATA_BASE + 0x1800;
        let sound_handle = PPC_DATA_BASE + 0x1900;
        let sound_ptr = PPC_DATA_BASE + 0x1a00;
        let callback = PPC_CODE_BASE + 0x180;
        let mut resource = vec![0; 100];
        let commands_end = 38usize;
        let header_one = commands_end;
        let header_two = 64usize;
        write_u16(&mut resource, 0, 2); // format 2
        write_u16(&mut resource, 4, 4); // four commands
        write_u16(&mut resource, 6, 0x8050); // first soundCmd -> bufferCmd
        write_u32(&mut resource, 10, header_one as u32);
        write_u16(&mut resource, 14, crate::sound::cmd::VOLUME);
        write_u32(&mut resource, 18, 0x0040_0040);
        write_u16(&mut resource, 22, 0x8051); // second bufferCmd
        write_u32(&mut resource, 26, header_two as u32);
        write_u16(&mut resource, 30, crate::sound::cmd::CALLBACK);
        write_u16(&mut resource, 32, 0x1234);
        write_u32(&mut resource, 34, 0x5678_9abc);

        for (header, samples) in [(header_one, [0xa0, 0xa0, 0xa0, 0xa0]),
            (header_two, [0xa0, 0xa0, 0xa0, 0xa0])]
        {
            write_u32(&mut resource, header, 0); // inline sample area
            write_u32(&mut resource, header + 4, samples.len() as u32);
            write_u32(&mut resource, header + 8, crate::sound::OUTPUT_RATE << 16);
            resource[header + 20] = 0; // stdSH
            resource[header + 21] = 60;
            resource[header + 22..header + 26].copy_from_slice(&samples);
        }

        loaded
            .memory
            .add_region(channel, vec![0; PPC_GUEST_SND_CHANNEL_SIZE as usize]);
        loaded.memory.add_region(sound_handle, vec![0; 4]);
        loaded.memory.add_region(sound_ptr, resource.clone());
        loaded.memory.write_u32_be(channel + 8, callback).unwrap();
        loaded.sound.manager.register_channel(
            channel,
            false,
            callback,
            CallbackTaskArchitecture::PowerPc,
        );
        loaded.memory.write_u32_be(sound_handle, sound_ptr).unwrap();
        test_handles!(loaded).push(PpcHandleRecord {
            handle: sound_handle,
            ptr: sound_ptr,
            size: resource.len() as u32,
            capacity: resource.len() as u32,
        });
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = sound_handle;
        loaded.cpu.gpr[5] = 1; // async; retained for the resource contract

        assert_eq!(
            ppc_snd_play(
                &loaded.cpu,
                &mut loaded.memory,
                &test_handle_records!(loaded),
                &mut loaded.sound,
            ),
            PPC_NO_ERR
        );
        assert_eq!(loaded.sound.manager.debug_buffer_cmd_count, 2);
        assert_eq!(loaded.sound.manager.channel_has_queued_commands(channel), Some(true));

        // First buffer is full volume. The queued volume command must be
        // applied before the second buffer starts, halving its excursion.
        assert_eq!(loaded.sound.manager.mix_frame(4), vec![0xa0; 4]);
        assert_eq!(loaded.sound.manager.mix_frame(1), vec![0x88]);
        loaded.sound.manager.mix_frame(4);
        loaded.sound.manager.mix_frame(1); // drain callback after buffer two
        assert_eq!(loaded.sound.manager.pending_sound_callbacks.len(), 1);
        loaded.sound.manager.mix_frame(1);
        assert_eq!(
            loaded.sound.manager.pending_sound_callbacks.len(),
            1,
            "one resource callback command must not be scheduled twice"
        );
        match &loaded.sound.manager.pending_sound_callbacks[0] {
            PendingSoundCallback::Command { cmd, chan_ptr, .. } => {
                assert_eq!(*chan_ptr, channel);
                assert_eq!(cmd.cmd, crate::sound::cmd::CALLBACK);
                assert_eq!(cmd.param1, 0x1234);
                assert_eq!(cmd.param2, 0x5678_9abc);
            }
            other => panic!("expected command callback, got {other:?}"),
        }
    }

    #[test]
    fn snd_do_command_reports_queue_full_for_both_wait_modes() {
        let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
        let channel = PPC_DATA_BASE + 0x1b00;
        let command_ptr = PPC_DATA_BASE + 0x1c00;
        loaded.memory.add_region(command_ptr, vec![0; 8]);
        loaded.sound.manager.register_channel(
            channel,
            false,
            0,
            CallbackTaskArchitecture::PowerPc,
        );
        for _ in 0..128 {
            assert!(loaded.sound.manager.enqueue_command(
                channel,
                crate::sound::SndCommand {
                    cmd: crate::sound::cmd::NULL,
                    param1: 0,
                    param2: 0,
                },
            ));
        }
        loaded.memory.write_u16_be(command_ptr, crate::sound::cmd::NULL).unwrap();
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = command_ptr;
        loaded.cpu.gpr[5] = 1; // noWait
        assert_eq!(
            ppc_snd_do_command(&loaded.cpu, &mut loaded.memory, &mut loaded.sound),
            -203
        );
        loaded.cpu.gpr[5] = 0; // blocking wait is unsupported by the host
        assert_eq!(
            ppc_snd_do_command(&loaded.cpu, &mut loaded.memory, &mut loaded.sound),
            -203
        );
    }

    #[test]
    fn snd_channel_status_reports_active_native_buffer_then_idle() {
        let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
        let channel = PPC_DATA_BASE + 0x1d00;
        let status_ptr = PPC_DATA_BASE + 0x1e00;
        loaded.memory.add_region(status_ptr, vec![0xaa; 22]);
        loaded.sound.manager.register_channel(
            channel,
            false,
            0,
            CallbackTaskArchitecture::PowerPc,
        );
        loaded.sound.manager.play_buffer_command_for_architecture(
            channel,
            vec![0xa0, 0xa0],
            crate::sound::OUTPUT_RATE << 16,
            CallbackTaskArchitecture::PowerPc,
        );
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 22;
        loaded.cpu.gpr[5] = status_ptr;
        assert_eq!(
            ppc_snd_channel_status(&mut loaded.cpu, &mut loaded.memory, &loaded.sound),
            PPC_NO_ERR
        );
        assert_eq!(loaded.memory.read_u8(status_ptr + 12), Some(1));
        assert_eq!(loaded.memory.read_u8(status_ptr + 14), Some(0));
        loaded.sound.manager.mix_frame(2);
        assert_eq!(
            ppc_snd_channel_status(&mut loaded.cpu, &mut loaded.memory, &loaded.sound),
            PPC_NO_ERR
        );
        assert_eq!(loaded.memory.read_u8(status_ptr + 12), Some(0));
    }

    #[test]
    fn hle_import_runner_handles_snd_play_double_buffer() {
        let pef = synthetic_pef_with_import(b"SndPlayDoubleBuffer");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel = PPC_DATA_BASE + 0x1000;
        let header = PPC_DATA_BASE + 0x2000;
        loaded
            .memory
            .add_region(channel, vec![0xdd; PPC_GUEST_SND_CHANNEL_SIZE as usize]);
        loaded.memory.add_region(header, vec![0; 24]);
        loaded.memory.add_region(PPC_DATA_BASE + 0x3000, vec![0; 24]);
        loaded.memory.add_region(PPC_DATA_BASE + 0x4000, vec![0; 24]);
        loaded
            .memory
            .write_u32_be(PPC_DATA_BASE + 0x3000, 2)
            .unwrap();
        loaded
            .memory
            .write_u32_be(PPC_DATA_BASE + 0x3004, 1)
            .unwrap();
        loaded
            .memory
            .write_bytes(
                PPC_DATA_BASE + 0x3010,
                &[0x00, 0x00, 0x00, 0x00, 0x7f, 0xff, 0x7f, 0xff],
            )
            .unwrap();
        loaded.memory.write_u16_be(header, 2).unwrap();
        loaded.memory.write_u16_be(header + 2, 16).unwrap();
        loaded
            .memory
            .write_u32_be(header + 8, 22_050u32 << 16)
            .unwrap();
        loaded
            .memory
            .write_u32_be(header + 12, PPC_DATA_BASE + 0x3000)
            .unwrap();
        loaded
            .memory
            .write_u32_be(header + 16, PPC_DATA_BASE + 0x4000)
            .unwrap();
        loaded
            .memory
            .write_u32_be(header + 20, PPC_CODE_BASE + 0x40)
            .unwrap();
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = header;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.double_buffer_play_count, 1);
        assert_eq!(loaded.sound.last_double_buffer_channel, channel);
        assert_eq!(loaded.sound.last_double_buffer_header, header);
        assert_eq!(
            loaded.sound.manager.double_buffer_playbacks,
            vec![PpcSoundDoubleBufferPlaybackRecord {
                channel,
                header,
                buffers: [PPC_DATA_BASE + 0x3000, PPC_DATA_BASE + 0x4000],
                callback: PPC_CODE_BASE + 0x40,
                callback_architecture: CallbackTaskArchitecture::PowerPc,
                sample_rate_fixed: 22_050u32 << 16,
                num_channels: 2,
                sample_size: 16,
                compression_id: 0,
                packet_size: 0,
                current_buffer_index: 0,
                callback_pending_mask: 0,
                active: true,
                host_initialized: true,
                host_buffer_loaded: true,
            }]
        );
        assert_eq!(loaded.sound.manager.debug_double_buffer_count, 1);
        assert!(loaded
            .sound
            .manager
            .channels
            .iter()
            .any(|candidate| candidate.guest_ptr == channel && candidate.has_active_playback()));
        assert!(loaded.sound.manager.pending_process_doublebacks.is_empty());

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.sound.double_buffer_play_count, 1);
    }

    #[test]
    fn hle_import_runner_handles_snd_dispose_channel() {
        let pef = synthetic_pef_with_import(b"SndDisposeChannel");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel = 0x0500_1000;
        loaded
            .sound
            .file_playbacks
            .push(PpcSoundFilePlaybackRecord {
                channel,
                ref_num: 0,
                resource_id: 0,
                buffer_size: 0,
                buffer: 0,
                selection: 0,
                completion: 0,
                completion_command: None,
                async_play: false,
                aiff: None,
                decoded_aiff: None,
            });
        loaded.sound.manager.play_file_buffer(
            channel,
            vec![0x80],
            crate::sound::OUTPUT_RATE << 16,
            None,
        );
        loaded.sound.decoded_file_playbacks.push(PpcDecodedAiffPlaybackRecord {
            file_playback_index: 0,
            channel,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            samples: vec![0xa0],
        });
        loaded.sound.manager.queue_sound_callback(
            PendingSoundCallback::FileCompletion {
                architecture: CallbackTaskArchitecture::PowerPc,
                callback_addr: PPC_CODE_BASE + 0x220,
                chan_ptr: channel,
            },
        );
        assert_eq!(loaded.sound.manager.toggle_file_paused(channel), Some(true));
        loaded.cpu.gpr[3] = channel;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.manager.file_playback_paused(channel), None);
        assert!(loaded.sound.file_playbacks.is_empty());
        assert!(loaded.sound.decoded_file_playbacks.is_empty());
        assert!(loaded.sound.manager.pending_sound_callbacks.is_empty());
    }

    #[test]
    fn hle_import_runner_snd_new_channel_rejects_unsupported_synth_without_side_effects() {
        let pef = synthetic_pef_with_import(b"SndNewChannel");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel_out_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(channel_out_ptr, vec![0xaa; 4]);
        let initial_heap_cursor = loaded.heap_cursor();
        loaded.set_last_mem_error(123);
        loaded.cpu.gpr[3] = channel_out_ptr;
        loaded.cpu.gpr[4] = 2;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0x0123_4567;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RES_PROBLEM));
        assert_eq!(loaded.heap_cursor(), initial_heap_cursor);
        assert_eq!(loaded.last_mem_error(), 123);
        assert_ppc_bytes_equal(&mut loaded.memory, channel_out_ptr, 4, 0xaa);
    }

    #[test]
    fn hle_import_runner_snd_new_channel_reuses_existing_channel_record() {
        let pef = synthetic_pef_with_import(b"SndNewChannel");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel_out_ptr = PPC_DATA_BASE + 0x1000;
        let channel = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(channel_out_ptr, vec![0; 4]);
        loaded
            .memory
            .add_region(channel, vec![0xdd; PPC_GUEST_SND_CHANNEL_SIZE as usize]);
        loaded
            .memory
            .write_u32_be(channel_out_ptr, channel)
            .unwrap();
        loaded
            .memory
            .write_u32_be(channel + 12, 0xcafe_babe)
            .unwrap();
        loaded.memory.write_u16_be(channel + 30, 4).unwrap();
        let initial_heap_cursor = loaded.heap_cursor();
        loaded.cpu.gpr[3] = channel_out_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0x89ab_cdef;
        loaded.cpu.gpr[6] = 0x0123_4567;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.heap_cursor(), initial_heap_cursor);
        assert_eq!(loaded.memory.read_u32_be(channel_out_ptr), Some(channel));
        assert_eq!(loaded.memory.read_u32_be(channel), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 4), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 8), Some(0x0123_4567));
        assert_eq!(loaded.memory.read_u32_be(channel + 12), Some(0xcafe_babe));
        assert_eq!(loaded.memory.read_u32_be(channel + 16), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 20), Some(0));
        assert_eq!(loaded.memory.read_u32_be(channel + 24), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 28), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 30), Some(4));
        assert_eq!(loaded.memory.read_u16_be(channel + 32), Some(0));
        assert_eq!(loaded.memory.read_u16_be(channel + 34), Some(0));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_handles_snd_channel_status() {
        let pef = synthetic_pef_with_import(b"SndChannelStatus");
        let mut loaded = load_pef_application(&pef).unwrap();
        let status_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(status_ptr, vec![0xaa; 22]);
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;
        loaded.cpu.gpr[4] = 22;
        loaded.cpu.gpr[5] = status_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        for offset in 0..22 {
            assert_eq!(loaded.memory.read_u8(status_ptr + offset), Some(0));
        }
    }

    #[test]
    fn hle_import_runner_prevalidates_sound_manager_output_buffers() {
        let pef = synthetic_pef_with_import(b"SndSoundManagerVersion");
        let mut loaded = load_pef_application(&pef).unwrap();
        let version_ptr = PPC_DATA_BASE + 0x1000;
        let volume_ptr = PPC_DATA_BASE + 0x1100;
        let channel_out_ptr = PPC_DATA_BASE + 0x1200;
        let status_ptr = PPC_DATA_BASE + 0x1300;
        let cmd_ptr = PPC_DATA_BASE + 0x1400;
        let rate_ptr = PPC_DATA_BASE + 0x1500;
        let offset_ptr = PPC_DATA_BASE + 0x1600;
        loaded.memory.add_region(version_ptr, vec![0xa1; 3]);
        loaded.memory.add_region(volume_ptr, vec![0xa2; 3]);
        loaded.memory.add_region(channel_out_ptr, vec![0xa3; 3]);
        loaded.memory.add_region(status_ptr, vec![0xa4; 7]);
        loaded.memory.add_region(cmd_ptr, vec![0; 8]);
        loaded.memory.add_region(rate_ptr, vec![0xa5; 3]);
        loaded.memory.add_region(offset_ptr, vec![0xa6; 3]);
        loaded.cpu.gpr[3] = version_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], version_ptr);
        for offset in 0..3 {
            assert_eq!(loaded.memory.read_u8(version_ptr + offset), Some(0xa1));
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetDefaultOutputVolume;
        loaded.cpu.gpr[3] = volume_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        for offset in 0..3 {
            assert_eq!(loaded.memory.read_u8(volume_ptr + offset), Some(0xa2));
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndNewChannel;
        let initial_heap_cursor = loaded.heap_cursor();
        loaded.set_last_mem_error(123);
        loaded.cpu.gpr[3] = channel_out_ptr;
        loaded.cpu.gpr[4] = 5;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.heap_cursor(), initial_heap_cursor);
        assert_eq!(loaded.last_mem_error(), 123);
        for offset in 0..3 {
            assert_eq!(loaded.memory.read_u8(channel_out_ptr + offset), Some(0xa3));
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndChannelStatus;
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;
        loaded.cpu.gpr[4] = 8;
        loaded.cpu.gpr[5] = status_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        for offset in 0..7 {
            assert_eq!(loaded.memory.read_u8(status_ptr + offset), Some(0xa4));
        }

        loaded.memory.write_u16_be(cmd_ptr, 85).unwrap();
        loaded.memory.write_u16_be(cmd_ptr + 2, 0).unwrap();
        loaded.memory.write_u32_be(cmd_ptr + 4, rate_ptr).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndDoImmediate;
        loaded.cpu.gpr[3] = PPC_HEAP_BASE;
        loaded.cpu.gpr[4] = cmd_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert!(loaded.sound.immediate_commands.is_empty());
        for offset in 0..3 {
            assert_eq!(loaded.memory.read_u8(rate_ptr + offset), Some(0xa5));
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetSoundHeaderOffset;
        loaded.cpu.gpr[3] = PPC_DATA_BASE + 0x7000;
        loaded.cpu.gpr[4] = offset_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        for offset in 0..3 {
            assert_eq!(loaded.memory.read_u8(offset_ptr + offset), Some(0xa6));
        }
    }

    #[test]
    fn hle_snd_do_command_decodes_external_extended_sound_header() {
        let pef = synthetic_pef_with_import(b"SndDoCommand");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel = 0x0500_1000;
        let cmd_ptr = PPC_DATA_BASE + 0x1000;
        let header_ptr = cmd_ptr + 0x20;
        let samples_ptr = cmd_ptr + 0x100;
        let samples = [0x80, 0x90, 0x70, 0xa0];
        loaded.memory.add_region(cmd_ptr, vec![0; 0x200]);
        loaded.memory.write_u16_be(cmd_ptr, 81).unwrap(); // bufferCmd
        loaded.memory.write_u32_be(cmd_ptr + 4, header_ptr).unwrap();
        loaded.memory.write_u32_be(header_ptr, samples_ptr).unwrap();
        loaded.memory.write_u32_be(header_ptr + 4, 1).unwrap();
        loaded
            .memory
            .write_u32_be(header_ptr + 8, 22_050u32 << 16)
            .unwrap();
        loaded.memory.write_u8(header_ptr + 20, 0xff).unwrap(); // extSH
        loaded
            .memory
            .write_u32_be(header_ptr + 22, samples.len() as u32)
            .unwrap();
        loaded.memory.write_u16_be(header_ptr + 48, 8).unwrap();
        loaded.memory.write_bytes(samples_ptr, &samples).unwrap();
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = cmd_ptr;
        loaded.cpu.gpr[5] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.manager.debug_buffer_cmd_count, 1);
        assert!(loaded
            .sound
            .manager
            .channels
            .iter()
            .any(|candidate| candidate.guest_ptr == channel && candidate.has_active_playback()));
        assert_eq!(
            loaded.sound.queued_commands,
            vec![PpcSndCommandRecord {
                channel,
                command: 81,
                param1: 0,
                param2: header_ptr,
            }]
        );
    }

    #[test]
    fn hle_import_runner_tracks_sound_commands_and_file_playback() {
        let pef = synthetic_pef_with_import(b"SndDoImmediate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let channel = 0x0500_1000;
        let cmd_ptr = PPC_DATA_BASE + 0x1000;
        let rate_ptr = PPC_DATA_BASE + 0x1010;
        loaded.memory.add_region(cmd_ptr, vec![0; 20]);
        loaded.memory.write_u16_be(cmd_ptr, 85).unwrap(); // getRateCmd
        loaded.memory.write_u16_be(cmd_ptr + 2, 0).unwrap();
        loaded.memory.write_u32_be(cmd_ptr + 4, rate_ptr).unwrap();
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = cmd_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(rate_ptr), Some(0x0001_0000));
        assert_eq!(
            loaded.sound.immediate_commands,
            vec![PpcSndCommandRecord {
                channel,
                command: 85,
                param1: 0,
                param2: rate_ptr,
            }]
        );

        loaded.memory.write_u16_be(cmd_ptr, 80).unwrap(); // volumeCmd
        loaded.memory.write_u16_be(cmd_ptr + 2, 0).unwrap();
        loaded
            .memory
            .write_u32_be(cmd_ptr + 4, 0x0040_0040)
            .unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndDoCommand;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = cmd_ptr;
        loaded.cpu.gpr[5] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.sound.queued_commands,
            vec![PpcSndCommandRecord {
                channel,
                command: 80,
                param1: 0,
                param2: 0x0040_0040,
            }]
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndStartFilePlay;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 128u32;
        loaded.cpu.gpr[5] = (-1i16) as u16 as u32;
        loaded.cpu.gpr[6] = 20_480;
        loaded.cpu.gpr[7] = PPC_DATA_BASE + 0x2000;
        loaded.cpu.gpr[8] = PPC_DATA_BASE + 0x2100;
        loaded.cpu.gpr[9] = PPC_CODE_BASE + 0x40;
        loaded.cpu.gpr[10] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.start_count, 1);
        assert_eq!(
            loaded.sound.file_playbacks.last().copied(),
            Some(PpcSoundFilePlaybackRecord {
                channel,
                ref_num: 128,
                resource_id: -1,
                buffer_size: 20_480,
                buffer: PPC_DATA_BASE + 0x2000,
                selection: PPC_DATA_BASE + 0x2100,
                completion: PPC_CODE_BASE + 0x40,
                completion_command: None,
                async_play: true,
                aiff: None,
                decoded_aiff: None,
            })
        );

        let status_ptr = PPC_DATA_BASE + 0x3000;
        loaded.memory.add_region(status_ptr, vec![0xaa; 22]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndChannelStatus;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 22;
        loaded.cpu.gpr[5] = status_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(status_ptr + 12), Some(1));
        assert_eq!(loaded.memory.read_u8(status_ptr + 14), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndPauseFilePlay;
        loaded.cpu.gpr[3] = channel;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.pause_count, 1);
        assert_eq!(
            loaded.sound.manager.file_playback_paused(channel),
            Some(true)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndChannelStatus;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 22;
        loaded.cpu.gpr[5] = status_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(status_ptr + 12), Some(1));
        assert_eq!(loaded.memory.read_u8(status_ptr + 14), Some(1));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SndStopFilePlay;
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.stop_count, 1);
        assert_eq!(loaded.sound.manager.file_playback_paused(channel), None);
    }

    #[test]
    fn hle_import_runner_sound_completion_callback_populates_parameter_area() {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        let tvector = PPC_HEAP_BASE + 0x1000;
        let callback_entry = PPC_HEAP_BASE + 0x1100;
        let callback_rtoc = PPC_HEAP_BASE + 0x2000;
        let channel = 0x0500_1234;
        let callback_sp = ppc_interrupt_callback_stack_pointer(loaded.cpu.gpr[1]);
        let command_ptr =
            callback_sp + PPC_PARAMETER_AREA_OFFSET + PPC_NATIVE_PARAMETER_GPR_COUNT as u32 * 4;
        let mut callback = Vec::new();
        for word in [
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(32, 5, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(32, 6, 1, (PPC_PARAMETER_AREA_OFFSET + 7 * 4) as u16),
            d_form_u(36, 5, 2, 8),
            d_form_u(36, 6, 2, 12),
            d_form_u(36, 1, 2, 16),
            BLR,
        ] {
            callback.extend_from_slice(&word.to_be_bytes());
        }
        loaded.memory.add_region(tvector, vec![0; 8]);
        loaded.memory.add_region(callback_entry, callback);
        loaded.memory.add_region(callback_rtoc, vec![0; 0x100]);
        loaded.memory.write_u32_be(tvector, callback_entry).unwrap();
        loaded
            .memory
            .write_u32_be(tvector + 4, callback_rtoc)
            .unwrap();
        for slot in 0..PPC_NATIVE_PARAMETER_GPR_COUNT {
            let addr = ppc_parameter_area_slot_addr(callback_sp, slot).unwrap();
            loaded
                .memory
                .write_u32_be(addr, 0xdead_1000 | u32::try_from(slot).unwrap())
                .unwrap();
        }
        loaded.cpu.cr = 0x1234_5678;
        loaded.cpu.lr = 0x8765_4321;
        let saved_sp = loaded.cpu.gpr[1];
        let red_zone_start = saved_sp - PPC_INTERRUPT_RED_ZONE_SIZE;
        let red_zone = vec![0x5a; PPC_INTERRUPT_RED_ZONE_SIZE as usize];
        loaded
            .memory
            .write_bytes(red_zone_start, &red_zone)
            .unwrap();
        let saved_rtoc = loaded.cpu.gpr[2];
        let saved_cr = loaded.cpu.cr;
        let saved_lr = loaded.cpu.lr;
        loaded.cpu.gpr[4] = 0xfeed_face;

        let probe = loaded.run_sound_completion_callback(
            PpcSoundCompletionRecord {
                file_playback_index: 3,
                channel,
                completion: tvector,
                command: Some(PpcSndCommandRecord {
                    channel,
                    command: 13,
                    param1: 0x5348,
                    param2: 0x0200_0000,
                }),
                tick: 7,
                instruction_count: 11,
                scheduled_tick: 13,
                scheduled_instruction_count: 17,
            },
            64,
            false,
            false,
        );

        assert!(matches!(
            probe.invocation.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                ..
            }
        ));
        assert_eq!(probe.invocation.callback_entry, callback_entry);
        assert_eq!(probe.invocation.callback_rtoc, callback_rtoc);
        assert_eq!(probe.invocation.end_sp, callback_sp);
        assert_eq!(probe.invocation.end_r3, channel);
        assert_eq!(loaded.cpu.gpr[1], saved_sp);
        assert_eq!(loaded.cpu.gpr[2], saved_rtoc);
        assert_eq!(loaded.cpu.cr, saved_cr);
        assert_eq!(loaded.cpu.lr, saved_lr);
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                red_zone_start,
                PPC_INTERRUPT_RED_ZONE_SIZE
            ),
            Some(red_zone),
            "an asynchronous native callback must preserve the interrupted leaf routine's Red Zone"
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(callback_sp + PPC_LINKAGE_BACK_CHAIN_OFFSET),
            Some(saved_sp)
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_CR_OFFSET),
            Some(saved_cr)
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_LR_OFFSET),
            Some(saved_lr)
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_RTOC_OFFSET),
            Some(saved_rtoc)
        );
        assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(channel));
        assert_eq!(
            loaded.memory.read_u32_be(callback_rtoc + 4),
            Some(command_ptr)
        );
        assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 8), Some(channel));
        assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 12), Some(0));
        assert_eq!(
            loaded.memory.read_u32_be(callback_rtoc + 16),
            Some(callback_sp)
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(ppc_parameter_area_slot_addr(callback_sp, 0).unwrap()),
            Some(channel)
        );
        assert_eq!(
            loaded
                .memory
                .read_u32_be(ppc_parameter_area_slot_addr(callback_sp, 1).unwrap()),
            Some(command_ptr)
        );
        assert_eq!(loaded.memory.read_u16_be(command_ptr), Some(13));
        assert_eq!(loaded.memory.read_u16_be(command_ptr + 2), Some(0x5348));
        assert_eq!(
            loaded.memory.read_u32_be(command_ptr + 4),
            Some(0x0200_0000)
        );
        assert_eq!(
            loaded.memory.read_u32_be(
                ppc_parameter_area_slot_addr(callback_sp, PPC_NATIVE_PARAMETER_GPR_COUNT - 1)
                    .unwrap()
            ),
            Some(0)
        );
    }

    #[test]
    fn hle_import_runner_captures_aiff_metadata_for_sound_file_playback() {
        let pef = synthetic_pef_with_import(b"SndStartFilePlay");
        let mut loaded = load_pef_application(&pef).unwrap();
        let ref_num = 128i16;
        let channel = 0x0500_1000;
        let path = "Data/Audio/TitleSong.aiff";
        let mut aiff = vec![0; 60];
        aiff[0..4].copy_from_slice(b"FORM");
        write_u32(&mut aiff, 4, 52);
        aiff[8..12].copy_from_slice(b"AIFF");
        aiff[12..16].copy_from_slice(b"COMM");
        write_u32(&mut aiff, 16, 18);
        write_u16(&mut aiff, 20, 2);
        write_u32(&mut aiff, 22, 1000);
        write_u16(&mut aiff, 26, 16);
        aiff[28..38].copy_from_slice(&[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
        aiff[38..42].copy_from_slice(b"SSND");
        write_u32(&mut aiff, 42, 14);
        write_u32(&mut aiff, 46, 0);
        write_u32(&mut aiff, 50, 0);
        aiff[54..60].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num,
            path: path.to_string(),
            position: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: path.to_string(),
            data: (aiff).into(),
            creator: u32::from_be_bytes(*b"ttxt"),
            file_type: u32::from_be_bytes(*b"AIFF"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = ref_num as u16 as u32;
        loaded.cpu.gpr[5] = (-1i16) as u16 as u32;
        loaded.cpu.gpr[6] = 20_480;
        loaded.cpu.gpr[7] = PPC_DATA_BASE + 0x2000;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 0;
        loaded.cpu.gpr[10] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.sound.file_playbacks.last().unwrap().aiff,
            Some(PpcAiffMetadata {
                form_type: u32::from_be_bytes(*b"AIFF"),
                channel_count: 2,
                sample_frame_count: 1000,
                sample_size: 16,
                sample_rate_hz: 44_100,
                compression_type: u32::from_be_bytes(*b"NONE"),
                sound_data_offset: 54,
                sound_data_size: 6,
            })
        );
        let mut expected_preview = [0; 16];
        expected_preview[0] = 130;
        assert_eq!(
            loaded.sound.file_playbacks.last().unwrap().decoded_aiff,
            Some(PpcDecodedAiffSamples {
                sample_rate_fixed: 44_100u32 << 16,
                sample_count: 1,
                preview_len: 1,
                preview: expected_preview,
            })
        );
        assert_eq!(loaded.sound.manager.debug_file_play_count, 1);
        assert!(loaded
            .sound
            .manager
            .channels
            .iter()
            .any(|candidate| candidate.guest_ptr == channel && candidate.has_active_playback()));
    }

    #[test]
    fn hle_import_runner_decodes_snd_resource_for_file_playback() {
        let pef = synthetic_pef_with_import(b"SndStartFilePlay");
        let mut loaded = load_pef_application(&pef).unwrap();
        let resource_id = 200i16;
        let channel = 0x0500_1000;
        let samples = [0x80, 0x90, 0x70, 0x80];
        let mut resource = vec![0; 40];
        write_u16(&mut resource, 0, 2); // format 2
        write_u16(&mut resource, 2, 0); // refCount
        write_u16(&mut resource, 4, 1); // numCommands
        write_u16(&mut resource, 6, 0x8050); // soundCmd + dataOffsetFlag
        write_u16(&mut resource, 8, 0);
        write_u32(&mut resource, 10, 14); // SoundHeader offset
        write_u32(&mut resource, 14, 0); // samplePtr = NIL, data follows header
        write_u32(&mut resource, 18, samples.len() as u32);
        write_u32(&mut resource, 22, crate::sound::OUTPUT_RATE << 16);
        write_u32(&mut resource, 26, 0); // loopStart
        write_u32(&mut resource, 30, 0); // loopEnd
        resource[34] = 0; // stdSH
        resource[35] = 60; // baseFrequency
        resource[36..40].copy_from_slice(&samples);
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_resource_refnum,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"snd "),
            res_id: resource_id,
            name: b"Roar".to_vec(),
            data: resource,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = channel;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = resource_id as u16 as u32;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = 0;
        loaded.cpu.gpr[10] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.sound.start_count, 1);
        let playback = loaded.sound.file_playbacks.last().unwrap();
        assert_eq!(playback.channel, channel);
        assert_eq!(playback.ref_num, 0);
        assert_eq!(playback.resource_id, resource_id);
        assert_eq!(playback.decoded_aiff.unwrap().sample_count, 4);
        assert_eq!(
            loaded.sound.decoded_file_playbacks,
            vec![PpcDecodedAiffPlaybackRecord {
                file_playback_index: 0,
                channel,
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                samples: samples.to_vec(),
            }]
        );
    }

    #[test]
    fn ppc_snd_resource_decoder_expands_mace3_headers() {
        let mut resource = vec![0; 80];
        write_u16(&mut resource, 0, 2); // format 2
        write_u16(&mut resource, 2, 0);
        write_u16(&mut resource, 4, 1);
        write_u16(&mut resource, 6, 0x8051); // bufferCmd + dataOffsetFlag
        write_u16(&mut resource, 8, 0);
        write_u32(&mut resource, 10, 14);
        write_u32(&mut resource, 14, 0); // samplePtr
        write_u32(&mut resource, 18, 1); // channels
        write_u32(&mut resource, 22, crate::sound::OUTPUT_RATE << 16);
        write_u32(&mut resource, 26, 0); // loopStart
        write_u32(&mut resource, 30, 0); // loopEnd
        resource[34] = 0xfe; // cmpSH
        resource[35] = 60;
        write_u32(&mut resource, 36, 1); // one MACE3 packet frame = 2 bytes
        write_u32(&mut resource, 54, u32::from_be_bytes(*b"MAC3"));
        write_u16(&mut resource, 70, 3); // threeToOne
        write_u16(&mut resource, 72, 16); // packet size bits
        write_u16(&mut resource, 76, 8); // expanded sample size
        resource[78] = 0;
        resource[79] = 0;

        let decoded = ppc_decode_snd_resource_samples(&resource).expect("MACE3 resource decodes");

        assert_eq!(
            decoded.summary.sample_rate_fixed,
            crate::sound::OUTPUT_RATE << 16
        );
        assert_eq!(decoded.summary.sample_count, 6);
        assert_eq!(decoded.samples, vec![0x80; 6]);
    }

    #[test]
    fn hle_import_runner_handles_get_sound_header_offset() {
        let pef = synthetic_pef_with_import(b"GetSoundHeaderOffset");
        let mut loaded = load_pef_application(&pef).unwrap();
        let mut resource = vec![0; 0x34];
        write_u16(&mut resource, 0, 1);
        write_u16(&mut resource, 2, 1);
        write_u16(&mut resource, 4, 5);
        write_u32(&mut resource, 6, 0);
        write_u16(&mut resource, 10, 2);
        write_u16(&mut resource, 12, 0);
        write_u16(&mut resource, 14, 0);
        write_u32(&mut resource, 16, 0);
        write_u16(&mut resource, 20, 0x8051);
        write_u16(&mut resource, 22, 0);
        write_u32(&mut resource, 24, 0x28);
        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &resource,
        );
        assert_ne!(handle, 0);
        let offset_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(offset_ptr, vec![0xaa; 4]);
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = offset_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(offset_ptr), Some(0x28));
    }
