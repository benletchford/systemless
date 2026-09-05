//! Architecture-neutral Trap Manager table authority.
//!
//! The two CPU adapters reach this service with the same guest-visible raw
//! table bytes. The service deliberately does not cache decoded handler
//! addresses: a native guest store may replace a table cell or any link in a
//! protected come-from chain between calls. Inside Macintosh: Operating
//! System Utilities (1994), pp. 8-4--8-9 and 8-22--8-31.

use std::collections::HashSet;

/// The observable target behind a raw trap-table long.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapTableTarget {
    Direct(u32),
    Protected {
        last_head: u32,
        logical_successor: u32,
    },
}

/// Follow every protected come-from head and return the last mutable edge.
/// Trap Manager getters hide all permanent heads, while setters replace the
/// exit JMP target in the last head. Inside Macintosh: Operating System
/// Utilities (1994), pp. 8-8--8-9 and 8-27--8-31.
#[cfg(test)]
pub(crate) fn resolve_trap_table_target(
    raw_target: u32,
    read_long: impl FnMut(u32) -> Option<u32>,
) -> Option<TrapTableTarget> {
    // Compatibility path for the PPC loader's pre-existing closure API. The
    // active 68K dispatcher uses the provenance-aware variant below; callers
    // that can identify system-owned code should prefer that variant so a
    // writable guest buffer containing the signature is not treated as a
    // permanent head.
    resolve_trap_table_target_with_provenance(raw_target, read_long, |_| true)
}

/// Resolve a raw table target while requiring explicit provenance for every
/// permanent come-from head. Matching the signature in ordinary writable guest
/// RAM is not sufficient to authorize the privileged chain path.
pub(crate) fn resolve_trap_table_target_with_provenance(
    raw_target: u32,
    mut read_long: impl FnMut(u32) -> Option<u32>,
    mut is_protected_head: impl FnMut(u32) -> bool,
) -> Option<TrapTableTarget> {
    let mut target = raw_target;
    let mut last_head = None;
    let mut visited = HashSet::new();
    loop {
        // Provenance is necessary but not sufficient: protected gateways and
        // other generated code share the same ownership ranges without being
        // come-from heads. Stop at the first address that is either ordinary
        // guest memory or does not carry the head signature. Once at least
        // one signed head was followed, that address is its logical
        // successor. The compatibility resolver supplies universal
        // provenance and therefore retains the historical signature-only
        // behavior.
        if !is_protected_head(target) || read_long(target) != Some(COME_FROM_PATCH_SIGNATURE) {
            return Some(match last_head {
                Some(last_head) => TrapTableTarget::Protected {
                    last_head,
                    logical_successor: target,
                },
                None => TrapTableTarget::Direct(target),
            });
        }
        if !visited.insert(target) {
            return None;
        }
        last_head = Some(target);
        target = read_long(target.checked_add(4)?)?;
    }
}

/// Mac OS 8.1 trap-table layout selected by the reference machine profile.
/// The Operating System table contains 256 routine addresses and the Toolbox
/// table contains 1,024. Inside Macintosh: Operating System Utilities (1994),
/// pp. 8-4--8-6 describes the table shapes; Inside Macintosh Volume VI (1991),
/// Gestalt Manager constants `gestaltOSTable` and `gestaltToolboxTable`, expose
/// their bases to applications.
pub(crate) const OS_TRAP_TABLE_BASE: u32 = 0x0400;
pub(crate) const TOOLBOX_TRAP_TABLE_BASE: u32 = 0x0E00;
pub(crate) const OS_TRAP_TABLE_SLOTS: u16 = 0x0100;
pub(crate) const TOOLBOX_TRAP_TABLE_SLOTS: u16 = 0x0400;
pub(crate) const COME_FROM_PATCH_SIGNATURE: u32 = 0x6006_4EF9;

/// Source-backed meanings for OS-trap routine bits 10 and 9.
///
/// These bits are private to each OS routine; they must not be interpreted as
/// global dispatcher flags. The Memory Manager meanings below come from
/// *Inside Macintosh: Memory* (1992), pp. 2-31, 2-33, 2-35, 2-53--2-55,
/// 2-65--2-68, and 2-71--2-74. The text transformations come from *Inside
/// Macintosh*, Volume VI (1991), pp. 14-62--14-63 and Appendix C, table C-2.
/// The string-comparison permutations come from *Inside Macintosh: Text*
/// (1993), pp. 5-51--5-52 and 5-60--5-61. The later table is authoritative
/// over the contradictory MARKS annotation in Volume II (1985), p. II-377.
/// UpperString's MARKS form comes from the same book, pp. 5-64--5-65.
/// Parameter-block synchronous, immediate, and asynchronous forms come from
/// *Inside Macintosh: Devices* (1994), p. 1-16.
/// Original and extended Time Manager task installation comes from *Inside
/// Macintosh: Processes* (1994), pp. 3-18--3-20.
/// Trap Manager legacy, new-OS, and new-Tool forms come from *Inside Macintosh:
/// Operating System Utilities* (1994), pp. 8-27--8-31 and 8-32--8-33.
/// Gestalt, NewGestalt, and ReplaceGestalt come from the same book,
/// pp. 1-31--1-36.
/// SleepQInstall and SleepQRemove come from *Inside Macintosh: Devices*
/// (1994), pp. 6-18, 6-26, and 6-33.
/// IdleUpdate, IdleState, and SerialPower come from the same book,
/// pp. 6-29--6-30 and 6-33--6-35.
/// File Manager synchronous, asynchronous, and HFS forms come from *Inside
/// Macintosh: Files* (1992), pp. 2-6, 2-238--2-239, and its assembly-language
/// summary. Universal Interfaces 3.4 independently declares reviewed exact
/// words in `MacMemory.h` (lines 436--1010, 1331--1362), `TextUtils.h` (lines
/// 404--455), `Devices.h` (lines 905--1044, 1282--1415), and `Files.h` (lines
/// 1315--3343); `Timer.h` lines 74--100 declares InsTime and InsXTime;
/// `Patches.h` lines 80--231 declares the Trap Manager forms;
/// `Gestalt.h` lines 55--105 declares the three Gestalt Manager forms;
/// `Power.h` lines 447--461 and 705--731 declares the sleep-queue record and
/// register entry points; lines 650--701 and 733--791 declares the idle-state
/// and serial-power entry points;
/// `StringCompare.h` lines 567--596 retains the comparison APIs;
/// `Devices.h` lines 1109--1141 declares DriverInstall and its bit-10
/// DriverInstallReserveMem form; `MacMemory.h` lines 1184--1202 declares
/// ReallocateHandle and ReallocateHandleSys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OsRoutineVariant {
    Unclassified,
    CurrentHeap,
    SystemHeap,
    CurrentHeapClear,
    SystemHeapClear,
    LowerText,
    StripText,
    UpperText,
    StripUpperText,
    TextCompareFoldCaseAndMarks,
    TextCompareFoldCase,
    TextCompareStripMarks,
    TextCompareExact,
    UpperStringPreserveMarks,
    UpperStringStripMarks,
    ParameterBlockSynchronous,
    ParameterBlockImmediate,
    ParameterBlockAsynchronous,
    TimeTaskOriginal,
    TimeTaskExtended,
    TrapAddressLegacy,
    TrapAddressNewOs,
    TrapAddressNewTool,
    GestaltQuery,
    GestaltRegister,
    GestaltReplace,
    SleepQueueInstall,
    SleepQueueRemove,
    PowerIdleUpdate,
    PowerIdleState,
    PowerSerial,
    DriverInstall,
    DriverInstallReserveMemory,
    FileSynchronous,
    FileAsynchronous,
    FileHfsSynchronous,
    FileHfsAsynchronous,
}

impl OsRoutineVariant {
    /// Returns `(case_sensitive, diacritic_sensitive)` for CmpString/RelString.
    pub(crate) const fn text_comparison_sensitivity(self) -> Option<(bool, bool)> {
        match self {
            Self::TextCompareFoldCaseAndMarks => Some((false, false)),
            Self::TextCompareFoldCase => Some((false, true)),
            Self::TextCompareStripMarks => Some((true, false)),
            Self::TextCompareExact => Some((true, true)),
            _ => None,
        }
    }
}

const fn classify_os_routine_variant(raw_word: u16) -> OsRoutineVariant {
    if (raw_word & 0x0800) != 0 {
        return OsRoutineVariant::Unclassified;
    }

    let slot = raw_word & 0x00FF;
    let routine_bits = raw_word & 0x0600;
    match (slot, routine_bits) {
        // NewPtr and NewHandle use bit 10 for SYS and bit 9 for CLEAR.
        (0x1E | 0x22, 0x0000) => OsRoutineVariant::CurrentHeap,
        (0x1E | 0x22, 0x0200) => OsRoutineVariant::CurrentHeapClear,
        (0x1E | 0x22, 0x0400) => OsRoutineVariant::SystemHeap,
        (0x1E | 0x22, 0x0600) => OsRoutineVariant::SystemHeapClear,

        // These Memory Manager routines document only bit 10 as SYS. Leave
        // their bit-9 forms unclassified rather than inventing a meaning.
        (0x1C | 0x1D | 0x27 | 0x28 | 0x40 | 0x4C | 0x4D | 0x61 | 0x62 | 0x66, 0x0000) => {
            OsRoutineVariant::CurrentHeap
        }
        (0x1C | 0x1D | 0x27 | 0x28 | 0x40 | 0x4C | 0x4D | 0x61 | 0x62 | 0x66, 0x0400) => {
            OsRoutineVariant::SystemHeap
        }

        // LowerText, StripText, UpperText, and StripUpperText share slot $56.
        (0x56, 0x0000) => OsRoutineVariant::LowerText,
        (0x56, 0x0200) => OsRoutineVariant::StripText,
        (0x56, 0x0400) => OsRoutineVariant::UpperText,
        (0x56, 0x0600) => OsRoutineVariant::StripUpperText,

        // Text 1993, pp. 5-51--5-52 and 5-60--5-61: MARKS (bit 9)
        // makes comparison diacritic-sensitive and CASE (bit 10) makes it
        // case-sensitive. Both CmpString and RelString use this table.
        (0x3C | 0x50, 0x0000) => OsRoutineVariant::TextCompareFoldCaseAndMarks,
        (0x3C | 0x50, 0x0200) => OsRoutineVariant::TextCompareFoldCase,
        (0x3C | 0x50, 0x0400) => OsRoutineVariant::TextCompareStripMarks,
        (0x3C | 0x50, 0x0600) => OsRoutineVariant::TextCompareExact,

        // Text 1993, pp. 5-64--5-65: bare UprString preserves marks while
        // MARKS (bit 9) strips them. Bit 10 has no documented meaning here.
        (0x54, 0x0000) => OsRoutineVariant::UpperStringPreserveMarks,
        (0x54, 0x0200) => OsRoutineVariant::UpperStringStripMarks,

        // Devices 1994, p. 1-16 and UI 3.4 Devices.h lines 905--1044 and
        // 1282--1415: these PB slots have exact Sync/Immed/Async declarations.
        // Slot $00 is excluded because $A200 also means PBHOpenSync/OpenSlotSync.
        (0x01..=0x06, 0x0000) => OsRoutineVariant::ParameterBlockSynchronous,
        (0x01..=0x06, 0x0200) => OsRoutineVariant::ParameterBlockImmediate,
        (0x01..=0x06, 0x0400) => OsRoutineVariant::ParameterBlockAsynchronous,

        // Processes 1994, pp. 3-18--3-20 and UI 3.4 Timer.h lines 74--100:
        // bit 10 selects InsXTime's extended, drift-free TMTask record.
        // Bit 9 has no documented meaning for this slot.
        (0x58, 0x0000) => OsRoutineVariant::TimeTaskOriginal,
        (0x58, 0x0400) => OsRoutineVariant::TimeTaskExtended,

        // Operating System Utilities 1994, pp. 8-27--8-31: bit 9 selects
        // the new typed form and bit 10 then selects the Toolbox table.
        // UI 3.4 Patches.h lines 80--231 declares the exact legacy, new-OS,
        // and new-Tool getter/setter words. Bit 10 alone is not declared.
        (0x46 | 0x47, 0x0000) => OsRoutineVariant::TrapAddressLegacy,
        (0x46 | 0x47, 0x0200) => OsRoutineVariant::TrapAddressNewOs,
        (0x46 | 0x47, 0x0600) => OsRoutineVariant::TrapAddressNewTool,

        // Operating System Utilities 1994, pp. 1-31--1-36 and UI 3.4
        // Gestalt.h lines 55--105: the three operations share slot $AD.
        // Bit 9 selects NewGestalt and bit 10 selects ReplaceGestalt; the
        // combined form remains unclassified. UI 3.4 Traps.h line 804 names
        // $A7AD as _GetGestaltProcPtr, but the reviewed sources do not define
        // its ABI or semantics.
        (0xAD, 0x0000) => OsRoutineVariant::GestaltQuery,
        (0xAD, 0x0200) => OsRoutineVariant::GestaltRegister,
        (0xAD, 0x0400) => OsRoutineVariant::GestaltReplace,

        // Devices 1994, pp. 6-18, 6-26, and 6-33; UI 3.4 Power.h lines
        // 447--461 and 705--731: bit 9 installs an A0-supplied SleepQRec and
        // bit 10 removes it. The combined form has no reviewed semantics.
        (0x8A, 0x0200) => OsRoutineVariant::SleepQueueInstall,
        (0x8A, 0x0400) => OsRoutineVariant::SleepQueueRemove,

        // Devices 1994, pp. 6-29--6-30 and 6-33--6-35; UI 3.4 Power.h
        // lines 650--701 and 733--791: bits 9 and 10 distinguish the three
        // Power Manager entry points sharing slot $85. The bare form has no
        // reviewed routine identity.
        (0x85, 0x0200) => OsRoutineVariant::PowerIdleUpdate,
        (0x85, 0x0400) => OsRoutineVariant::PowerIdleState,
        (0x85, 0x0600) => OsRoutineVariant::PowerSerial,

        // Devices 1994, pp. 1-83--1-85 and UI 3.4 Devices.h lines
        // 1109--1141: bit 10 selects DriverInstallReserveMem, which calls
        // ReserveMem before the shared DCE installation. Bit 9 and the
        // combined form have no reviewed meanings.
        (0x3D, 0x0000) => OsRoutineVariant::DriverInstall,
        (0x3D, 0x0400) => OsRoutineVariant::DriverInstallReserveMemory,

        // Files 1992 identifies bit 10 as ASYNC and bit 9 as newHFS. These
        // reviewed slots have exact basic Sync/Async declarations in UI 3.4
        // Files.h; do not extend the meanings to other OS-table slots.
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x11 | 0x12 | 0x13 | 0x14
            | 0x15 | 0x18 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45,
            0x0000,
        ) => OsRoutineVariant::FileSynchronous,
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x11 | 0x12 | 0x13 | 0x14
            | 0x15 | 0x18 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45,
            0x0400,
        ) => OsRoutineVariant::FileAsynchronous,

        // These slots additionally have exact PBH...Sync/PBH...Async words.
        // A200/PBHOpen is deliberately excluded because UI 3.4 also declares
        // that word as PBOpenImmed, so one label would overstate the evidence.
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x14 | 0x15 | 0x41 | 0x42,
            0x0200,
        ) => OsRoutineVariant::FileHfsSynchronous,
        (
            0x07 | 0x08 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x10 | 0x14 | 0x15 | 0x41 | 0x42,
            0x0600,
        ) => OsRoutineVariant::FileHfsAsynchronous,
        _ => OsRoutineVariant::Unclassified,
    }
}

/// One raw A-line word's table selection and preserved variant metadata.
///
/// The Trap Dispatcher must classify the complete word before masking it to
/// a table slot. Operating System words use bits 10--8 as routine/A0 flags
/// and bits 7--0 as the slot; Toolbox words use bit 10 as auto-pop and bits
/// 9--0 as the slot. Inside Macintosh: Operating System Utilities (1994),
/// pp. 8-10--8-15 and 8-20--8-21.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawTrapRoute {
    pub(crate) raw_word: u16,
    pub(crate) canonical_word: u16,
    pub(crate) table_slot: u16,
    pub(crate) table_index: u16,
    pub(crate) table_address: u32,
    pub(crate) is_toolbox: bool,
    pub(crate) os_flags: u16,
    pub(crate) os_routine_variant: OsRoutineVariant,
    pub(crate) os_returns_a0: bool,
    pub(crate) toolbox_auto_pop: bool,
}

const EMPTY_RAW_TRAP_ROUTE: RawTrapRoute = RawTrapRoute {
    raw_word: 0,
    canonical_word: 0,
    table_slot: 0,
    table_index: 0,
    table_address: 0,
    is_toolbox: false,
    os_flags: 0,
    os_routine_variant: OsRoutineVariant::Unclassified,
    os_returns_a0: false,
    toolbox_auto_pop: false,
};

const fn generate_raw_trap_routes() -> [RawTrapRoute; 4096] {
    let mut routes = [EMPTY_RAW_TRAP_ROUTE; 4096];
    let mut low_word = 0u16;
    while low_word < 4096 {
        let raw_word = 0xA000 | low_word;
        let is_toolbox = (raw_word & 0x0800) != 0;
        let table_slot = if is_toolbox {
            raw_word & 0x03FF
        } else {
            raw_word & 0x00FF
        };
        routes[low_word as usize] = RawTrapRoute {
            raw_word,
            canonical_word: if is_toolbox {
                0xA800 | table_slot
            } else {
                0xA000 | table_slot
            },
            table_slot,
            table_index: if is_toolbox {
                OS_TRAP_TABLE_SLOTS + table_slot
            } else {
                table_slot
            },
            table_address: if is_toolbox {
                TOOLBOX_TRAP_TABLE_BASE + table_slot as u32 * 4
            } else {
                OS_TRAP_TABLE_BASE + table_slot as u32 * 4
            },
            is_toolbox,
            os_flags: if is_toolbox { 0 } else { raw_word & 0x0700 },
            os_routine_variant: classify_os_routine_variant(raw_word),
            os_returns_a0: !is_toolbox && (raw_word & 0x0100) != 0,
            toolbox_auto_pop: is_toolbox && (raw_word & 0x0400) != 0,
        };
        low_word += 1;
    }
    routes
}

/// Complete generated routing denominator for `$A000..=$AFFF`.
const RAW_TRAP_ROUTES: [RawTrapRoute; 4096] = generate_raw_trap_routes();

pub(crate) fn raw_trap_route(trap_word: u16) -> &'static RawTrapRoute {
    &RAW_TRAP_ROUTES[usize::from(trap_word & 0x0FFF)]
}

/// Which Trap Manager table a getter or setter addresses.
///
/// Legacy forms infer the table from the trap number. The newer forms carry
/// the table choice explicitly, while both still mask the supplied trap to
/// the selected table's slot width. Inside Macintosh: Operating System
/// Utilities (1994), pp. 8-26--8-33.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapTableKind {
    Legacy,
    OperatingSystem,
    Toolbox,
}

/// Result of a Trap Manager setter that could not safely update guest bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapManagerSetError {
    /// The caller supplied a permanent come-from head as its patch body.
    InvalidComeFromHead,
    /// The raw table cell or a protected-chain link was not readable.
    UnreadableTable,
    /// The protected chain was cyclic or otherwise malformed.
    MalformedComeFromChain,
    /// The selected guest cell rejected the write.
    WriteRejected,
}

/// One memory operation requested by the Trap Manager service.
///
/// A single operation closure keeps reads and writes on the same guest
/// adapter without borrowing an ISA-specific bus through three independent
/// closures. The service itself only knows these raw longword operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapManagerMemoryOp {
    ReadLong(u32),
    WriteLong { address: u32, value: u32 },
    WriteProtectedLong { address: u32, value: u32 },
}

/// Result returned by a [`TrapManagerMemoryOp`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapManagerMemoryResult {
    Long(u32),
    Written,
}

/// The single architecture-neutral Trap Manager service.
///
/// Topology is immutable in the generated [`RawTrapRoute`] map and in the
/// profile materialized by the dispatcher. Process-specific patch state is
/// retained only in guest table bytes; pending invocation state remains dispatcher
/// execution state. Keeping this service stateless is intentional: every
/// operation re-reads the guest bytes so direct table writes are authoritative.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TrapManager;

impl TrapManager {
    /// Normalize a getter/setter argument to the canonical raw table word.
    pub(crate) fn canonical_trap_word(trap_word: u16, kind: TrapTableKind) -> u16 {
        let trap_num = trap_word & 0x03FF;
        let typed_word = match kind {
            TrapTableKind::OperatingSystem => 0xA000 | (trap_num & 0x00FF),
            TrapTableKind::Toolbox => 0xA800 | trap_num,
            TrapTableKind::Legacy => {
                if matches!(trap_num, 0x000..=0x04F | 0x054 | 0x057) {
                    0xA000 | (trap_num & 0x00FF)
                } else {
                    0xA800 | trap_num
                }
            }
        };
        raw_trap_route(typed_word).canonical_word
    }

    /// Return the raw guest table cell for a getter/setter argument.
    pub(crate) fn table_address(trap_word: u16, kind: TrapTableKind) -> u32 {
        raw_trap_route(Self::canonical_trap_word(trap_word, kind)).table_address
    }

    /// Read the logical handler behind a raw guest table cell.
    ///
    /// Every permanent come-from head is hidden from the caller. A direct
    /// cell returns its value; a protected cell returns the last link's
    /// successor. The supplied reader is called for the table cell and each
    /// chain longword, so both CPU adapters observe one byte authority.
    #[cfg(test)]
    pub(crate) fn get_address(
        trap_word: u16,
        kind: TrapTableKind,
        mut access: impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
    ) -> Option<u32> {
        let raw = Self::read_long(&mut access, Self::table_address(trap_word, kind))?;
        match resolve_trap_table_target(raw, |address| Self::read_long(&mut access, address))? {
            TrapTableTarget::Direct(target) => Some(target),
            TrapTableTarget::Protected {
                logical_successor, ..
            } => Some(logical_successor),
        }
    }

    /// Read a logical handler while requiring explicit system provenance for
    /// each protected come-from head.
    pub(crate) fn get_address_with_provenance(
        trap_word: u16,
        kind: TrapTableKind,
        mut access: impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
        mut is_protected_head: impl FnMut(u32) -> bool,
    ) -> Option<u32> {
        let raw = Self::read_long(&mut access, Self::table_address(trap_word, kind))?;
        match resolve_trap_table_target_with_provenance(
            raw,
            |address| Self::read_long(&mut access, address),
            &mut is_protected_head,
        )? {
            TrapTableTarget::Direct(target) => Some(target),
            TrapTableTarget::Protected {
                logical_successor, ..
            } => Some(logical_successor),
        }
    }

    /// Validate the body of a handler supplied to SetTrapAddress/NSetTrapAddress.
    ///
    /// A permanent come-from head is system-owned patch machinery, not a
    /// guest-installable successor. The Trap Manager reports that malformed
    /// request before it reads or mutates a table cell. A handler outside the
    /// readable guest address space remains valid here: focused pre-
    /// materialization fixtures and native callers may use an address that is
    /// only meaningful to their surrounding execution harness. Inside
    /// Macintosh: Operating System Utilities (1994), p. 8-30.
    #[cfg(test)]
    pub(crate) fn validate_handler(
        handler: u32,
        mut read_long: impl FnMut(u32) -> Option<u32>,
    ) -> Result<(), TrapManagerSetError> {
        if read_long(handler) == Some(COME_FROM_PATCH_SIGNATURE) {
            Err(TrapManagerSetError::InvalidComeFromHead)
        } else {
            Ok(())
        }
    }

    /// Validate a handler while requiring explicit system provenance before a
    /// signed address is treated as a protected come-from head.
    pub(crate) fn validate_handler_with_provenance(
        handler: u32,
        mut read_long: impl FnMut(u32) -> Option<u32>,
        mut is_protected_head: impl FnMut(u32) -> bool,
    ) -> Result<(), TrapManagerSetError> {
        if is_protected_head(handler) && read_long(handler) == Some(COME_FROM_PATCH_SIGNATURE) {
            Err(TrapManagerSetError::InvalidComeFromHead)
        } else {
            Ok(())
        }
    }

    /// Replace a logical handler while preserving protected come-from heads.
    ///
    /// Setters write the final exit-JMP target in a protected chain and write
    /// the raw table cell for a direct entry. The caller supplies separate
    /// ordinary and privileged writers because system-owned come-from code is
    /// read-only to guest stores. Inside Macintosh: Operating System
    /// Utilities (1994), pp. 8-8--8-9 and 8-27--8-31.
    #[cfg(test)]
    pub(crate) fn set_address(
        trap_word: u16,
        kind: TrapTableKind,
        handler: u32,
        mut access: impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
    ) -> Result<(), TrapManagerSetError> {
        Self::validate_handler(handler, |address| Self::read_long(&mut access, address))?;

        let table_address = Self::table_address(trap_word, kind);
        let raw = Self::read_long(&mut access, table_address)
            .ok_or(TrapManagerSetError::UnreadableTable)?;
        let target =
            resolve_trap_table_target(raw, |address| Self::read_long(&mut access, address))
                .ok_or(TrapManagerSetError::MalformedComeFromChain)?;
        match target {
            TrapTableTarget::Direct(_) => Self::write_long(
                &mut access,
                TrapManagerMemoryOp::WriteLong {
                    address: table_address,
                    value: handler,
                },
            )
            .ok_or(TrapManagerSetError::WriteRejected),
            TrapTableTarget::Protected { last_head, .. } => {
                let link = last_head
                    .checked_add(4)
                    .ok_or(TrapManagerSetError::MalformedComeFromChain)?;
                Self::write_long(
                    &mut access,
                    TrapManagerMemoryOp::WriteProtectedLong {
                        address: link,
                        value: handler,
                    },
                )
                .ok_or(TrapManagerSetError::WriteRejected)
            }
        }
    }

    /// Replace a handler with provenance-aware protected-chain resolution.
    /// Protected chain links are selected only when their head is both
    /// system-owned and signed;
    /// ordinary writable memory containing the same four bytes remains a
    /// direct table target and receives an ordinary write.
    pub(crate) fn set_address_with_provenance(
        trap_word: u16,
        kind: TrapTableKind,
        handler: u32,
        mut access: impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
        mut is_protected_head: impl FnMut(u32) -> bool,
    ) -> Result<(), TrapManagerSetError> {
        Self::validate_handler_with_provenance(
            handler,
            |address| Self::read_long(&mut access, address),
            &mut is_protected_head,
        )?;

        let table_address = Self::table_address(trap_word, kind);
        let raw = Self::read_long(&mut access, table_address)
            .ok_or(TrapManagerSetError::UnreadableTable)?;
        let target = resolve_trap_table_target_with_provenance(
            raw,
            |address| Self::read_long(&mut access, address),
            &mut is_protected_head,
        )
        .ok_or(TrapManagerSetError::MalformedComeFromChain)?;
        match target {
            TrapTableTarget::Direct(_) => Self::write_long(
                &mut access,
                TrapManagerMemoryOp::WriteLong {
                    address: table_address,
                    value: handler,
                },
            )
            .ok_or(TrapManagerSetError::WriteRejected),
            TrapTableTarget::Protected { last_head, .. } => {
                let link = last_head
                    .checked_add(4)
                    .ok_or(TrapManagerSetError::MalformedComeFromChain)?;
                Self::write_long(
                    &mut access,
                    TrapManagerMemoryOp::WriteProtectedLong {
                        address: link,
                        value: handler,
                    },
                )
                .ok_or(TrapManagerSetError::WriteRejected)
            }
        }
    }

    fn read_long(
        access: &mut impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
        address: u32,
    ) -> Option<u32> {
        match access(TrapManagerMemoryOp::ReadLong(address))? {
            TrapManagerMemoryResult::Long(value) => Some(value),
            TrapManagerMemoryResult::Written => None,
        }
    }

    fn write_long(
        access: &mut impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult>,
        operation: TrapManagerMemoryOp,
    ) -> Option<()> {
        match access(operation)? {
            TrapManagerMemoryResult::Long(_) => None,
            TrapManagerMemoryResult::Written => Some(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn read_write_access<'a>(
        words: &'a mut HashMap<u32, u32>,
        operations: &'a mut Vec<TrapManagerMemoryOp>,
    ) -> impl FnMut(TrapManagerMemoryOp) -> Option<TrapManagerMemoryResult> + 'a {
        move |operation| {
            operations.push(operation);
            match operation {
                TrapManagerMemoryOp::ReadLong(address) => words
                    .get(&address)
                    .copied()
                    .map(TrapManagerMemoryResult::Long),
                TrapManagerMemoryOp::WriteLong { address, value }
                | TrapManagerMemoryOp::WriteProtectedLong { address, value } => {
                    words.insert(address, value);
                    Some(TrapManagerMemoryResult::Written)
                }
            }
        }
    }

    #[test]
    fn service_get_and_set_follow_a_protected_chain() {
        let trap_word = 0xA078;
        let table_address = TrapManager::table_address(trap_word, TrapTableKind::OperatingSystem);
        let head = 0x0010_0000;
        let successor = 0x0020_0000;
        let replacement = 0x0030_0000;
        let mut words = HashMap::new();
        words.insert(table_address, head);
        words.insert(head, COME_FROM_PATCH_SIGNATURE);
        words.insert(head + 4, successor);

        let mut get_operations = Vec::new();
        let address = TrapManager::get_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            read_write_access(&mut words, &mut get_operations),
        );
        assert_eq!(address, Some(successor));

        let mut set_operations = Vec::new();
        let result = TrapManager::set_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            replacement,
            read_write_access(&mut words, &mut set_operations),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(words.get(&table_address), Some(&head));
        assert_eq!(words.get(&(head + 4)), Some(&replacement));
        assert!(set_operations.iter().any(|operation| matches!(
            operation,
            TrapManagerMemoryOp::WriteProtectedLong {
                address,
                value
            } if *address == head + 4 && *value == replacement
        )));
    }

    #[test]
    fn service_rejects_a_come_from_head_before_writing_the_table() {
        let trap_word = 0xA078;
        let table_address = TrapManager::table_address(trap_word, TrapTableKind::OperatingSystem);
        let head = 0x0010_0000;
        let original = 0x0020_0000;
        let mut words = HashMap::new();
        words.insert(table_address, original);
        words.insert(head, COME_FROM_PATCH_SIGNATURE);
        let mut operations = Vec::new();

        let result = TrapManager::set_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            head,
            read_write_access(&mut words, &mut operations),
        );

        assert_eq!(result, Err(TrapManagerSetError::InvalidComeFromHead));
        assert_eq!(words.get(&table_address), Some(&original));
        assert!(!operations.iter().any(|operation| matches!(
            operation,
            TrapManagerMemoryOp::ReadLong(address) if *address == table_address
        )));
        assert!(!operations.iter().any(|operation| matches!(
            operation,
            TrapManagerMemoryOp::WriteLong { .. } | TrapManagerMemoryOp::WriteProtectedLong { .. }
        )));
    }

    #[test]
    fn service_reports_unreadable_and_malformed_tables_and_rejected_writes() {
        let trap_word = 0xA047;
        let table_address = TrapManager::table_address(trap_word, TrapTableKind::OperatingSystem);

        let mut unreadable_words = HashMap::new();
        let mut unreadable_operations = Vec::new();
        let unreadable = TrapManager::set_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            0x0020_0000,
            read_write_access(&mut unreadable_words, &mut unreadable_operations),
        );
        assert_eq!(unreadable, Err(TrapManagerSetError::UnreadableTable));

        let head = 0x0010_0000;
        let mut cyclic_words = HashMap::new();
        cyclic_words.insert(table_address, head);
        cyclic_words.insert(head, COME_FROM_PATCH_SIGNATURE);
        cyclic_words.insert(head + 4, head);
        let mut cyclic_operations = Vec::new();
        let malformed = TrapManager::set_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            0x0020_0000,
            read_write_access(&mut cyclic_words, &mut cyclic_operations),
        );
        assert_eq!(malformed, Err(TrapManagerSetError::MalformedComeFromChain));

        let original = 0x0020_0000;
        let replacement = 0x0030_0000;
        let mut rejected_words = HashMap::new();
        rejected_words.insert(table_address, original);
        let rejected = TrapManager::set_address(
            trap_word,
            TrapTableKind::OperatingSystem,
            replacement,
            |operation| match operation {
                TrapManagerMemoryOp::ReadLong(address) => rejected_words
                    .get(&address)
                    .copied()
                    .map(TrapManagerMemoryResult::Long),
                TrapManagerMemoryOp::WriteLong { .. }
                | TrapManagerMemoryOp::WriteProtectedLong { .. } => None,
            },
        );
        assert_eq!(rejected, Err(TrapManagerSetError::WriteRejected));
        assert_eq!(rejected_words.get(&table_address), Some(&original));
    }

    #[test]
    fn provenance_keeps_a_signature_in_writable_memory_as_a_direct_target() {
        let head = 0x0010_0000;
        let successor = 0x0020_0000;
        let read = |address| match address {
            address if address == head => Some(COME_FROM_PATCH_SIGNATURE),
            address if address == head + 4 => Some(successor),
            _ => None,
        };

        assert_eq!(
            resolve_trap_table_target_with_provenance(head, read, |_| false),
            Some(TrapTableTarget::Direct(head))
        );
        assert_eq!(
            resolve_trap_table_target_with_provenance(head, read, |address| address == head),
            Some(TrapTableTarget::Protected {
                last_head: head,
                logical_successor: successor,
            })
        );
    }
}
