//! Sound Manager trap handlers.
//!
//! Handles SndPlay, SndNewChannel, SndDisposeChannel, SndDoCommand,
//! SndDoImmediate, SoundDispatch, and SysBeep.
//! Reference: Inside Macintosh: Sound 1994; references/executor/src/sound/sound.cpp

use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::sound::{self, SndChannel, SndCommand, StereoSample};
use crate::Result;

/// Size of the guest SndChannel record we allocate.
/// Must be large enough for the game to read fields it expects.
/// Sound 1994, 2-93: nextChan(4) + firstMod(4) + callBack(4) + userInfo(4)
///   + wait(4) + cmdInProgress(8) + flags(2) + qLength(2) + qHead(2) + qTail(2)
///   + `queue[128*8]` = 1060 bytes. We allocate 1088 for alignment.
const GUEST_SND_CHANNEL_SIZE: u32 = 1088;
const GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET: u32 = 20;
const GUEST_SND_CHANNEL_FLAGS_OFFSET: u32 = 28;
const GUEST_SND_CHANNEL_Q_LENGTH_OFFSET: u32 = 30;
const GUEST_SND_CHANNEL_Q_HEAD_OFFSET: u32 = 32;
const GUEST_SND_CHANNEL_Q_TAIL_OFFSET: u32 = 34;
const GUEST_SND_CHANNEL_QUEUE_OFFSET: u32 = 36;
const SQUARE_WAVE_SYNTH_ID: i16 = 1;
const WAVE_TABLE_SYNTH_ID: i16 = 3;
const SAMPLED_SYNTH_ID: i16 = 5;
const SOUND_MANAGER_3_SYNTH_VERSION: u32 = 0x0003_0000;

use std::sync::OnceLock;
static TRACE_SOUND: OnceLock<bool> = OnceLock::new();
fn trace_sound_enabled() -> bool {
    *TRACE_SOUND.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

static TRACE_SOUND_SAMPLES: OnceLock<bool> = OnceLock::new();
fn trace_sound_samples_enabled() -> bool {
    *TRACE_SOUND_SAMPLES
        .get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND_SAMPLES").is_some())
}

fn sound_dispatch_param_bytes(selector: u32) -> u32 {
    let encoded = ((selector >> 24) & 0xFF) * 2;
    if encoded != 0 {
        return encoded;
    }

    match selector {
        // The Inside Macintosh VI selector table documents these SoundDispatch
        // selectors with a zero param-size byte even though the Pascal
        // routines have stack arguments. Some clients pass those literals.
        // Inside Macintosh Volume VI, VI-576 to VI-577.
        0x0010_0008 => 10, // SndChannelStatus(chan, theLength, theStatus)
        0x0014_0008 => 6,  // SndManagerStatus(theLength, theStatus)
        0x0018_0008 => 4,  // SndGetSysBeepState(VAR state)
        0x001C_0008 => 2,  // SndSetSysBeepState(state)
        0x0020_0008 => 8,  // SndPlayDoubleBuffer(chan, theParams)
        _ => 0,
    }
}

fn synth_sys_beep_samples() -> Vec<u8> {
    let len = (sound::OUTPUT_RATE as usize * 90) / 1000;
    let half_period = (sound::OUTPUT_RATE as usize / (880 * 2)).max(1);
    let mut samples = Vec::with_capacity(len);
    for i in 0..len {
        let polarity = if (i / half_period) & 1 == 0 { 1 } else { -1 };
        let envelope = (len - i) as i32;
        let amplitude = 72 * envelope / len.max(1) as i32;
        samples.push((0x80i32 + polarity * amplitude).clamp(0, 255) as u8);
    }
    samples
}

impl super::TrapDispatcher {
    fn clear_guest_sound_channel_state(&self, bus: &mut MacMemoryBus, chan_ptr: u32) {
        if chan_ptr == 0 {
            return;
        }

        // Scrub the guest-visible record before the channel block is reused
        // or returned to the allocator.
        bus.write_long(chan_ptr, 0); // nextChan
        bus.write_long(chan_ptr + 4, 0); // firstMod
        bus.write_long(chan_ptr + 8, 0); // callBack
        bus.write_long(chan_ptr + 12, 0); // userInfo
        bus.write_long(chan_ptr + 16, 0); // wait
        bus.write_long(chan_ptr + 20, 0); // cmdInProgress.cmd/param1
        bus.write_long(chan_ptr + 24, 0); // cmdInProgress.param2
        bus.write_word(chan_ptr + 28, 0); // flags
        bus.write_word(chan_ptr + 30, 0); // qLength
        bus.write_word(chan_ptr + 32, 0); // qHead
        bus.write_word(chan_ptr + 34, 0); // qTail
    }

    fn release_sound_channel(&mut self, bus: &mut MacMemoryBus, chan_ptr: u32) {
        self.sound_manager
            .pending_callbacks
            .retain(|pending| pending.chan_ptr != chan_ptr);
        self.sound_manager
            .pending_sound_callbacks
            .retain(|pending| match pending {
                crate::sound::PendingSoundCallback::Command {
                    chan_ptr: pending_chan,
                    ..
                }
                | crate::sound::PendingSoundCallback::FileCompletion {
                    chan_ptr: pending_chan,
                    ..
                } => *pending_chan != chan_ptr,
            });

        if let Some(chan) = self.sound_manager.take_channel(chan_ptr) {
            self.clear_guest_sound_channel_state(bus, chan.guest_ptr);
            if chan.allocated {
                bus.free(chan.guest_ptr);
            }
        }
    }

    pub(crate) fn release_finished_internal_sound_channels(&mut self, bus: &mut MacMemoryBus) {
        let releasable = self.sound_manager.idle_auto_dispose_channel_ptrs();
        for chan_ptr in releasable {
            self.release_sound_channel(bus, chan_ptr);
        }
    }

    fn play_sys_beep(&mut self, bus: &mut MacMemoryBus) {
        let volume = self.sound_manager.sys_beep_volume();
        if volume & 0xFFFF == 0 && (volume >> 16) & 0xFFFF == 0 {
            return;
        }

        let guest_ptr = bus.alloc(GUEST_SND_CHANNEL_SIZE);
        let mut chan = SndChannel::new(guest_ptr, true);
        chan.set_volume(volume);
        chan.mark_auto_dispose_when_idle();
        chan.play_buffer(
            synth_sys_beep_samples(),
            crate::sound::OUTPUT_RATE << 16,
            sound::PlaybackKind::Buffer,
            0,
        );
        self.sound_manager.channels.push(chan);
    }

    pub(crate) fn dispatch_sound<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        Some(match (is_tool, trap_num) {
            // SndPlay ($A805)
            // FUNCTION SndPlay(chan: SndChannelPtr; sndHdl: SndListHandle; async: BOOLEAN): OSErr;
            // Stack: SP+0: async(2), SP+2: sndHdl(4), SP+6: chan(4), SP+10: result(2)
            // Sound 1994, 2-121 to 2-123
            // SndPlay ($A805): Parses snd format 1 & 2, handles bufferCmd with stdSH
            // (encode=$00), returns resProblem (-204) for NIL/unloaded handles and
            // badFormat (-206) for malformed resources per IM:Sound 1994 p. 2-122..2-123.
            (true, 0x005) => {
                let sp = cpu.read_reg(Register::A7);
                let async_flag = bus.read_word(sp) as i16;
                let snd_handle = bus.read_long(sp + 2);
                let chan_ptr = bus.read_long(sp + 6);
                let err: i16 = if snd_handle != 0 {
                    let snd_ptr = bus.read_long(snd_handle);
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] SndPlay async={} chan=${:08X} handle=${:08X} ptr=${:08X}",
                            async_flag, chan_ptr, snd_handle, snd_ptr
                        );
                    }
                    if snd_ptr != 0 {
                        self.snd_play_resource(bus, chan_ptr, async_flag != 0, snd_ptr)
                    } else {
                        // IM:Sound 1994 p. 2-122: unloaded sound handle -> resProblem.
                        -204
                    }
                } else {
                    // A NIL Handle is not a loadable sound resource.
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] SndPlay async={} chan=${:08X} handle=NULL -> resProblem",
                            async_flag, chan_ptr
                        );
                    }
                    -204
                };

                bus.write_word(sp + 10, err as u16);
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // ============================================================
            // Sound Manager Stub family (HLE-no-audio rationale)
            // ============================================================
            //
            // Two legacy Sound Manager traps below — $A802 SndAddModifier
            // and $A806 SndControl — stay intentionally narrow in Systemless
            // because the obsolete modifier/control facilities are not
            // modelled. SndControl still reports documented query outputs for
            // built-in synth ids. SysBeep is handled separately and emits a
            // short synthesized alert sample.
            //
            // Per-trap HLE rationale:
            //
            //   $A802 SndAddModifier — per IM:Sound 6808: "for
            //   internal Sound Manager use only. You should not call
            //   it in your application." The trap loads a 'snth'
            //   resource and links a modifier proc to a sound
            //   channel — Systemless runs no synthesizer modifier procs
            //   (no guest-fn dispatch infrastructure for the modifier
            //   ProcPtr; same compromise documented for ModalDialog
            //   filterProc + Alert filterProc + Pack1 LSearch
            //   searchProc).
            //
            //   $A806 SndControl — per IM:Sound 6232: "In Sound
            //   Manager version 3.0 and later, however, you virtually
            //   never need to call SndControl. The capabilities that
            //   SndControl provides are either provided by the
            //   Gestalt function or are no longer supported. The
            //   SndControl function is documented here for
            //   completeness only." Systemless reports a Sound Manager
            //   3.x NumVersion via SndSoundManagerVersion ($0C) and
            //   answers the documented SndControl query commands that
            //   older callers can still issue.
            //
            // Per-trap status table:
            //   $A802 SndAddModifier — Stub: FUNCTION returning OSErr,
            //                          writes 0 (noErr).
            //   $A806 SndControl     — Partial: FUNCTION returning OSErr,
            //                          writes 0 (noErr) and fills query outputs.
            //
            // SoundDispatch sub-routine SCStatus / SMStatus introspection
            // ($0010 SndChannelStatus / $0014 SndManagerStatus) are
            // promoted to Partial below — they fill the documented
            // VAR-out status records with HLE-defensible defaults
            // (zero-fill SCStatus; smNumChannels = active channel
            // count; smMaxCPULoad = default capacity; smCurCPULoad = 0).

            // SysBeep ($A9C8)
            // PROCEDURE SysBeep(duration: INTEGER);
            // Stack: SP+0: duration(2)
            // Inside Macintosh Volume II, II-385
            // Inside Macintosh: Sound 1994, 2-181 (line 1551..1572)
            //
            // HLE: synthesize a short alert sample at the current system beep
            // volume. The duration parameter is ignored on most System 7-era
            // machines, so we use a fixed compact beep.
            (true, 0x1C8) => {
                let sp = cpu.read_reg(Register::A7);
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] SysBeep duration={} volume=${:08X}",
                        bus.read_word(sp) as i16,
                        self.sound_manager.sys_beep_volume()
                    );
                }
                self.play_sys_beep(bus);
                cpu.write_reg(Register::A7, sp + 2);
                Ok(())
            }

            // SoundDispatch ($A800)
            // Multi-purpose Sound Manager dispatcher. Selector in D0.
            // Bits 31-24: parameter bytes / 2
            // Bits 23-16: routine selector
            // Sound 1994, 2-256
            //
            // Caveat on the paramSize byte: Inside Macintosh Volume VI's
            // trap selector table lists several SoundDispatch routines with
            // a zero param-size byte even though their Pascal signatures have
            // stack arguments. `sound_dispatch_param_bytes` maps those
            // documented literals back to their real frame sizes so clients
            // that pass table values directly do not return through args.
            // SoundDispatch ($A800): Routes by selector; SndPlayDoubleBuffer ($20) with doubleback callbacks, SndStartFilePlay ($00) plays 'snd ' resources by ID or AIFF file by refnum with async completion callbacks, SndStopFilePlay ($08) stops channel, SndPauseFilePlay ($04) toggles file pause, SndSoundManagerVersion ($0C) returns a Sound Manager 3.x NumVersion; SndChannelStatus ($10) zero-fills 24-byte SCStatus per IM:Sound 2-101 (no async playback so all fields are documented-correct at zero); SndManagerStatus ($14) fills 6-byte SMStatus with smNumChannels = active channel count, default smMaxCPULoad = 100, and zero current load per IM:Sound 2-101 + 2-201; volume selectors ($24/$28/$2C/$30) persist Sound Manager 3.x packed L/R levels
            (true, 0x000) => {
                let sp = cpu.read_reg(Register::A7);
                let selector = cpu.read_reg(Register::D0);
                let param_bytes = sound_dispatch_param_bytes(selector);
                let routine = (selector >> 16) & 0xFF;
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] SoundDispatch selector=${:08X} routine=${:02X} param_bytes={}",
                        selector, routine, param_bytes
                    );
                }
                match routine {
                    // SndSoundManagerVersion (routine $0C, sel $000C0008)
                    // FUNCTION SndSoundManagerVersion: NumVersion;
                    // Sound 1994, 2-201
                    0x0C => {
                        // NumVersion uses the first two bytes for major and
                        // minor release numbers, followed by release stage.
                        // Return Sound Manager 3.3.3 final so version gates
                        // that require 3.1+ take the Sound Manager 3.x path.
                        // Sound 1994, 2-34.
                        bus.write_long(sp, 0x0333_8000);
                    }

                    // SndPlayDoubleBuffer (routine $20, sel $00200008)
                    // FUNCTION SndPlayDoubleBuffer(chan: SndChannelPtr;
                    //     params: SndDoubleBufferHeaderPtr): OSErr;
                    // Stack: SP+0: params(4), SP+4: chan(4), SP+8: result(2)
                    // Sound 1994, 2-145; Inside Macintosh Volume VI, VI-577
                    0x20 => {
                        let params_ptr = bus.read_long(sp);
                        let chan_ptr = bus.read_long(sp + 4);
                        if trace_sound_enabled() {
                            eprintln!(
                                "[SOUND] SndPlayDoubleBuffer chan=${:08X} params=${:08X}",
                                chan_ptr, params_ptr
                            );
                        }
                        let err = self.snd_play_double_buffer(bus, chan_ptr, params_ptr);
                        bus.write_word(sp + param_bytes, err as u16);
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // GetSysBeepVolume (routine $24, sel $02240024)
                    // FUNCTION GetSysBeepVolume(VAR level: LongInt): OSErr;
                    // Sound 1994, 2-203
                    0x24 => {
                        let level_ptr = bus.read_long(sp);
                        if level_ptr != 0 {
                            let level = self.sound_manager.sys_beep_volume();
                            bus.write_long(level_ptr, level);
                            if trace_sound_enabled() {
                                eprintln!(
                                    "[SOUND] GetSysBeepVolume level=${:08X} -> ${:08X}",
                                    level_ptr, level
                                );
                            }
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SetSysBeepVolume (routine $28, sel $02280024)
                    // FUNCTION SetSysBeepVolume(level: LongInt): OSErr;
                    // Sound 1994, 2-204
                    0x28 => {
                        let level = bus.read_long(sp);
                        self.sound_manager.set_sys_beep_volume(level);
                        if trace_sound_enabled() {
                            eprintln!("[SOUND] SetSysBeepVolume level=${:08X}", level);
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // GetDefaultOutputVolume (routine $2C, sel $022C0024)
                    // FUNCTION GetDefaultOutputVolume(VAR level: LongInt): OSErr;
                    // Sound 1994, 2-205
                    0x2C => {
                        let level_ptr = bus.read_long(sp);
                        if level_ptr != 0 {
                            let level = self.sound_manager.default_output_volume();
                            bus.write_long(level_ptr, level);
                            if trace_sound_enabled() {
                                eprintln!(
                                    "[SOUND] GetDefaultOutputVolume level=${:08X} -> ${:08X}",
                                    level_ptr, level
                                );
                            }
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SetDefaultOutputVolume (routine $30, sel $02300024)
                    // FUNCTION SetDefaultOutputVolume(level: LongInt): OSErr;
                    // Sound 1994, 2-206
                    0x30 => {
                        let level = bus.read_long(sp);
                        self.sound_manager.set_default_output_volume(level);
                        if trace_sound_enabled() {
                            eprintln!("[SOUND] SetDefaultOutputVolume level=${:08X}", level);
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SndStartFilePlay (routine $00, sel $0D000008)
                    // FUNCTION SndStartFilePlay(chan: SndChannelPtr; fRefNum: Integer;
                    //     resNum: Integer; bufferSize: LongInt; theBuffer: Ptr;
                    //     theSelection: AudioSelectionPtr; theCompletion: ProcPtr;
                    //     async: Boolean): OSErr;
                    // Sound 1994, 2-138
                    0x00 => {
                        let async_flag = bus.read_word(sp) as i16;
                        let completion = bus.read_long(sp + 2);
                        let selection = bus.read_long(sp + 6);
                        let the_buffer = bus.read_long(sp + 10);
                        let buffer_size = bus.read_long(sp + 14);
                        let res_num = bus.read_word(sp + 18) as i16;
                        let f_ref_num = bus.read_word(sp + 20) as i16;
                        let chan_ptr = bus.read_long(sp + 22);
                        if trace_sound_enabled() {
                            eprintln!(
                                "[SOUND] SndStartFilePlay chan=${:08X} fRefNum={} resNum={} buffer_size={} buffer=${:08X} selection=${:08X} completion=${:08X} async={}",
                                chan_ptr,
                                f_ref_num,
                                res_num,
                                buffer_size,
                                the_buffer,
                                selection,
                                completion,
                                async_flag
                            );
                        }

                        let err = self.snd_start_file_play(
                            bus, chan_ptr, f_ref_num, res_num, selection, completion, async_flag,
                        );

                        bus.write_word(sp + param_bytes, err as u16);
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SndPauseFilePlay (routine $04, sel $02040008)
                    // Sound 1994, 2-139
                    //
                    // GetSoundHeaderOffset also uses routine byte $04
                    // under selector $04040024; dispatch on the full
                    // selector before treating this as file-play pause.
                    0x04 => {
                        if selector == 0x0404_0024 {
                            let offset_ptr = bus.read_long(sp);
                            let snd_handle = bus.read_long(sp + 4);
                            let err = if snd_handle == 0 {
                                if offset_ptr != 0 {
                                    bus.write_long(offset_ptr, 0);
                                }
                                -204 // resProblem
                            } else {
                                let snd_ptr = bus.read_long(snd_handle);
                                if snd_ptr == 0 {
                                    if offset_ptr != 0 {
                                        bus.write_long(offset_ptr, 0);
                                    }
                                    -204 // resProblem
                                } else if let Some(offset) =
                                    Self::sound_header_offset_from_resource(bus, snd_ptr)
                                {
                                    if offset_ptr != 0 {
                                        bus.write_long(offset_ptr, offset);
                                    }
                                    if trace_sound_enabled() {
                                        eprintln!(
                                            "[SOUND] GetSoundHeaderOffset handle=${:08X} ptr=${:08X} -> {}",
                                            snd_handle, snd_ptr, offset
                                        );
                                    }
                                    0
                                } else {
                                    if offset_ptr != 0 {
                                        bus.write_long(offset_ptr, 0);
                                    }
                                    -206 // badFormat
                                }
                            };
                            bus.write_word(sp + param_bytes, err as u16);
                            cpu.write_reg(Register::A7, sp + param_bytes);
                        } else {
                            let chan_ptr = bus.read_long(sp);
                            if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                                chan.pause_file_playback_toggle();
                            }
                            bus.write_word(sp + param_bytes, 0); // noErr
                            cpu.write_reg(Register::A7, sp + param_bytes);
                        }
                    }

                    // SndStopFilePlay (routine $08, sel $03080008)
                    // FUNCTION SndStopFilePlay(chan: SndChannelPtr;
                    //     quietNow: Boolean): OSErr;
                    // Stack: SP+0: quietNow(2), SP+2: chan(4), SP+6: result(2)
                    // Sound 1994, 2-140
                    0x08 => {
                        let _quiet_now = bus.read_word(sp) as i16;
                        let chan_ptr = bus.read_long(sp + 2);
                        if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                            chan.quiet();
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SndChannelStatus (routine $10)
                    // FUNCTION SndChannelStatus(chan: SndChannelPtr;
                    //                           theLength: Integer;
                    //                           theStatus: SCStatusPtr): OSErr;
                    // Pascal stack frame (param_bytes = 10):
                    //   SP+0 theStatus SCStatusPtr (4)  — last-pushed
                    //   SP+4 theLength INTEGER (2)
                    //   SP+6 chan SndChannelPtr (4)     — first-pushed
                    //   SP+10 result OSErr (2)
                    // IM:Sound 6286 documents the canonical selector
                    // as $00100008 (paramSize byte = 0); MPW glue
                    // computes $05100008 (paramSize = 5 = 10 bytes).
                    // Inside Macintosh: Sound 1994, 2-101 + 2-200 (line 6267..6300)
                    //
                    // HLE: zero-fill the 24-byte SCStatus record per
                    // IM:Sound 7182..7194 layout. With no async audio
                    // playback in Systemless, all fields are documented-
                    // correct at zero: scStartTime/scEndTime/
                    // scCurrentTime = 0 (not playing from disk),
                    // scChannelBusy = FALSE (no commands processing),
                    // scChannelDisposed = FALSE (channel still
                    // valid), scChannelPaused = FALSE, scUnused = 0,
                    // scChannelAttributes = 0 (no init flags set),
                    // scCPULoad = 0 (per IM:Sound 3043: "this field
                    // is obsolete, and you should not rely on its
                    // value"). Apps using Listing 2-13's pattern
                    // (reading scChannelPaused) get the IM-correct
                    // FALSE; apps polling scChannelBusy (Listing 2-25)
                    // get FALSE — exactly what they'd see on real
                    // Mac after the channel has finished processing
                    // all commands.
                    0x10 => {
                        let the_status = bus.read_long(sp);
                        let _the_length = bus.read_word(sp + 4) as i16;
                        let _chan = bus.read_long(sp + 6);
                        if the_status != 0 {
                            // SCStatus is 24 bytes per IM:Sound 7182..7194:
                            //   scStartTime Fixed(4) + scEndTime Fixed(4)
                            //   + scCurrentTime Fixed(4) + scChannelBusy Bool(1)
                            //   + scChannelDisposed Bool(1) + scChannelPaused Bool(1)
                            //   + scUnused Bool(1) + scChannelAttributes LongInt(4)
                            //   + scCPULoad LongInt(4) = 24 bytes total
                            for off in 0..24 {
                                bus.write_byte(the_status + off, 0);
                            }
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SndManagerStatus (routine $14)
                    // FUNCTION SndManagerStatus(theLength: Integer;
                    //                           theStatus: SMStatusPtr): OSErr;
                    // Pascal stack frame (param_bytes = 6):
                    //   SP+0 theStatus SMStatusPtr (4) — last-pushed
                    //   SP+4 theLength INTEGER (2)     — first-pushed
                    //   SP+6 result OSErr (2)
                    // MPW glue computes $03140008 (paramSize = 3 = 6 bytes).
                    // Inside Macintosh: Sound 1994, 2-101 + 2-201 (line 6302..6330)
                    //
                    // HLE: fill the 6-byte PACKED SMStatus record per
                    // IM:Sound 7196..7204 layout. smNumChannels gets
                    // the live count from self.sound_manager.channels
                    // (so apps using Listing 2-14's MyGetNumChannels
                    // pattern see the correct allocated-channel
                    // count). smMaxCPULoad is the documented startup
                    // default, 100; smCurCPULoad is 0 because the HLE
                    // mixer does not consume guest CPU budget. Sound
                    // 1994, 2-39.
                    0x14 => {
                        let _the_length = bus.read_word(sp + 4) as i16;
                        let the_status = bus.read_long(sp);
                        if the_status != 0 {
                            // SMStatus is 6 bytes (PACKED RECORD per
                            // IM:Sound 7198..7203):
                            //   smMaxCPULoad INTEGER (2)
                            //   smNumChannels INTEGER (2)
                            //   smCurCPULoad INTEGER (2)
                            bus.write_word(the_status, 100); // smMaxCPULoad default
                            let num_channels = self.sound_manager.channels.len() as u16;
                            bus.write_word(the_status + 2, num_channels);
                            bus.write_word(the_status + 4, 0); // smCurCPULoad
                        }
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // SndGetSysBeepState (routine $18, sel $00180008)
                    // Sound 1994, 2-202
                    //
                    // Sound Input Manager also uses routine byte $18 under
                    // selector family $0014: SPBOpenDevice is $05180014.
                    // Disambiguate on the low selector word before treating
                    // this as the beep-state procedure.
                    0x18 => {
                        if selector & 0xFFFF == 0x0014 {
                            // SPBOpenDevice(deviceName, permission, VAR inRefNum): OSErr.
                            // Stack: SP+0 inRefNum ptr, SP+4 permission,
                            // SP+6 deviceName ptr, SP+10 result.
                            let in_ref_num_ptr = bus.read_long(sp);
                            let _permission = bus.read_word(sp + 4) as i16;
                            let _device_name = bus.read_long(sp + 6);
                            if in_ref_num_ptr != 0 {
                                bus.write_long(in_ref_num_ptr, 1);
                            }
                            bus.write_word(sp + param_bytes, 0); // noErr
                            cpu.write_reg(Register::A7, sp + param_bytes);
                        } else {
                            // Write 1 (sysBeepEnable) to the VAR param.
                            let state_ptr = bus.read_long(sp);
                            if state_ptr != 0 {
                                bus.write_word(state_ptr, 1);
                            }
                            bus.write_word(sp + param_bytes, 0); // noErr
                            cpu.write_reg(Register::A7, sp + param_bytes);
                        }
                    }

                    // SndSetSysBeepState (routine $1C, sel $001C0008)
                    // Sound 1994, 2-202
                    0x1C => {
                        bus.write_word(sp + param_bytes, 0); // noErr
                        cpu.write_reg(Register::A7, sp + param_bytes);
                    }

                    // GetSoundHeaderOffset (routine $0404>>8=04, sel $04040024)
                    // Actually routine byte is at bits 16-23, so this would be
                    // the same $04 as SndPauseFilePlay. The full selector
                    // distinguishes them via the low word. For now, $04 handles
                    // both — SndPauseFilePlay is a no-op anyway.
                    _ => {
                        // Unknown selector — pop params and return noErr.
                        if param_bytes > 0 {
                            bus.write_word(sp + param_bytes, 0); // noErr
                            cpu.write_reg(Register::A7, sp + param_bytes);
                        }
                    }
                }
                cpu.write_reg(Register::D0, 0);
                Ok(())
            }

            // SndNewChannel ($A807)
            // FUNCTION SndNewChannel(VAR chan: SndChannelPtr; synth: Integer;
            //                        init: LongInt; userRoutine: ProcPtr): OSErr;
            // Stack: SP+0: userRoutine(4), SP+4: init(4), SP+8: synth(2),
            //        SP+10: chan(4, VAR), SP+14: result(2)
            // Sound 1994, 2-195
            // SndNewChannel ($A807): Accepts synth 0 (no linked synth)
            // or the documented built-in synth ids 1/3/5; other nonzero
            // synth ids fail with resProblem (-204) before any channel
            // allocation or caller memory write. For successful calls,
            // allocates/initializes the guest channel record with callback
            // procedure and tracks it in SoundManager.
            (true, 0x007) => {
                let sp = cpu.read_reg(Register::A7);
                let user_routine = bus.read_long(sp);
                let init = bus.read_long(sp + 4);
                let synth = bus.read_word(sp + 8) as i16;
                let chan_ptr_ptr = bus.read_long(sp + 10);

                let supported_synth = matches!(
                    synth,
                    0 | SQUARE_WAVE_SYNTH_ID | WAVE_TABLE_SYNTH_ID | SAMPLED_SYNTH_ID
                );
                if !supported_synth {
                    bus.write_word(sp + 14, (-204i16) as u16); // resProblem
                    cpu.write_reg(Register::A7, sp + 14);
                    Ok(())
                } else {
                    let existing_guest_ptr = if chan_ptr_ptr != 0 {
                        bus.read_long(chan_ptr_ptr)
                    } else {
                        0
                    };
                    let (guest_ptr, allocated) = if existing_guest_ptr != 0 {
                        (existing_guest_ptr, false)
                    } else {
                        (bus.alloc(GUEST_SND_CHANNEL_SIZE), true)
                    };
                    let preserved_user_info = if existing_guest_ptr != 0 {
                        bus.read_long(guest_ptr + 12)
                    } else {
                        0
                    };
                    let q_length = if existing_guest_ptr != 0 {
                        bus.read_word(guest_ptr + 30).max(1)
                    } else {
                        128
                    };

                    // Initialize key fields in guest memory so the game can inspect them.
                    // SndChannel layout: nextChan(4), firstMod(4), callBack(4), userInfo(4),
                    //   wait(4), cmdInProgress(8), flags(2), qLength(2), qHead(2), qTail(2)
                    bus.write_long(guest_ptr, 0); // nextChan
                    bus.write_long(guest_ptr + 4, 0); // firstMod
                    bus.write_long(guest_ptr + 8, user_routine); // callBack
                    bus.write_long(guest_ptr + 12, preserved_user_info); // userInfo
                    bus.write_long(guest_ptr + 16, 0); // wait
                    bus.write_long(guest_ptr + 20, 0); // cmdInProgress.cmd/param1
                    bus.write_long(guest_ptr + 24, 0); // cmdInProgress.param2
                    bus.write_word(guest_ptr + 28, 0); // flags
                    bus.write_word(guest_ptr + 30, q_length); // qLength
                    bus.write_word(guest_ptr + 32, 0); // qHead
                    bus.write_word(guest_ptr + 34, 0); // qTail

                    if chan_ptr_ptr != 0 {
                        bus.write_long(chan_ptr_ptr, guest_ptr);
                    }

                    self.sound_manager.remove_channel(guest_ptr);
                    let mut chan = SndChannel::new(guest_ptr, allocated);
                    chan.callback_addr = user_routine;
                    self.sound_manager.channels.push(chan);
                    if trace_sound_enabled() {
                        // Log caller PC + surrounding bytes for back-referencing
                        // sound-init call sites.
                        let pc = cpu.read_reg(Register::PC);
                        eprintln!(
                            "[SOUND] SndNewChannel synth={} init=${:08X} userRoutine=${:08X} -> chan=${:08X} (PC=${:08X})",
                            synth, init, user_routine, guest_ptr, pc
                        );
                        // 8 words of instruction context around PC, starting
                        // 8 bytes before (typical post-trap PC points to the
                        // instruction AFTER the $A807 word; walking back
                        // gives us the call site).
                        let ctx_start = pc.saturating_sub(64);
                        eprint!("[SOUND] SndNewChannel ctx @${:08X}:", ctx_start);
                        for i in 0..40 {
                            eprint!(" {:04X}", bus.read_word(ctx_start + (i as u32) * 2));
                        }
                        eprintln!();
                        // Also read A5 so we can decode -$4052(A5) pushes.
                        let a5 = cpu.read_reg(Register::A5);
                        eprintln!(
                            "[SOUND] SndNewChannel A5=${:08X}  -$4052(A5)=${:08X}  -$4050(A5)=${:08X}  -$4054(A5)=${:08X}",
                            a5,
                            bus.read_long(a5.wrapping_sub(0x4052)),
                            bus.read_long(a5.wrapping_sub(0x4050)),
                            bus.read_long(a5.wrapping_sub(0x4054))
                        );
                    }
                    bus.write_word(sp + 14, 0); // noErr
                    cpu.write_reg(Register::A7, sp + 14);
                    Ok(())
                }
            }

            // SndDisposeChannel ($A801)
            // FUNCTION SndDisposeChannel(chan: SndChannelPtr; quietNow: BOOLEAN): OSErr;
            // Stack: SP+0: quietNow(2), SP+2: chan(4), SP+6: result(2)
            // Sound 1994, 2-196
            // SndDisposeChannel ($A801): Removes channel from SoundManager
            (true, 0x001) => {
                let sp = cpu.read_reg(Register::A7);
                let quiet_now = bus.read_word(sp) as i16;
                let chan_ptr = bus.read_long(sp + 2);

                self.release_sound_channel(bus, chan_ptr);
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] SndDisposeChannel chan=${:08X} quietNow={}",
                        chan_ptr, quiet_now
                    );
                }

                bus.write_word(sp + 6, 0); // noErr
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            // SndDoCommand ($A803)
            // pascal OSErr SndDoCommand(SndChannelPtr chan, const SndCommand *cmd,
            //                           Boolean noWait);
            // Stack: SP+0: noWait(2), SP+2: cmd(4, ptr), SP+6: chan(4), SP+10: result(2)
            // Sound 1994, 2-130
            // SndDoCommand ($A803): Public entry point rejects NIL/unknown
            // channels with badChannel (-205). On a busy channel, commands
            // enter the channel FIFO; SndDoImmediate is the queue-bypass path.
            // Idle-channel commands execute immediately in the HLE so query
            // commands like getRateCmd still return observable results.
            (true, 0x003) => {
                let sp = cpu.read_reg(Register::A7);
                let no_wait = bus.read_word(sp) as i16;
                let cmd_ptr = bus.read_long(sp + 2);
                let chan_ptr = bus.read_long(sp + 6);
                let valid_channel = chan_ptr != 0
                    && self
                        .sound_manager
                        .channels
                        .iter()
                        .any(|chan| chan.guest_ptr == chan_ptr);

                let err = if valid_channel {
                    // Only touch the guest command record after the channel
                    // has been validated.
                    let cmd = self.read_snd_command(bus, cmd_ptr);
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] SndDoCommand chan=${:08X} noWait={} cmd={} param1={} param2=${:08X}",
                            chan_ptr, no_wait, cmd.cmd, cmd.param1, cmd.param2
                        );
                    }
                    let channel_busy = self
                        .sound_manager
                        .find_channel_mut(chan_ptr)
                        .map(|chan| chan.has_active_playback() || chan.double_buffer.is_some())
                        .unwrap_or(false);
                    if channel_busy {
                        if cmd.cmd == sound::cmd::CALLBACK {
                            if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                                chan.queue_callback(cmd);
                                0
                            } else {
                                -205 // badChannel
                            }
                        } else {
                            self.enqueue_guest_sound_command(bus, chan_ptr, cmd)
                        }
                    } else {
                        self.execute_sound_command(bus, chan_ptr, cmd);
                        0
                    }
                } else {
                    -205 // badChannel
                };

                bus.write_word(sp + 10, err as u16);
                cpu.write_reg(Register::A7, sp + 10);
                Ok(())
            }

            // SndDoImmediate ($A804)
            // pascal OSErr SndDoImmediate(SndChannelPtr chan, const SndCommand *cmd);
            // Stack: SP+0: cmd(4, ptr), SP+4: chan(4), SP+8: result(2)
            // Sound 1994, 2-131
            // SndDoImmediate ($A804): Same command support as SndDoCommand,
            // including badChannel (-205) for NIL/unknown channels.
            (true, 0x004) => {
                let sp = cpu.read_reg(Register::A7);
                let cmd_ptr = bus.read_long(sp);
                let chan_ptr = bus.read_long(sp + 4);
                let valid_channel = chan_ptr != 0
                    && self
                        .sound_manager
                        .channels
                        .iter()
                        .any(|chan| chan.guest_ptr == chan_ptr);

                let err = if valid_channel {
                    // Only touch the guest command record after the channel
                    // has been validated.
                    let cmd = self.read_snd_command(bus, cmd_ptr);
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] SndDoImmediate chan=${:08X} cmd={} param1={} param2=${:08X}",
                            chan_ptr, cmd.cmd, cmd.param1, cmd.param2
                        );
                    }
                    self.execute_sound_command(bus, chan_ptr, cmd);
                    0
                } else {
                    -205 // badChannel
                };

                bus.write_word(sp + 8, err as u16);
                cpu.write_reg(Register::A7, sp + 8);
                Ok(())
            }

            // SndAddModifier ($A802)
            // FUNCTION SndAddModifier(chan: SndChannelPtr; modifier: ProcPtr;
            //                         id: INTEGER; init: LongInt): OSErr;
            // Stack: SP+0 init(4), SP+4 id(2), SP+6 modifier(4), SP+10 chan(4), SP+14 result(2)
            // Inside Macintosh: Sound 1994, 2-197 (line 6790..6820)
            //
            // HLE: per IM:Sound 6808 "for internal Sound Manager use
            // only. You should not call it in your application."
            // On stock BasiliskII System 7.5.3, a direct application
            // call with a NIL `chan` still pops the documented
            // 14-byte frame and returns badChannel (-205). Systemless matches that modern
            // runtime envelope instead of the older nominal-success
            // prose, because we do not model Sound Manager internal
            // modifier linkage or 68K guest-fn dispatch (same
            // compromise as ModalDialog filterProc, Alert filterProc,
            // Pack1 LSearch searchProc).
            //
            // Regression coverage:
            //   sound::tests::sndaddmodifier_nil_channel_returns_badchannel_and_consumes_pascal_frame
            // SndAddModifier ($A802): Pops 14-byte Pascal frame (init LongInt + id INTEGER + modifier ProcPtr + chan SndChannelPtr) per IM:Sound 1994 2-197 and writes badChannel (-205) at SP+14 to match BasiliskII's observed NIL-channel behavior for this obsolete internal-only trap.
            (true, 0x002) => {
                let sp = cpu.read_reg(Register::A7);
                bus.write_word(sp + 14, (-205i16) as u16); // badChannel
                cpu.write_reg(Register::A7, sp + 14);
                Ok(())
            }

            // SndControl ($A806)
            // FUNCTION SndControl(id: INTEGER; VAR cmd: SndCommand): OSErr;
            // Stack: SP+0 cmd(4, ptr), SP+4 id(2), SP+6 result(2)
            // Inside Macintosh: Sound 1994, 2-134 to 2-135
            //
            // HLE: Systemless reports a Sound Manager 3.x NumVersion via
            // SndSoundManagerVersion ($A800 selector $0C), so the
            // legacy control-query surface is narrow. Still, callers
            // that do use SndControl expect the documented query
            // commands to round-trip through the SndCommand record.
            //
            // We therefore model the 3.x-compatible subset:
            //   - availableCmd: report zero-init support for the
            //     documented built-in synth ids (square/wavetable/
            //     sampled) by rewriting param1 to 1; otherwise 0.
            //   - totalLoadCmd/loadCmd: obsolete in 3.x, normalize
            //     the documented load-factor output word to 0.
            //   - versionCmd: report a Sound Manager 3.x synthesizer
            //     version for the documented built-in synth ids.
            //
            // Unknown commands still succeed as noErr and preserve the
            // caller's record contents, which keeps the HLE tolerant of
            // stray information probes while making the documented
            // query subset observable to fixture code.
            //
            // Regression coverage:
            //   sound::tests::sndcontrol_consumes_cmdptr_and_id_and_returns_noerr
            //   sound::tests::sndcontrol_availablecmd_zero_init_known_synth_sets_param1_true
            //   sound::tests::sndcontrol_versioncmd_sampled_synth_reports_sound_manager_3_version
            // SndControl ($A806): Pops 6-byte Pascal frame (cmd Ptr + id INTEGER) per IM:Sound 1994 2-134..2-135; writes noErr at SP+6 and normalizes the documented control-query outputs for availableCmd/versionCmd/totalLoadCmd/loadCmd.
            (true, 0x006) => {
                let sp = cpu.read_reg(Register::A7);
                let cmd_ptr = bus.read_long(sp);
                let synth_id = bus.read_word(sp + 4);
                if cmd_ptr != 0 {
                    let mut cmd = self.read_snd_command(bus, cmd_ptr);
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] SndControl id={} cmd={} param1={} param2=${:08X}",
                            synth_id, cmd.cmd, cmd.param1, cmd.param2
                        );
                    }
                    let supported_synth = matches!(
                        synth_id as i16,
                        SQUARE_WAVE_SYNTH_ID | WAVE_TABLE_SYNTH_ID | SAMPLED_SYNTH_ID
                    );
                    match cmd.cmd {
                        sound::cmd::AVAILABLE => {
                            cmd.param1 = if supported_synth && cmd.param2 == 0 {
                                1
                            } else {
                                0
                            };
                        }
                        sound::cmd::VERSION => {
                            cmd.param1 = 0;
                            cmd.param2 = if supported_synth {
                                SOUND_MANAGER_3_SYNTH_VERSION
                            } else {
                                0
                            };
                        }
                        sound::cmd::TOTAL_LOAD | sound::cmd::LOAD => {
                            cmd.param1 = 0;
                        }
                        _ => {}
                    }
                    self.write_guest_snd_command(bus, cmd_ptr, Some(cmd));
                }
                bus.write_word(sp + 6, 0); // noErr
                cpu.write_reg(Register::A7, sp + 6);
                Ok(())
            }

            _ => return None,
        })
    }

    /// Read a SndCommand record from guest memory.
    /// SndCommand: cmd(2) + param1(2) + param2(4) = 8 bytes.
    /// Sound 1994, 2-99
    fn read_snd_command(&self, bus: &MacMemoryBus, addr: u32) -> SndCommand {
        SndCommand {
            cmd: bus.read_word(addr),
            param1: bus.read_word(addr + 2) as i16,
            param2: bus.read_long(addr + 4),
        }
    }

    fn write_guest_snd_command(&self, bus: &mut MacMemoryBus, addr: u32, cmd: Option<SndCommand>) {
        let cmd = cmd.unwrap_or(SndCommand {
            cmd: 0,
            param1: 0,
            param2: 0,
        });
        bus.write_word(addr, cmd.cmd);
        bus.write_word(addr + 2, cmd.param1 as u16);
        bus.write_long(addr + 4, cmd.param2);
    }

    fn enqueue_guest_sound_command(
        &mut self,
        bus: &mut MacMemoryBus,
        chan_ptr: u32,
        cmd: SndCommand,
    ) -> i16 {
        // IM:Sound 1994 pp. 2-13 and 2-99: SndChannel owns a FIFO queue of
        // SndCommand records after qHead/qTail.
        let q_length = bus
            .read_word(chan_ptr + GUEST_SND_CHANNEL_Q_LENGTH_OFFSET)
            .max(1) as usize;
        let q_head = bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET) as usize % q_length;
        let q_tail = bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET) as usize % q_length;
        let next_tail = (q_tail + 1) % q_length;
        if next_tail == q_head {
            return -203; // queueFull
        }

        let cmd_addr = chan_ptr + GUEST_SND_CHANNEL_QUEUE_OFFSET + (q_tail as u32) * 8;
        self.write_guest_snd_command(bus, cmd_addr, Some(cmd));
        bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET, next_tail as u16);
        0
    }

    pub(crate) fn sync_guest_sound_channel_state(&self, bus: &mut MacMemoryBus) {
        for chan in &self.sound_manager.channels {
            if chan.guest_ptr == 0 {
                continue;
            }

            let active = chan.has_active_playback() || chan.double_buffer.is_some();
            bus.write_word(
                chan.guest_ptr + GUEST_SND_CHANNEL_FLAGS_OFFSET,
                if active { 1 } else { 0 },
            );
            if !active {
                self.write_guest_snd_command(
                    bus,
                    chan.guest_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET,
                    None,
                );
            }
        }
    }

    /// Drain queue entries written directly into the guest SndChannel record.
    ///
    /// Sound 1994 documents the queue embedded in SndChannel, and Executor's
    /// interrupt-driven sound callback consumes commands from that queue.
    /// Some games update it directly instead of calling SndDoCommand for every
    /// command. Keep the guest-visible queue/head/tail authoritative here.
    pub(crate) fn service_guest_sound_queues(&mut self, bus: &mut MacMemoryBus) {
        let channel_ptrs: Vec<u32> = self
            .sound_manager
            .channels
            .iter()
            .map(|chan| chan.guest_ptr)
            .filter(|ptr| *ptr != 0)
            .collect();

        for chan_ptr in channel_ptrs {
            let mut serviced = 0usize;
            loop {
                serviced += 1;
                if serviced > 16 {
                    break;
                }

                let active = self
                    .sound_manager
                    .find_channel_mut(chan_ptr)
                    .map(|chan| chan.has_active_playback() || chan.double_buffer.is_some())
                    .unwrap_or(false);
                if active {
                    break;
                }

                let q_length = bus
                    .read_word(chan_ptr + GUEST_SND_CHANNEL_Q_LENGTH_OFFSET)
                    .max(1) as usize;
                let q_head = bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET) as usize;
                let q_tail = bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET) as usize;
                if q_head == q_tail {
                    let cmd = self
                        .read_snd_command(bus, chan_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET);
                    if cmd.cmd == 0 {
                        break;
                    }
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] guest cmdInProgress chan=${:08X} cmd={} param1={} param2=${:08X}",
                            chan_ptr, cmd.cmd, cmd.param1, cmd.param2
                        );
                    }
                    self.execute_sound_command(bus, chan_ptr, cmd);
                    let still_active = self
                        .sound_manager
                        .find_channel_mut(chan_ptr)
                        .map(|chan| chan.has_active_playback() || chan.double_buffer.is_some())
                        .unwrap_or(false);
                    if !still_active {
                        self.write_guest_snd_command(
                            bus,
                            chan_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET,
                            None,
                        );
                    }
                    continue;
                }

                let slot = q_head % q_length;
                let cmd_addr = chan_ptr + GUEST_SND_CHANNEL_QUEUE_OFFSET + (slot as u32) * 8;
                let cmd = self.read_snd_command(bus, cmd_addr);
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] guest queue chan=${:08X} head={} tail={} cmd={} param1={} param2=${:08X}",
                        chan_ptr, q_head, q_tail, cmd.cmd, cmd.param1, cmd.param2
                    );
                }
                bus.write_word(
                    chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET,
                    ((slot + 1) % q_length) as u16,
                );
                self.write_guest_snd_command(
                    bus,
                    chan_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET,
                    Some(cmd.clone()),
                );
                self.execute_sound_command(bus, chan_ptr, cmd);
            }
        }

        self.sync_guest_sound_channel_state(bus);
    }

    /// Execute a sound command on a channel, handling bufferCmd to load samples.
    fn execute_sound_command(&mut self, bus: &mut MacMemoryBus, chan_ptr: u32, cmd: SndCommand) {
        self.sound_manager.debug_cmd_count += 1;
        // Dedup-record this cmd code so runtime diagnostics can surface
        // the cmd-mix distribution per-game.
        if !self.sound_manager.debug_cmd_codes_seen.contains(&cmd.cmd) {
            self.sound_manager.debug_cmd_codes_seen.push(cmd.cmd);
        }
        match cmd.cmd {
            sound::cmd::NULL => {}
            sound::cmd::QUIET => {
                if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                    chan.quiet();
                }
            }
            sound::cmd::FLUSH => {
                if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                    chan.flush();
                }
            }
            sound::cmd::REST => {
                // Sound 1994, 2-95: restCmd inserts a rest of `param1`
                // half-frames in a sequence-channel (note synth path).
                // For a sample-mixing channel rests don't translate to
                // anything observable in the PCM stream; the channel
                // either has buffered samples to play or doesn't. Accept
                // as a recognised no-op.
            }
            sound::cmd::CALLBACK => {
                if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                    if chan.has_active_playback() || chan.double_buffer.is_some() {
                        chan.queue_callback(cmd);
                    } else {
                        let callback_addr = chan.callback_addr;
                        let chan_ptr = chan.guest_ptr;
                        if callback_addr != 0 {
                            self.sound_manager.pending_sound_callbacks.push(
                                crate::sound::PendingSoundCallback::Command {
                                    callback_addr,
                                    chan_ptr,
                                    cmd,
                                },
                            );
                        }
                    }
                }
            }
            sound::cmd::VOLUME => {
                if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                    chan.set_volume(cmd.param2);
                }
            }
            sound::cmd::BUFFER | sound::cmd::SOUND => {
                self.execute_buffer_cmd(bus, chan_ptr, &cmd);
            }
            sound::cmd::RATE => {
                if let Some(chan) = self.sound_manager.find_channel_mut(chan_ptr) {
                    chan.set_rate(cmd.param2);
                }
            }
            sound::cmd::GET_RATE => {
                if cmd.param2 != 0 {
                    let rate = self
                        .sound_manager
                        .find_channel_mut(chan_ptr)
                        .map(|c| c.current_rate())
                        .unwrap_or(0x0001_0000);
                    bus.write_long(cmd.param2, rate);
                }
            }
            _ => {
                if !self.sound_manager.debug_unhandled_cmds.contains(&cmd.cmd) {
                    self.sound_manager.debug_unhandled_cmds.push(cmd.cmd);
                }
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] unhandled cmd={} param1={} param2=${:08X} chan=${:08X}",
                        cmd.cmd, cmd.param1, cmd.param2, chan_ptr
                    );
                }
            }
        }
    }

    /// Handle bufferCmd: read SoundHeader from guest memory and start playback.
    /// Supports stdSH (encode=$00), extSH (encode=$FF), and MACE cmpSH (encode=$FE).
    ///
    /// stdSH layout (Sound 1994, 2-104):
    ///   +0: samplePtr (long), +4: length (long), +8: sampleRate (Fixed),
    ///   +12: loopStart, +16: loopEnd, +20: encode ($00), +21: baseFreq,
    ///   +22: sampleArea — unsigned 8-bit mono PCM
    ///
    /// extSH layout (Sound 1994, 2-106):
    ///   +0: samplePtr (long), +4: numChannels (long), +8: sampleRate (Fixed),
    ///   +12: loopStart, +16: loopEnd, +20: encode ($FF), +21: baseFreq,
    ///   +22: numFrames (long), +26: AIFFSampleRate (10 bytes),
    ///   +36: markerChunk (4), +40: instrumentChunks (4), +44: AESRecording (4),
    ///   +48: sampleSize (word), +50: futureUse (14 bytes),
    ///   +64: sampleArea — interleaved, 8 or 16 bit
    ///
    /// cmpSH layout (Sound 1994, 2-109):
    ///   +0: samplePtr, +4: numChannels, +8: sampleRate, +20: encode ($FE),
    ///   +22: numFrames, +40: format, +56: compressionID, +58: packetSize,
    ///   +62: sampleSize, +64: compressed sampleArea.
    fn execute_buffer_cmd(&mut self, bus: &mut MacMemoryBus, chan_ptr: u32, cmd: &SndCommand) {
        self.sound_manager.debug_buffer_cmd_count += 1;
        let header_addr = cmd.param2;
        if header_addr == 0 {
            if trace_sound_enabled() {
                eprintln!(
                    "[SOUND] bufferCmd with null header on chan=${:08X}",
                    chan_ptr
                );
            }
            return;
        }

        let sample_ptr = bus.read_long(header_addr);
        let sample_rate = bus.read_long(header_addr + 8);
        let encode = bus.read_byte(header_addr + 20);
        if trace_sound_enabled() {
            eprintln!(
                "[SOUND] bufferCmd chan=${:08X} header=${:08X} sample_ptr=${:08X} sample_rate=${:08X} encode=${:02X}",
                chan_ptr, header_addr, sample_ptr, sample_rate, encode
            );
        }

        let samples = match encode {
            0x00 => {
                // stdSH: unsigned 8-bit mono
                let length = bus.read_long(header_addr + 4) as usize;
                if length == 0 {
                    return;
                }
                let data_addr = if sample_ptr != 0 {
                    sample_ptr
                } else {
                    header_addr + 22
                };
                let mut buf = vec![0u8; length];
                for (i, byte) in buf.iter_mut().enumerate() {
                    *byte = bus.read_byte(data_addr + i as u32);
                }
                buf
            }
            0xFF => {
                // extSH: multi-channel, 8 or 16 bit
                let num_channels = bus.read_long(header_addr + 4) as usize;
                let num_frames = bus.read_long(header_addr + 22) as usize;
                let sample_size = bus.read_word(header_addr + 48) as usize;
                if num_frames == 0 || num_channels == 0 {
                    return;
                }
                let data_addr = if sample_ptr != 0 {
                    sample_ptr
                } else {
                    header_addr + 64
                };
                let bytes_per_sample = sample_size / 8;
                let total_bytes = num_channels * num_frames * bytes_per_sample;
                // Read raw interleaved data
                let mut raw = vec![0u8; total_bytes];
                for (i, byte) in raw.iter_mut().enumerate() {
                    *byte = bus.read_byte(data_addr + i as u32);
                }
                // Convert to unsigned 8-bit mono
                let mut buf = vec![0u8; num_frames];
                for (frame, dst) in buf.iter_mut().enumerate() {
                    let mut accum: i32 = 0;
                    for ch in 0..num_channels {
                        let offset = (frame * num_channels + ch) * bytes_per_sample;
                        if sample_size == 16 {
                            // Big-endian signed 16-bit
                            let s16 = i16::from_be_bytes([raw[offset], raw[offset + 1]]);
                            accum += (s16 >> 8) as i32; // scale to -128..127
                        } else {
                            // 8-bit unsigned (same as stdSH)
                            accum += raw[offset] as i32 - 128;
                        }
                    }
                    // Mix channels and convert back to unsigned 8-bit
                    let mixed = accum / num_channels as i32;
                    *dst = (mixed + 128).clamp(0, 255) as u8;
                }
                buf
            }
            0xFE => {
                // cmpSH: compressed sampled sound. Playroom stores most
                // effects as mono MACE 3:1 packets (`compressionID` 3).
                let num_channels = bus.read_long(header_addr + 4) as usize;
                let num_frames = bus.read_long(header_addr + 22) as usize;
                let format = bus.read_long(header_addr + 40);
                let compression_id = bus.read_word(header_addr + 56) as i16;
                let packet_size = bus.read_word(header_addr + 58) as usize;
                let sample_size = bus.read_word(header_addr + 62) as usize;
                if num_frames == 0 || num_channels == 0 {
                    return;
                }
                let data_addr = if sample_ptr != 0 {
                    sample_ptr
                } else {
                    header_addr + 64
                };

                let mace_kind = match (compression_id, format) {
                    (3, _) | (-1, MACE3_FORMAT) => Some(MaceCompression::ThreeToOne),
                    (4, _) | (-1, MACE6_FORMAT) => Some(MaceCompression::SixToOne),
                    _ => None,
                };
                let Some(mace_kind) = mace_kind else {
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] bufferCmd: unsupported cmpSH compression={} format=${:08X} packet={} bits={} channels={}",
                            compression_id, format, packet_size, sample_size, num_channels
                        );
                    }
                    return;
                };

                if num_channels != 1 || sample_size != 8 {
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] bufferCmd: unsupported cmpSH layout compression={:?} channels={} bits={}",
                            mace_kind, num_channels, sample_size
                        );
                    }
                    return;
                }

                let expected_packet_bits = mace_kind.packet_size_bits();
                let packet_size_bits = if packet_size == 0 {
                    expected_packet_bits
                } else {
                    packet_size
                };
                if packet_size_bits != expected_packet_bits {
                    if trace_sound_enabled() {
                        eprintln!(
                            "[SOUND] bufferCmd: unsupported cmpSH packet size compression={:?} packet={} expected={}",
                            mace_kind, packet_size_bits, expected_packet_bits
                        );
                    }
                    return;
                }

                let bytes_per_packet = packet_size_bits / 8;
                let Some(total_bytes) = num_frames
                    .checked_mul(num_channels)
                    .and_then(|frames| frames.checked_mul(bytes_per_packet))
                else {
                    return;
                };
                let mut compressed = vec![0u8; total_bytes];
                for (i, byte) in compressed.iter_mut().enumerate() {
                    *byte = bus.read_byte(data_addr + i as u32);
                }

                match mace_kind {
                    MaceCompression::ThreeToOne => decode_mace3_mono_to_u8(&compressed),
                    MaceCompression::SixToOne => decode_mace6_mono_to_u8(&compressed),
                }
            }
            _ => {
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] bufferCmd: unsupported encode=${:02X} at header=${:08X}",
                        encode, header_addr
                    );
                }
                return;
            }
        };
        let sample_count = samples.len();

        // If no channel specified (chan_ptr == 0), use or create a default channel.
        let chan = if chan_ptr == 0 {
            if self.sound_manager.channels.is_empty() {
                self.sound_manager.channels.push(SndChannel::new(0, true));
            }
            self.sound_manager.channels.last_mut().unwrap()
        } else {
            if let Some(c) = self.sound_manager.find_channel_mut(chan_ptr) {
                c
            } else {
                // Channel not found — create one implicitly.
                self.sound_manager
                    .channels
                    .push(SndChannel::new(chan_ptr, false));
                self.sound_manager.channels.last_mut().unwrap()
            }
        };

        chan.play_buffer(samples, sample_rate, sound::PlaybackKind::Buffer, 0);
        if trace_sound_enabled() {
            eprintln!(
                "[SOUND] bufferCmd started playback on chan=${:08X} samples={} rate=${:08X}",
                chan.guest_ptr, sample_count, sample_rate
            );
        }
    }

    /// Parse a snd resource and play it.
    /// Handles format 1 and format 2.
    /// Reference: executor sound.cpp ROMlib_get_snd_cmds / C_SndPlay
    fn snd_play_resource(
        &mut self,
        bus: &mut MacMemoryBus,
        chan_ptr: u32,
        async_play: bool,
        snd_ptr: u32,
    ) -> i16 {
        let format = bus.read_word(snd_ptr) as i16;
        let alloc_size = bus.get_alloc_size(snd_ptr);
        let mut offset: u32;

        match format {
            1 => {
                if let Some(size) = alloc_size {
                    if size < 6 {
                        return -206;
                    }
                }
                // Format 1: +0=format(2), +2=num_data_types(2),
                //   then [data_type_id(2) + init_option(4)] × num_data_types,
                //   then num_commands(2), then SndCommand[...]
                let num_data_types = bus.read_word(snd_ptr + 2) as u32;
                let Some(header_end) = 4u32.checked_add(num_data_types.saturating_mul(6)) else {
                    return -206;
                };
                if let Some(size) = alloc_size {
                    if header_end
                        .checked_add(2)
                        .is_none_or(|required| required > size)
                    {
                        return -206;
                    }
                }
                offset = header_end;
            }
            2 => {
                if let Some(size) = alloc_size {
                    if size < 6 {
                        return -206;
                    }
                }
                // Format 2: +0=format(2), +2=refCount(2), then
                // numCommands(2) and SndCommand[...]. Inside Macintosh marks
                // refCount as application-owned metadata; it is not a generic
                // reference list to skip.
                offset = 4;
            }
            // IM:Sound 1994 p. 2-123 result codes: malformed resource -> badFormat.
            _ => return -206,
        }

        let num_commands = bus.read_word(snd_ptr + offset) as usize;
        offset += 2;
        if let Some(size) = alloc_size {
            let command_bytes = match (num_commands as u32).checked_mul(8) {
                Some(bytes) => bytes,
                None => return -206,
            };
            if offset
                .checked_add(command_bytes)
                .is_none_or(|required| required > size)
            {
                return -206;
            }
        }
        if trace_sound_enabled() {
            eprintln!(
                "[SOUND] snd resource ptr=${:08X} format={} num_commands={} requested_chan=${:08X}",
                snd_ptr, format, num_commands, chan_ptr
            );
        }

        // Ensure we have a channel to play on.
        let (effective_chan, temporary_channel) = if chan_ptr != 0 {
            (chan_ptr, false)
        } else {
            // IM:Sound 1994 2-122: when chan is NIL, SndPlay allocates an
            // internal channel and releases it after synchronous playback.
            // The HLE cannot block the guest until the PCM stream drains, so
            // keep the channel alive and auto-release it from the runner after
            // the mixer has consumed the pending samples.
            let guest_ptr = bus.alloc(GUEST_SND_CHANNEL_SIZE);
            bus.write_long(guest_ptr, 0); // nextChan
            bus.write_long(guest_ptr + 4, 0); // firstMod
            bus.write_long(guest_ptr + 8, 0); // callBack
            bus.write_long(guest_ptr + 12, 0); // userInfo
            bus.write_long(guest_ptr + 16, 0); // wait
            self.write_guest_snd_command(
                bus,
                guest_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET,
                None,
            );
            bus.write_word(guest_ptr + GUEST_SND_CHANNEL_FLAGS_OFFSET, 0);
            bus.write_word(guest_ptr + GUEST_SND_CHANNEL_Q_LENGTH_OFFSET, 128);
            bus.write_word(guest_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET, 0);
            bus.write_word(guest_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET, 0);
            let mut chan = SndChannel::new(guest_ptr, true);
            chan.mark_auto_dispose_when_idle();
            self.sound_manager.channels.push(chan);
            (guest_ptr, true)
        };

        for i in 0..num_commands {
            let cmd_offset = offset + (i as u32) * 8;
            let mut cmd_word = bus.read_word(snd_ptr + cmd_offset);
            let param1 = bus.read_word(snd_ptr + cmd_offset + 2) as i16;
            let mut param2 = bus.read_long(snd_ptr + cmd_offset + 4);

            // dataOffsetFlag: if bit 15 of cmd is set, param2 is an offset
            // from the start of the resource data, not an absolute pointer.
            // Reference: executor sound.cpp lines 368-372
            if cmd_word & 0x8000 != 0 {
                param2 += snd_ptr;
                cmd_word &= !0x8000;
            }

            // Format 2 with first command being soundCmd: convert to bufferCmd.
            // Reference: executor sound.cpp lines 375-378
            if format == 2 && i == 0 && cmd_word == sound::cmd::SOUND {
                cmd_word = sound::cmd::BUFFER;
            }

            let cmd = SndCommand {
                cmd: cmd_word,
                param1,
                param2,
            };
            if trace_sound_enabled() {
                eprintln!(
                    "[SOUND] snd cmd[{}] chan=${:08X} cmd={} param1={} param2=${:08X}",
                    i, effective_chan, cmd.cmd, cmd.param1, cmd.param2
                );
            }

            let queue_for_later = async_play
                && effective_chan != 0
                && self
                    .sound_manager
                    .find_channel_mut(effective_chan)
                    .map(|chan| {
                        chan.has_active_playback()
                            || chan.double_buffer.is_some()
                            || bus.read_word(effective_chan + GUEST_SND_CHANNEL_Q_HEAD_OFFSET)
                                != bus.read_word(effective_chan + GUEST_SND_CHANNEL_Q_TAIL_OFFSET)
                    })
                    .unwrap_or(false);

            if queue_for_later {
                let err = self.enqueue_guest_sound_command(bus, effective_chan, cmd);
                if err != 0 {
                    return err;
                }
            } else {
                self.execute_sound_command(bus, effective_chan, cmd);
            }
        }

        if temporary_channel
            && !self
                .sound_manager
                .find_channel_mut(effective_chan)
                .map(|chan| chan.has_active_playback() || chan.double_buffer.is_some())
                .unwrap_or(false)
        {
            self.release_sound_channel(bus, effective_chan);
        }

        0
    }

    fn sound_header_offset_from_resource(bus: &MacMemoryBus, snd_ptr: u32) -> Option<u32> {
        let format = bus.read_word(snd_ptr);
        let alloc_size = bus.get_alloc_size(snd_ptr);
        let offset = match format {
            1 => {
                if let Some(size) = alloc_size {
                    if size < 6 {
                        return None;
                    }
                }
                let num_data_types = bus.read_word(snd_ptr + 2) as u32;
                let header_end = 4u32.checked_add(num_data_types.checked_mul(6)?)?;
                if let Some(size) = alloc_size {
                    if header_end.checked_add(2)? > size {
                        return None;
                    }
                }
                header_end
            }
            2 => {
                if let Some(size) = alloc_size {
                    if size < 6 {
                        return None;
                    }
                }
                4
            }
            _ => return None,
        };

        let num_commands = bus.read_word(snd_ptr + offset) as usize;
        let commands_offset = offset.checked_add(2)?;
        if let Some(size) = alloc_size {
            let command_bytes = (num_commands as u32).checked_mul(8)?;
            if commands_offset.checked_add(command_bytes)? > size {
                return None;
            }
        }

        for i in 0..num_commands {
            let cmd_offset = commands_offset + (i as u32) * 8;
            let cmd_word = bus.read_word(snd_ptr + cmd_offset);
            let cmd = cmd_word & !0x8000;
            if cmd != sound::cmd::BUFFER && cmd != sound::cmd::SOUND {
                continue;
            }

            let param2 = bus.read_long(snd_ptr + cmd_offset + 4);
            if cmd_word & 0x8000 != 0 {
                return Some(param2);
            }

            if let Some(size) = alloc_size {
                if param2 < size {
                    return Some(param2);
                }
                if param2 >= snd_ptr {
                    let relative = param2 - snd_ptr;
                    if relative < size {
                        return Some(relative);
                    }
                }
            }
        }

        None
    }

    /// SndPlayDoubleBuffer: set up double-buffer playback on a channel.
    /// The game pre-fills two buffers and provides a callback to refill them.
    ///
    /// SndDoubleBufferHeader layout (Sound 1994, 2-111):
    ///   +0:  dbhNumChannels (2)
    ///   +2:  dbhSampleSize (2)
    ///   +4:  dbhCompressionID (2)
    ///   +6:  dbhPacketSize (2)
    ///   +8:  dbhSampleRate (4, Fixed 16.16)
    ///   +12: `dbhBufferPtr[0]` (4)
    ///   +16: `dbhBufferPtr[1]` (4)
    ///   +20: dbhDoubleBack (4, ProcPtr)
    ///
    /// SndDoubleBuffer layout (Sound 1994, 2-112):
    ///   +0:  dbNumFrames (4)
    ///   +4:  dbFlags (4)
    ///   +8:  `dbUserInfo[0]` (4)
    ///   +12: `dbUserInfo[1]` (4)
    ///   +16: dbSoundData[...]
    fn snd_play_double_buffer(
        &mut self,
        bus: &mut MacMemoryBus,
        chan_ptr: u32,
        header_ptr: u32,
    ) -> i16 {
        if header_ptr == 0 {
            return -50; // paramErr
        }

        let num_channels = bus.read_word(header_ptr) as usize;
        let sample_size = bus.read_word(header_ptr + 2) as usize;
        let compression_id = bus.read_word(header_ptr + 4) as i16;
        let packet_size = bus.read_word(header_ptr + 6);
        let raw_sample_rate = bus.read_long(header_ptr + 8);
        let format = if compression_id == -1 {
            Some(bus.read_long(header_ptr + 24))
        } else {
            None
        };
        let sample_rate = if raw_sample_rate == 0 {
            // dbhSampleRate is a Fixed sample rate. A zero value is not a
            // useful playback rate; when old low-level clients leave it
            // unset, treat the supplied double-buffer frames as already being
            // in the classic Sound Manager output stream rate. Playing those
            // frames as an 11 kHz source makes pre-converted effects
            // half-speed and muffled. Sound 1994, 2-16, 2-111, and 2-147.
            sound::RATE_22KHZ_FIXED
        } else {
            raw_sample_rate
        };
        let buf0_ptr = bus.read_long(header_ptr + 12);
        let buf1_ptr = bus.read_long(header_ptr + 16);
        let callback_addr = bus.read_long(header_ptr + 20);
        if trace_sound_enabled() {
            eprintln!(
                "[SOUND] snd_play_double_buffer chan=${:08X} header=${:08X} channels={} bits={} compression={} packet={} format={} rate=${:08X} raw_rate=${:08X} buf0=${:08X} buf1=${:08X} callback=${:08X}",
                chan_ptr,
                header_ptr,
                num_channels,
                sample_size,
                compression_id,
                packet_size,
                format
                    .map(|f| format!("${:08X}", f))
                    .unwrap_or_else(|| "none".to_string()),
                sample_rate,
                raw_sample_rate,
                buf0_ptr,
                buf1_ptr,
                callback_addr
            );
        }

        // Track valid double-buffer submissions separately from bufferCmd-style
        // single-buffer ones. Both feed mix_frame but through different
        // code paths.
        self.sound_manager.debug_double_buffer_count += 1;

        // Find or create the channel.
        let chan = if let Some(c) = self.sound_manager.find_channel_mut(chan_ptr) {
            c
        } else {
            self.sound_manager
                .channels
                .push(SndChannel::new(chan_ptr, false));
            self.sound_manager.channels.last_mut().unwrap()
        };

        // Set up double-buffer state.
        chan.double_buffer = Some(sound::DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate,
            num_channels,
            sample_size,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });

        // Read samples from buffer 0 and start playing.
        Self::load_double_buffer_samples(
            bus,
            chan,
            buf0_ptr,
            sample_rate,
            num_channels,
            sample_size,
        );
        0
    }

    /// Read samples from a SndDoubleBuffer record into the channel's PlayingBuffer.
    /// Called when starting double-buffer playback and after a callback refills a buffer.
    pub fn load_double_buffer_samples(
        bus: &mut MacMemoryBus,
        chan: &mut SndChannel,
        buf_ptr: u32,
        sample_rate: u32,
        num_channels: usize,
        sample_size: usize,
    ) {
        if buf_ptr == 0 {
            return;
        }

        let num_frames = bus.read_long(buf_ptr) as usize;
        let flags = bus.read_long(buf_ptr + 4);
        let data_addr = buf_ptr + 16; // dbSoundData starts at offset 16
        if trace_sound_enabled() {
            eprintln!(
                "[SOUND] load_double_buffer_samples chan=${:08X} buf=${:08X} frames={} flags=${:08X} rate=${:08X} channels={} bits={}",
                chan.guest_ptr, buf_ptr, num_frames, flags, sample_rate, num_channels, sample_size
            );
        }

        // dbBufferReady = $00000001, dbLastBuffer = $00000004
        // Sound 1994, 2-113
        if flags & 0x01 == 0 {
            return;
        }

        if num_frames == 0 {
            return;
        }

        let Some(samples) =
            decode_double_buffer_samples(bus, data_addr, num_frames, num_channels, sample_size)
        else {
            if trace_sound_enabled() {
                eprintln!(
                    "[SOUND] load_double_buffer_samples unsupported format chan=${:08X} buf=${:08X} channels={} bits={}",
                    chan.guest_ptr, buf_ptr, num_channels, sample_size
                );
            }
            return;
        };
        let non_silent_frames = samples
            .iter()
            .filter(|sample| sample.left != 0x80 || sample.right != 0x80)
            .count();
        chan.debug_double_buffer_loads = chan.debug_double_buffer_loads.saturating_add(1);
        chan.debug_double_buffer_frames_loaded = chan
            .debug_double_buffer_frames_loaded
            .saturating_add(samples.len() as u64);
        chan.debug_double_buffer_non_silent_frames = chan
            .debug_double_buffer_non_silent_frames
            .saturating_add(non_silent_frames as u64);
        let capture_remaining = sound::DEBUG_DOUBLE_BUFFER_CAPTURE_LIMIT
            .saturating_sub(chan.debug_double_buffer_captured_samples.len());
        chan.debug_double_buffer_captured_samples.extend(
            samples
                .iter()
                .take(capture_remaining)
                .copied()
                .map(StereoSample::downmix),
        );
        if non_silent_frames > 0 {
            chan.debug_double_buffer_non_silent_loads =
                chan.debug_double_buffer_non_silent_loads.saturating_add(1);
        }
        if trace_sound_samples_enabled() {
            trace_double_buffer_sample_stats(chan.guest_ptr, buf_ptr, &samples);
        }

        chan.play_stereo_buffer(samples, sample_rate, sound::PlaybackKind::Buffer, 0);

        // Check for last buffer flag.
        if flags & 0x04 != 0 {
            if let Some(ref mut db) = chan.double_buffer {
                db.last_buffer_seen = true;
            }
        }
    }

    fn snd_start_file_play(
        &mut self,
        bus: &mut MacMemoryBus,
        chan_ptr: u32,
        f_ref_num: i16,
        res_num: i16,
        _selection_ptr: u32,
        completion: u32,
        async_flag: i16,
    ) -> i16 {
        if f_ref_num == 0 && res_num != 0 {
            if let Some((_, snd_ptr)) = self.find_resource_any(*b"snd ", res_num) {
                if trace_sound_enabled() {
                    eprintln!(
                        "[SOUND] SndStartFilePlay using 'snd ' resource id={} ptr=${:08X}",
                        res_num, snd_ptr
                    );
                }
                let err = self.snd_play_resource(bus, chan_ptr, async_flag != 0, snd_ptr);
                if err == 0 {
                    self.sound_manager.debug_file_play_count += 1;
                }
                return err;
            }
            if trace_sound_enabled() {
                eprintln!(
                    "[SOUND] SndStartFilePlay missing 'snd ' resource id={}",
                    res_num
                );
            }
            return -192; // resNotFound
        }

        if f_ref_num == 0 {
            return 0;
        }

        let Some(filename) = self.open_files.get(&(f_ref_num as u16)).cloned() else {
            return -51; // rfNumErr
        };
        let Some(file_data) = self.vfs.get(&filename).cloned() else {
            return -43; // fnfErr
        };
        let Some((samples, sample_rate_fixed)) = parse_aiff_samples(&file_data) else {
            if trace_sound_enabled() {
                eprintln!(
                    "[SOUND] SndStartFilePlay unsupported file format refnum={} file=\"{}\"",
                    f_ref_num, filename
                );
            }
            return -200; // paramErr
        };

        let effective_chan = if chan_ptr != 0 {
            chan_ptr
        } else if async_flag != 0 {
            return -205; // badChannel
        } else {
            let guest_ptr = bus.alloc(GUEST_SND_CHANNEL_SIZE);
            self.sound_manager
                .channels
                .push(SndChannel::new(guest_ptr, true));
            guest_ptr
        };

        let chan = if let Some(chan) = self.sound_manager.find_channel_mut(effective_chan) {
            chan
        } else {
            self.sound_manager
                .channels
                .push(SndChannel::new(effective_chan, false));
            self.sound_manager.channels.last_mut().unwrap()
        };
        chan.play_buffer(
            samples,
            sample_rate_fixed,
            sound::PlaybackKind::File,
            if async_flag != 0 { completion } else { 0 },
        );
        // Bump the SndStartFilePlay submission counter AFTER play_buffer
        // succeeds (i.e. AIFF parsed + playback installed).
        self.sound_manager.debug_file_play_count += 1;
        0
    }
}

const MACE3_FORMAT: u32 = 0x4D41_4333; // 'MAC3'
const MACE6_FORMAT: u32 = 0x4D41_4336; // 'MAC6'

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaceCompression {
    ThreeToOne,
    SixToOne,
}

impl MaceCompression {
    fn packet_size_bits(self) -> usize {
        match self {
            Self::ThreeToOne => 16,
            Self::SixToOne => 8,
        }
    }
}

#[derive(Default)]
struct MaceChannelState {
    index: i32,
    factor: i32,
    prev2: i32,
    previous: i32,
    level: i32,
}

// MACE predictor tables, ported from FFmpeg's LGPL MACE decoder.
const MACE_TAB1: [i32; 8] = [-13, 8, 76, 222, 222, 76, 8, -13];
const MACE_TAB3: [i32; 4] = [-18, 140, 140, -18];

const MACE_TAB2: &[[i32; 4]] = &[
    [37, 116, 206, 330],
    [39, 121, 216, 346],
    [41, 127, 225, 361],
    [42, 132, 235, 377],
    [44, 137, 245, 392],
    [46, 144, 256, 410],
    [48, 150, 267, 428],
    [51, 157, 280, 449],
    [53, 165, 293, 470],
    [55, 172, 306, 490],
    [58, 179, 319, 511],
    [60, 187, 333, 534],
    [63, 195, 348, 557],
    [66, 205, 364, 583],
    [69, 214, 380, 609],
    [72, 223, 396, 635],
    [75, 233, 414, 663],
    [79, 244, 433, 694],
    [82, 254, 453, 725],
    [86, 265, 472, 756],
    [90, 278, 495, 792],
    [94, 290, 516, 826],
    [98, 303, 538, 862],
    [102, 316, 562, 901],
    [107, 331, 588, 942],
    [112, 345, 614, 983],
    [117, 361, 641, 1027],
    [122, 377, 670, 1074],
    [127, 394, 701, 1123],
    [133, 411, 732, 1172],
    [139, 430, 764, 1224],
    [145, 449, 799, 1280],
    [152, 469, 835, 1337],
    [159, 490, 872, 1397],
    [166, 512, 911, 1459],
    [173, 535, 951, 1523],
    [181, 558, 993, 1590],
    [189, 584, 1038, 1663],
    [197, 610, 1085, 1738],
    [206, 637, 1133, 1815],
    [215, 665, 1183, 1895],
    [225, 695, 1237, 1980],
    [235, 726, 1291, 2068],
    [246, 759, 1349, 2161],
    [257, 792, 1409, 2257],
    [268, 828, 1472, 2357],
    [280, 865, 1538, 2463],
    [293, 903, 1606, 2572],
    [306, 944, 1678, 2688],
    [319, 986, 1753, 2807],
    [334, 1030, 1832, 2933],
    [349, 1076, 1914, 3065],
    [364, 1124, 1999, 3202],
    [380, 1174, 2088, 3344],
    [398, 1227, 2182, 3494],
    [415, 1281, 2278, 3649],
    [434, 1339, 2380, 3811],
    [453, 1398, 2486, 3982],
    [473, 1461, 2598, 4160],
    [495, 1526, 2714, 4346],
    [517, 1594, 2835, 4540],
    [540, 1665, 2961, 4741],
    [564, 1740, 3093, 4953],
    [589, 1818, 3232, 5175],
    [615, 1898, 3375, 5405],
    [643, 1984, 3527, 5647],
    [671, 2072, 3683, 5898],
    [701, 2164, 3848, 6161],
    [733, 2261, 4020, 6438],
    [766, 2362, 4199, 6724],
    [800, 2467, 4386, 7024],
    [836, 2578, 4583, 7339],
    [873, 2692, 4786, 7664],
    [912, 2813, 5001, 8008],
    [952, 2938, 5223, 8364],
    [995, 3070, 5457, 8739],
    [1039, 3207, 5701, 9129],
    [1086, 3350, 5956, 9537],
    [1134, 3499, 6220, 9960],
    [1185, 3655, 6497, 10404],
    [1238, 3818, 6788, 10869],
    [1293, 3989, 7091, 11355],
    [1351, 4166, 7407, 11861],
    [1411, 4352, 7738, 12390],
    [1474, 4547, 8084, 12946],
    [1540, 4750, 8444, 13522],
    [1609, 4962, 8821, 14126],
    [1680, 5183, 9215, 14756],
    [1756, 5415, 9626, 15415],
    [1834, 5657, 10057, 16104],
    [1916, 5909, 10505, 16822],
    [2001, 6173, 10975, 17574],
    [2091, 6448, 11463, 18356],
    [2184, 6736, 11974, 19175],
    [2282, 7037, 12510, 20032],
    [2383, 7351, 13068, 20926],
    [2490, 7679, 13652, 21861],
    [2601, 8021, 14260, 22834],
    [2717, 8380, 14897, 23854],
    [2838, 8753, 15561, 24918],
    [2965, 9144, 16256, 26031],
    [3097, 9553, 16982, 27193],
    [3236, 9979, 17740, 28407],
    [3380, 10424, 18532, 29675],
    [3531, 10890, 19359, 31000],
    [3688, 11375, 20222, 32382],
    [3853, 11883, 21125, 32767],
    [4025, 12414, 22069, 32767],
    [4205, 12967, 23053, 32767],
    [4392, 13546, 24082, 32767],
    [4589, 14151, 25157, 32767],
    [4793, 14783, 26280, 32767],
    [5007, 15442, 27452, 32767],
    [5231, 16132, 28678, 32767],
    [5464, 16851, 29957, 32767],
    [5708, 17603, 31294, 32767],
    [5963, 18389, 32691, 32767],
    [6229, 19210, 32767, 32767],
    [6507, 20067, 32767, 32767],
    [6797, 20963, 32767, 32767],
    [7101, 21899, 32767, 32767],
    [7418, 22876, 32767, 32767],
    [7749, 23897, 32767, 32767],
    [8095, 24964, 32767, 32767],
    [8456, 26078, 32767, 32767],
    [8833, 27242, 32767, 32767],
    [9228, 28457, 32767, 32767],
    [9639, 29727, 32767, 32767],
];

const MACE_TAB4: &[[i32; 2]] = &[
    [64, 216],
    [67, 226],
    [70, 236],
    [74, 246],
    [77, 257],
    [80, 268],
    [84, 280],
    [88, 294],
    [92, 307],
    [96, 321],
    [100, 334],
    [104, 350],
    [109, 365],
    [114, 382],
    [119, 399],
    [124, 416],
    [130, 434],
    [136, 454],
    [142, 475],
    [148, 495],
    [155, 519],
    [162, 541],
    [169, 564],
    [176, 590],
    [185, 617],
    [193, 644],
    [201, 673],
    [210, 703],
    [220, 735],
    [230, 767],
    [240, 801],
    [251, 838],
    [262, 876],
    [274, 914],
    [286, 955],
    [299, 997],
    [312, 1041],
    [326, 1089],
    [341, 1138],
    [356, 1188],
    [372, 1241],
    [388, 1297],
    [406, 1354],
    [424, 1415],
    [443, 1478],
    [462, 1544],
    [483, 1613],
    [505, 1684],
    [527, 1760],
    [551, 1838],
    [576, 1921],
    [601, 2007],
    [628, 2097],
    [656, 2190],
    [686, 2288],
    [716, 2389],
    [748, 2496],
    [781, 2607],
    [816, 2724],
    [853, 2846],
    [891, 2973],
    [930, 3104],
    [972, 3243],
    [1016, 3389],
    [1061, 3539],
    [1108, 3698],
    [1158, 3862],
    [1209, 4035],
    [1264, 4216],
    [1320, 4403],
    [1379, 4599],
    [1441, 4806],
    [1505, 5019],
    [1572, 5244],
    [1642, 5477],
    [1715, 5722],
    [1792, 5978],
    [1872, 6245],
    [1955, 6522],
    [2043, 6813],
    [2134, 7118],
    [2229, 7436],
    [2329, 7767],
    [2432, 8114],
    [2541, 8477],
    [2655, 8854],
    [2773, 9250],
    [2897, 9663],
    [3026, 10094],
    [3162, 10546],
    [3303, 11016],
    [3450, 11508],
    [3604, 12020],
    [3765, 12556],
    [3933, 13118],
    [4108, 13703],
    [4292, 14315],
    [4483, 14953],
    [4683, 15621],
    [4892, 16318],
    [5111, 17046],
    [5339, 17807],
    [5577, 18602],
    [5826, 19433],
    [6086, 20300],
    [6358, 21205],
    [6642, 22152],
    [6938, 23141],
    [7248, 24173],
    [7571, 25252],
    [7909, 26380],
    [8262, 27557],
    [8631, 28786],
    [9016, 30072],
    [9419, 31413],
    [9839, 32767],
    [10278, 32767],
    [10737, 32767],
    [11216, 32767],
    [11717, 32767],
    [12240, 32767],
    [12786, 32767],
    [13356, 32767],
    [13953, 32767],
    [14576, 32767],
    [15226, 32767],
    [15906, 32767],
    [16615, 32767],
];

fn parse_aiff_samples(file_data: &[u8]) -> Option<(Vec<u8>, u32)> {
    if file_data.len() < 12 || &file_data[0..4] != b"FORM" {
        return None;
    }
    let form_size =
        u32::from_be_bytes([file_data[4], file_data[5], file_data[6], file_data[7]]) as usize;
    let form_end = 8usize.checked_add(form_size)?;
    if form_end < 12 || form_end > file_data.len() {
        return None;
    }
    let form_kind = &file_data[8..12];
    if form_kind != b"AIFF" && form_kind != b"AIFC" {
        return None;
    }

    let mut offset = 12usize;
    let mut channels: Option<usize> = None;
    let mut sample_size: Option<usize> = None;
    let mut sample_rate_hz: Option<f64> = None;
    let mut ssnd_data: Option<&[u8]> = None;

    while form_end.saturating_sub(offset) >= 8 {
        let chunk_id = &file_data[offset..offset + 4];
        let chunk_size = u32::from_be_bytes([
            file_data[offset + 4],
            file_data[offset + 5],
            file_data[offset + 6],
            file_data[offset + 7],
        ]) as usize;
        offset += 8;
        let chunk_end = offset.checked_add(chunk_size)?;
        if chunk_end > form_end {
            return None;
        }
        let chunk = &file_data[offset..chunk_end];
        match chunk_id {
            b"COMM" => {
                if chunk.len() < 18 {
                    return None;
                }
                channels = Some(u16::from_be_bytes([chunk[0], chunk[1]]) as usize);
                sample_size = Some(u16::from_be_bytes([chunk[6], chunk[7]]) as usize);
                sample_rate_hz = Some(extended80_to_f64(&chunk[8..18])?);
                if form_kind == b"AIFC" {
                    if chunk.len() < 22 {
                        return None;
                    }
                    let compression = &chunk[18..22];
                    if compression != b"NONE" && compression != b"raw " {
                        return None;
                    }
                }
            }
            b"SSND" => {
                if chunk.len() < 8 {
                    return None;
                }
                let data_offset =
                    u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as usize;
                let data_start = 8usize.checked_add(data_offset)?;
                if data_start > chunk.len() {
                    return None;
                }
                ssnd_data = Some(&chunk[data_start..]);
            }
            _ => {}
        }
        let padded_size = chunk_size.checked_add(chunk_size & 1)?;
        offset = offset.checked_add(padded_size)?;
    }

    let channels = channels?;
    let sample_size = sample_size?;
    let sample_rate_hz = sample_rate_hz?;
    let ssnd_data = ssnd_data?;
    if channels == 0 || sample_size == 0 {
        return None;
    }

    let bytes_per_sample = sample_size / 8;
    if bytes_per_sample == 0 {
        return None;
    }
    let frame_bytes = channels.checked_mul(bytes_per_sample)?;
    if frame_bytes == 0 {
        return None;
    }
    let frame_count = ssnd_data.len() / frame_bytes;
    let mut samples = Vec::with_capacity(frame_count);

    for frame in 0..frame_count {
        let mut accum = 0i32;
        for channel in 0..channels {
            let offset = frame * frame_bytes + channel * bytes_per_sample;
            let sample = match sample_size {
                8 => ssnd_data[offset] as i8 as i32,
                16 => i16::from_be_bytes([ssnd_data[offset], ssnd_data[offset + 1]]) as i32 >> 8,
                _ => return None,
            };
            accum += sample;
        }
        let mono = (accum / channels as i32 + 0x80).clamp(0, 255) as u8;
        samples.push(mono);
    }

    let sample_rate_fixed = (sample_rate_hz * 65536.0).round() as u32;
    Some((samples, sample_rate_fixed))
}

fn extended80_to_f64(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 10 {
        return None;
    }
    let sign = (bytes[0] & 0x80) != 0;
    let exponent = (((bytes[0] & 0x7F) as i32) << 8 | bytes[1] as i32) - 16383;
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    if exponent == -16383 && mantissa == 0 {
        return Some(0.0);
    }
    let value = (mantissa as f64 / (1u64 << 63) as f64) * 2f64.powi(exponent);
    Some(if sign { -value } else { value })
}

fn decode_double_buffer_samples(
    bus: &MacMemoryBus,
    data_addr: u32,
    num_frames: usize,
    num_channels: usize,
    sample_size: usize,
) -> Option<Vec<StereoSample>> {
    if num_frames == 0 || num_channels == 0 {
        return Some(Vec::new());
    }

    match sample_size {
        8 => {
            let frame_bytes = num_channels;
            let total_bytes = num_frames.checked_mul(frame_bytes)?;
            let mut raw = vec![0u8; total_bytes];
            for (offset, byte) in raw.iter_mut().enumerate() {
                *byte = bus.read_byte(data_addr + offset as u32);
            }

            let mut out = Vec::with_capacity(num_frames);
            for frame in 0..num_frames {
                let base = frame * frame_bytes;
                out.push(stereo_sample_from_channels(num_channels, |channel| {
                    raw[base + channel]
                }));
            }
            Some(out)
        }
        16 => {
            let frame_bytes = num_channels.checked_mul(2)?;
            let total_bytes = num_frames.checked_mul(frame_bytes)?;
            let mut raw = vec![0u8; total_bytes];
            for (offset, byte) in raw.iter_mut().enumerate() {
                *byte = bus.read_byte(data_addr + offset as u32);
            }

            let mut out = Vec::with_capacity(num_frames);
            for frame in 0..num_frames {
                let base = frame * frame_bytes;
                out.push(stereo_sample_from_channels(num_channels, |channel| {
                    let offset = base + channel * 2;
                    signed_16_to_unsigned_8(i16::from_be_bytes([raw[offset], raw[offset + 1]]))
                }));
            }
            Some(out)
        }
        _ => None,
    }
}

fn stereo_sample_from_channels(
    num_channels: usize,
    mut sample_at: impl FnMut(usize) -> u8,
) -> StereoSample {
    match num_channels {
        1 => StereoSample::mono(sample_at(0)),
        2 => StereoSample {
            left: sample_at(0),
            right: sample_at(1),
        },
        _ => {
            let mut accum = 0i32;
            for channel in 0..num_channels {
                accum += sample_at(channel) as i32 - 128;
            }
            StereoSample::mono((accum / num_channels as i32 + 128).clamp(0, 255) as u8)
        }
    }
}

fn signed_16_to_unsigned_8(sample: i16) -> u8 {
    ((sample as i32 >> 8) + 128).clamp(0, 255) as u8
}

fn trace_double_buffer_sample_stats(chan_ptr: u32, buf_ptr: u32, samples: &[StereoSample]) {
    if samples.is_empty() {
        eprintln!(
            "[SOUND-SAMPLES] chan=${:08X} buf=${:08X} frames=0",
            chan_ptr, buf_ptr
        );
        return;
    }

    let mut min_left = u8::MAX;
    let mut max_left = u8::MIN;
    let mut min_right = u8::MAX;
    let mut max_right = u8::MIN;
    let mut non_silent = 0usize;
    let mut first_non_silent = None;
    let mut last_non_silent = None;
    let mut large_jumps = 0usize;
    let mut prev = samples[0];
    for (idx, &sample) in samples.iter().enumerate() {
        min_left = min_left.min(sample.left);
        max_left = max_left.max(sample.left);
        min_right = min_right.min(sample.right);
        max_right = max_right.max(sample.right);
        if sample.left != 0x80 || sample.right != 0x80 {
            non_silent += 1;
            first_non_silent.get_or_insert(idx);
            last_non_silent = Some(idx);
        }
        if (sample.left as i16 - prev.left as i16).abs() > 64
            || (sample.right as i16 - prev.right as i16).abs() > 64
        {
            large_jumps += 1;
        }
        prev = sample;
    }
    let preview = samples
        .iter()
        .take(16)
        .map(|sample| format!("{:02X}/{:02X}", sample.left, sample.right))
        .collect::<Vec<_>>()
        .join(" ");
    let active_preview = first_non_silent
        .map(|start| {
            samples
                .iter()
                .skip(start)
                .take(16)
                .map(|sample| format!("{:02X}/{:02X}", sample.left, sample.right))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| "none".to_string());
    eprintln!(
        "[SOUND-SAMPLES] chan=${:08X} buf=${:08X} frames={} non_silent={} first_non_silent={:?} last_non_silent={:?} L={:02X}..{:02X} R={:02X}..{:02X} jumps_gt64={} first={} active={}",
        chan_ptr,
        buf_ptr,
        samples.len(),
        non_silent,
        first_non_silent,
        last_non_silent,
        min_left,
        max_left,
        min_right,
        max_right,
        large_jumps,
        preview,
        active_preview
    );
}

fn decode_mace3_mono_to_u8(compressed: &[u8]) -> Vec<u8> {
    let usable_len = compressed.len() & !1;
    let mut state = MaceChannelState::default();
    let mut out = Vec::with_capacity(usable_len * 3);

    for &packet in &compressed[..usable_len] {
        let values = [packet & 0x07, (packet >> 3) & 0x03, packet >> 5];
        for (tab_idx, value) in values.into_iter().enumerate() {
            out.push(mace3_chomp(&mut state, value, tab_idx));
        }
    }

    out
}

fn decode_mace6_mono_to_u8(compressed: &[u8]) -> Vec<u8> {
    let mut state = MaceChannelState::default();
    let mut out = Vec::with_capacity(compressed.len() * 6);

    for &packet in compressed {
        let values = [packet >> 5, (packet >> 3) & 0x03, packet & 0x07];
        for (tab_idx, value) in values.into_iter().enumerate() {
            out.extend_from_slice(&mace6_chomp(&mut state, value, tab_idx));
        }
    }

    out
}

fn mace3_chomp(state: &mut MaceChannelState, value: u8, tab_idx: usize) -> u8 {
    let current = mace_clip_i16(mace_read_table(state, value, tab_idx) + state.level);
    state.level = current - (current >> 3);
    mace_i16_to_u8(current)
}

fn mace6_chomp(state: &mut MaceChannelState, value: u8, tab_idx: usize) -> [u8; 2] {
    let mut current = mace_read_table(state, value, tab_idx);

    if (state.previous ^ current) >= 0 {
        state.factor = (state.factor + 506).min(32767);
    } else {
        state.factor = (state.factor - 314).max(-32767);
    }

    current = mace_clip_i16(current + state.level);
    state.level = (current * state.factor) >> 15;
    current >>= 1;

    let out0 = mace_clip_i16(state.previous + state.prev2 - ((state.prev2 - current) >> 2));
    let out1 = mace_clip_i16(state.previous + current + ((state.prev2 - current) >> 2));
    state.prev2 = state.previous;
    state.previous = current;

    [mace_i16_to_u8(out0), mace_i16_to_u8(out1)]
}

fn mace_read_table(state: &mut MaceChannelState, value: u8, tab_idx: usize) -> i32 {
    let row = ((state.index & 0x7F0) >> 4) as usize;
    let (stride, delta_table, current) = match tab_idx {
        1 => {
            let value = value as usize;
            let current = if value < 2 {
                MACE_TAB4[row][value]
            } else {
                -1 - MACE_TAB4[row][4 - value - 1]
            };
            (2, &MACE_TAB3[..], current)
        }
        _ => {
            let value = value as usize;
            let current = if value < 4 {
                MACE_TAB2[row][value]
            } else {
                -1 - MACE_TAB2[row][8 - value - 1]
            };
            (4, &MACE_TAB1[..], current)
        }
    };

    debug_assert!((value as usize) < stride * 2);
    state.index += delta_table[value as usize] - (state.index >> 5);
    if state.index < 0 {
        state.index = 0;
    }
    current
}

fn mace_clip_i16(value: i32) -> i32 {
    if value > 32767 {
        32767
    } else if value < -32768 {
        -32767
    } else {
        value
    }
}

fn mace_i16_to_u8(value: i32) -> u8 {
    ((value >> 8) + 128).clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::{
        decode_double_buffer_samples, decode_mace3_mono_to_u8, decode_mace6_mono_to_u8,
        extended80_to_f64, parse_aiff_samples, synth_sys_beep_samples,
        GUEST_SND_CHANNEL_Q_HEAD_OFFSET, GUEST_SND_CHANNEL_Q_LENGTH_OFFSET,
        GUEST_SND_CHANNEL_Q_TAIL_OFFSET, GUEST_SND_CHANNEL_SIZE, SAMPLED_SYNTH_ID,
        SOUND_MANAGER_3_SYNTH_VERSION,
    };
    use crate::cpu::{CpuOps, Register};
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::sound::{
        cmd, PendingDoubleBackCallback, PendingSoundCallback, PlaybackKind, SndChannel, SndCommand,
        StereoSample,
    };
    use crate::trap::test_helpers::{setup, TEST_SP};

    /// Assemble a minimal AIFF/AIFC file in memory.
    fn build_aiff(
        form_kind: &[u8; 4],
        channels: u16,
        sample_size_bits: u16,
        sample_rate_extended80: [u8; 10],
        ssnd_samples: &[u8],
        comm_compression: Option<&[u8; 4]>,
    ) -> Vec<u8> {
        // COMM chunk payload:
        //   channels(2) numSampleFrames(4) sampleSize(2) sampleRate(10) [+ comp(4) for AIFC]
        let mut comm = Vec::new();
        comm.extend_from_slice(&channels.to_be_bytes());
        let num_frames =
            (ssnd_samples.len() / (channels as usize * (sample_size_bits as usize / 8))) as u32;
        comm.extend_from_slice(&num_frames.to_be_bytes());
        comm.extend_from_slice(&sample_size_bits.to_be_bytes());
        comm.extend_from_slice(&sample_rate_extended80);
        if let Some(cc) = comm_compression {
            comm.extend_from_slice(cc);
        }

        // SSND chunk payload: offset(4) blockSize(4) data
        let mut ssnd = Vec::new();
        ssnd.extend_from_slice(&0u32.to_be_bytes());
        ssnd.extend_from_slice(&0u32.to_be_bytes());
        ssnd.extend_from_slice(ssnd_samples);

        // Assemble FORM.
        let mut out = Vec::new();
        out.extend_from_slice(b"FORM");
        let form_size = 4 + (8 + comm.len()) + (8 + ssnd.len());
        out.extend_from_slice(&(form_size as u32).to_be_bytes());
        out.extend_from_slice(form_kind);

        out.extend_from_slice(b"COMM");
        out.extend_from_slice(&(comm.len() as u32).to_be_bytes());
        out.extend_from_slice(&comm);

        out.extend_from_slice(b"SSND");
        out.extend_from_slice(&(ssnd.len() as u32).to_be_bytes());
        out.extend_from_slice(&ssnd);
        out
    }

    fn write_minimal_format2_snd_handle(
        bus: &mut MacMemoryBus,
        snd_handle: u32,
        snd_ptr: u32,
        format_word: u16,
    ) {
        bus.write_long(snd_handle, snd_ptr);
        bus.write_word(snd_ptr, format_word); // format
        bus.write_word(snd_ptr + 2, 0); // refCount
        bus.write_word(snd_ptr + 4, 0); // numCommands
    }

    fn alloc_minimal_format2_snd_handle(
        bus: &mut MacMemoryBus,
        format_word: u16,
        data_size: u32,
    ) -> (u32, u32) {
        let snd_handle = bus.alloc(4);
        let snd_ptr = bus.alloc(data_size);
        assert_ne!(snd_handle, 0, "sound handle allocation must succeed");
        assert_ne!(snd_ptr, 0, "sound data allocation must succeed");
        write_minimal_format2_snd_handle(bus, snd_handle, snd_ptr, format_word);
        (snd_handle, snd_ptr)
    }

    /// Locks in `parse_aiff_samples` end-to-end decoding — the entry
    /// point for SndStartFilePlay AIFF decode. Covers 1-channel 8-bit,
    /// 2-channel 16-bit downmix, AIFC "NONE" compression, and rejection
    /// of non-AIFF magic / unsupported AIFC compression.
    #[test]
    fn parse_aiff_samples_decodes_canonical_files() {
        // 1.0 Hz extended80 encoding used for deterministic
        // sample_rate_fixed = 0x00010000.
        let one_hz = [0x3F, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

        // Case 1: 1-channel 8-bit, two samples (+64, -64).
        // i8 signed +64 / -64 → u8 output (+0x80) = 0xC0 / 0x40.
        let aiff = build_aiff(b"AIFF", 1, 8, one_hz, &[0x40, 0xC0], None);
        let (samples, rate) = parse_aiff_samples(&aiff).expect("valid AIFF");
        assert_eq!(samples, vec![0xC0, 0x40]);
        assert_eq!(rate, 0x0001_0000, "1.0 Hz → unity 16.16 fixed");

        // Case 2: 2-channel 16-bit, one stereo frame. Both
        // channels at +0x4000 (i16) → per-channel i32>>8 = +0x40
        // → accum=+0x80 → downmix(+0x80/2)+0x80 = 0xC0.
        let frame_16bit_stereo: [u8; 4] = [0x40, 0x00, 0x40, 0x00];
        let aiff = build_aiff(b"AIFF", 2, 16, one_hz, &frame_16bit_stereo, None);
        let (samples, rate) = parse_aiff_samples(&aiff).expect("valid stereo AIFF");
        assert_eq!(samples, vec![0xC0]);
        assert_eq!(rate, 0x0001_0000);

        // Case 3: AIFC with "NONE" compression must be accepted.
        let aiff = build_aiff(b"AIFC", 1, 8, one_hz, &[0x40, 0xC0], Some(b"NONE"));
        let (samples, _) = parse_aiff_samples(&aiff).expect("AIFC-NONE accepted");
        assert_eq!(samples, vec![0xC0, 0x40]);

        // Case 4: AIFC with unsupported compression rejected.
        let aiff = build_aiff(b"AIFC", 1, 8, one_hz, &[0x40, 0xC0], Some(b"ima4"));
        assert!(
            parse_aiff_samples(&aiff).is_none(),
            "AIFC with unsupported compression must be rejected"
        );

        // Case 5: non-AIFF FORM type rejected (e.g. AIFL).
        let aiff = build_aiff(b"AIFL", 1, 8, one_hz, &[0x40], None);
        assert!(
            parse_aiff_samples(&aiff).is_none(),
            "non-AIFF/AIFC FORM kind must be rejected"
        );

        // Case 6: too short for FORM header → None.
        assert!(parse_aiff_samples(b"FOR").is_none());
    }

    #[test]
    fn parse_aiff_samples_ignores_bytes_after_declared_form() {
        // Marathon Infinity's Music data fork is a valid AIFF whose declared
        // FORM length ends before a short trailing blob. The parser must stop
        // at the FORM boundary instead of treating trailing bytes as another
        // malformed chunk and rejecting the whole file.
        let one_hz = [0x3F, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut aiff = build_aiff(b"AIFF", 1, 8, one_hz, &[0x40, 0xC0], None);
        aiff.extend_from_slice(&[0xFF; 8]);

        let (samples, rate) = parse_aiff_samples(&aiff).expect("valid FORM with trailing bytes");
        assert_eq!(samples, vec![0xC0, 0x40]);
        assert_eq!(rate, 0x0001_0000);
    }

    #[test]
    fn parse_aiff_samples_rejects_overflowing_chunk_bounds() {
        // SndStartFilePlay can be pointed at files that are not valid AIFF.
        // On wasm32, unchecked `offset + chunk_size` / `8 + data_offset`
        // arithmetic overflows before the parser can reject the file.
        let mut huge_chunk = Vec::new();
        huge_chunk.extend_from_slice(b"FORM");
        huge_chunk.extend_from_slice(&12u32.to_be_bytes());
        huge_chunk.extend_from_slice(b"AIFF");
        huge_chunk.extend_from_slice(b"JUNK");
        huge_chunk.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_aiff_samples(&huge_chunk).is_none());

        let one_hz = [0x3F, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut ssnd_overflow = Vec::new();
        ssnd_overflow.extend_from_slice(b"FORM");
        ssnd_overflow.extend_from_slice(&42u32.to_be_bytes());
        ssnd_overflow.extend_from_slice(b"AIFF");
        ssnd_overflow.extend_from_slice(b"COMM");
        ssnd_overflow.extend_from_slice(&18u32.to_be_bytes());
        ssnd_overflow.extend_from_slice(&1u16.to_be_bytes());
        ssnd_overflow.extend_from_slice(&0u32.to_be_bytes());
        ssnd_overflow.extend_from_slice(&8u16.to_be_bytes());
        ssnd_overflow.extend_from_slice(&one_hz);
        ssnd_overflow.extend_from_slice(b"SSND");
        ssnd_overflow.extend_from_slice(&8u32.to_be_bytes());
        ssnd_overflow.extend_from_slice(&u32::MAX.to_be_bytes());
        ssnd_overflow.extend_from_slice(&0u32.to_be_bytes());
        assert!(parse_aiff_samples(&ssnd_overflow).is_none());
    }

    /// Locks in the Apple-SANE extended-80 → f64 conversion used by
    /// `parse_aiff_samples` to decode the AIFF COMM chunk's `sampleRate`
    /// field (IEEE 80-bit extended format stored big-endian).
    ///
    /// Encoded bytes (big-endian):
    ///   byte 0 MSB          = sign bit
    ///   byte 0 low 7 + byte 1 = biased exponent (15 bits, bias 16383)
    ///   bytes 2..10         = mantissa (integer bit in MSB of byte 2,
    ///                                  then 63 fraction bits)
    #[test]
    fn extended80_to_f64_parses_canonical_values() {
        // Zero: sign=0, exp=0, mantissa=0.
        let zero = [0u8; 10];
        assert_eq!(extended80_to_f64(&zero), Some(0.0));

        // 1.0: sign=0, biased-exp=16383 (0x3FFF), mantissa=integer bit only.
        let one = [0x3F, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extended80_to_f64(&one), Some(1.0));

        // 2.0: biased-exp=16384 (0x4000), mantissa=integer bit only.
        let two = [0x40, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extended80_to_f64(&two), Some(2.0));

        // -1.0: sign bit set.
        let neg_one = [0xBF, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extended80_to_f64(&neg_one), Some(-1.0));

        // 0.5: biased-exp=16382 (0x3FFE).
        let half = [0x3F, 0xFE, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(extended80_to_f64(&half), Some(0.5));

        // Too-short input rejected.
        let short = [0u8; 9];
        assert_eq!(extended80_to_f64(&short), None);
    }

    #[test]
    fn sndplay_loaded_format2_resource_returns_noerr() {
        // Inside Macintosh: Sound (1994), pp. 2-121 to 2-123:
        // SndPlay returns noErr for a valid sound resource and
        // leaves the active Sound Manager channel count unchanged
        // when chan is NIL.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x210000;
        let snd_ptr = 0x210100;
        write_minimal_format2_snd_handle(&mut bus, snd_handle, snd_ptr, 2);

        let channel_count_before = disp.sound_manager.channels.len();
        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before);
    }

    #[test]
    fn sndplay_format2_refcount_does_not_shift_command_stream() {
        // Format-2 'snd ' refCount is application-owned metadata. The Sound
        // Manager command stream still starts immediately after the format-2
        // header, so a nonzero refCount must not shift numCommands.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr = 0x250000;
        disp.sound_manager
            .channels
            .push(SndChannel::new(chan_ptr, false));

        let (snd_handle, snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 80);
        let header_offset = 16u32;
        let header = snd_ptr + header_offset;

        bus.write_word(snd_ptr, 2); // format
        bus.write_word(snd_ptr + 2, 1); // refCount
        bus.write_word(snd_ptr + 4, 1); // numCommands
        bus.write_word(snd_ptr + 6, 0x8000 | cmd::BUFFER); // dataOffsetFlag + bufferCmd
        bus.write_word(snd_ptr + 8, 0); // param1
        bus.write_long(snd_ptr + 10, header_offset); // param2 = sound header offset

        bus.write_long(header, 0); // samplePtr = NIL, data follows stdSH
        bus.write_long(header + 4, 4); // length
        bus.write_long(header + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_long(header + 12, 0); // loopStart
        bus.write_long(header + 16, 0); // loopEnd
        bus.write_byte(header + 20, 0); // stdSH
        bus.write_byte(header + 21, 60); // baseFrequency
        bus.write_bytes(header + 22, &[0x40, 0x80, 0xC0, 0xFF]);

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert!(disp
            .sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .is_playing());
        assert_eq!(
            disp.sound_manager.mix_frame(4),
            vec![0x40, 0x80, 0xC0, 0xFF]
        );
    }

    #[test]
    fn sndplay_async_busy_channel_queues_buffer_resource_until_current_playback_finishes() {
        // Sound 1994, pp. 2-13, 2-121, and 2-123: a SndChannel is a FIFO
        // command queue, and SndPlay sends the sound resource's commands into
        // that channel. An asynchronous SndPlay must not restart an active
        // bufferCmd already playing on the same channel.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr = 0x250400;
        disp.sound_manager
            .channels
            .push(SndChannel::new(chan_ptr, false));
        bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_LENGTH_OFFSET, 128);
        bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET, 0);
        bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET, 0);
        disp.sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .play_buffer(
                vec![0x10, 0x11],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );

        let (snd_handle, snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 80);
        let header_offset = 16u32;
        let header = snd_ptr + header_offset;
        bus.write_word(snd_ptr + 4, 1); // numCommands
        bus.write_word(snd_ptr + 6, 0x8000 | cmd::BUFFER); // dataOffsetFlag + bufferCmd
        bus.write_word(snd_ptr + 8, 0);
        bus.write_long(snd_ptr + 10, header_offset);
        bus.write_long(header, 0);
        bus.write_long(header + 4, 2);
        bus.write_long(header + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_long(header + 12, 0);
        bus.write_long(header + 16, 0);
        bus.write_byte(header + 20, 0);
        bus.write_byte(header + 21, 60);
        bus.write_bytes(header + 22, &[0xA0, 0xA1]);

        bus.write_word(sp, 1); // async = TRUE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(
            bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET),
            1,
            "resource command should enter the channel FIFO"
        );
        assert_eq!(
            disp.sound_manager.mix_frame(2),
            vec![0x10, 0x11],
            "current buffer must finish before queued SndPlay data starts"
        );

        disp.service_guest_sound_queues(&mut bus);
        assert_eq!(disp.sound_manager.mix_frame(2), vec![0xA0, 0xA1]);
    }

    #[test]
    fn sndplay_nil_chan_async_true_is_accepted() {
        // Inside Macintosh: Sound (1994), p. 2-122:
        // If chan is NIL, async is ignored.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x210200;
        let snd_ptr = 0x210300;
        write_minimal_format2_snd_handle(&mut bus, snd_handle, snd_ptr, 2);

        bus.write_word(sp, 1); // async = TRUE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
    }

    #[test]
    fn sndplay_nil_chan_sync_path_reclaims_internal_channel_before_return() {
        // Inside Macintosh: Sound (1994), p. 2-122:
        // NIL-chan SndPlay allocates an internal channel and releases it
        // after the synchronous play completes.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x210350;
        let snd_ptr = 0x210450;
        let channel_count_before = disp.sound_manager.channels.len();
        write_minimal_format2_snd_handle(&mut bus, snd_handle, snd_ptr, 2);

        bus.write_word(sp, 1); // async = TRUE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before);
    }

    #[test]
    fn sndplay_nil_chan_sync_path_returns_internal_guest_channel_allocation_to_free_list() {
        // The internal NIL-chan SndPlay path should release the temporary
        // Sound Manager-owned guest SndChannel block once the synchronous
        // play completes.
        let (mut disp, mut cpu, mut bus) = setup();
        let recycled = bus.alloc(GUEST_SND_CHANNEL_SIZE);
        assert_ne!(recycled, 0);
        bus.write_long(recycled, 0x1111_0001);
        bus.write_long(recycled + 4, 0x1111_0002);
        bus.write_long(recycled + 8, 0x1111_0003);
        bus.write_long(recycled + 12, 0x1111_0004);
        bus.write_long(recycled + 16, 0x1111_0005);
        bus.write_long(recycled + 20, 0x1111_0006);
        bus.write_long(recycled + 24, 0x1111_0007);
        bus.write_word(recycled + 28, 0x1111);
        bus.write_word(recycled + 30, 0x2222);
        bus.write_word(recycled + 32, 0x3333);
        bus.write_word(recycled + 34, 0x4444);
        bus.free(recycled);

        let sp = TEST_SP;
        let snd_handle = 0x210700;
        let snd_ptr = 0x210800;
        write_minimal_format2_snd_handle(&mut bus, snd_handle, snd_ptr, 2);

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        let reallocated = bus.alloc(GUEST_SND_CHANNEL_SIZE);
        assert_eq!(
            reallocated, recycled,
            "internal NIL-chan SndPlay must return its guest channel block to the allocator"
        );
        assert_eq!(bus.read_long(reallocated), 0, "nextChan cleared");
        assert_eq!(bus.read_long(reallocated + 4), 0, "firstMod cleared");
        assert_eq!(bus.read_long(reallocated + 8), 0, "callBack cleared");
        assert_eq!(bus.read_long(reallocated + 12), 0, "userInfo cleared");
        assert_eq!(bus.read_long(reallocated + 16), 0, "wait cleared");
        assert_eq!(
            bus.read_long(reallocated + 20),
            0,
            "cmdInProgress cmd/param1 cleared"
        );
        assert_eq!(
            bus.read_long(reallocated + 24),
            0,
            "cmdInProgress param2 cleared"
        );
        assert_eq!(bus.read_word(reallocated + 28), 0, "flags cleared");
        assert_eq!(bus.read_word(reallocated + 30), 0, "qLength cleared");
        assert_eq!(bus.read_word(reallocated + 32), 0, "qHead cleared");
        assert_eq!(bus.read_word(reallocated + 34), 0, "qTail cleared");
        bus.free(reallocated);
    }

    #[test]
    fn sndplay_nil_chan_active_playback_survives_until_mixed_then_releases() {
        // NIL-channel SndPlay owns its temporary channel until the sampled
        // buffer has actually been mixed. Releasing at trap return drops the
        // sound entirely, which is audible as missing menu effects in games
        // that use high-level SndPlay for short UI sounds.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let (snd_handle, snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 80);
        let header_offset = 16u32;
        let header = snd_ptr + header_offset;

        bus.write_word(snd_ptr + 4, 1); // numCommands
        bus.write_word(snd_ptr + 6, 0x8000 | cmd::BUFFER); // dataOffsetFlag + bufferCmd
        bus.write_word(snd_ptr + 8, 0);
        bus.write_long(snd_ptr + 10, header_offset);

        bus.write_long(header, 0);
        bus.write_long(header + 4, 4);
        bus.write_long(header + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_long(header + 12, 0);
        bus.write_long(header + 16, 0);
        bus.write_byte(header + 20, 0);
        bus.write_byte(header + 21, 60);
        bus.write_bytes(header + 22, &[0x40, 0x80, 0xC0, 0xFF]);

        let channel_count_before = disp.sound_manager.channels.len();
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before + 1);

        assert_eq!(
            disp.sound_manager.mix_frame(4),
            vec![0x40, 0x80, 0xC0, 0xFF]
        );
        disp.release_finished_internal_sound_channels(&mut bus);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before);
    }

    #[test]
    fn sndplay_unloaded_handle_returns_resproblem() {
        // Inside Macintosh: Sound (1994), p. 2-122:
        // unloaded handle -> resProblem (-204).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x210400;
        bus.write_long(snd_handle, 0); // NIL master pointer (unloaded)

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10) as i16, -204);
    }

    #[test]
    fn sndplay_nil_handle_returns_resproblem() {
        // Inside Macintosh: Sound (1994), p. 2-122:
        // missing sound resource input returns resProblem (-204).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, 0); // sndHdl = NIL
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10) as i16, -204);
    }

    #[test]
    fn sndplay_bad_format_returns_badformat() {
        // Inside Macintosh: Sound (1994), pp. 2-122 to 2-123:
        // malformed/unsupported format -> badFormat (-206).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x210500;
        let snd_ptr = 0x210600;
        write_minimal_format2_snd_handle(&mut bus, snd_handle, snd_ptr, 3);

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0x220000);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10) as i16, -206);
    }

    #[test]
    fn sndplay_truncated_format2_handle_returns_badformat() {
        // Inside Macintosh: Sound (1994), p. 2-123:
        // corrupt/unusable sound resources return badFormat (-206).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let (snd_handle, _snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 4);

        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10) as i16, -206);
    }

    #[test]
    fn sysbeep_consumes_duration_word_and_queues_beep_audio() {
        // Inside Macintosh Volume II (1985), p. II-385 and
        // Inside Macintosh: Sound (1991), pp. 22-80 and 22-95:
        // SysBeep is a procedure with one Integer argument. Systemless
        // synthesizes a short alert sound because packaged games do not run
        // with a real System file alert-sound resource.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 30); // duration ticks
        bus.write_word(sp + 2, 0xBEEF); // sentinel: no function-result slot
        let channel_count_before = disp.sound_manager.channels.len();

        let result = disp.dispatch_sound(true, 0x1C8, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0xBEEF);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before + 1);
        assert_eq!(
            disp.sound_manager
                .mix_frame(256)
                .iter()
                .filter(|&&sample| sample != 0x80)
                .count(),
            256
        );
        let remaining = synth_sys_beep_samples().len();
        let _ = disp.sound_manager.mix_frame(remaining);
        disp.release_finished_internal_sound_channels(&mut bus);
        assert_eq!(disp.sound_manager.channels.len(), channel_count_before);
    }

    #[test]
    fn sounddispatch_reports_sound_manager_3x_final_numversion() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x000C_0008); // SndSoundManagerVersion
        bus.write_long(sp, 0xDEAD_BEEF);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(sp),
            0x0333_8000,
            "NumVersion bytes must encode Sound Manager 3.3.3 final"
        );
        assert_eq!(
            bus.read_byte(sp + 1),
            0x33,
            "minorAndBugRev must be BCD-style 3.3, not raw decimal bytes"
        );
        assert_eq!(
            bus.read_byte(sp + 2),
            0x80,
            "release stage byte must mark a final release"
        );
    }

    #[test]
    fn sounddispatch_volume_selectors_persist_and_return_levels() {
        // Inside Macintosh: Sound 1994, 2-139..2-142:
        // Sound Manager 3.x exposes packed right/left output volumes
        // through SoundDispatch selectors $24/$28/$2C/$30.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let level_ptr = 0x230800;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x022C_0024); // GetDefaultOutputVolume
        bus.write_long(sp, level_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(bus.read_long(level_ptr), 0x0100_0100);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0230_0024); // SetDefaultOutputVolume
        bus.write_long(sp, 0x0040_00C0);
        bus.write_word(sp + 4, 0xBEEF);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(disp.sound_manager.default_output_volume(), 0x0040_00C0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x022C_0024); // GetDefaultOutputVolume
        bus.write_long(sp, level_ptr);
        bus.write_long(level_ptr, 0);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(level_ptr), 0x0040_00C0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0228_0024); // SetSysBeepVolume
        bus.write_long(sp, 0x0020_0060);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.sound_manager.sys_beep_volume(), 0x0020_0060);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0224_0024); // GetSysBeepVolume
        bus.write_long(sp, level_ptr);
        bus.write_long(level_ptr, 0);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(level_ptr), 0x0020_0060);
    }

    #[test]
    fn sounddispatch_sndmanagerstatus_reports_default_cpu_load_capacity() {
        // Inside Macintosh: Sound 1994, 2-39: smMaxCPULoad is initialized
        // to 100 at startup; smCurCPULoad is approximate. The HLE mixer does
        // not consume guest CPU, so current load stays 0.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let status_ptr = 0x230A00;
        disp.sound_manager
            .channels
            .push(SndChannel::new(0x0039_38C8, false));

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0314_0008); // SndManagerStatus
        bus.write_long(sp, status_ptr);
        bus.write_word(sp + 4, 6); // sizeof(SMStatus)
        bus.write_word(sp + 6, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(status_ptr), 100);
        assert_eq!(bus.read_word(status_ptr + 2), 1);
        assert_eq!(bus.read_word(status_ptr + 4), 0);
    }

    #[test]
    fn sounddispatch_documented_zero_param_selectors_pop_real_pascal_frames() {
        // Inside Macintosh Volume VI, VI-576 to VI-577 documents these
        // SoundDispatch selectors with a zero param-size byte. The routines
        // still consume their Pascal arguments.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let sc_status_ptr = 0x230A80;
        let sm_status_ptr = 0x230AA0;
        let state_ptr = 0x230AC0;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0010_0008); // SndChannelStatus
        bus.write_long(sp, sc_status_ptr);
        bus.write_word(sp + 4, 24);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xFFFF);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0014_0008); // SndManagerStatus
        bus.write_long(sp, sm_status_ptr);
        bus.write_word(sp + 4, 6);
        bus.write_word(sp + 6, 0xFFFF);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0018_0008); // SndGetSysBeepState
        bus.write_long(sp, state_ptr);
        bus.write_word(sp + 4, 0xFFFF);
        bus.write_word(state_ptr, 0);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(bus.read_word(state_ptr), 1);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x001C_0008); // SndSetSysBeepState
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0xFFFF);
        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0);
    }

    #[test]
    fn sounddispatch_get_sound_header_offset_returns_embedded_header_offset() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let (snd_handle, snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 80);
        let offset_ptr = 0x230C00;
        let header_offset = 16u32;

        bus.write_word(snd_ptr + 4, 1); // numCommands
        bus.write_word(snd_ptr + 6, 0x8000 | cmd::BUFFER);
        bus.write_word(snd_ptr + 8, 0);
        bus.write_long(snd_ptr + 10, header_offset);
        bus.write_long(snd_ptr + header_offset, 0); // samplePtr
        bus.write_long(snd_ptr + header_offset + 4, 1); // length
        bus.write_long(snd_ptr + header_offset + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_byte(snd_ptr + header_offset + 20, 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0404_0024);
        bus.write_long(sp, offset_ptr);
        bus.write_long(sp + 4, snd_handle);
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(bus.read_long(offset_ptr), header_offset);
    }

    #[test]
    fn sndaddmodifier_nil_channel_returns_badchannel_and_consumes_pascal_frame() {
        // Inside Macintosh: Sound (1991), p. 22-82:
        // SndAddModifier(chan, modifier, id, init): OSErr.
        // BasiliskII System 7.5.3 returns badChannel for a direct
        // application call with a NIL channel pointer.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // init
        bus.write_word(sp + 4, 5); // id
        bus.write_long(sp + 6, 0); // modifier
        bus.write_long(sp + 10, 0); // chan = NIL
        bus.write_word(sp + 14, 0xFFFF); // result slot sentinel

        let result = disp.dispatch_sound(true, 0x002, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(sp + 14), (-205i16) as u16);
        assert!(disp.sound_manager.channels.is_empty());
    }

    #[test]
    fn sndcontrol_consumes_cmdptr_and_id_and_returns_noerr() {
        // Inside Macintosh: Sound (1994), pp. 2-134 to 2-135:
        // SndControl(id, VAR cmd): OSErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let cmd_ptr = 0x230600;
        bus.write_word(cmd_ptr, 0x7777);
        bus.write_word(cmd_ptr + 2, 0x8888);
        bus.write_long(cmd_ptr + 4, 0x99AA_BBCC);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr); // cmd
        bus.write_word(sp + 4, 5); // id
        bus.write_word(sp + 6, 0xFFFF); // result slot sentinel

        let result = disp.dispatch_sound(true, 0x006, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(cmd_ptr), 0x7777);
        assert_eq!(bus.read_word(cmd_ptr + 2), 0x8888);
        assert_eq!(bus.read_long(cmd_ptr + 4), 0x99AA_BBCC);
    }

    #[test]
    fn sndcontrol_availablecmd_zero_init_known_synth_sets_param1_true() {
        // IM:Sound 1994, 2-92 + 2-134..2-135: availableCmd returns 1 in
        // param1 when the Sound Manager supports the initialization
        // options specified in param2. A zero-init query against a
        // documented built-in synth id should therefore rewrite param1
        // to 1 while still returning noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let cmd_ptr = 0x230700;
        bus.write_word(cmd_ptr, cmd::AVAILABLE);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr);
        bus.write_word(sp + 4, 5); // sampledSynth
        bus.write_word(sp + 6, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x006, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(cmd_ptr), cmd::AVAILABLE);
        assert_eq!(bus.read_word(cmd_ptr + 2), 1);
        assert_eq!(bus.read_long(cmd_ptr + 4), 0);
    }

    #[test]
    fn sndcontrol_versioncmd_sampled_synth_reports_sound_manager_3_version() {
        // IM:VI 22-40..22-41: versionCmd sent through SndControl
        // returns the synthesizer version in param2 as major.highword
        // and minor.lowword; Systemless advertises a Sound Manager 3.x
        // sampled-synth surface.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let cmd_ptr = 0x230720;
        bus.write_word(cmd_ptr, cmd::VERSION);
        bus.write_word(cmd_ptr + 2, 0x7FFF);
        bus.write_long(cmd_ptr + 4, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr);
        bus.write_word(sp + 4, SAMPLED_SYNTH_ID as u16);
        bus.write_word(sp + 6, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x006, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(cmd_ptr), cmd::VERSION);
        assert_eq!(bus.read_word(cmd_ptr + 2), 0);
        assert_eq!(bus.read_long(cmd_ptr + 4), SOUND_MANAGER_3_SYNTH_VERSION);
    }

    #[test]
    fn sounddispatch_spb_open_device_pops_frame_and_returns_refnum() {
        // Sound Input Manager selector $05180014 shares routine byte $18
        // with SndGetSysBeepState. It must still consume its 10-byte
        // Pascal frame and write an input device refnum.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let refnum_ptr = 0x240000;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0518_0014);
        bus.write_long(sp, refnum_ptr);
        bus.write_word(sp + 4, 0); // permission
        bus.write_long(sp + 6, 0); // default device name
        bus.write_word(sp + 10, 0xFFFF);
        bus.write_long(refnum_ptr, 0xDEAD_BEEF);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(bus.read_long(refnum_ptr), 1);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn sndnewchannel_returns_noerr_and_writes_nonnull_channel_pointer() {
        // Inside Macintosh: Sound (1994), p. 2-195:
        // SndNewChannel stores a newly allocated channel pointer through
        // the VAR chan argument and returns noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x220000;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(sp, 0); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 5); // sampledSynth
        bus.write_long(sp + 10, chan_ptr_ptr);
        bus.write_word(sp + 14, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(sp + 14), 0);

        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);
        assert!(disp.sound_manager.find_channel_mut(chan_ptr).is_some());
    }

    #[test]
    fn sndnewchannel_sets_channel_callback_to_userroutine() {
        // Inside Macintosh: Sound (1994), p. 2-195:
        // SndNewChannel associates userRoutine with the created channel.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x220100;
        let user_routine = 0x00C0_FFEE;

        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(sp, user_routine);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 5);
        bus.write_long(sp + 10, chan_ptr_ptr);

        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);
        assert_eq!(bus.read_long(chan_ptr + 8), user_routine);
        let chan = disp
            .sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel tracked");
        assert_eq!(chan.callback_addr, user_routine);
    }

    #[test]
    fn sndnewchannel_unsupported_synth_returns_resproblem_and_does_not_allocate() {
        // Inside Macintosh: Sound (1994), p. 2-195:
        // a nonzero synth id requests loading/linking a 'snth' resource,
        // and resProblem reports failure loading that resource.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x220180;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(sp, 0); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0x7FFF); // unsupported synth id
        bus.write_long(sp + 10, chan_ptr_ptr);
        bus.write_word(sp + 14, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(sp + 14), (-204i16) as u16);
        assert_eq!(bus.read_long(chan_ptr_ptr), 0, "chan VAR remains NIL");
        assert!(
            disp.sound_manager.channels.is_empty(),
            "unsupported synth must not create a tracked channel"
        );
    }

    #[test]
    fn snddisposechannel_returns_noerr_and_removes_channel_from_manager_state() {
        // Inside Macintosh: Sound (1994), p. 2-196:
        // SndDisposeChannel disposes a channel returned by SndNewChannel.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220200;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert!(disp.sound_manager.find_channel_mut(chan_ptr).is_some());

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // quietNow = TRUE
        bus.write_long(sp + 2, chan_ptr);
        bus.write_word(sp + 6, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert!(disp.sound_manager.find_channel_mut(chan_ptr).is_none());
        assert!(
            bus.get_alloc_size(chan_ptr).is_none(),
            "Sound Manager-owned channel storage must be released on dispose"
        );
        assert_eq!(
            bus.alloc(GUEST_SND_CHANNEL_SIZE),
            chan_ptr,
            "disposed Sound Manager-owned channel block must return to allocator"
        );
    }

    #[test]
    fn snddisposechannel_preserves_caller_owned_channel_storage() {
        // Inside Macintosh: Sound (1994), p. 2-196:
        // if the application created its own SndChannel record, the Sound
        // Manager does not dispose that memory.
        let (mut disp, mut cpu, mut bus) = setup();
        let chan_ptr = bus.alloc(GUEST_SND_CHANNEL_SIZE);
        assert_ne!(chan_ptr, 0);
        let chan_ptr_ptr = 0x220280;
        let create_sp = TEST_SP;
        bus.write_long(chan_ptr_ptr, chan_ptr);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert!(disp.sound_manager.find_channel_mut(chan_ptr).is_some());
        bus.write_long(chan_ptr, 0x1111_0001);
        bus.write_long(chan_ptr + 4, 0x1111_0002);
        bus.write_long(chan_ptr + 8, 0x1111_0003);
        bus.write_long(chan_ptr + 12, 0x1111_0004);
        bus.write_long(chan_ptr + 16, 0x1111_2222);
        bus.write_long(chan_ptr + 20, 0x3333_4444);
        bus.write_long(chan_ptr + 24, 0x5555_6666);
        bus.write_word(chan_ptr + 28, 0x7777);
        bus.write_word(chan_ptr + 30, 0xAAAA);
        bus.write_word(chan_ptr + 32, 0x8888);
        bus.write_word(chan_ptr + 34, 0x9999);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // quietNow = FALSE
        bus.write_long(sp + 2, chan_ptr);
        bus.write_word(sp + 6, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0);
        assert!(disp.sound_manager.find_channel_mut(chan_ptr).is_none());
        assert_eq!(
            bus.get_alloc_size(chan_ptr),
            Some(GUEST_SND_CHANNEL_SIZE),
            "caller-owned channel storage must stay allocated after dispose"
        );
        assert_eq!(bus.read_long(chan_ptr), 0, "nextChan cleared");
        assert_eq!(bus.read_long(chan_ptr + 4), 0, "firstMod cleared");
        assert_eq!(bus.read_long(chan_ptr + 8), 0, "callBack cleared");
        assert_eq!(bus.read_long(chan_ptr + 12), 0, "userInfo cleared");
        assert_eq!(bus.read_long(chan_ptr + 16), 0, "wait field cleared");
        assert_eq!(
            bus.read_long(chan_ptr + 20),
            0,
            "cmdInProgress cmd/param1 cleared"
        );
        assert_eq!(
            bus.read_long(chan_ptr + 24),
            0,
            "cmdInProgress param2 cleared"
        );
        assert_eq!(bus.read_word(chan_ptr + 28), 0, "flags cleared");
        assert_eq!(bus.read_word(chan_ptr + 30), 0, "qLength cleared");
        assert_eq!(bus.read_word(chan_ptr + 32), 0, "qHead cleared");
        assert_eq!(bus.read_word(chan_ptr + 34), 0, "qTail cleared");
    }

    #[test]
    fn snddisposechannel_discards_pending_callbacks_for_disposed_channel() {
        // Inside Macintosh: Sound (1994), p. 2-196:
        // disposing a channel removes it from Sound Manager bookkeeping, so
        // completion/callback queues should no longer retain work for it.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x2202C0;
        let other_chan_ptr = 0x220340;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);

        disp.sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::Command {
                callback_addr: 0x00AB_CDEF,
                chan_ptr,
                cmd: crate::sound::SndCommand {
                    cmd: cmd::CALLBACK,
                    param1: 7,
                    param2: 0x1111_2222,
                },
            });
        disp.sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::FileCompletion {
                callback_addr: 0x00FE_DCBA,
                chan_ptr,
            });
        disp.sound_manager
            .pending_sound_callbacks
            .push(PendingSoundCallback::FileCompletion {
                callback_addr: 0x0000_2222,
                chan_ptr: other_chan_ptr,
            });
        disp.sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr: 0x00CA_FE00,
                chan_ptr,
                header_ptr: 0x0022_4400,
                exhausted_buffer_index: 1,
            });
        disp.sound_manager
            .pending_callbacks
            .push(PendingDoubleBackCallback {
                callback_addr: 0x0000_3333,
                chan_ptr: other_chan_ptr,
                header_ptr: 0x0022_5500,
                exhausted_buffer_index: 0,
            });

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // quietNow = TRUE
        bus.write_long(sp + 2, chan_ptr);
        bus.write_word(sp + 6, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(disp.sound_manager.pending_sound_callbacks.len(), 1);
        assert!(matches!(
            &disp.sound_manager.pending_sound_callbacks[0],
            PendingSoundCallback::FileCompletion {
                callback_addr: 0x0000_2222,
                chan_ptr: ptr
            } if *ptr == other_chan_ptr
        ));
        assert_eq!(disp.sound_manager.pending_callbacks.len(), 1);
        assert_eq!(
            disp.sound_manager.pending_callbacks[0].chan_ptr, other_chan_ptr,
            "double-back callbacks for disposed channel must be removed"
        );
    }

    #[test]
    fn snddocommand_nullcmd_returns_noerr() {
        // Inside Macintosh: Sound (1994), p. 2-130:
        // SndDoCommand processes a SndCommand for the given channel.
        // A nullCmd command should return noErr.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220300;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);

        let cmd_ptr = 0x230000;
        bus.write_word(cmd_ptr, cmd::NULL);
        bus.write_word(cmd_ptr + 2, 0x1234);
        bus.write_long(cmd_ptr + 4, 0x89AB_CDEF);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // noWait = TRUE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
    }

    #[test]
    fn snddocommand_queues_quiet_behind_active_buffer() {
        // Inside Macintosh: Sound (1994), pp. 2-13 and 2-130:
        // SndDoCommand enters the channel FIFO. It must not bypass an
        // active bufferCmd; SndDoImmediate is the bypass path.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220380;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        disp.sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .play_buffer(
                vec![0x80; 128],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );

        let cmd_ptr = 0x230040;
        bus.write_word(cmd_ptr, cmd::QUIET);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, 0);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // noWait = TRUE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert!(disp
            .sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .is_playing());
        assert_eq!(disp.sound_manager.mix_frame(64).len(), 64);
    }

    #[test]
    fn snddocommand_callbackcmd_on_busy_channel_fires_after_buffer_exhaustion() {
        // callBackCmd is completion work for the active sound command. If
        // it sits in the generic FIFO until after playback goes idle, games
        // that poll their channel userInfo miss the buffer boundary.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x2203C0;
        let user_routine = 0x00AB_CDEF;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, user_routine);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        disp.sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .play_buffer(
                vec![0x80, 0x80],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );

        let cmd_ptr = 0x230060;
        bus.write_word(cmd_ptr, cmd::CALLBACK);
        bus.write_word(cmd_ptr + 2, 7);
        bus.write_long(cmd_ptr + 4, 0x1122_3344);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // noWait = TRUE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert!(
            disp.sound_manager.pending_sound_callbacks.is_empty(),
            "callback must wait for the active buffer to finish"
        );

        disp.sound_manager.mix_frame(4);

        assert_eq!(disp.sound_manager.pending_sound_callbacks.len(), 1);
        match &disp.sound_manager.pending_sound_callbacks[0] {
            PendingSoundCallback::Command {
                callback_addr,
                chan_ptr: callback_chan,
                cmd,
            } => {
                assert_eq!(*callback_addr, user_routine);
                assert_eq!(*callback_chan, chan_ptr);
                assert_eq!(cmd.cmd, cmd::CALLBACK);
                assert_eq!(cmd.param1, 7);
                assert_eq!(cmd.param2, 0x1122_3344);
            }
            other => panic!("unexpected callback variant: {other:?}"),
        }
    }

    #[test]
    fn snddocommand_nil_channel_returns_badchannel() {
        // Inside Macintosh: Sound (1994), p. 2-130:
        // SndDoCommand requires a valid sound channel and returns
        // badChannel when chan is corrupt or unusable.
        let (mut disp, mut cpu, mut bus) = setup();
        let cmd_ptr = 0x230080;
        bus.write_word(cmd_ptr, cmd::NULL);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, 0);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // noWait = TRUE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), (-205i16) as u16);
    }

    #[test]
    fn snddocommand_callbackcmd_uses_channel_callback_proc() {
        // Inside Macintosh: Sound (1994), pp. 2-126 and 2-130:
        // callBackCmd executes via the channel callback procedure set by
        // SndNewChannel.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220400;
        let user_routine = 0x00AB_CDEF;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, user_routine);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        disp.sound_manager.pending_sound_callbacks.clear();

        let cmd_ptr = 0x230100;
        bus.write_word(cmd_ptr, cmd::CALLBACK);
        bus.write_word(cmd_ptr + 2, 7);
        bus.write_long(cmd_ptr + 4, 0x1122_3344);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0); // noWait = FALSE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, chan_ptr);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.sound_manager.pending_sound_callbacks.len(), 1);
        match &disp.sound_manager.pending_sound_callbacks[0] {
            PendingSoundCallback::Command {
                callback_addr,
                chan_ptr: callback_chan,
                cmd,
            } => {
                assert_eq!(*callback_addr, user_routine);
                assert_eq!(*callback_chan, chan_ptr);
                assert_eq!(cmd.cmd, cmd::CALLBACK);
                assert_eq!(cmd.param1, 7);
                assert_eq!(cmd.param2, 0x1122_3344);
            }
            other => panic!("unexpected callback variant: {other:?}"),
        }
    }

    #[test]
    fn snddoimmediate_nullcmd_returns_noerr() {
        // Inside Macintosh: Sound (1994), p. 2-131:
        // SndDoImmediate executes the supplied command immediately.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220500;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);

        let cmd_ptr = 0x230200;
        bus.write_word(cmd_ptr, cmd::NULL);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, 0);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr);
        bus.write_long(sp + 4, chan_ptr);
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x004, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0);
    }

    #[test]
    fn snddoimmediate_nil_channel_returns_badchannel() {
        // Inside Macintosh: Sound (1994), p. 2-131:
        // SndDoImmediate returns badChannel when chan is corrupt or unusable.
        let (mut disp, mut cpu, mut bus) = setup();
        let cmd_ptr = 0x230180;
        bus.write_word(cmd_ptr, cmd::NULL);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, 0);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr);
        bus.write_long(sp + 4, 0); // chan = NIL
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x004, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), (-205i16) as u16);
    }

    #[test]
    fn snddoimmediate_getratecmd_writes_fixed_rate_to_param2() {
        // Inside Macintosh: Sound (1994), pp. 2-97 and 2-131:
        // getRateCmd writes the channel's current Fixed rate to param2.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x220600;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        disp.sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .set_rate(0x0001_8000);

        let rate_out_ptr = 0x230300;
        bus.write_long(rate_out_ptr, 0xDEAD_BEEF);
        let cmd_ptr = 0x230308;
        bus.write_word(cmd_ptr, cmd::GET_RATE);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, rate_out_ptr);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, cmd_ptr);
        bus.write_long(sp + 4, chan_ptr);
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x004, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(bus.read_long(rate_out_ptr), 0x0001_8000);
    }

    #[test]
    fn decode_double_buffer_samples_preserves_stereo_8bit() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let data_addr = 0x1000;
        let bytes = [0x00, 0xFF, 0x40, 0xC0];
        for (idx, byte) in bytes.into_iter().enumerate() {
            bus.write_byte(data_addr + idx as u32, byte);
        }

        let samples =
            decode_double_buffer_samples(&bus, data_addr, 2, 2, 8).expect("stereo 8-bit audio");

        assert_eq!(
            samples,
            vec![
                StereoSample {
                    left: 0x00,
                    right: 0xFF
                },
                StereoSample {
                    left: 0x40,
                    right: 0xC0
                },
            ]
        );
        assert_eq!(
            samples
                .iter()
                .copied()
                .map(StereoSample::downmix)
                .collect::<Vec<_>>(),
            vec![128, 128]
        );
    }

    #[test]
    fn snddocommand_busy_channel_queues_buffer_cmd_for_later_bus_backed_decode() {
        // A queued bufferCmd needs MacMemoryBus access when it eventually
        // starts, so it must live in the guest SndChannel FIFO drained by
        // service_guest_sound_queues, not the mixer-only internal queue.
        let (mut disp, mut cpu, mut bus) = setup();
        let create_sp = TEST_SP;
        let chan_ptr_ptr = 0x2203A0;
        bus.write_long(chan_ptr_ptr, 0);
        bus.write_long(create_sp, 0);
        bus.write_long(create_sp + 4, 0);
        bus.write_word(create_sp + 8, 5);
        bus.write_long(create_sp + 10, chan_ptr_ptr);
        assert!(disp
            .dispatch_sound(true, 0x007, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        disp.sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .play_buffer(
                vec![0x20, 0x21],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );

        let header = 0x230180;
        bus.write_long(header, 0);
        bus.write_long(header + 4, 2);
        bus.write_long(header + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_long(header + 12, 0);
        bus.write_long(header + 16, 0);
        bus.write_byte(header + 20, 0);
        bus.write_byte(header + 21, 60);
        bus.write_bytes(header + 22, &[0xB0, 0xB1]);

        let cmd_ptr = 0x230160;
        bus.write_word(cmd_ptr, cmd::BUFFER);
        bus.write_word(cmd_ptr + 2, 0);
        bus.write_long(cmd_ptr + 4, header);

        let sp = TEST_SP + 0x40;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // noWait = TRUE
        bus.write_long(sp + 2, cmd_ptr);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(disp.sound_manager.mix_frame(2), vec![0x20, 0x21]);

        disp.service_guest_sound_queues(&mut bus);
        assert_eq!(disp.sound_manager.mix_frame(2), vec![0xB0, 0xB1]);
    }

    #[test]
    fn decode_mace3_mono_expands_packet_frame_to_unsigned_samples() {
        let samples = decode_mace3_mono_to_u8(&[0x00, 0x00]);

        assert_eq!(samples, vec![0x80; 6]);
    }

    #[test]
    fn decode_mace6_mono_expands_packet_to_unsigned_samples() {
        let samples = decode_mace6_mono_to_u8(&[0x00]);

        assert_eq!(samples, vec![0x80; 6]);
    }

    #[test]
    fn buffercmd_cmpsh_mace3_starts_buffer_playback() {
        let (mut disp, _cpu, mut bus) = setup();
        let chan_ptr = 0x250000;
        disp.sound_manager
            .channels
            .push(SndChannel::new(chan_ptr, false));

        let header = 0x260000;
        bus.write_long(header, 0); // samplePtr = NIL, data follows header
        bus.write_long(header + 4, 1); // mono
        bus.write_long(header + 8, crate::sound::OUTPUT_RATE << 16);
        bus.write_long(header + 12, 0); // loopStart
        bus.write_long(header + 16, 0); // loopEnd
        bus.write_byte(header + 20, 0xFE); // cmpSH
        bus.write_byte(header + 21, 60); // baseFrequency
        bus.write_long(header + 22, 1); // one MACE3 packet frame = 2 bytes
        bus.write_long(header + 40, super::MACE3_FORMAT);
        bus.write_word(header + 56, 3); // threeToOne
        bus.write_word(header + 58, 16); // threeToOnePacketSize
        bus.write_word(header + 62, 8); // 8-bit samples after expansion
        bus.write_byte(header + 64, 0x00);
        bus.write_byte(header + 65, 0x00);

        disp.execute_buffer_cmd(
            &mut bus,
            chan_ptr,
            &SndCommand {
                cmd: cmd::BUFFER,
                param1: 0,
                param2: header,
            },
        );

        assert!(disp
            .sound_manager
            .find_channel_mut(chan_ptr)
            .expect("channel exists")
            .is_playing());
        assert_eq!(disp.sound_manager.mix_frame(6), vec![0x80; 6]);
    }

    #[test]
    fn sndplaydoublebuffer_zero_sample_rate_defaults_to_classic_22khz() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP + 0x80;
        let chan_ptr = 0x250000;
        let header_ptr = 0x260000;
        let buf0_ptr = 0x270000;
        let buf1_ptr = 0x280000;

        bus.write_word(header_ptr, 1); // dbhNumChannels
        bus.write_word(header_ptr + 2, 8); // dbhSampleSize
        bus.write_word(header_ptr + 4, 0); // dbhCompressionID
        bus.write_word(header_ptr + 6, 0); // dbhPacketSize
        bus.write_long(header_ptr + 8, 0); // dbhSampleRate: tolerated zero/default rate
        bus.write_long(header_ptr + 12, buf0_ptr);
        bus.write_long(header_ptr + 16, buf1_ptr);
        bus.write_long(header_ptr + 20, 0x1234_5678); // dbhDoubleBack

        bus.write_long(buf0_ptr, 1); // dbNumFrames
        bus.write_long(buf0_ptr + 4, 0x0000_0001); // dbBufferReady
        bus.write_byte(buf0_ptr + 16, 0x40);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0020_0008); // documented SndPlayDoubleBuffer selector
        bus.write_long(sp, header_ptr);
        bus.write_long(sp + 4, chan_ptr);
        bus.write_word(sp + 8, 0xFFFF);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(disp.sound_manager.debug_double_buffer_count, 1);
        let chan = disp
            .sound_manager
            .find_channel_mut(chan_ptr)
            .expect("zero-rate probe should still create a channel");
        assert!(chan.is_playing());
        assert_eq!(
            chan.double_buffer
                .as_ref()
                .expect("double-buffer state")
                .sample_rate,
            crate::sound::RATE_22KHZ_FIXED
        );
        assert_eq!(
            disp.sound_manager.mix_frame(2),
            vec![0x40, 0x80],
            "output-rate fallback must not hold an already converted double-buffer frame for two output samples"
        );
    }

    /// Edge-case coverage for `decode_double_buffer_samples` — the
    /// SndPlayDoubleBuffer decode path. Catches regressions dropping an
    /// `if num_frames == 0` short-circuit (infinite loop), omitting the
    /// sample_size match-arm fallthrough (None for unsupported widths),
    /// or mishandling the mono-channel accum/1 divide.
    #[test]
    fn decode_double_buffer_samples_edge_cases() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let data_addr = 0x3000;
        bus.write_bytes(data_addr, &[0xAA; 64]);

        // num_frames=0 → Some(empty) short-circuit.
        let out = decode_double_buffer_samples(&bus, data_addr, 0, 2, 16)
            .expect("empty frames → Some(empty)");
        assert!(out.is_empty(), "num_frames=0 produces empty Vec");

        // num_channels=0 → Some(empty) short-circuit.
        let out = decode_double_buffer_samples(&bus, data_addr, 4, 0, 8)
            .expect("zero channels → Some(empty)");
        assert!(out.is_empty(), "num_channels=0 produces empty Vec");

        // sample_size=24 (not 8 or 16) → None.
        assert!(
            decode_double_buffer_samples(&bus, data_addr, 4, 1, 24).is_none(),
            "unsupported sample_size=24 must be rejected"
        );
        assert!(
            decode_double_buffer_samples(&bus, data_addr, 4, 1, 32).is_none(),
            "unsupported sample_size=32 must be rejected"
        );

        // Mono 8-bit pass-through: each byte is already unsigned
        // PCM in [0, 255]. accum=(byte-128) / 1 + 128 = byte.
        bus.write_byte(data_addr, 0x40);
        bus.write_byte(data_addr + 1, 0x80);
        bus.write_byte(data_addr + 2, 0xC0);
        let out =
            decode_double_buffer_samples(&bus, data_addr, 3, 1, 8).expect("mono 8-bit frames");
        assert_eq!(
            out,
            vec![
                StereoSample::mono(0x40),
                StereoSample::mono(0x80),
                StereoSample::mono(0xC0)
            ],
            "mono 8-bit pass-through"
        );

        // Mono 16-bit: sample i16 >> 8 gives the upper byte.
        // +0x4000 big-endian = [0x40, 0x00] → i16 = 0x4000 → >>8 = 0x40
        //   → +128 = 0xC0.
        // -0x4000 big-endian = [0xC0, 0x00] → i16 = -0x4000 → >>8 = -0x40
        //   → +128 = 0x40.
        bus.write_byte(data_addr, 0x40);
        bus.write_byte(data_addr + 1, 0x00);
        bus.write_byte(data_addr + 2, 0xC0);
        bus.write_byte(data_addr + 3, 0x00);
        let out =
            decode_double_buffer_samples(&bus, data_addr, 2, 1, 16).expect("mono 16-bit frames");
        assert_eq!(
            out,
            vec![StereoSample::mono(0xC0), StereoSample::mono(0x40)],
            "mono 16-bit upper-byte + center"
        );
    }

    #[test]
    fn decode_double_buffer_samples_preserves_stereo_16bit() {
        let mut bus = MacMemoryBus::new(1024 * 1024);
        let data_addr = 0x2000;
        let bytes = [
            0x80, 0x00, 0x7F, 0xFF, // frame 0: hard left + right
            0x20, 0x00, 0x60, 0x00, // frame 1: medium amplitudes
        ];
        for (idx, byte) in bytes.into_iter().enumerate() {
            bus.write_byte(data_addr + idx as u32, byte);
        }

        let samples =
            decode_double_buffer_samples(&bus, data_addr, 2, 2, 16).expect("stereo 16-bit audio");

        assert_eq!(
            samples,
            vec![
                StereoSample {
                    left: 0x00,
                    right: 0xFF
                },
                StereoSample {
                    left: 0xA0,
                    right: 0xE0
                },
            ]
        );
        assert_eq!(
            samples
                .iter()
                .copied()
                .map(StereoSample::downmix)
                .collect::<Vec<_>>(),
            vec![128, 192]
        );
    }
}
