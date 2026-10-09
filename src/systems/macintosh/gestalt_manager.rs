//! Architecture-neutral evaluation helpers and canonical structures for
//! Macintosh Gestalt Manager operations.
//!
//! Inside Macintosh: Operating System Utilities (1994), Chapter 1 ("Gestalt Manager")
//! Inside Macintosh: Volume VI (1991), Chapter 3
//! Inside Macintosh: Volume II (1985), pp. II-379--384

#![allow(dead_code)]

use crate::machine_profile::{
    APPEARANCE_MANAGER_VERSION_BCD, KEYBOARD_ENVIRON_TYPE, POWERPC_GESTALT_MACHINE_TYPE,
    POWERPC_SYSTEM_VERSION_BCD, QUICKTIME_NUM_VERSION, REFERENCE_MACHINE_PROFILE,
};
use crate::systems::macintosh::machine_profile::{
    POWERPC_CARBON_VERSION_BCD, REFERENCE_M68K_EXECUTION_CAPABILITIES,
    REFERENCE_POWERPC_CPU_CLOCK_HZ, REFERENCE_POWERPC_EXECUTION_CAPABILITIES,
};
use crate::trap::dispatch::{OS_TRAP_TABLE_BASE, TOOLBOX_TRAP_TABLE_BASE};

/// `noErr` (0).
pub const NO_ERR: i16 = 0;

/// `paramErr` (-50).
pub const PARAM_ERR: i16 = -50;

/// `gestaltUndefSelectorErr` (-5551). Returned by `Gestalt` /
/// `ReplaceGestalt` when the selector code is not recognised.
/// Inside Macintosh: Operating System Utilities 1994, 1-32 / 1-35.
pub const GESTALT_UNDEF_SELECTOR_ERR: i16 = -5551;

/// `gestaltDupSelectorErr` (-5552). Returned by `NewGestalt` when the
/// caller tries to register a selector that is already known to the
/// Gestalt Manager (built-in or previously installed).
/// Inside Macintosh: Operating System Utilities 1994, 1-34.
pub const GESTALT_DUP_SELECTOR_ERR: i16 = -5552;

/// `gestaltLocationErr` (-5553). Returned by `NewGestalt` / `ReplaceGestalt`
/// when the handler function is not in the system heap.
/// Inside Macintosh: Operating System Utilities 1994, 1-34.
pub const GESTALT_LOCATION_ERR: i16 = -5553;

/// 32-bit unsigned representation of `gestaltUndefSelectorErr` ($FFFFEA51).
pub const GESTALT_UNDEF_SELECTOR_ERR_U32: u32 = 0xFFFF_EA51;

/// 32-bit unsigned representation of `gestaltDupSelectorErr` ($FFFFEA50).
pub const GESTALT_DUP_SELECTOR_ERR_U32: u32 = 0xFFFF_EA50;

/// 32-bit unsigned representation of `gestaltLocationErr` ($FFFFEA4F).
pub const GESTALT_LOCATION_ERR_U32: u32 = 0xFFFF_EA4F;

/// System architecture code for 68k processor (`gestalt68k` = 1).
pub const GESTALT_SYS_ARCH_68K: u32 = 1;

/// System architecture code for PowerPC processor (`gestaltPowerPC` = 2).
pub const GESTALT_SYS_ARCH_POWERPC: u32 = 2;

/// Logical page size in bytes for the flat 68040 profile (4096 bytes).
pub const GESTALT_68040_LOGICAL_PAGE_SIZE_BYTES: u32 = 4096;

/// QuickDraw 3D release version reported under `q3v ` ($01608000 = QD3D 1.6).
pub const GESTALT_QD3D_VERSION: u32 = crate::loader::ppc::PPC_QD3D_VERSION;

/// Execution architecture for Gestalt Manager evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestaltArchitecture {
    /// Motorola 680x0 classic Macintosh architecture.
    M68k {
        /// Whether 32-bit addressing mode is currently active.
        mmu_mode_32: bool,
    },
    /// PowerPC Macintosh architecture.
    PowerPc,
}

/// Canonical evaluation context encapsulating machine profile and runtime
/// execution capabilities for Gestalt queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestaltEvaluationContext {
    /// Guest execution architecture.
    pub architecture: GestaltArchitecture,
    /// System software version in BCD (e.g. $0753 or $0755).
    pub system_version_bcd: u32,
    /// System architecture identifier (1 for 68k, 2 for PowerPC).
    pub system_architecture: Option<u32>,
    /// Native CPU type code.
    pub native_cpu_type: u32,
    /// Processor type code.
    pub processor_type: u32,
    /// Gestalt machine type code.
    pub gestalt_machine_type: u32,
    /// Floating point unit type code.
    pub fpu_type: u32,
    /// Memory management unit type code.
    pub mmu_type: u32,
    /// Total guest physical RAM size in bytes.
    pub physical_ram_size: u32,
    /// Addressing mode attribute bitmask.
    pub addressing_mode_attributes: u32,
    /// Low-level hardware attribute bitmask.
    pub hardware_attributes: u32,
    /// Thread Manager attribute bitmask.
    pub thread_manager_attributes: u32,
    /// Whether PowerPC QuickTimeLib is registered.
    pub has_powerpc_quicktime: bool,
    /// Base address of the writable OS trap dispatch table.
    pub os_trap_table_base: u32,
    /// Base address of the writable Toolbox trap dispatch table.
    pub toolbox_trap_table_base: u32,
}

impl GestaltEvaluationContext {
    /// Constructs a canonical Gestalt evaluation context for the 68040 Macintosh profile.
    pub fn for_m68k(physical_ram_size: u32, mmu_mode_32: bool) -> Self {
        Self {
            architecture: GestaltArchitecture::M68k { mmu_mode_32 },
            system_version_bcd: u32::from(REFERENCE_MACHINE_PROFILE.system_version_bcd),
            system_architecture: REFERENCE_M68K_EXECUTION_CAPABILITIES.system_architecture,
            native_cpu_type: REFERENCE_M68K_EXECUTION_CAPABILITIES.native_cpu_type,
            processor_type: REFERENCE_M68K_EXECUTION_CAPABILITIES.processor_type,
            gestalt_machine_type: u32::from(REFERENCE_MACHINE_PROFILE.gestalt_machine_type),
            fpu_type: REFERENCE_M68K_EXECUTION_CAPABILITIES.fpu_type,
            mmu_type: REFERENCE_M68K_EXECUTION_CAPABILITIES.mmu_type,
            physical_ram_size,
            addressing_mode_attributes: 0b110 | u32::from(mmu_mode_32),
            // Quadra-class hardware attribute bitmask: VIA1(0), VIA2(1), ASC(3), SCC(4), SCSI(7)
            hardware_attributes: (1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 7),
            // Thread Manager present (bit 0)
            thread_manager_attributes: 1,
            has_powerpc_quicktime: false,
            os_trap_table_base: OS_TRAP_TABLE_BASE,
            toolbox_trap_table_base: TOOLBOX_TRAP_TABLE_BASE,
        }
    }

    /// Constructs a canonical Gestalt evaluation context for the Power Macintosh profile.
    pub fn for_powerpc(physical_ram_size: u32) -> Self {
        Self {
            architecture: GestaltArchitecture::PowerPc,
            system_version_bcd: u32::from(POWERPC_SYSTEM_VERSION_BCD),
            system_architecture: REFERENCE_POWERPC_EXECUTION_CAPABILITIES.system_architecture,
            native_cpu_type: REFERENCE_POWERPC_EXECUTION_CAPABILITIES.native_cpu_type,
            processor_type: REFERENCE_POWERPC_EXECUTION_CAPABILITIES.processor_type,
            gestalt_machine_type: u32::from(POWERPC_GESTALT_MACHINE_TYPE),
            fpu_type: REFERENCE_POWERPC_EXECUTION_CAPABILITIES.fpu_type,
            mmu_type: REFERENCE_POWERPC_EXECUTION_CAPABILITIES.mmu_type,
            physical_ram_size,
            // 32-bit clean addressing mode always active on PowerPC
            addressing_mode_attributes: 0b111,
            // Power Macintosh 9500 hardware attribute bitmask: VIA1(0), VIA2(1), SCC(4), 53C96 SCSI(19, 21, 22)
            hardware_attributes: (1 << 0) | (1 << 1) | (1 << 4) | (1 << 19) | (1 << 21) | (1 << 22),
            // Thread Manager present (bit 0) and ThreadsLib available (bit 2)
            thread_manager_attributes: 0b101,
            has_powerpc_quicktime: true,
            os_trap_table_base: OS_TRAP_TABLE_BASE,
            toolbox_trap_table_base: TOOLBOX_TRAP_TABLE_BASE,
        }
    }
}

/// Evaluated response value and error code for a built-in Gestalt selector query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestaltQueryResult {
    /// Evaluated 32-bit response value.
    pub response: u32,
    /// Result error code (`noErr` 0 or `gestaltUndefSelectorErr` -5551).
    pub error_code: i16,
}

impl GestaltQueryResult {
    /// Constructs a successful Gestalt response with `noErr` (0).
    pub const fn ok(response: u32) -> Self {
        Self {
            response,
            error_code: NO_ERR,
        }
    }

    /// Constructs an error Gestalt response (typically `gestaltUndefSelectorErr`).
    pub const fn err(error_code: i16) -> Self {
        Self {
            response: 0,
            error_code,
        }
    }
}

/// Evaluates a built-in Gestalt selector against the canonical evaluation context.
/// Returns `Some(GestaltQueryResult)` if the selector is recognized as a built-in,
/// or `None` if it is not recognized as built-in.
pub fn evaluate_gestalt_selector(
    selector: u32,
    context: &GestaltEvaluationContext,
) -> Option<GestaltQueryResult> {
    let sel = selector.to_be_bytes();
    match &sel {
        // gestaltVersion ('vers') -> Gestalt Manager version 1.
        b"vers" => Some(GestaltQueryResult::ok(0x0001)),
        // gestaltSystemVersion ('sysv') -> canonical profile version in BCD.
        b"sysv" => Some(GestaltQueryResult::ok(context.system_version_bcd)),
        // gestaltSysArchitecture ('sysa') -> native system architecture.
        b"sysa" => context.system_architecture.map(GestaltQueryResult::ok),
        // gestaltOSTable ('ostt') -> writable OS trap table base.
        b"ostt" => Some(GestaltQueryResult::ok(context.os_trap_table_base)),
        // gestaltToolboxTable ('tbtt') -> writable Toolbox trap table base.
        b"tbtt" => Some(GestaltQueryResult::ok(context.toolbox_trap_table_base)),
        // gestaltAppleEventsAttr ('evnt') -> AppleEvents present (bit 0).
        b"evnt" => Some(GestaltQueryResult::ok(0x0001)),
        // gestaltEditionMgrAttr ('edtn') -> Edition Manager present (bit 0).
        b"edtn" => Some(GestaltQueryResult::ok(1)),
        // gestaltNativeCPUtype ('cput') -> processor model code.
        b"cput" => Some(GestaltQueryResult::ok(context.native_cpu_type)),
        // gestaltProcessorType ('proc') -> processor type code.
        b"proc" => Some(GestaltQueryResult::ok(context.processor_type)),
        // gestaltMachineType ('mach') -> machine model code.
        b"mach" => Some(GestaltQueryResult::ok(context.gestalt_machine_type)),
        // gestaltKeyboardType ('kbd ') -> Extended ADB Keyboard (4).
        b"kbd " => Some(GestaltQueryResult::ok(u32::from(KEYBOARD_ENVIRON_TYPE))),
        // gestaltQuickdrawVersion ('qd  ') -> System 7 32-bit Color QuickDraw v1.3 ($0230).
        b"qd  " => Some(GestaltQueryResult::ok(0x0230)),
        // gestaltQuickdrawFeatures ('qdrw') -> hasColor | hasDeepGWorlds ($000F).
        b"qdrw" => Some(GestaltQueryResult::ok(0x000F)),
        // gestaltPhysicalRAMSize ('ram ') -> physical RAM size in bytes.
        b"ram " => Some(GestaltQueryResult::ok(context.physical_ram_size)),
        // gestaltLogicalRAMSize ('lram') -> logical RAM size in bytes (equals physical RAM without VM).
        b"lram" => Some(GestaltQueryResult::ok(context.physical_ram_size)),
        // gestaltFPUType ('fpu ') -> floating point coprocessor type code.
        b"fpu " => Some(GestaltQueryResult::ok(context.fpu_type)),
        // gestaltMMUType ('mmu ') -> memory management unit type code.
        b"mmu " => Some(GestaltQueryResult::ok(context.mmu_type)),
        // gestaltSoundAttr ('snd ') -> 16-bit stereo multi-channel sound profile ($1CFB).
        b"snd " => Some(GestaltQueryResult::ok(0x1CFB)),
        // gestaltTimeMgrVersion ('tmgr') -> revised Timer Manager (2).
        b"tmgr" => Some(GestaltQueryResult::ok(2)),
        // gestaltThreadMgrAttr ('thds') -> Thread Manager capabilities.
        b"thds" => Some(GestaltQueryResult::ok(context.thread_manager_attributes)),
        // gestaltDisplayMgrVers ('dplv') -> Display Manager 2.0.6 ($00020006).
        b"dplv" => Some(GestaltQueryResult::ok(0x0002_0006)),
        // gestaltDisplayMgrAttr ('dply') -> Display Manager attributes ($00000007).
        b"dply" => Some(GestaltQueryResult::ok(0x0000_0007)),
        // gestaltAliasMgrAttr ('alis') -> Alias Manager present (1).
        b"alis" => Some(GestaltQueryResult::ok(1)),
        // gestaltFSAttr ('fs  ') -> FSSpec calls and extended dispatch ((1 << 0) | (1 << 1)).
        b"fs  " => Some(GestaltQueryResult::ok((1 << 0) | (1 << 1))),
        // gestaltFindFolderAttr ('fold') -> FindFolder present (1).
        b"fold" => Some(GestaltQueryResult::ok(1)),
        // gestaltQuickTime ('qtim') -> QuickTime release version.
        b"qtim" => Some(GestaltQueryResult::ok(QUICKTIME_NUM_VERSION)),
        // Apple TN1083: gestaltQuickTimeFeatures ('qtrs') bit 0 indicates registered PowerPC QuickTimeLib.
        b"qtrs" => Some(GestaltQueryResult::ok(if context.has_powerpc_quicktime {
            1
        } else {
            0
        })),
        // gestaltDragMgrAttr ('drag') -> Drag Manager absent (0).
        b"drag" => Some(GestaltQueryResult::ok(0)),
        // gestaltOSAttr ('os  ') -> System 7 OS attributes ($00FF).
        b"os  " => Some(GestaltQueryResult::ok(0x00FF)),
        // gestaltPowerMgrAttr ('powr') -> no portable Power Manager (0).
        b"powr" => Some(GestaltQueryResult::ok(0)),
        // gestaltAppearanceAttr ('appr') -> Appearance Manager present (1).
        b"appr" => Some(GestaltQueryResult::ok(1)),
        // gestaltAppearanceVersion ('apvr') -> Appearance Manager version in BCD ($0101).
        b"apvr" => Some(GestaltQueryResult::ok(u32::from(
            APPEARANCE_MANAGER_VERSION_BCD,
        ))),
        // gestaltAddressingModeAttr ('addr') -> addressing mode capabilities.
        b"addr" => Some(GestaltQueryResult::ok(context.addressing_mode_attributes)),
        // gestaltHardwareAttr ('hdwr') -> low-level hardware attributes.
        b"hdwr" => Some(GestaltQueryResult::ok(context.hardware_attributes)),
        // gestaltSoundDeviceAttr ('sdev') -> no sound device selected (0).
        b"sdev" => Some(GestaltQueryResult::ok(0)),
        // gestaltStandardFileAttr ('stdf') -> Standard File Mgr >= 5.8 present (1).
        b"stdf" => Some(GestaltQueryResult::ok(1)),
        // gestaltHelpMgrAttr ('help') -> Help Manager present (1).
        b"help" => Some(GestaltQueryResult::ok(1)),
        // gestaltVMAttr ('vm  ') -> Virtual Memory absent (0).
        b"vm  " => Some(GestaltQueryResult::ok(0)),
        // gestaltAUXVersion ('a/ux') -> not running under A/UX (gestaltUndefSelectorErr, response 0).
        b"a/ux" => Some(GestaltQueryResult::err(GESTALT_UNDEF_SELECTOR_ERR)),

        // Architecture-specific selectors:
        _ => match context.architecture {
            GestaltArchitecture::M68k { .. } => match &sel {
                // gestaltCollectionMgrVersion ('cltn') -> Collection Manager 1.0 ($01000000).
                b"cltn" => Some(GestaltQueryResult::ok(0x0100_0000)),
                // gestaltPPCToolboxAttr ('ppc ') -> PPC Toolbox present (1).
                b"ppc " => Some(GestaltQueryResult::ok(0x0001)),
                // gestaltLogicalPageSize ('pgsz') -> logical page size (4096).
                b"pgsz" => Some(GestaltQueryResult::ok(
                    GESTALT_68040_LOGICAL_PAGE_SIZE_BYTES,
                )),
                // gestaltScreenSaverAttr ('SAVR') -> After Dark absent (gestaltUndefSelectorErr, response 0).
                b"SAVR" => Some(GestaltQueryResult::err(GESTALT_UNDEF_SELECTOR_ERR)),
                // gestaltSpeechAttr ('ttsc') -> Speech Manager absent (0).
                b"ttsc" => Some(GestaltQueryResult::ok(0)),
                // gestaltTextEditVersion ('te  ') -> TextEdit 5.
                b"te  " => Some(GestaltQueryResult::ok(5)),
                // gestaltTEAttr ('teat') -> TextEdit attributes (0).
                b"teat" => Some(GestaltQueryResult::ok(0)),
                // gestaltResourceMgrAttr ('rsrc') -> partial resource routines present (1).
                b"rsrc" => Some(GestaltQueryResult::ok(1)),
                // gestaltScriptCount ('scr#') -> Roman script system count (1).
                b"scr#" => Some(GestaltQueryResult::ok(1)),
                _ => None,
            },
            GestaltArchitecture::PowerPc => match &sel {
                // Script Manager version in low word ($0700).
                b"scri" => Some(GestaltQueryResult::ok(0x0700)),
                // Text Services Manager 1.5 ($0150).
                b"tsmv" => Some(GestaltQueryResult::ok(0x0150)),
                // Carbon version in BCD ($0130).
                b"cbon" => Some(GestaltQueryResult::ok(u32::from(
                    POWERPC_CARBON_VERSION_BCD,
                ))),
                // gestaltNativeCPUfamily ('cpuf') reports the processor family.
                b"cpuf" => Some(GestaltQueryResult::ok(context.native_cpu_type)),
                // gestaltProcClkSpeed ('pclk') reports the processor clock rate in hertz.
                b"pclk" => Some(GestaltQueryResult::ok(REFERENCE_POWERPC_CPU_CLOCK_HZ)),
                // gestaltPowerPCProcessorFeatures ('ppcf') reports optional CPU features (0).
                b"ppcf" => Some(GestaltQueryResult::ok(0)),
                // Code Fragment Manager present (1).
                b"cfrg" => Some(GestaltQueryResult::ok(1)),
                // Mixed Mode Manager present (1).
                b"mixd" => Some(GestaltQueryResult::ok(1)),
                // QuickDraw 3D present (1).
                b"qd3d" => Some(GestaltQueryResult::ok(1)),
                // QuickDraw 3D release version ($01608000).
                b"q3v " => Some(GestaltQueryResult::ok(GESTALT_QD3D_VERSION)),
                _ => None,
            },
        },
    }
}

/// Returns whether a given selector is a recognized built-in selector that yields `noErr`.
/// Used by `NewGestalt` and `ReplaceGestalt` to distinguish between registered and unregistered selectors.
pub fn is_builtin_gestalt_selector(selector: u32, context: &GestaltEvaluationContext) -> bool {
    matches!(
        evaluate_gestalt_selector(selector, context),
        Some(GestaltQueryResult {
            error_code: NO_ERR,
            ..
        })
    )
}

/// Parameters for a Gestalt query with a memory destination pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestaltQueryParameters {
    /// 4-character selector code.
    pub selector: u32,
    /// Destination pointer for the 32-bit response value.
    pub response_ptr: u32,
}

/// Validates Gestalt query parameters, rejecting null pointers with `paramErr` (-50).
pub fn evaluate_gestalt_query_parameters(
    selector: u32,
    response_ptr: u32,
) -> Result<GestaltQueryParameters, i16> {
    if response_ptr == 0 {
        return Err(PARAM_ERR);
    }
    Ok(GestaltQueryParameters {
        selector,
        response_ptr,
    })
}

/// Full evaluation outcome for a Gestalt query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestaltQueryEvaluation {
    /// 4-character selector code queried.
    pub selector: u32,
    /// Evaluated 32-bit response value (or 0 on error).
    pub response: u32,
    /// Result error code (`noErr` 0 or `gestaltUndefSelectorErr` -5551).
    pub error_code: i16,
}

impl GestaltQueryEvaluation {
    /// Returns the result error code as an unsigned 32-bit trap status code.
    pub fn error_code_u32(&self) -> u32 {
        (self.error_code as i32) as u32
    }
}

/// Evaluates a Gestalt query across built-in selectors and dynamic registered selectors.
pub fn evaluate_gestalt_query(
    selector: u32,
    context: &GestaltEvaluationContext,
    dynamic_value: Option<u32>,
) -> GestaltQueryEvaluation {
    if let Some(builtin) = evaluate_gestalt_selector(selector, context) {
        GestaltQueryEvaluation {
            selector,
            response: builtin.response,
            error_code: builtin.error_code,
        }
    } else if let Some(value) = dynamic_value {
        GestaltQueryEvaluation {
            selector,
            response: value,
            error_code: NO_ERR,
        }
    } else {
        GestaltQueryEvaluation {
            selector,
            response: 0,
            error_code: GESTALT_UNDEF_SELECTOR_ERR,
        }
    }
}

/// Action outcome for registering a selector via `NewGestalt` or `NewGestaltValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewGestaltAction {
    /// The selector is already known (built-in or dynamically registered).
    DuplicateSelector,
    /// The selector is new and should be registered.
    Register {
        /// 4-character selector code.
        selector: u32,
        /// Selector function pointer or 32-bit value.
        value: u32,
    },
}

impl NewGestaltAction {
    /// Returns the corresponding Mac OS result code.
    pub fn error_code(&self) -> i16 {
        match self {
            Self::DuplicateSelector => GESTALT_DUP_SELECTOR_ERR,
            Self::Register { .. } => NO_ERR,
        }
    }
}

/// Evaluates a `NewGestalt` or `NewGestaltValue` request.
pub fn evaluate_new_gestalt(selector: u32, value: u32, is_already_known: bool) -> NewGestaltAction {
    if is_already_known {
        NewGestaltAction::DuplicateSelector
    } else {
        NewGestaltAction::Register { selector, value }
    }
}

/// Action outcome for replacing a selector handler via `ReplaceGestalt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceGestaltAction {
    /// Replaces an existing custom registered handler, returning the prior handler.
    ReplaceCustom {
        /// Prior registered handler address.
        old_handler: u32,
        /// New replacement handler address.
        new_handler: u32,
    },
    /// Replaces a built-in selector; prior handler address is reported as 0.
    ReplaceBuiltin {
        /// New replacement handler address.
        new_handler: u32,
    },
    /// Selector is unknown; operation fails with `gestaltUndefSelectorErr`.
    UndefinedSelector,
}

impl ReplaceGestaltAction {
    /// Returns the corresponding Mac OS result code.
    pub fn error_code(&self) -> i16 {
        match self {
            Self::ReplaceCustom { .. } | Self::ReplaceBuiltin { .. } => NO_ERR,
            Self::UndefinedSelector => GESTALT_UNDEF_SELECTOR_ERR,
        }
    }

    /// Returns the old handler value to write to A0, or preserves `default_value` on error.
    pub fn old_handler_or_default(&self, default_value: u32) -> u32 {
        match self {
            Self::ReplaceCustom { old_handler, .. } => *old_handler,
            Self::ReplaceBuiltin { .. } => 0,
            Self::UndefinedSelector => default_value,
        }
    }
}

/// Evaluates a `ReplaceGestalt` request.
pub fn evaluate_replace_gestalt(
    _selector: u32,
    new_handler: u32,
    existing_custom_handler: Option<u32>,
    is_builtin: bool,
) -> ReplaceGestaltAction {
    if let Some(old_handler) = existing_custom_handler {
        ReplaceGestaltAction::ReplaceCustom {
            old_handler,
            new_handler,
        }
    } else if is_builtin {
        ReplaceGestaltAction::ReplaceBuiltin { new_handler }
    } else {
        ReplaceGestaltAction::UndefinedSelector
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestalt_evaluation_context_profiles() {
        let m68k_ctx = GestaltEvaluationContext::for_m68k(8 * 1024 * 1024, true);
        assert_eq!(m68k_ctx.system_architecture, Some(GESTALT_SYS_ARCH_68K));
        assert_eq!(m68k_ctx.physical_ram_size, 8 * 1024 * 1024);
        assert_eq!(m68k_ctx.addressing_mode_attributes, 0b111);
        assert!(!m68k_ctx.has_powerpc_quicktime);

        let m68k_24bit_ctx = GestaltEvaluationContext::for_m68k(4 * 1024 * 1024, false);
        assert_eq!(m68k_24bit_ctx.addressing_mode_attributes, 0b110);

        let ppc_ctx = GestaltEvaluationContext::for_powerpc(32 * 1024 * 1024);
        assert_eq!(ppc_ctx.system_architecture, Some(GESTALT_SYS_ARCH_POWERPC));
        assert_eq!(ppc_ctx.physical_ram_size, 32 * 1024 * 1024);
        assert_eq!(ppc_ctx.addressing_mode_attributes, 0b111);
        assert!(ppc_ctx.has_powerpc_quicktime);
    }

    #[test]
    fn gestalt_selector_universal_and_architecture_evaluation() {
        let m68k_ctx = GestaltEvaluationContext::for_m68k(16 * 1024 * 1024, true);
        let ppc_ctx = GestaltEvaluationContext::for_powerpc(16 * 1024 * 1024);

        // Universal selectors on both architectures:
        for selector in [
            *b"vers", *b"sysv", *b"cput", *b"mach", *b"kbd ", *b"qd  ", *b"qdrw", *b"ram ",
            *b"lram", *b"snd ", *b"tmgr", *b"alis", *b"fs  ", *b"fold", *b"qtim", *b"os  ",
            *b"appr", *b"apvr", *b"addr", *b"hdwr", *b"stdf", *b"help", *b"vm  ",
        ] {
            let sel_u32 = u32::from_be_bytes(selector);
            let m68k_res = evaluate_gestalt_selector(sel_u32, &m68k_ctx);
            let ppc_res = evaluate_gestalt_selector(sel_u32, &ppc_ctx);
            assert!(
                m68k_res.is_some(),
                "selector {:?} missing on 68k",
                std::str::from_utf8(&selector)
            );
            assert!(
                ppc_res.is_some(),
                "selector {:?} missing on PPC",
                std::str::from_utf8(&selector)
            );
            assert_eq!(m68k_res.unwrap().error_code, NO_ERR);
            assert_eq!(ppc_res.unwrap().error_code, NO_ERR);
        }

        // 68k-specific selectors:
        for selector in [
            *b"cltn", *b"ppc ", *b"pgsz", *b"ttsc", *b"te  ", *b"rsrc", *b"scr#",
        ] {
            let sel_u32 = u32::from_be_bytes(selector);
            let res = evaluate_gestalt_selector(sel_u32, &m68k_ctx);
            assert!(
                res.is_some(),
                "68k selector {:?} not found",
                std::str::from_utf8(&selector)
            );
            assert_eq!(res.unwrap().error_code, NO_ERR);
        }

        // PPC-specific selectors:
        for selector in [
            *b"scri", *b"tsmv", *b"cbon", *b"cpuf", *b"pclk", *b"cfrg", *b"mixd", *b"qd3d",
            *b"q3v ",
        ] {
            let sel_u32 = u32::from_be_bytes(selector);
            let res = evaluate_gestalt_selector(sel_u32, &ppc_ctx);
            assert!(
                res.is_some(),
                "PPC selector {:?} not found",
                std::str::from_utf8(&selector)
            );
            assert_eq!(res.unwrap().error_code, NO_ERR);
        }

        // Selectors returning gestaltUndefSelectorErr:
        let aux_res = evaluate_gestalt_selector(u32::from_be_bytes(*b"a/ux"), &m68k_ctx).unwrap();
        assert_eq!(aux_res.error_code, GESTALT_UNDEF_SELECTOR_ERR);
        assert_eq!(aux_res.response, 0);

        let savr_res = evaluate_gestalt_selector(u32::from_be_bytes(*b"SAVR"), &m68k_ctx).unwrap();
        assert_eq!(savr_res.error_code, GESTALT_UNDEF_SELECTOR_ERR);
        assert_eq!(savr_res.response, 0);

        // Unknown selector:
        assert!(evaluate_gestalt_selector(u32::from_be_bytes(*b"????"), &m68k_ctx).is_none());
    }

    #[test]
    fn builtin_gestalt_selector_filtering() {
        let m68k_ctx = GestaltEvaluationContext::for_m68k(4 * 1024 * 1024, true);

        assert!(is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"vers"),
            &m68k_ctx
        ));
        assert!(is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"sysv"),
            &m68k_ctx
        ));
        assert!(is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"mach"),
            &m68k_ctx
        ));
        assert!(is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"cltn"),
            &m68k_ctx
        ));

        // a/ux and SAVR are not built-in known with noErr:
        assert!(!is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"a/ux"),
            &m68k_ctx
        ));
        assert!(!is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"SAVR"),
            &m68k_ctx
        ));
        assert!(!is_builtin_gestalt_selector(
            u32::from_be_bytes(*b"zzzz"),
            &m68k_ctx
        ));
    }

    #[test]
    fn gestalt_query_parameters_validation() {
        assert_eq!(
            evaluate_gestalt_query_parameters(0x12345678, 0),
            Err(PARAM_ERR)
        );
        let params = evaluate_gestalt_query_parameters(0x12345678, 0x1000).unwrap();
        assert_eq!(params.selector, 0x12345678);
        assert_eq!(params.response_ptr, 0x1000);
    }

    #[test]
    fn gestalt_mutation_actions_evaluation() {
        assert_eq!(
            evaluate_new_gestalt(0x11112222, 0x33334444, true),
            NewGestaltAction::DuplicateSelector
        );
        assert_eq!(
            evaluate_new_gestalt(0x11112222, 0x33334444, false),
            NewGestaltAction::Register {
                selector: 0x11112222,
                value: 0x33334444,
            }
        );

        assert_eq!(
            evaluate_replace_gestalt(0x11112222, 0x55556666, Some(0x77778888), false),
            ReplaceGestaltAction::ReplaceCustom {
                old_handler: 0x77778888,
                new_handler: 0x55556666,
            }
        );
        assert_eq!(
            evaluate_replace_gestalt(0x11112222, 0x55556666, None, true),
            ReplaceGestaltAction::ReplaceBuiltin {
                new_handler: 0x55556666,
            }
        );
        assert_eq!(
            evaluate_replace_gestalt(0x11112222, 0x55556666, None, false),
            ReplaceGestaltAction::UndefinedSelector
        );
    }
}
