use super::{
    decode_double_buffer_samples, decode_mace3_mono_to_u8, decode_mace6_mono_to_u8,
    extended80_to_f64, parse_aiff_samples, synth_sys_beep_samples,
    GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET, GUEST_SND_CHANNEL_Q_HEAD_OFFSET,
    GUEST_SND_CHANNEL_Q_LENGTH_OFFSET, GUEST_SND_CHANNEL_Q_TAIL_OFFSET, GUEST_SND_CHANNEL_SIZE,
    MEM_FULL_ERR, SAMPLED_SYNTH_ID, SOUND_MANAGER_3_SYNTH_VERSION,
};
use crate::cpu::{CpuOps, Register};
use crate::managers::resource::ResourceFork;
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::sound::{
    cmd, PendingDoubleBackCallback, PendingSoundCallback, PlaybackKind, SndChannel, SndCommand,
    StereoSample,
};
use crate::trap::test_helpers::{setup, TEST_SP};

#[test]
fn sound_dispatch_generated_routes_preserve_exact_long_values() {
    assert_eq!(super::SOUND_DISPATCH_OPERATION_ROUTES.len(), 54);
    assert!(super::SOUND_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (0x0000_000C, "SpeechManagerVersion"),
        (0x0000_0010, "MACEVersion"),
        (0x0000_0014, "SPBVersion"),
        (0x0004_0010, "Comp3to1"),
        (0x0008_0010, "Exp1to3"),
        (0x000C_0008, "SndSoundManagerVersion"),
        (0x000C_0010, "Comp6to1"),
        (0x0010_0010, "Exp1to6"),
        (0x003C_000C, "SpeechBusy"),
        (0x0040_000C, "SpeechBusySystemWide"),
        (0x0108_000C, "CountVoices"),
        (0x0110_0014, "SPBSignOutDevice"),
        (0x0204_0008, "SndPauseFilePlay"),
        (0x021C_000C, "DisposeSpeechChannel"),
        (0x021C_0014, "SPBCloseDevice"),
        (0x0220_000C, "SpeakString"),
        (0x0228_0014, "SPBPauseRecording"),
        (0x022C_000C, "StopSpeech"),
        (0x022C_0014, "SPBResumeRecording"),
        (0x0230_0014, "SPBStopRecording"),
        (0x0238_000C, "ContinueSpeech"),
        (0x0308_0008, "SndStopFilePlay"),
        (0x030C_000C, "GetIndVoice"),
        (0x030C_0014, "SPBSignInDevice"),
        (0x0320_0014, "SPBRecord"),
        (0x0418_000C, "NewSpeechChannel"),
        (0x0424_0014, "SPBRecordToFile"),
        (0x0430_000C, "StopSpeechAt"),
        (0x0434_000C, "PauseSpeechAt"),
        (0x0440_0014, "SPBMillisecondsToBytes"),
        (0x0444_000C, "SetSpeechRate"),
        (0x0444_0014, "SPBBytesToMilliseconds"),
        (0x0448_000C, "GetSpeechRate"),
        (0x044C_000C, "SetSpeechPitch"),
        (0x0450_000C, "GetSpeechPitch"),
        (0x0460_000C, "UseDictionary"),
        (0x0514_0014, "SPBGetIndexedDevice"),
        (0x0518_0014, "SPBOpenDevice"),
        (0x0604_000C, "MakeVoiceSpec"),
        (0x0610_000C, "GetVoiceDescription"),
        (0x0614_000C, "GetVoiceInfo"),
        (0x0624_000C, "SpeakText"),
        (0x0638_0014, "SPBGetDeviceInfo"),
        (0x063C_0014, "SPBSetDeviceInfo"),
        (0x0654_000C, "SetSpeechInfo"),
        (0x0658_000C, "GetSpeechInfo"),
        (0x0708_0014, "SndRecordToFile"),
        (0x0804_0014, "SndRecord"),
        (0x0828_000C, "SpeakBuffer"),
        (0x0A5C_000C, "TextToPhonemes"),
        (0x0B4C_0014, "SetupAIFFHeader"),
        (0x0D00_0008, "SndStartFilePlay"),
        (0x0D48_0014, "SetupSndHeader"),
        (0x0E34_0014, "SPBGetRecordingStatus"),
    ] {
        let route =
            super::sound_dispatch_operation_route(0xA800, selector).expect("SoundDispatch route");
        assert_eq!(route.routine_name, routine_name);
    }

    let midi_stop_time_route = super::sound_dispatch_operation_route(0xA800, 0x0064_0004)
        .expect("SoundDispatch MIDIStopTime route");
    assert_eq!(midi_stop_time_route.routine_name, "MIDIStopTime");
    assert_eq!(
        midi_stop_time_route.operation_id,
        "selector-operation:_SoundDispatch:0x00640004:d0-long-immediate:32"
    );

    for (trap_word, selector) in [
        (0xA900, 0x003C_000C),
        (0xA800, 0x003D_000C),
        (0xA800, 0x0010_0008),
        (0xA900, 0x0064_0004),
        (0xA800, 0x0060_0004),
        (0xA800, 0x0068_0004),
        (0xA800, 0x0064_0008),
        (0xA800, 0x0065_0004),
        (0xA800, 0x060C_0018),
        (0xA800, 0x0000_203C),
    ] {
        assert!(super::sound_dispatch_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn sound_dispatch_records_exact_identity_and_rejects_wrong_routine_value() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.current_trap_word = 0xA800;

    cpu.write_reg(Register::D0, 0x003C_000C);
    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
    assert!(result.expect("SoundDispatch arm").is_ok());
    assert_eq!(
        disp.current_selector_operation,
        Some(super::SOUND_DISPATCH_OPERATION_ROUTES[8].operation_id)
    );

    let sp = TEST_SP + 0x80;
    bus.write_word(sp, 0x003B);
    bus.write_long(sp + 2, 0x003B_2E3C);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0x0064_0004);
    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
    assert!(result.expect("SoundDispatch arm").is_ok());
    assert_eq!(
        disp.current_selector_operation,
        Some(super::MIDI_STOP_TIME_OPERATION_ROUTE.operation_id)
    );

    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x003D_000C);
    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
    assert!(result.expect("SoundDispatch arm").is_ok());
    assert_eq!(disp.current_selector_operation, None);
}

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

#[test]
fn legacy_sound_driver_decodes_supported_synths() {
    let (mut disp, _cpu, mut bus) = setup();
    bus.write_byte(crate::memory::globals::addr::SD_VOLUME, 7);

    let free_form = 0x2F0000;
    bus.write_word(free_form, 0);
    bus.write_long(free_form + 2, 0x0001_0000);
    bus.write_bytes(free_form + 6, &[0x20, 0xE0, 0x20, 0xE0]);

    assert_eq!(
        disp.write_device_driver(&mut bus, -4, free_form, 10),
        Ok(10)
    );
    let chan = disp
        .legacy_sound_driver_channel
        .and_then(|ptr| disp.sound_manager.find_channel(ptr))
        .expect("legacy sound driver channel allocated");
    assert_eq!(
        chan.playback_sample_rate(),
        Some(crate::sound::RATE_22KHZ_FIXED)
    );
    let free_form_mix = disp.sound_manager.mix_frame(4);
    assert!(free_form_mix.iter().any(|sample| *sample != 0x80));

    // Test fractional samplingRate (e.g. Stunt Copter 1/6th factor $2AAA -> ~3,709 Hz)
    let free_form_frac = 0x2F1000;
    bus.write_word(free_form_frac, 0);
    bus.write_long(free_form_frac + 2, 0x0000_2AAA);
    bus.write_bytes(free_form_frac + 6, &[0x20, 0xE0, 0x20, 0xE0]);
    assert_eq!(
        disp.write_device_driver(&mut bus, -4, free_form_frac, 10),
        Ok(10)
    );
    let chan_frac = disp
        .legacy_sound_driver_channel
        .and_then(|ptr| disp.sound_manager.find_channel(ptr))
        .expect("legacy sound driver channel allocated");
    let expected_frac_rate = ((u64::from(crate::sound::RATE_22KHZ_FIXED) * 0x2AAA) >> 16) as u32;
    assert_eq!(chan_frac.playback_sample_rate(), Some(expected_frac_rate));

    let square = 0x300000;
    bus.write_word(square, (-1i16) as u16);
    bus.write_word(square + 2, 14_243); // pitch period
    bus.write_word(square + 4, 255); // amplitude
    bus.write_word(square + 6, 1); // one tick
    bus.write_word(square + 8, 0); // terminating triplet
    bus.write_word(square + 10, 0);
    bus.write_word(square + 12, 0);

    assert_eq!(disp.write_device_driver(&mut bus, -4, square, 14), Ok(14));
    let square_mix = disp.sound_manager.mix_frame(128);
    assert!(square_mix.iter().any(|sample| *sample != 0x80));

    let synth = 0x310000;
    let record = 0x311000;
    let wave = 0x312000;
    bus.write_word(synth, 1);
    bus.write_long(synth + 2, record);
    bus.write_word(record, 1); // one tick
    bus.write_long(record + 2, 0x0001_0000); // voice 1 rate
    bus.write_long(record + 6, 0); // voice 1 phase
    for voice in 1..4u32 {
        bus.write_long(record + 2 + voice * 8, 0);
        bus.write_long(record + 6 + voice * 8, 0);
    }
    bus.write_long(record + 34, wave);
    bus.write_long(record + 38, 0);
    bus.write_long(record + 42, 0);
    bus.write_long(record + 46, 0);
    for offset in 0..256u32 {
        bus.write_byte(wave + offset, 0xE0);
    }

    assert_eq!(disp.write_device_driver(&mut bus, -4, synth, 6), Ok(6));
    let four_tone_mix = disp.sound_manager.mix_frame(128);
    assert!(four_tone_mix.iter().any(|sample| *sample != 0x80));
    assert_eq!(disp.sound_manager.channels.len(), 1);
}

#[test]
fn square_wave_synth_rejects_oversized_decoded_output() {
    let (mut disp, _cpu, mut bus) = setup();
    let square = 0x300000;
    bus.write_word(square, (-1i16) as u16);
    for index in 0..2u32 {
        let tone = square + 2 + index * 6;
        bus.write_word(tone, 14_243);
        bus.write_word(tone + 2, 255);
        bus.write_word(tone + 4, u16::MAX);
    }

    assert_eq!(
        disp.write_device_driver(&mut bus, -4, square, 14),
        Err(MEM_FULL_ERR)
    );
    assert!(disp.sound_manager.channels.is_empty());
    assert_eq!(disp.legacy_sound_driver_channel, None);
}

#[test]
fn four_tone_phase_is_a_waveform_byte_offset() {
    let (_disp, _cpu, mut bus) = setup();
    let synth = 0x310000;
    let record = 0x311000;
    let wave = 0x312000;
    bus.write_word(synth, 1);
    bus.write_long(synth + 2, record);
    bus.write_word(record, 1);
    bus.write_long(record + 2, 0);
    bus.write_long(record + 6, 1);
    for voice in 1..4u32 {
        bus.write_long(record + 2 + voice * 8, 0);
        bus.write_long(record + 6 + voice * 8, 0);
    }
    bus.write_long(record + 34, wave);
    bus.write_long(record + 38, 0);
    bus.write_long(record + 42, 0);
    bus.write_long(record + 46, 0);
    bus.write_byte(wave, 0x20);
    bus.write_byte(wave + 1, 0xE0);

    let (samples, _) =
        super::super::TrapDispatcher::decode_four_tone_synth(&bus, synth, 6).unwrap();

    assert_eq!(samples[0], 0xE0);
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
fn parse_aiff_samples_accepts_truncated_final_sound_data_chunk() {
    // Some StuffIt-preserved AIFF data forks declare a FORM/SSND length a
    // few bytes past EOF. SndStartFilePlay should still accept the file
    // and play the complete sample frames that are actually present.
    let one_hz = [0x3F, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let mut aiff = build_aiff(b"AIFF", 2, 8, one_hz, &[0x40, 0x40, 0xC0, 0xC0], None);
    aiff.truncate(aiff.len() - 2);

    let (samples, rate) = parse_aiff_samples(&aiff).expect("truncated final SSND accepted");
    assert_eq!(samples, vec![0xC0]);
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

#[test]
fn sndstartfileplay_loads_non_preloaded_sound_resource() {
    // OpenResFile loads only resPreload resources, while resource-returning
    // operations normally materialize requested data on demand.
    // Inside Macintosh Volume I (1985), I-115 and I-118;
    // Inside Macintosh: Sound (1994), 2-138 to 2-140.
    let (mut disp, _cpu, mut bus) = setup();
    let fork = ResourceFork::from_test_resources(vec![(
        *b"snd ",
        30_000,
        vec![0x00, 0x02, 0x00, 0x00, 0x00, 0x00],
    )]);
    disp.load_resources(&fork, &mut bus);
    assert!(disp.find_loaded_resource_any(*b"snd ", 30_000).is_none());

    let chan_ptr = 0x250000;
    disp.sound_manager
        .add_channel(SndChannel::new(chan_ptr, false));

    assert_eq!(
        disp.snd_start_file_play(&mut bus, chan_ptr, 0, 30_000, 0, 0, 1),
        0
    );
    assert!(disp.find_loaded_resource_any(*b"snd ", 30_000).is_some());
    assert_eq!(disp.sound_manager.debug_file_play_count, 1);
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
        .add_channel(SndChannel::new(chan_ptr, false));

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
        .find_channel(chan_ptr)
        .expect("channel exists")
        .is_playing());
    assert_eq!(
        disp.sound_manager.mix_frame(4),
        vec![0x40, 0x80, 0xC0, 0xFF]
    );
}

#[test]
fn sndplay_resolves_legacy_format2_sampled_voice_header() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP;
    let chan_ptr = 0x250200;
    disp.sound_manager
        .add_channel(SndChannel::new(chan_ptr, false));

    let (snd_handle, snd_ptr) = alloc_minimal_format2_snd_handle(&mut bus, 2, 80);
    let compact = snd_ptr + 14;
    bus.write_word(snd_ptr + 4, 1);
    bus.write_word(snd_ptr + 6, 0x8000 | cmd::SOUND);
    bus.write_word(snd_ptr + 8, 0);
    bus.write_long(snd_ptr + 10, 20); // legacy soundCmd points six bytes into header
    bus.write_long(compact, 0); // inline samples
    bus.write_long(compact + 4, 4);
    bus.write_long(compact + 8, crate::sound::OUTPUT_RATE << 16);
    bus.write_long(compact + 12, 0);
    bus.write_long(compact + 16, 0);
    bus.write_byte(compact + 20, 0);
    bus.write_byte(compact + 21, 60);
    bus.write_bytes(compact + 22, &[0x40, 0x80, 0xC0, 0xFF]);

    bus.write_word(sp, 1);
    bus.write_long(sp + 2, snd_handle);
    bus.write_long(sp + 6, chan_ptr);
    bus.write_word(sp + 10, 0xFFFF);

    let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 10), 0);
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
        .add_channel(SndChannel::new(chan_ptr, false));
    bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_LENGTH_OFFSET, 128);
    bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET, 0);
    bus.write_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET, 0);
    disp.sound_manager
        .with_channel_mut(chan_ptr, |channel| {
            channel.play_buffer(
                vec![0x10, 0x11],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );
        })
        .expect("channel exists");

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
fn sounddispatch_unsigned_fixed_mul_div_returns_result_and_pops_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP + 0x80;

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0x060C_0018); // UnsignedFixedMulDiv
    bus.write_long(sp, 0x5622_0000); // divisor
    bus.write_long(sp + 4, 0x5622_0000); // multiplier
    bus.write_long(sp + 8, 0x0001_0000); // value
    bus.write_long(sp + 12, 0xDEAD_BEEF); // result placeholder

    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_long(sp + 12), 0x0001_0000);
}

#[test]
fn setup_snd_header_builds_standard_format_one_resource() {
    // Inside Macintosh: Sound (1994), pp. 2-76 to 2-77 and 3-44
    // to 3-46: 8-bit mono NONE data uses the 22-byte SoundHeader
    // after a 20-byte format-1 resource prefix.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP + 0x80;
    let handle = 0x230800;
    let resource = 0x230900;
    let header_len = 0x230880;
    bus.write_long(handle, resource);

    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0x0D48_0014);
    bus.write_long(sp, header_len);
    bus.write_long(sp + 4, 0x2000);
    bus.write_word(sp + 8, 0x43);
    bus.write_long(sp + 10, u32::from_be_bytes(*b"NONE"));
    bus.write_word(sp + 14, 8);
    bus.write_long(sp + 16, 0x5622_0000);
    bus.write_word(sp + 20, 1);
    bus.write_long(sp + 22, handle);
    bus.write_word(sp + 26, 0xBEEF);

    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 26);
    assert_eq!(bus.read_word(sp + 26), 0);
    assert_eq!(bus.read_word(header_len), 42);
    assert_eq!(bus.read_word(resource), 1);
    assert_eq!(bus.read_word(resource + 2), 1);
    assert_eq!(bus.read_word(resource + 4), 5);
    assert_eq!(bus.read_long(resource + 6), 0x80);
    assert_eq!(bus.read_word(resource + 10), 1);
    assert_eq!(bus.read_word(resource + 12), 0x8051);
    assert_eq!(bus.read_long(resource + 16), 20);
    assert_eq!(bus.read_long(resource + 24), 0x2000);
    assert_eq!(bus.read_long(resource + 28), 0x5622_0000);
    assert_eq!(bus.read_byte(resource + 40), 0);
    assert_eq!(bus.read_byte(resource + 41), 0x43);
}

#[test]
fn setup_snd_header_builds_extended_and_mace_headers() {
    // The same routine selects ExtSoundHeader for uncompressed stereo
    // and CmpSoundHeader for the two documented MACE formats.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP + 0x80;
    let handle = 0x230A00;
    let resource = 0x230B00;
    let header_len = 0x230A80;
    bus.write_long(handle, resource);

    let mut invoke = |compression: u32, channels: u16, sample_size: u16, num_bytes: u32| {
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0D48_0014);
        bus.write_long(sp, header_len);
        bus.write_long(sp + 4, num_bytes);
        bus.write_word(sp + 8, 60);
        bus.write_long(sp + 10, compression);
        bus.write_word(sp + 14, sample_size);
        bus.write_long(sp + 16, 0x5622_0000);
        bus.write_word(sp + 20, channels);
        bus.write_long(sp + 22, handle);
        bus.write_word(sp + 26, 0xBEEF);
        disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        (
            bus.read_word(header_len),
            bus.read_long(resource + 6),
            bus.read_byte(resource + 40),
            bus.read_long(resource + 42),
            bus.read_long(resource + 60),
            bus.read_word(resource + 68),
            bus.read_word(resource + 76),
            bus.read_word(resource + 78),
        )
    };

    let extended = invoke(u32::from_be_bytes(*b"NONE"), 2, 16, 0x2000);
    assert_eq!(extended.0, 84);
    assert_eq!(extended.1, 0xC0);
    assert_eq!(extended.2, 0xFF);
    assert_eq!(extended.3, 0x800);
    assert_eq!(extended.5, 16);

    let mace3 = invoke(u32::from_be_bytes(*b"MAC3"), 1, 8, 1000);
    assert_eq!(mace3.0, 84);
    assert_eq!(mace3.1, 0x380);
    assert_eq!(mace3.2, 0xFE);
    assert_eq!(mace3.3, 3000);
    assert_eq!(mace3.4, u32::from_be_bytes(*b"MAC3"));
    assert_eq!(mace3.6, 0xFFFF);
    assert_eq!(mace3.7, 16);

    let mace6 = invoke(u32::from_be_bytes(*b"MAC6"), 1, 8, 1000);
    assert_eq!(mace6.1, 0x480);
    assert_eq!(mace6.3, 6000);
    assert_eq!(mace6.7, 8);
}

#[test]
fn setup_snd_header_rejects_unknown_compression_without_outputs() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP + 0x80;
    let handle = 0x230C00;
    let resource = 0x230D00;
    let header_len = 0x230C80;
    bus.write_long(handle, resource);
    bus.write_long(resource, 0xDEAD_BEEF);
    bus.write_word(header_len, 0xCAFE);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0x0D48_0014);
    bus.write_long(sp, header_len);
    bus.write_long(sp + 4, 1000);
    bus.write_word(sp + 8, 60);
    bus.write_long(sp + 10, u32::from_be_bytes(*b"BAD!"));
    bus.write_word(sp + 14, 8);
    bus.write_long(sp + 16, 0x5622_0000);
    bus.write_word(sp + 20, 1);
    bus.write_long(sp + 22, handle);

    disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::A7), sp + 26);
    assert_eq!(bus.read_word(sp + 26) as i16, -223);
    assert_eq!(bus.read_word(header_len), 0xCAFE);
    assert_eq!(bus.read_long(resource), 0xDEAD_BEEF);
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
        .add_channel(SndChannel::new(0x0039_38C8, false));

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
fn sounddispatch_midi_stop_time_consumes_refnum_argument() {
    // Universal Interfaces 3.4 MIDI.h line 610:
    // EXTERN_API( void ) MIDIStopTime(short refnum) FOURWORDINLINE(0x203C, 0x0064, 0x0004, 0xA800);
    // PROCEDURE MIDIStopTime(refnum: INTEGER);
    // Consumes one 16-bit refnum argument without a function-result slot.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.current_trap_word = 0xA800;
    let sp = TEST_SP + 0x80;
    let refnum: i16 = 0x003B;
    let caller_ret_addr: u32 = 0x003B_2E3C;
    let caller_stack_sentinel: u32 = 0xA5A5_5A5A;
    bus.write_word(sp, refnum as u16);
    bus.write_long(sp + 2, caller_ret_addr);
    bus.write_long(sp + 6, caller_stack_sentinel);
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 0x0064_0004);

    let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.current_selector_operation,
        Some("selector-operation:_SoundDispatch:0x00640004:d0-long-immediate:32")
    );
    assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    assert_eq!(bus.read_long(sp + 2), caller_ret_addr);
    assert_eq!(bus.read_long(sp + 6), caller_stack_sentinel);
    assert_eq!(cpu.read_reg(Register::D0), 0);
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
    assert!(disp.sound_manager.find_channel(chan_ptr).is_some());
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
        .find_channel(chan_ptr)
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
    assert!(disp.sound_manager.find_channel(chan_ptr).is_some());

    let sp = TEST_SP + 0x40;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // quietNow = TRUE
    bus.write_long(sp + 2, chan_ptr);
    bus.write_word(sp + 6, 0xFFFF);

    let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert_eq!(bus.read_word(sp + 6), 0);
    assert!(disp.sound_manager.find_channel(chan_ptr).is_none());
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
    assert!(disp.sound_manager.find_channel(chan_ptr).is_some());
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
    assert!(disp.sound_manager.find_channel(chan_ptr).is_none());
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
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: crate::callback_manager::CallbackTaskArchitecture::M68k,
            callback_addr: 0x00AB_CDEF,
            chan_ptr,
            cmd: crate::sound::SndCommand {
                cmd: cmd::CALLBACK,
                param1: 7,
                param2: 0x1111_2222,
            },
        });
    disp.sound_manager
        .queue_sound_callback(PendingSoundCallback::FileCompletion {
            architecture: crate::callback_manager::CallbackTaskArchitecture::M68k,
            callback_addr: 0x00FE_DCBA,
            chan_ptr,
        });
    disp.sound_manager
        .queue_sound_callback(PendingSoundCallback::FileCompletion {
            architecture: crate::callback_manager::CallbackTaskArchitecture::M68k,
            callback_addr: 0x0000_2222,
            chan_ptr: other_chan_ptr,
        });
    disp.sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr: 0x00CA_FE00,
            chan_ptr,
            header_ptr: 0x0022_4400,
            exhausted_buffer_index: 1,
        });
    disp.sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
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
            chan_ptr: ptr,
            ..
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
        .with_channel_mut(chan_ptr, |channel| {
            channel.play_buffer(
                vec![0x80; 128],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );
        })
        .expect("channel exists");

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
        .find_channel(chan_ptr)
        .expect("channel exists")
        .is_playing());
    assert_eq!(disp.sound_manager.mix_frame(64).len(), 64);
}

#[test]
fn snddoimmediate_flush_discards_guest_fifo_entries() {
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
        .with_channel_mut(chan_ptr, |channel| {
            channel.play_buffer(
                vec![0xa0; 8],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );
        })
        .expect("channel exists");

    let cmd_ptr = 0x230030;
    bus.write_word(cmd_ptr, cmd::VOLUME);
    bus.write_word(cmd_ptr + 2, 0);
    bus.write_long(cmd_ptr + 4, 0);
    let sp = TEST_SP + 0x40;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // noWait = TRUE
    bus.write_long(sp + 2, cmd_ptr);
    bus.write_long(sp + 6, chan_ptr);
    bus.write_word(sp + 10, 0xffff);
    assert!(disp
        .dispatch_sound(true, 0x003, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_ne!(
        bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET),
        bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET)
    );

    bus.write_word(cmd_ptr, cmd::FLUSH);
    bus.write_long(sp, cmd_ptr); // SndDoImmediate cmd pointer
    bus.write_long(sp + 4, chan_ptr);
    bus.write_word(sp + 8, 0xffff);
    cpu.write_reg(Register::A7, sp);
    assert!(disp
        .dispatch_sound(true, 0x004, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    assert_eq!(bus.read_word(sp + 8), 0);
    assert_eq!(
        bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_HEAD_OFFSET),
        bus.read_word(chan_ptr + GUEST_SND_CHANNEL_Q_TAIL_OFFSET),
        "flush must discard entries still resident in guest FIFO"
    );
    assert_eq!(
        bus.read_long(chan_ptr + GUEST_SND_CHANNEL_CMD_IN_PROGRESS_OFFSET),
        0
    );
    disp.service_guest_sound_queues(&mut bus);
    assert_eq!(disp.sound_manager.mix_frame(4), vec![0xa0; 4]);
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
        .with_channel_mut(chan_ptr, |channel| {
            channel.play_buffer(
                vec![0x80, 0x80],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );
        })
        .expect("channel exists");

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
            architecture,
            callback_addr,
            chan_ptr: callback_chan,
            cmd,
        } => {
            assert_eq!(
                *architecture,
                crate::callback_manager::CallbackTaskArchitecture::M68k
            );
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
    disp.sound_manager
        .with_mut(|manager| manager.pending_sound_callbacks.clear());

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
            architecture,
            callback_addr,
            chan_ptr: callback_chan,
            cmd,
        } => {
            assert_eq!(
                *architecture,
                crate::callback_manager::CallbackTaskArchitecture::M68k
            );
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
        .with_channel_mut(chan_ptr, |channel| channel.set_rate(0x0001_8000))
        .expect("channel exists");

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
        .with_channel_mut(chan_ptr, |channel| {
            channel.play_buffer(
                vec![0x20, 0x21],
                crate::sound::OUTPUT_RATE << 16,
                PlaybackKind::Buffer,
                0,
            );
        })
        .expect("channel exists");

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
        .add_channel(SndChannel::new(chan_ptr, false));

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
        .find_channel(chan_ptr)
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
        .find_channel(chan_ptr)
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
    let out = decode_double_buffer_samples(&bus, data_addr, 3, 1, 8).expect("mono 8-bit frames");
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
    let out = decode_double_buffer_samples(&bus, data_addr, 2, 1, 16).expect("mono 16-bit frames");
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
