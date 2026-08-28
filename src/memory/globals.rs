//! Low-memory globals
//!
//! Mac OS stores important system variables in the low-memory area ($0000-$0FFF).
//! These are documented in Inside Macintosh and are essential for Toolbox operation.
//!
//! References:
//! - Inside Macintosh Volume II, II-19 (Low-Memory Global Variables)
//! - Inside Macintosh Volume IV, IV-246 (Additional globals)

use std::collections::HashMap;

/// Initial per-process SysEvtMask: every low-level event except keyUp.
/// Macintosh Toolbox Essentials (1992), pp. 2-28--2-29 and 2-99.
pub(crate) const DEFAULT_SYS_EVT_MASK: u16 = 0xFFEF;

/// Low-memory global variable addresses
pub mod addr {
    // System globals
    /// ScreenRow: rowBytes of the active screen (word).
    /// MPW Interfaces/AIncludes/LowMemEqu.a defines `ScreenRow` at $0106.
    pub const SCREEN_ROW: u32 = 0x0106;
    pub const MEM_TOP: u32 = 0x0108; // Top of memory (ptr)
    pub const BUF_PTR: u32 = 0x010C; // Sound/disk buffer (ptr)
    pub const HEAP_END: u32 = 0x0114; // End of heap zone (ptr)
    pub const THE_ZONE: u32 = 0x0118; // Current heap zone (ptr)
    /// SysEvtMask: current process's low-level event posting mask (word).
    /// Macintosh Toolbox Essentials (1992), pp. 2-28--2-29 and 2-99--2-100;
    /// Inside Macintosh Volume III (1985), low-memory globals table.
    pub const SYS_EVT_MASK: u32 = 0x0144;
    pub const RND_SEED: u32 = 0x0156; // Random number seed (long) - Inside Macintosh Volume II, II-387
    pub const TICKS: u32 = 0x016A; // Tick count (long) - system timer
    pub const MB_STATE: u32 = 0x0172; // Mouse button state (byte) - 0=down, $80=up
    /// KeyMapLM: current keyboard bitmap, 4 longs / 16 bytes.
    /// Inside Macintosh Volume I, I-260 documents the GetKeys KeyMap as
    /// the current key state indexed by key code; MPW's SysEqu.h names the
    /// low-memory mirror `KeyMapLM` at $0174.
    pub const KEY_MAP_LM: u32 = 0x0174;
    pub const TIME: u32 = 0x020C; // Current date/time in seconds since 1904-01-01 (long)
    /// MemErr: current value of MemError (word).
    /// Inside Macintosh Volume IV, IV-80; low-memory table IV-246.
    pub const MEM_ERR: u32 = 0x0220;
    /// DoubleTime: maximum TickCount interval recognized as a double-click.
    /// Classic applications commonly read this long directly instead of
    /// calling through the Event Manager.
    /// Inside Macintosh Volume I, I-260 documents the value and global name;
    /// MPW SysEqu.h defines `DoubleTime` at $02F0.
    pub const DOUBLE_TIME: u32 = 0x02F0;
    pub const ROM85: u32 = 0x028E; // Version number of ROM (word) - Inside Macintosh V, V-578

    /// SdVolume: current speaker volume (1 byte, low-order three bits).
    /// Inside Macintosh Volume III, III-425 lists `SdVolume` at $0260 and
    /// describes it as the current speaker volume, with values 0..7.
    ///
    /// Marathon 1's sound module reads this byte at CODE 5 +`$0003F2`
    /// (`MOVE.B (mem $260).W, (A0)`) and uses it as a "Sound Driver alive"
    /// sentinel — if zero, M1 short-circuits its entire audio submission
    /// path. Systemless's HLE bypasses the legacy Sound Driver layer, so this
    /// byte must be initialized non-zero at boot to satisfy classic clients.
    pub const SD_VOLUME: u32 = 0x0260;

    /// SoundBase: pointer to the free-form synthesizer's main sound buffer.
    /// Inside Macintosh Volume III, III-425; Volume IV, IV-247 documents
    /// that programs should use this low-memory global rather than fixed
    /// hardware-dependent sound-buffer addresses.
    pub const SOUND_BASE: u32 = 0x0266;

    /// SoundLevel: amplitude in the Sound Driver's 740-byte buffer (1 byte).
    /// Inside Macintosh Volume III, III-425.
    pub const SOUND_LEVEL: u32 = 0x027F;

    // Menu Manager globals
    /// MenuList: handle to the current Menu Manager menu list.
    /// Inside Macintosh Volume III (1985), low-memory globals table;
    /// Inside Macintosh Volume V (1986), pp. V-228–V-230.
    pub const MENU_LIST: u32 = 0x0A1C;
    pub const MBAR_HEIGHT: u32 = 0x0BAA; // Menu bar height in pixels (word) - Inside Macintosh V, V-245
    pub const MENU_FLASH: u32 = 0x0A24; // Number of times menu item blinks (word) - Inside Macintosh Volume I, I-361

    /// MenuDisable: menu ID + item number of the last menu item the cursor
    /// passed over while a menu was down (4 bytes, LongInt — high word =
    /// menuID, low word = itemNumber). Maintained by the standard menu
    /// definition procedure ('MDEF' 0) on each cursor-into-item transition,
    /// regardless of whether the item is enabled or disabled. Read by
    /// MenuChoice ($AA66) when the application's MenuSelect / MenuKey
    /// returned zero, to surface "which disabled item did the user click?"
    /// for help/explanation UI. Per IM:V V-248 + MTb 1992 3-118 (the
    /// canonical EQU at IM:V V-571 line 8689: `MenuDisable EQU $0B54`).
    /// Systemless's HLE reads this lowmem word directly in MenuChoice; it
    /// still does not synthesize the MDEF cursor-tracking writes that
    /// classic ROMs receive, so tests seed the value explicitly when they
    /// need a deterministic result.
    /// Inside Macintosh Volume V, V-248 (MenuChoice routine description)
    /// and V-571 (assembly globals table); Macintosh Toolbox Essentials
    /// 1992, 3-118..3-119 (MenuChoice canonical chapter).
    pub const MENU_DISABLE: u32 = 0x0B54;

    /// MenuCInfo: handle to the current menu color information table
    /// (4 bytes, MCTableHandle). Created by InitMenus and maintained by
    /// the Menu Color Manager traps: GetMCInfo ($AA61) returns a deep
    /// copy of the current table, SetMCInfo ($AA62) replaces the current
    /// table, DispMCInfo ($AA63) disposes a caller-supplied table,
    /// GetMCEntry ($AA64) returns a pointer into the live table, and
    /// SetMCEntries / DelMCEntries ($AA65 / $AA60) update or remove
    /// entries. Systemless HLE still does not auto-load 'mctb' resources,
    /// but it now stores a real live table here for API compatibility.
    /// Per IM:V V-247 + V-571 line 8688: `MenuCInfo EQU $0D50`. The
    /// Menu Color Manager was deprecated in System 7.5 by the Theme
    /// Manager (Macintosh Toolbox Essentials 1992 treats the routines
    /// as compatibility-only).
    /// Inside Macintosh Volume V, V-247..V-248 (Menu Color Manager
    /// routines) and V-571 (assembly globals table).
    pub const MENU_C_INFO: u32 = 0x0D50;

    // QuickDraw globals
    pub const THE_PORT: u32 = 0x09DA; // Current GrafPort (ptr)
    pub const SCRN_BASE: u32 = 0x0824; // Screen base address (ptr) - Inside Macintosh II, II-19

    /// GhostWindow: pointer to a window that FrontWindow must not consider
    /// frontmost, even when it is first in the Window Manager list.
    /// Inside Macintosh Volume I, I-287; Volume III, low-memory globals table.
    pub const GHOST_WINDOW: u32 = 0x0A84;

    // Mouse position globals (Points are 4 bytes: v word, h word)
    // Reference: Executor docs/globals.cpp
    pub const M_TEMP: u32 = 0x0828; // Temporary mouse position (Point) - interrupt level
    pub const MOUSE_LOC: u32 = 0x082C; // Mouse location (Point) - "RawMouse"
    pub const MOUSE_LOC2: u32 = 0x0830; // Secondary mouse location (Point)
    /// JHideCursor: QuickDraw glue vector for HideCursor.
    ///
    /// The classic low-memory vector table places the argument-free
    /// `JHideCursor` bottleneck at `$0800`, immediately below `JShowCursor`.
    pub const J_HIDE_CURSOR: u32 = 0x0800;
    /// JShowCursor: QuickDraw glue vector for ShowCursor.
    ///
    /// The low-memory globals table in On Macintosh Programming: Advanced
    /// Techniques (1990) identifies `JShowCursor` at `$0804`.
    pub const J_SHOW_CURSOR: u32 = 0x0804;
    /// JShieldCursor: address of QuickDraw's low-level cursor shielding
    /// procedure (QDJShieldCursorProcPtr).
    ///
    /// MPW Universal Interfaces Quickdraw.h declares this callback as four
    /// Pascal INTEGER arguments: left, top, right, and bottom.
    pub const J_SHIELD_CURSOR: u32 = 0x0808;
    /// JInitCrsr: low-level cursor initialization vector.
    ///
    /// MPW Interfaces/AIncludes/LowMemEqu.a declares `JInitCrsr EQU $814`.
    /// Applications may call the vector directly instead of issuing the
    /// `_InitCursor` trap.
    pub const J_INIT_CRSR: u32 = 0x0814;
    /// JSwapFont: address of the Font Manager's FMSwapFont routine (ProcPtr).
    ///
    /// This private vector is called directly by QuickDraw text code. Executor's
    /// clean-room low-memory table identifies `$08E0` as `JSwapFont`, and its
    /// startup path initializes the vector from the `$A901` FMSwapFont routine.
    pub const J_SWAP_FONT: u32 = 0x08E0;
    /// JCrsrTask: address of the cursor VBL task routine (ProcPtr).
    /// MPW Interfaces/AIncludes/LowMemEqu.a lists `JCrsrTask EQU $8EE`
    /// immediately after `CrsrThresh` and before the interrupt mouse globals.
    pub const J_CRSR_TASK: u32 = 0x08EE;

    // screenBits BitMap structure (14 bytes: baseAddr(4) + rowBytes(2) + bounds(8))
    // On a real Mac this lives in QD globals, but apps read it during InitGraf.
    // We store it at $083C to avoid conflicting with mouse globals at $0828-$0833.
    pub const SCREEN_BITS: u32 = 0x083C;

    // File Manager globals
    pub const SF_SAVE_DISK: u32 = 0x0214; // Negative of volume reference number (word) - Inside Macintosh Volume IV, IV-72
    pub const FCB_S_PTR: u32 = 0x034E; // FCB array pointer
    pub const DEF_VCB_PTR: u32 = 0x0352; // Default VCB pointer
    pub const VCB_Q_HDR: u32 = 0x0356; // VCB queue header
    pub const FS_Q_HDR: u32 = 0x0360; // File I/O queue header
    pub const CUR_DIR_STORE: u32 = 0x0398; // Directory ID of directory last opened (long) - Inside Macintosh Volume IV, IV-72
    pub const FS_FCB_LEN: u32 = 0x03F6; // Size of a file control block (word) - Files 1992, 2-384

    /// Callable OS trap-table entry for SwapMMUMode ($A05D).
    ///
    /// Inside Macintosh Volume V, V-593 identifies SwapMMUMode as trap
    /// `$A05D`. OS trap-table entries begin at `$0400`, so selector `$5D`
    /// occupies `$0400 + ($5D * 4) = $0574`.
    pub const SWAP_MMU_MODE_TRAP: u32 = 0x0574;

    // Memory Manager globals (for NewPtr, etc.)
    pub const APP_L_ZONE: u32 = 0x02AA; // Application zone (ptr)
    pub const SYS_ZONE: u32 = 0x02A6; // System zone (ptr)
    /// MMU32Bit: TRUE when 32-bit addressing mode is in effect.
    /// Inside Macintosh: Memory 1992, p. 4-25 and low-memory table
    /// line 8838; also listed in Inside Macintosh Volume V, V-593.
    pub const MMU32_BIT: u32 = 0x0CB2;

    /// ResumeProc: address of the system error resume procedure
    /// (4 bytes, ProcPtr). Set by InitDialogs ($A97B) from its
    /// `resumeProc` parameter; read by the System Error Handler
    /// when a fatal system error occurs. Inside Macintosh Volume I,
    /// I-411 (and the Dialog Mgr globals summary table at I-432).
    pub const RESUME_PROC: u32 = 0x0A8C;

    /// DSErrCode: current system error ID (word) written by SysError.
    /// Inside Macintosh Volume III (1985), low-memory globals table.
    pub const DS_ERR_CODE: u32 = 0x0AF0;

    /// ANumber: resource ID of the last alert that occurred (2
    /// bytes, INTEGER). Written by Alert/StopAlert/NoteAlert/
    /// CautionAlert ($A985..$A988) on each successful ALRT lookup.
    /// Inside Macintosh Volume I, I-423.
    pub const ANUMBER: u32 = 0x0A98;

    /// AlertStage / ACount: stage of the last occurrence of an
    /// alert (2 bytes, INTEGER per IM:I I-423; MPW reads via
    /// `#define GetAlertStage() (* (short*) 0x0A9A)` per
    /// MTb 1992 22620). Holds 0..3 with stage = word+1. The
    /// Alert/StopAlert/NoteAlert/CautionAlert trio inspect this
    /// word to choose which 4-bit nibble of the ALRT template's
    /// `stages` word to apply, then increment (capped at 3 — IM:I
    /// I-417). InitDialogs ($A97B) zeros it. ResetAlertStage and
    /// GetAlertStage are documented "[Not in ROM]" per IM:I I-422
    /// — ResetAlertStage compiles to a direct `CLR.W $0A9A.W`
    /// store; GetAlertStage compiles to a direct word load. Read
    /// and write this address with `read_word` / `write_word` —
    /// using `read_byte` reads the high byte (always 0 for stages
    /// 0..3 on big-endian 68k) which is a subtle latent bug.
    /// Inside Macintosh Volume I, I-417 + I-423.
    pub const ALERT_STAGE: u32 = 0x0A9A;

    /// DABeeper: address of the current alert sound procedure
    /// (4 bytes, ProcPtr). Set by InitDialogs ($A97B) to the
    /// standard sound procedure; replaced by ErrorSound ($A98C)
    /// from its `soundProc` argument. NIL means "no sound (and no
    /// menu bar blink) at all" per IM:I I-411.
    /// Inside Macintosh Volume I, I-411.
    pub const DA_BEEPER: u32 = 0x0A9C;

    /// DlgFont: font number for subsequently created dialog and alert
    /// grafPorts. SetDAFont / SetDialogFont are documented glue routines;
    /// assembly code can also set this low-memory global directly.
    /// Inside Macintosh Volume I, I-412; Macintosh Toolbox Essentials 1992,
    /// p. 6-104.
    pub const DLG_FONT: u32 = 0x0AFA;

    /// TEScrpLength: size of the TextEdit scrap in bytes (2-byte
    /// INTEGER). Per IM:I I-389 + I-390 + assembly note at
    /// I-12606: contains the byte count of the cut/copied text
    /// currently held in the TE-private scrap (separate from the
    /// shared desk scrap). Set to 0 by TEInit ($A9CC); rewritten
    /// by TECopy / TECut / TEPaste / TEFromScrap each time the
    /// scrap is touched. Apps that probe this from assembly to
    /// detect "is there text on the TE clipboard?" rely on it
    /// being correctly maintained.
    /// Inside Macintosh Volume I, I-389 (TEScrpLength global).
    pub const TE_SCRP_LENGTH: u32 = 0x0AB0;

    /// TEScrpHandle: handle to the TextEdit scrap (4 bytes,
    /// Handle). Per IM:I I-389 + assembly note at I-12598: the
    /// allocated relocatable block holding the cut/copied text
    /// bytes. TEInit ($A9CC) allocates a zero-length handle
    /// here on first call; TECopy / TECut / TEPaste resize the
    /// underlying block as needed. NIL if TEInit hasn't run yet
    /// — defensive callers should check before dereferencing.
    /// Inside Macintosh Volume I, I-389 (TEScrpHandle global).
    pub const TE_SCRP_HANDLE: u32 = 0x0AB4;

    /// DAStrings: handles to the four ParamText strings
    /// (16 bytes = 4 × Handle). Substitutable into dialog and
    /// alert text via the `^0`..`^3` escapes at draw time.
    /// InitDialogs ($A97B) zeros all 4 entries (== "" empty
    /// strings); ParamText ($A98B) replaces each entry with a
    /// fresh handle to the caller's Pascal string.
    /// Inside Macintosh Volume I, I-421 (DAStrings global array).
    pub const DA_STRINGS: u32 = 0x0AA0;

    // Application globals
    pub const CUR_APNAME: u32 = 0x0910; // Current app name (Str31)
    pub const CUR_APREF_NUM: u32 = 0x0900; // Current app ref num (int)
    /// AppParmHandle: handle to Finder launch information.
    /// The data begins with a message word and selected-file count word.
    /// Inside Macintosh Volume II, II-57; Files 1992, 1-58.
    pub const APP_PARM_HANDLE: u32 = 0x0AEC;
    pub const CURRENT_A5: u32 = 0x0904; // Current A5 (ptr) - Inside Macintosh Memory 1-77
    pub const CUR_JT_OFFSET: u32 = 0x0934; // Jump table offset from A5 (word) - Inside Macintosh Volume II, II-62

    // Stack and heap limits
    pub const CUR_STACK_BASE: u32 = 0x0908; // Stack base (ptr)
    pub const APPL_LIMIT: u32 = 0x0130; // Application heap limit (ptr)
}

/// Manager for low-memory globals
pub struct LowMemGlobals {
    /// Storage for global values (sparse, only populated as needed)
    values: HashMap<u32, u32>,
}

impl LowMemGlobals {
    /// Create new low-memory globals
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Get a 32-bit global value
    pub fn get_long(&self, address: u32) -> u32 {
        *self.values.get(&address).unwrap_or(&0)
    }

    /// Set a 32-bit global value
    pub fn set_long(&mut self, address: u32, value: u32) {
        self.values.insert(address, value);
    }

    /// Get a 16-bit global value
    pub fn get_word(&self, address: u32) -> u16 {
        (self.get_long(address & !1) >> ((1 - (address & 1)) * 8)) as u16
    }

    /// Set a 16-bit global value
    pub fn set_word(&mut self, address: u32, value: u16) {
        let aligned = address & !1;
        let current = self.get_long(aligned);
        let new_value = if (address & 1) == 0 {
            (current & 0x0000_FFFF) | ((value as u32) << 16)
        } else {
            (current & 0xFFFF_0000) | (value as u32)
        };
        self.set_long(aligned, new_value);
    }

    // Convenience accessors for common globals

    /// Get FCB array pointer
    pub fn fcb_ptr(&self) -> u32 {
        self.get_long(addr::FCB_S_PTR)
    }

    /// Set FCB array pointer
    pub fn set_fcb_ptr(&mut self, ptr: u32) {
        self.set_long(addr::FCB_S_PTR, ptr);
    }

    /// Get default VCB pointer
    pub fn def_vcb_ptr(&self) -> u32 {
        self.get_long(addr::DEF_VCB_PTR)
    }

    /// Set default VCB pointer
    pub fn set_def_vcb_ptr(&mut self, ptr: u32) {
        self.set_long(addr::DEF_VCB_PTR, ptr);
    }

    /// Get current GrafPort
    pub fn the_port(&self) -> u32 {
        self.get_long(addr::THE_PORT)
    }

    /// Set current GrafPort
    pub fn set_the_port(&mut self, ptr: u32) {
        self.set_long(addr::THE_PORT, ptr);
    }

    /// Get top of memory
    pub fn mem_top(&self) -> u32 {
        self.get_long(addr::MEM_TOP)
    }

    /// Set top of memory
    pub fn set_mem_top(&mut self, ptr: u32) {
        self.set_long(addr::MEM_TOP, ptr);
    }
}

impl Default for LowMemGlobals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lock in low-memory global addresses against IM:I App-A. A typo
    /// (e.g. swapping a digit) in any of these would silently break
    /// apps that read the global directly.
    #[test]
    fn low_mem_global_addresses_match_inside_macintosh() {
        // SdVolume ($0260) is the M1 sound-unlock load-bearing
        // constant. Marathon 1's CODE 5 +$0003F2 reads the byte here
        // as a "Sound Driver alive" sentinel; with zero, M1 short-
        // circuits its entire audio submission path. Inside Macintosh
        // Volume III, III-425.
        assert_eq!(
            addr::SD_VOLUME,
            0x0260,
            "SdVolume must be at $0260 per Inside Macintosh Volume III — \
             changing this address breaks M1 audio unlock."
        );
        assert_eq!(
            addr::SOUND_LEVEL,
            0x027F,
            "SoundLevel must be at $027F per Inside Macintosh Volume III."
        );
        assert_eq!(
            addr::SOUND_BASE,
            0x0266,
            "SoundBase must be at $0266 per Inside Macintosh Volume III."
        );

        // Other heavily-load-bearing globals; a regression in any
        // of these has been historically catastrophic.
        assert_eq!(addr::TICKS, 0x016A, "Ticks per IM:II II-387");
        assert_eq!(
            addr::DOUBLE_TIME,
            0x02F0,
            "DoubleTime at $02F0 per MPW SysEqu.h; semantics documented in IM:I I-260"
        );
        assert_eq!(
            addr::RND_SEED,
            0x0156,
            "RndSeed per IM:II II-387 — regression breaks random sequences"
        );
        assert_eq!(
            addr::MB_STATE,
            0x0172,
            "MBState mouse button — wrong address = button stuck"
        );
        assert_eq!(
            addr::KEY_MAP_LM,
            0x0174,
            "KeyMapLM low-memory keyboard bitmap — wrong address = direct key polling breaks"
        );
        assert_eq!(addr::CURRENT_A5, 0x0904, "CurrentA5 per IM:Memory 1-77");
        assert_eq!(
            addr::MMU32_BIT,
            0x0CB2,
            "MMU32Bit per IM:Memory 1992 low-memory table"
        );
    }
}
