//! Typed Gestalt Manager dispatch for PowerPC imports.

use super::*;
use crate::machine_profile::KEYBOARD_ENVIRON_TYPE;

pub(super) struct PpcGestaltDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_gestalt_import(
    context: PpcGestaltDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcGestaltDispatchContext {
        binding,
        cpu,
        memory,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::Gestalt => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_gestalt(cpu, memory, toolbox_startup),
        ))),
        PpcImportDispatcherTarget::NewGestaltValue => {
            let selector = cpu.gpr[3];
            let value = cpu.gpr[4];
            let error = if ppc_gestalt_response(selector, toolbox_startup.physical_ram_size)
                .is_some()
                || toolbox_startup.gestalt_values.contains_key(&selector)
            {
                PPC_GESTALT_DUP_SELECTOR_ERR
            } else {
                toolbox_startup.gestalt_values.insert(selector, value);
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        _ => None,
    }
}

fn ppc_gestalt(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    toolbox_startup: &PpcToolboxStartupState,
) -> i16 {
    let selector = cpu.gpr[3];
    let response_ptr = cpu.gpr[4];
    if response_ptr == 0 {
        return PPC_PARAM_ERR;
    }

    let Some((response, err)) = ppc_gestalt_response(selector, toolbox_startup.physical_ram_size)
        .or_else(|| {
            toolbox_startup
                .gestalt_values
                .get(&selector)
                .copied()
                .map(|value| (value, PPC_NO_ERR))
        })
    else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Gestalt({:?}) -> gestaltUndefSelectorErr",
                ppc_res_type_text(selector)
            );
        }
        if memory.write_u32_be(response_ptr, 0).is_none() {
            return PPC_PARAM_ERR;
        }
        return PPC_GESTALT_UNDEF_SELECTOR_ERR;
    };
    if memory.write_u32_be(response_ptr, response).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Gestalt({:?}) -> ${response:08X} err={err} lr=${:08X}",
            ppc_res_type_text(selector),
            cpu.lr
        );
    }
    err
}

fn ppc_gestalt_response(selector: u32, physical_ram_size: u32) -> Option<(u32, i16)> {
    match &selector.to_be_bytes() {
        b"vers" => Some((0x0001, PPC_NO_ERR)),
        b"sysv" => Some((u32::from(POWERPC_SYSTEM_VERSION_BCD), PPC_NO_ERR)),
        // Script Manager version is returned in the low word. Photoshop 3
        // checks for its presence before opening the native application.
        b"scri" => Some((0x0700, PPC_NO_ERR)),
        // Text Services Manager 1.5 is part of the classic Mac OS profile.
        b"tsmv" => Some((0x0150, PPC_NO_ERR)),
        // Edition Manager present (bit 0); Photoshop checks this at startup.
        b"edtn" => Some((1, PPC_NO_ERR)),
        b"cbon" => Some((u32::from(POWERPC_CARBON_VERSION_BCD), PPC_NO_ERR)),
        b"ostt" => Some((crate::trap::dispatch::OS_TRAP_TABLE_BASE, PPC_NO_ERR)),
        b"tbtt" => Some((crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE, PPC_NO_ERR)),
        b"evnt" => Some((0x0001, PPC_NO_ERR)),
        b"kbd " => Some((u32::from(KEYBOARD_ENVIRON_TYPE), PPC_NO_ERR)),
        b"cput" => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.native_cpu_type,
            PPC_NO_ERR,
        )),
        // gestaltNativeCPUfamily ('cpuf') reports the processor family.
        // Mac OS 8 Technote TN1102, "Native CPU Family Gestalt": a 604
        // reports gestaltCPU604 for both 'cpuf' and 'cput'.
        b"cpuf" => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.native_cpu_type,
            PPC_NO_ERR,
        )),
        // Gestalt.h: gestaltProcClkSpeed ('pclk') reports the processor's
        // clock rate in hertz. Keep this guest machine property separate
        // from host execution throughput.
        b"pclk" => Some((REFERENCE_POWERPC_CPU_CLOCK_HZ, PPC_NO_ERR)),
        b"sysa" => REFERENCE_POWERPC_EXECUTION_CAPABILITIES
            .system_architecture
            .map(|architecture| (architecture, PPC_NO_ERR)),
        b"proc" => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.processor_type,
            PPC_NO_ERR,
        )),
        // Apple Gestalt Manager: gestaltPowerPCProcessorFeatures ('ppcf')
        // reports optional CPU instruction sets as feature bits. Keep the
        // emulated processor's optional feature mask empty until each
        // instruction set is verified in the PowerPC interpreter.
        b"ppcf" => Some((0, PPC_NO_ERR)),
        b"mach" => Some((
            u32::from(crate::machine_profile::POWERPC_GESTALT_MACHINE_TYPE),
            PPC_NO_ERR,
        )),
        // A Power Macintosh 9500 has VIA controllers and 53C96 SCSI buses.
        // Inside Macintosh: Operating System Utilities (1994), Gestalt
        // Manager hardware attribute bit definitions.
        b"hdwr" => Some((
            (1 << 0) | (1 << 1) | (1 << 4) | (1 << 19) | (1 << 21) | (1 << 22),
            PPC_NO_ERR,
        )),
        // Match the 68K toolbox profile: System 7 Color QuickDraw 1.3.
        // Reporting only the original 32-Bit QuickDraw release makes native
        // PPC applications select obsolete monochrome-GWorld fallbacks.
        b"qd  " => Some((0x0230, PPC_NO_ERR)),
        b"qdrw" => Some((0x000F, PPC_NO_ERR)),
        b"ram " => Some((physical_ram_size, PPC_NO_ERR)),
        // With virtual memory disabled, logical and physical RAM are equal.
        b"lram" => Some((physical_ram_size, PPC_NO_ERR)),
        b"fpu " => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.fpu_type,
            PPC_NO_ERR,
        )),
        b"mmu " => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.mmu_type,
            PPC_NO_ERR,
        )),
        b"snd " => Some((0x1CFB, PPC_NO_ERR)),
        b"tmgr" => Some((2, PPC_NO_ERR)),
        // Thread Manager (1999), p. 19: bits 0 and 2 advertise the manager
        // and its PowerPC ThreadsLib. Exact stack-size matching (bit 1)
        // remains unavailable.
        b"thds" => Some((0b101, PPC_NO_ERR)),
        b"dplv" => Some((0x0002_0006, PPC_NO_ERR)),
        b"dply" => Some((0x0000_0007, PPC_NO_ERR)),
        b"alis" => Some((1, PPC_NO_ERR)),
        b"fs  " => Some(((1 << 0) | (1 << 1), PPC_NO_ERR)),
        b"fold" => Some((1, PPC_NO_ERR)),
        b"qtim" => Some((crate::machine_profile::QUICKTIME_NUM_VERSION, PPC_NO_ERR)),
        // Apple TN1083: 'qtrs' bit 0 reports a registered PowerPC
        // QuickTimeLib, whose movie entry points are bound by this runtime.
        b"qtrs" => Some((1, PPC_NO_ERR)),
        b"drag" => Some((0, PPC_NO_ERR)),
        b"os  " => Some((0x00FF, PPC_NO_ERR)),
        b"powr" => Some((0, PPC_NO_ERR)),
        b"appr" => Some((1, PPC_NO_ERR)),
        // Apple Gestalt Manager, gestaltAppearanceVersion ('apvr'):
        // version 1.0.1 is encoded as BCD $0101 in the low word.
        b"apvr" => Some((
            u32::from(crate::machine_profile::APPEARANCE_MANAGER_VERSION_BCD),
            PPC_NO_ERR,
        )),
        b"addr" => Some((0b111, PPC_NO_ERR)),
        b"sdev" => Some((0, PPC_NO_ERR)),
        b"stdf" => Some((1, PPC_NO_ERR)),
        b"help" => Some((1, PPC_NO_ERR)),
        b"vm  " => Some((0, PPC_NO_ERR)),
        b"cfrg" => Some((1, PPC_NO_ERR)),
        b"mixd" => Some((1, PPC_NO_ERR)),
        b"qd3d" => Some((1, PPC_NO_ERR)),
        b"q3v " => Some((PPC_QD3D_VERSION, PPC_NO_ERR)),
        b"a/ux" => Some((0, PPC_GESTALT_UNDEF_SELECTOR_ERR)),
        _ => None,
    }
}
