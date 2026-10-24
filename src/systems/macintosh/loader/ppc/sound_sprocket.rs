//! SoundSprocket: 3-D sound localization for Sound Manager channels.
//!
//! SoundSprocketLib keeps listeners and sources and turns their geometry
//! into an `SSpLocalizationData` record. A game installs the SoundSprocket
//! filter on a channel (`siPreMixerSoundComponent` with an `'snfx'`/`'snd3'`
//! component) and passes the record with `SndSetInfo(siSSpLocalization)`.
//! The runtime renders that as distance attenuation and stereo panning.
//! Record layouts and constants follow SoundSprocket.h (Universal
//! Interfaces 3.4).

use super::*;
use std::collections::HashMap;

/// The SoundSprocket filter component: kSoundEffectsType, subtype 'snd3'.
const SSP_FILTER_TYPE: u32 = u32::from_be_bytes(*b"snfx");
const SSP_FILTER_SUBTYPE: u32 = u32::from_be_bytes(*b"snd3");
/// Opaque listener and source references.
const SSP_REFERENCE_BASE: u32 = 0x5353_0000;

const SOURCE_MODE_UNFILTERED: u32 = 0;
const SOURCE_MODE_AMBIENT: u32 = 2;
/// SSpLocalizationData field offsets.
const LOC_SOURCE_MODE: u32 = 24;
const LOC_REFERENCE_DISTANCE: u32 = 28;
const LOC_CONE_ANGLE_COS: u32 = 32;
const LOC_ELEVATION: u32 = 40;
const LOC_AZIMUTH: u32 = 44;
const LOC_DISTANCE: u32 = 48;
const LOC_PROJECTION_ANGLE: u32 = 52;
const LOC_SOURCE_VELOCITY: u32 = 56;
const LOC_LISTENER_VELOCITY: u32 = 60;
const LOC_VIRTUAL_SOURCE_COUNT: u32 = 80;
const LOC_SIZE: u32 = 196;

type Vec3 = [f32; 3];

#[derive(Debug, Clone, Copy)]
struct Listener {
    position: Vec3,
    forward: Vec3,
    up: Vec3,
    velocity: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        // QuickDraw 3D convention: looking down -z with +y up.
        Self {
            position: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            velocity: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Source {
    position: Vec3,
    velocity: Vec3,
    mode: u32,
    reference_distance: f32,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            velocity: [0.0; 3],
            mode: 1, // kSSpSourceMode_Localized
            reference_distance: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PpcSoundSprocketState {
    listeners: HashMap<u32, Listener>,
    sources: HashMap<u32, Source>,
    next_reference: u32,
}

impl PpcSoundSprocketState {
    fn allocate(&mut self) -> u32 {
        self.next_reference = self.next_reference.wrapping_add(1);
        SSP_REFERENCE_BASE + self.next_reference * 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcSoundSprocketFunction {
    ConfigureSpeakerSetup,
    ListenerNew,
    ListenerDispose,
    ListenerSetPosition,
    ListenerSetOrientation,
    ListenerSetUpVector,
    ListenerSetVelocity,
    /// Medium, reverb, metres per unit, and similar settings that do not
    /// change the rendered gain.
    ListenerAccept,
    SourceNew,
    SourceDispose,
    SourceSetPosition,
    SourceSetVelocity,
    SourceSetMode,
    SourceSetReferenceDistance,
    /// Orientation, cone, size, and CPU load settings.
    SourceAccept,
    SourceCalcLocalization,
}

impl PpcSoundSprocketFunction {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "SSpConfigureSpeakerSetup" => Self::ConfigureSpeakerSetup,
            "SSpListener_New" => Self::ListenerNew,
            "SSpListener_Dispose" => Self::ListenerDispose,
            "SSpListener_SetPosition" => Self::ListenerSetPosition,
            "SSpListener_SetOrientation" => Self::ListenerSetOrientation,
            "SSpListener_SetUpVector" => Self::ListenerSetUpVector,
            "SSpListener_SetVelocity" | "SSpListener_SetActualVelocity" => {
                Self::ListenerSetVelocity
            }
            "SSpListener_SetMedium" | "SSpListener_SetReverb" | "SSpListener_SetMetersPerUnit" => {
                Self::ListenerAccept
            }
            "SSpSource_New" => Self::SourceNew,
            "SSpSource_Dispose" => Self::SourceDispose,
            "SSpSource_SetPosition" => Self::SourceSetPosition,
            "SSpSource_SetVelocity" | "SSpSource_SetActualVelocity" => Self::SourceSetVelocity,
            "SSpSource_SetMode" => Self::SourceSetMode,
            "SSpSource_SetReferenceDistance" => Self::SourceSetReferenceDistance,
            "SSpSource_SetOrientation"
            | "SSpSource_SetUpVector"
            | "SSpSource_SetAngularAttenuation"
            | "SSpSource_SetSize"
            | "SSpSource_SetCPULoad" => Self::SourceAccept,
            "SSpSource_CalcLocalization" => Self::SourceCalcLocalization,
            _ => return None,
        })
    }
}

/// paramErr: a nil or unknown reference or output pointer.
const SSP_PARAMETER_ERR: i32 = PPC_PARAM_ERR as i32;

fn read_vec3(memory: &mut PpcSectionMem, address: u32) -> Option<Vec3> {
    let mut v = [0.0; 3];
    for (index, component) in v.iter_mut().enumerate() {
        *component = f32::from_bits(memory.read_u32_be(address + index as u32 * 4)?);
    }
    Some(v)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    if length > 0.0 {
        [v[0] / length, v[1] / length, v[2] / length]
    } else {
        v
    }
}

pub(crate) fn ppc_dispatch_sound_sprocket(
    function: PpcSoundSprocketFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    state: &mut PpcSoundSprocketState,
) -> PpcImportAction {
    use PpcSoundSprocketFunction as F;
    let status = |value: i32| PpcImportAction::Return(value as u32);
    let reference = cpu.gpr[3];
    match function {
        // The single stereo output needs no speaker configuration.
        F::ConfigureSpeakerSetup | F::ListenerAccept | F::SourceAccept => status(0),
        F::ListenerNew | F::SourceNew => {
            let out = cpu.gpr[3];
            if out == 0 {
                return status(SSP_PARAMETER_ERR);
            }
            let handle = state.allocate();
            if memory.write_u32_be(out, handle).is_none() {
                return status(SSP_PARAMETER_ERR);
            }
            if function == F::ListenerNew {
                state.listeners.insert(handle, Listener::default());
            } else {
                state.sources.insert(handle, Source::default());
            }
            status(0)
        }
        F::ListenerDispose => status(if state.listeners.remove(&reference).is_some() {
            0
        } else {
            SSP_PARAMETER_ERR
        }),
        F::SourceDispose => status(if state.sources.remove(&reference).is_some() {
            0
        } else {
            SSP_PARAMETER_ERR
        }),
        F::ListenerSetPosition
        | F::ListenerSetOrientation
        | F::ListenerSetUpVector
        | F::ListenerSetVelocity => {
            let (Some(value), Some(listener)) = (
                read_vec3(memory, cpu.gpr[4]),
                state.listeners.get_mut(&reference),
            ) else {
                return status(SSP_PARAMETER_ERR);
            };
            match function {
                F::ListenerSetPosition => listener.position = value,
                F::ListenerSetOrientation => listener.forward = value,
                F::ListenerSetUpVector => listener.up = value,
                _ => listener.velocity = value,
            }
            status(0)
        }
        F::SourceSetPosition | F::SourceSetVelocity => {
            let (Some(value), Some(source)) = (
                read_vec3(memory, cpu.gpr[4]),
                state.sources.get_mut(&reference),
            ) else {
                return status(SSP_PARAMETER_ERR);
            };
            if function == F::SourceSetPosition {
                source.position = value;
            } else {
                source.velocity = value;
            }
            status(0)
        }
        F::SourceSetMode => match state.sources.get_mut(&reference) {
            Some(source) => {
                source.mode = cpu.gpr[4];
                status(0)
            }
            None => status(SSP_PARAMETER_ERR),
        },
        F::SourceSetReferenceDistance => match state.sources.get_mut(&reference) {
            Some(source) => {
                // The distance is a float argument, passed in f1.
                source.reference_distance = f64::from_bits(cpu.fpr[1]) as f32;
                status(0)
            }
            None => status(SSP_PARAMETER_ERR),
        },
        F::SourceCalcLocalization => {
            let (source, listener, out) = (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            let (Some(source), Some(listener)) = (
                state.sources.get(&source).copied(),
                state.listeners.get(&listener).copied(),
            ) else {
                return status(SSP_PARAMETER_ERR);
            };
            if out == 0 || !ppc_memory_can_write_bytes(memory, out, LOC_SIZE) {
                return status(SSP_PARAMETER_ERR);
            }
            write_localization(memory, out, &source, &listener);
            status(0)
        }
    }
}

/// Fill an SSpLocalizationData record: an air medium with no reverb, the
/// source's mode and reference distance, an omnidirectional cone, and the
/// source's elevation, azimuth (positive to the listener's right), and
/// distance in the listener's frame.
fn write_localization(memory: &mut PpcSectionMem, out: u32, source: &Source, listener: &Listener) {
    let _ = memory.write_bytes(out, &[0; LOC_SIZE as usize]);
    let offset = sub(source.position, listener.position);
    let distance = dot(offset, offset).sqrt();
    let forward = normalize(listener.forward);
    let up = normalize(listener.up);
    let right = normalize(cross(forward, up));
    let (elevation, azimuth) = if distance > 0.0 {
        let direction = [
            offset[0] / distance,
            offset[1] / distance,
            offset[2] / distance,
        ];
        (
            dot(direction, up).clamp(-1.0, 1.0).asin(),
            dot(direction, right).atan2(dot(direction, forward)),
        )
    } else {
        (0.0, 0.0)
    };
    let toward_listener = if distance > 0.0 {
        [
            -offset[0] / distance,
            -offset[1] / distance,
            -offset[2] / distance,
        ]
    } else {
        [0.0; 3]
    };
    let write = |memory: &mut PpcSectionMem, field: u32, value: f32| {
        let _ = memory.write_u32_be(out + field, value.to_bits());
    };
    let _ = memory.write_u32_be(out + LOC_SOURCE_MODE, source.mode);
    write(memory, LOC_REFERENCE_DISTANCE, source.reference_distance);
    write(memory, LOC_CONE_ANGLE_COS, -1.0);
    write(memory, LOC_ELEVATION, elevation);
    write(memory, LOC_AZIMUTH, azimuth);
    write(memory, LOC_DISTANCE, distance);
    write(memory, LOC_PROJECTION_ANGLE, 1.0);
    // Radial speeds: positive when moving apart.
    write(
        memory,
        LOC_SOURCE_VELOCITY,
        -dot(source.velocity, toward_listener),
    );
    write(
        memory,
        LOC_LISTENER_VELOCITY,
        dot(listener.velocity, toward_listener),
    );
    let _ = memory.write_u32_be(out + LOC_VIRTUAL_SOURCE_COUNT, 0);
}

/// Whether a `siPreMixerSoundComponent` link names the SoundSprocket filter.
pub(crate) fn ppc_is_sound_sprocket_filter(memory: &mut PpcSectionMem, link: u32) -> bool {
    memory.read_u32_be(link) == Some(SSP_FILTER_TYPE)
        && memory.read_u32_be(link + 4) == Some(SSP_FILTER_SUBTYPE)
}

/// Packed stereo gain (right in the high word, 0x0100 full) for an
/// SSpLocalizationData record: inverse-distance attenuation beyond the
/// reference distance and a constant-power pan by azimuth.
pub(crate) fn ppc_sound_sprocket_gain(memory: &mut PpcSectionMem, record: u32) -> Option<u32> {
    let float = |memory: &mut PpcSectionMem, field: u32| {
        memory.read_u32_be(record + field).map(f32::from_bits)
    };
    let mode = memory.read_u32_be(record + LOC_SOURCE_MODE)?;
    if matches!(mode, SOURCE_MODE_UNFILTERED | SOURCE_MODE_AMBIENT) {
        return Some(0x0100_0100);
    }
    let reference = float(memory, LOC_REFERENCE_DISTANCE)?;
    let distance = float(memory, LOC_DISTANCE)?;
    let azimuth = float(memory, LOC_AZIMUTH)?;
    let attenuation = if reference > 0.0 && distance > reference {
        reference / distance
    } else {
        1.0
    };
    let angle = (azimuth.sin() + 1.0) * std::f32::consts::FRAC_PI_4;
    let channel = |pan: f32| {
        let gain = (pan * std::f32::consts::SQRT_2).min(1.0) * attenuation;
        (gain.clamp(0.0, 1.0) * 256.0).round() as u32
    };
    Some((channel(angle.sin()) << 16) | channel(angle.cos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRATCH: u32 = 0x1000;

    fn call(
        function: PpcSoundSprocketFunction,
        cpu: &mut PpcCpu,
        memory: &mut PpcSectionMem,
        state: &mut PpcSoundSprocketState,
    ) -> u32 {
        match ppc_dispatch_sound_sprocket(function, cpu, memory, state) {
            PpcImportAction::Return(value) => value,
            other => panic!("unexpected {other:?}"),
        }
    }

    fn write_vec3(memory: &mut PpcSectionMem, address: u32, v: Vec3) {
        for (index, component) in v.iter().enumerate() {
            memory
                .write_u32_be(address + index as u32 * 4, component.to_bits())
                .unwrap();
        }
    }

    #[test]
    fn sound_sprocket_library_symbols_resolve() {
        for symbol in [
            "SSpConfigureSpeakerSetup",
            "SSpListener_New",
            "SSpListener_SetUpVector",
            "SSpSource_CalcLocalization",
            "SSpSource_SetReferenceDistance",
        ] {
            assert!(
                matches!(
                    dispatcher_target_for_import("SoundSprocketLib", symbol),
                    PpcImportDispatcherTarget::SoundSprocket(_)
                ),
                "{symbol}"
            );
        }
    }

    #[test]
    fn a_source_to_the_right_pans_right_and_fades_with_distance() {
        use PpcSoundSprocketFunction as F;
        let mut memory = PpcSectionMem::default();
        memory.add_region(SCRATCH, vec![0; 0x400]);
        let mut cpu = PpcCpu::default();
        let mut state = PpcSoundSprocketState::default();
        let (listener_out, source_out, vector, record) =
            (SCRATCH, SCRATCH + 4, SCRATCH + 0x10, SCRATCH + 0x100);

        cpu.gpr[3] = listener_out;
        assert_eq!(call(F::ListenerNew, &mut cpu, &mut memory, &mut state), 0);
        cpu.gpr[3] = source_out;
        assert_eq!(call(F::SourceNew, &mut cpu, &mut memory, &mut state), 0);
        let listener = memory.read_u32_be(listener_out).unwrap();
        let source = memory.read_u32_be(source_out).unwrap();

        write_vec3(&mut memory, vector, [4.0, 0.0, 0.0]);
        cpu.gpr[3] = source;
        cpu.gpr[4] = vector;
        assert_eq!(
            call(F::SourceSetPosition, &mut cpu, &mut memory, &mut state),
            0
        );
        cpu.fpr[1] = 2.0f64.to_bits();
        assert_eq!(
            call(
                F::SourceSetReferenceDistance,
                &mut cpu,
                &mut memory,
                &mut state
            ),
            0
        );

        cpu.gpr[3] = source;
        cpu.gpr[4] = listener;
        cpu.gpr[5] = record;
        assert_eq!(
            call(F::SourceCalcLocalization, &mut cpu, &mut memory, &mut state),
            0
        );
        let float = |memory: &mut PpcSectionMem, field: u32| {
            f32::from_bits(memory.read_u32_be(record + field).unwrap())
        };
        assert!((float(&mut memory, LOC_DISTANCE) - 4.0).abs() < 1e-5);
        assert!((float(&mut memory, LOC_AZIMUTH) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((float(&mut memory, LOC_REFERENCE_DISTANCE) - 2.0).abs() < 1e-5);

        // Twice the reference distance halves the level; hard right is silent
        // on the left.
        let gain = ppc_sound_sprocket_gain(&mut memory, record).unwrap();
        assert_eq!(gain >> 16, 0x80);
        assert_eq!(gain & 0xFFFF, 0);

        memory
            .write_u32_be(record + LOC_SOURCE_MODE, SOURCE_MODE_AMBIENT)
            .unwrap();
        assert_eq!(
            ppc_sound_sprocket_gain(&mut memory, record),
            Some(0x0100_0100)
        );

        cpu.gpr[3] = source;
        assert_eq!(call(F::SourceDispose, &mut cpu, &mut memory, &mut state), 0);
        assert_eq!(
            call(F::SourceDispose, &mut cpu, &mut memory, &mut state),
            PPC_PARAM_ERR as i32 as u32
        );
    }

    #[test]
    fn only_the_sound_sprocket_component_is_a_premixer_filter() {
        let mut memory = PpcSectionMem::default();
        memory.add_region(SCRATCH, vec![0; 0x20]);
        memory.write_bytes(SCRATCH, b"snfxsnd3").unwrap();
        assert!(ppc_is_sound_sprocket_filter(&mut memory, SCRATCH));
        memory.write_bytes(SCRATCH, b"snfxrvb ").unwrap();
        assert!(!ppc_is_sound_sprocket_filter(&mut memory, SCRATCH));
    }
}
