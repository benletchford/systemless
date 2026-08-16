//! 32-bit PowerPC user-mode interpreter.
//!
//! Architecture-only: register state, instruction decoder,
//! dispatch loop, memory-bus trait. No Mac specifics, no PEF, no
//! Toolbox — consumer crates layer those on top. Mirrors the
//! shape of the [`m68k`](https://crates.io/crates/m68k) crate for
//! the classic-Macintosh emulator family.
//!
//! References:
//!
//! - *PowerPC User Instruction Set Architecture, Book I,
//!   Version 2.01* — instruction encodings cited inline at every
//!   dispatch site.
//! - *Inside Macintosh: PowerPC System Software* (1994), Ch 1 —
//!   the calling convention most practical PowerPC programs use
//!   (GPR1 = stack pointer, GPR2 = RTOC, GPR3..GPR10 = integer
//!   args, FPR1..FPR13 = float args). The interpreter itself is
//!   convention-agnostic; this is just the most common ABI a
//!   host expects.
//!
//! The 32-bit base architecture defines:
//!
//! | Surface | Count | Width |
//! | ------- | ----- | ----- |
//! | GPR0–GPR31 (general-purpose registers) | 32 | 32 |
//! | FPR0–FPR31 (floating-point registers)  | 32 | 64 |
//! | CR (condition register)                |  1 | 32 (8 × 4-bit fields) |
//! | LR (link register)                     |  1 | 32 |
//! | CTR (count register)                   |  1 | 32 |
//! | XER (fixed-point exception register)   |  1 | 32 |
//! | FPSCR (floating-point status/control)  |  1 | 32 |
//! | MSR (machine state register)           |  1 | 32 |
//! | PC (program counter / NIA)             |  1 | 32 |
//!
//! Multi-byte memory access is big-endian via the [`PpcMemory`]
//! trait. Real PowerPC has an MSR LE bit; classic Mac PowerPC
//! always leaves it clear, so the byte order is wired in.

use std::collections::BTreeMap;

const PPC_DECODE_CACHE_MAX_ENTRIES: usize = 4096;
const PPC_DECODE_CACHE_INDEX_MASK: usize = PPC_DECODE_CACHE_MAX_ENTRIES - 1;
type PpcDecodeCacheEntry = Option<(u32, Result<PpcInstr, PpcDecodeError>)>;
const PPC_CFM_IMPORT_STUB_CACHE_MAX_ENTRIES: usize = 1024;
const PPC_CFM_IMPORT_STUB_CACHE_INDEX_MASK: usize = PPC_CFM_IMPORT_STUB_CACHE_MAX_ENTRIES - 1;
type PpcCfmImportStubCacheEntry = Option<(u32, u32)>;

/// Host policy for unaligned memory accesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcAlignmentPolicy {
    /// Surface unaligned instruction/data accesses as architected
    /// alignment exceptions.
    Trap,
    /// Allow unaligned data loads/stores to fall through to the
    /// byte-granular memory bus. Instruction fetch alignment still
    /// traps because an unaligned PC cannot identify a valid PPC
    /// instruction boundary.
    EmulateData,
}

/// One successfully fetched instruction word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcFetchedInstruction {
    pub pc: u32,
    pub word: u32,
}

/// Optional hook called after an instruction word is fetched from
/// memory and before it is decoded/executed.
pub trait PpcFetchObserver {
    fn on_fetch(&mut self, pc: u32, word: u32);

    fn on_fetch_cpu(&mut self, cpu: &PpcCpu, word: u32) {
        self.on_fetch(cpu.pc, word);
    }
}

/// No-op fetch observer used by the default run-loop APIs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PpcNoopFetchObserver;

impl PpcFetchObserver for PpcNoopFetchObserver {
    fn on_fetch(&mut self, _pc: u32, _word: u32) {}
}

impl PpcFetchObserver for Vec<PpcFetchedInstruction> {
    fn on_fetch(&mut self, pc: u32, word: u32) {
        self.push(PpcFetchedInstruction { pc, word });
    }
}

/// Optional hook called after a guest-store byte is successfully
/// written through the memory bus.
pub trait PpcMemoryWriteObserver {
    fn observes_writes(&self) -> bool {
        true
    }

    fn on_write(&mut self, pc: u32, lr: u32, rtoc: u32, sp: u32, addr: u32, value: u8);
}

/// No-op memory-write observer used by the default run-loop APIs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PpcNoopMemoryWriteObserver;

impl PpcMemoryWriteObserver for PpcNoopMemoryWriteObserver {
    fn observes_writes(&self) -> bool {
        false
    }

    fn on_write(&mut self, _pc: u32, _lr: u32, _rtoc: u32, _sp: u32, _addr: u32, _value: u8) {}
}

/// Bounded reachable-instruction summary for real runs. It counts
/// successful fetches without retaining every PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcFetchHistogram {
    total: u64,
    primary: [u64; 64],
    secondary: BTreeMap<(u8, u16), u64>,
    words: BTreeMap<u32, u64>,
    pcs: BTreeMap<u32, u64>,
}

/// Decoder coverage summary for a [`PpcFetchHistogram`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcFetchDecodeSummary {
    total: u64,
    decoded: u64,
    unsupported_primary: BTreeMap<u8, u64>,
    unsupported_secondary: BTreeMap<(u8, u16), u64>,
}

impl PpcFetchDecodeSummary {
    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn decoded(&self) -> u64 {
        self.decoded
    }

    pub fn unsupported(&self) -> u64 {
        self.total.saturating_sub(self.decoded)
    }

    pub fn is_fully_decoded(&self) -> bool {
        self.unsupported() == 0
    }

    pub fn unsupported_primary(&self) -> &BTreeMap<u8, u64> {
        &self.unsupported_primary
    }

    pub fn unsupported_secondary(&self) -> &BTreeMap<(u8, u16), u64> {
        &self.unsupported_secondary
    }
}

impl Default for PpcFetchHistogram {
    fn default() -> Self {
        Self {
            total: 0,
            primary: [0; 64],
            secondary: BTreeMap::new(),
            words: BTreeMap::new(),
            pcs: BTreeMap::new(),
        }
    }
}

impl PpcFetchHistogram {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn primary_count(&self, primary: u8) -> u64 {
        self.primary
            .get(usize::from(primary))
            .copied()
            .unwrap_or_default()
    }

    pub fn secondary_count(&self, primary: u8, secondary: u16) -> u64 {
        self.secondary
            .get(&(primary, secondary))
            .copied()
            .unwrap_or_default()
    }

    pub fn word_count(&self, word: u32) -> u64 {
        self.words.get(&word).copied().unwrap_or_default()
    }

    pub fn secondary_counts(&self) -> &BTreeMap<(u8, u16), u64> {
        &self.secondary
    }

    pub fn word_counts(&self) -> &BTreeMap<u32, u64> {
        &self.words
    }

    pub fn pc_count(&self, pc: u32) -> u64 {
        self.pcs.get(&pc).copied().unwrap_or_default()
    }

    pub fn pc_counts(&self) -> &BTreeMap<u32, u64> {
        &self.pcs
    }

    pub fn decode_summary(&self) -> PpcFetchDecodeSummary {
        let mut summary = PpcFetchDecodeSummary {
            total: self.total,
            decoded: 0,
            unsupported_primary: BTreeMap::new(),
            unsupported_secondary: BTreeMap::new(),
        };
        for (&word, &count) in &self.words {
            match crate::decode::decode(word) {
                Ok(_) => summary.decoded = summary.decoded.saturating_add(count),
                Err(crate::decode::PpcDecodeError::UnsupportedPrimaryOpcode(primary)) => {
                    *summary.unsupported_primary.entry(primary).or_default() += count;
                }
                Err(crate::decode::PpcDecodeError::UnsupportedSecondaryOpcode {
                    primary,
                    secondary,
                }) => {
                    *summary
                        .unsupported_secondary
                        .entry((primary, secondary))
                        .or_default() += count;
                }
            }
        }
        summary
    }

    pub fn merge_from(&mut self, other: &Self) {
        self.total = self.total.saturating_add(other.total);
        for (dst, src) in self.primary.iter_mut().zip(other.primary.iter()) {
            *dst = dst.saturating_add(*src);
        }
        for (&key, &count) in &other.secondary {
            let entry = self.secondary.entry(key).or_default();
            *entry = entry.saturating_add(count);
        }
        for (&word, &count) in &other.words {
            let entry = self.words.entry(word).or_default();
            *entry = entry.saturating_add(count);
        }
        for (&pc, &count) in &other.pcs {
            let entry = self.pcs.entry(pc).or_default();
            *entry = entry.saturating_add(count);
        }
    }

    pub fn record_fetch(&mut self, word: u32) {
        self.record_fetch_at(None, word);
    }

    pub fn record_fetch_at(&mut self, pc: Option<u32>, word: u32) {
        let primary = ((word >> 26) & 0x3F) as u8;
        self.total = self.total.saturating_add(1);
        self.primary[usize::from(primary)] = self.primary[usize::from(primary)].saturating_add(1);
        if let Some(secondary) = Self::secondary_opcode(word) {
            *self.secondary.entry((primary, secondary)).or_default() += 1;
        }
        *self.words.entry(word).or_default() += 1;
        if let Some(pc) = pc {
            *self.pcs.entry(pc).or_default() += 1;
        }
    }

    fn secondary_opcode(word: u32) -> Option<u16> {
        let primary = ((word >> 26) & 0x3F) as u8;
        match primary {
            19 | 31 => Some(((word >> 1) & 0x03FF) as u16),
            59 => Some(((word >> 1) & 0x001F) as u16),
            63 => {
                let xo_5 = ((word >> 1) & 0x001F) as u16;
                match xo_5 {
                    18 | 20 | 21 | 22 | 23 | 25 | 28 | 29 | 30 | 31 => Some(xo_5),
                    _ => Some(((word >> 1) & 0x03FF) as u16),
                }
            }
            _ => None,
        }
    }
}

impl PpcFetchObserver for PpcFetchHistogram {
    fn on_fetch(&mut self, pc: u32, word: u32) {
        self.record_fetch_at(Some(pc), word);
    }
}

/// PowerPC architectural state. Registers are stored as `u32` /
/// `u64` rather than as bitfield structs because every dispatch
/// site reads them as plain integers — the architectural sub-fields
/// (e.g. CR's eight 4-bit fields, XER's SO/OV/CA flags) are decoded
/// at access time.
#[derive(Debug, Clone)]
pub struct PpcCpu {
    /// General-purpose registers (GPR0..GPR31). GPR1 is the stack
    /// pointer and GPR2 is `RTOC` (Table of Contents register).
    pub gpr: [u32; 32],

    /// Floating-point registers (FPR0..FPR31). 64-bit IEEE-754
    /// double-precision values per the PowerPC FPU model.
    pub fpr: [u64; 32],

    /// Condition Register: eight 4-bit fields CR0..CR7. CR0 is
    /// implicitly written by record-form integer instructions as
    /// LT/GT/EQ/SO; CR1 is written by record-form floating-point
    /// instructions as FPSCR FX/FEX/VX/OX; explicit compare
    /// instructions can write other CR fields.
    pub cr: u32,

    /// Link Register: holds the return address for branch-and-link
    /// instructions (`bl`, `bclrl`, etc.).
    pub lr: u32,

    /// Count Register: branch target for `bcctr` and decrement
    /// counter for branch-on-count.
    pub ctr: u32,

    /// Fixed-point Exception Register. SO/OV/CA flags + a 7-bit
    /// byte count for `lswx`/`stswx`.
    pub xer: u32,

    /// Floating-point Status and Control Register. This currently
    /// tracks the architectural bit layout, floating-point result
    /// flags, and record-form CR1 source bits; full sticky
    /// exception/enable semantics are intentionally still minimal.
    pub fpscr: u32,

    /// Machine State Register. PowerPC supervisor-mode bits. For
    /// user-level Classic Mac emulation the important default
    /// assumptions are big-endian execution (`LE` clear) and the
    /// floating-point facility available (`FP` set).
    pub msr: u32,

    /// Program counter (next instruction address to fetch).
    /// PowerPC architecture also calls this NIA ("next
    /// instruction address") in the prog model; we keep the
    /// 68k-side naming for cross-architecture readability.
    pub pc: u32,

    /// Host policy for unaligned data accesses. Classic Mac OS can
    /// fix up some unaligned data loads/stores; pure ISA tests keep
    /// the architected trap policy.
    pub alignment_policy: PpcAlignmentPolicy,

    reservation_addr: Option<u32>,

    import_call_stack: Vec<PpcImportReturnFrame>,

    // These host-only caches are heap-backed so PpcCpu and every enclosing
    // loader result remain small enough to pass through debug stack frames.
    decode_cache: Box<[PpcDecodeCacheEntry]>,

    cfm_import_stub_cache: Box<[PpcCfmImportStubCacheEntry]>,
}

/// How a native import callback should finalize GPR3 when it
/// reaches its synthetic return PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcNativeReturnGpr3 {
    /// Leave the callback's GPR3 value unchanged.
    Preserve,
    /// Keep only the bits selected by the mask.
    Mask(u32),
    /// Overwrite GPR3 with the supplied value.
    Set(u32),
    /// Select one of two values according to whether the callback returned 0.
    ZeroOrSet { zero: u32, nonzero: u32 },
    /// Overwrite GPR3 with one Condition Register bit as 0 or 1.
    CrBit(u8),
    /// Overwrite GPR3 with XER.CA as 0 or 1.
    XerCa,
    /// Overwrite GPR3 with XER.OV as 0 or 1.
    XerOv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcImportReturnFrame {
    return_pc: u32,
    final_pc: u32,
    restore_rtoc: u32,
    return_gpr3: PpcNativeReturnGpr3,
}

enum PpcFastImportStubResult {
    Continue,
    Stop(PpcRunResult),
}

impl Default for PpcCpu {
    fn default() -> Self {
        Self::new()
    }
}

impl PpcCpu {
    /// Construct a freshly-initialised PowerPC core with integer,
    /// floating-point, and status registers cleared. MSR starts with
    /// the floating-point facility enabled to match the Classic Mac
    /// user-mode ABI assumption.
    pub fn new() -> Self {
        Self {
            gpr: [0; 32],
            fpr: [0; 32],
            cr: 0,
            lr: 0,
            ctr: 0,
            xer: 0,
            fpscr: 0,
            msr: PPC_MSR_FP_AVAILABLE_MASK,
            pc: 0,
            alignment_policy: PpcAlignmentPolicy::Trap,
            reservation_addr: None,
            import_call_stack: Vec::new(),
            decode_cache: vec![None; PPC_DECODE_CACHE_MAX_ENTRIES].into_boxed_slice(),
            cfm_import_stub_cache: vec![None; PPC_CFM_IMPORT_STUB_CACHE_MAX_ENTRIES]
                .into_boxed_slice(),
        }
    }

    /// Read CR field N (0–7). Each field is 4 bits; field 0 is the
    /// most significant nibble of `self.cr` per PowerPC's MSB=0
    /// bit ordering.
    pub fn cr_field(&self, n: u8) -> u8 {
        debug_assert!(n < 8, "CR field index must be in 0..8");
        let shift = 28 - (n as u32) * 4;
        ((self.cr >> shift) & 0x0F) as u8
    }

    /// Write CR field N (0–7). `value` low-4 bits are stored.
    pub fn set_cr_field(&mut self, n: u8, value: u8) {
        debug_assert!(n < 8, "CR field index must be in 0..8");
        let shift = 28 - (n as u32) * 4;
        let mask = 0x0F_u32 << shift;
        self.cr = (self.cr & !mask) | ((u32::from(value) & 0x0F) << shift);
    }

    /// Read the 32-bit Floating-point Status and Control Register.
    pub fn fpscr(&self) -> u32 {
        self.fpscr
    }

    /// Replace the full 32-bit Floating-point Status and Control
    /// Register. Hosts can use this to restore saved numerics state.
    pub fn set_fpscr(&mut self, value: u32) {
        self.fpscr = value;
    }

    /// Read FPSCR field N (0-7). Like CR fields, FPSCR fields are
    /// 4-bit groups in PowerPC MSB=0 order.
    pub fn fpscr_field(&self, n: u8) -> u8 {
        debug_assert!(n < 8, "FPSCR field index must be in 0..8");
        let shift = 28 - (n as u32) * 4;
        ((self.fpscr >> shift) & 0x0F) as u8
    }

    /// Write FPSCR field N (0-7). `value` low-4 bits are stored.
    pub fn set_fpscr_field(&mut self, n: u8, value: u8) {
        debug_assert!(n < 8, "FPSCR field index must be in 0..8");
        let shift = 28 - (n as u32) * 4;
        let mask = 0x0F_u32 << shift;
        self.fpscr = (self.fpscr & !mask) | ((u32::from(value) & 0x0F) << shift);
    }

    /// Read one FPSCR bit by ISA MSB=0 bit index.
    pub fn fpscr_bit(&self, bit_index: u8) -> bool {
        debug_assert!(bit_index < 32);
        ((self.fpscr >> (31 - bit_index)) & 1) != 0
    }

    /// Write one FPSCR bit by ISA MSB=0 bit index.
    pub fn set_fpscr_bit(&mut self, bit_index: u8, value: bool) {
        debug_assert!(bit_index < 32);
        let mask = 1u32 << (31 - bit_index);
        if value {
            self.fpscr |= mask;
        } else {
            self.fpscr &= !mask;
        }
    }

    /// Read one MSR bit by ISA MSB=0 bit index.
    pub fn msr_bit(&self, bit_index: u8) -> bool {
        debug_assert!(bit_index < 32);
        ((self.msr >> (31 - bit_index)) & 1) != 0
    }

    /// Write one MSR bit by ISA MSB=0 bit index.
    pub fn set_msr_bit(&mut self, bit_index: u8, value: bool) {
        debug_assert!(bit_index < 32);
        let mask = 1u32 << (31 - bit_index);
        if value {
            self.msr |= mask;
        } else {
            self.msr &= !mask;
        }
    }

    /// Whether the MSR floating-point facility bit is enabled. If
    /// clear, decoded FP instructions surface
    /// [`PpcException::FloatingPointUnavailable`] before execution.
    pub fn msr_fp_available(&self) -> bool {
        (self.msr & PPC_MSR_FP_AVAILABLE_MASK) != 0
    }

    /// Enable or disable the MSR floating-point facility bit.
    pub fn set_msr_fp_available(&mut self, available: bool) {
        self.set_msr_bit(PPC_MSR_FP_AVAILABLE_BIT, available);
    }

    /// Set or clear the XER carry flag (CA) at MSB=0 bit 2 →
    /// host bit 29. Per ISA Book I §3.2.2.
    pub fn set_xer_ca(&mut self, ca: bool) {
        if ca {
            self.xer |= XER_CA_MASK;
        } else {
            self.xer &= !XER_CA_MASK;
        }
    }

    /// Read the current XER carry flag (CA) as a bool.
    pub fn xer_ca(&self) -> bool {
        (self.xer & XER_CA_MASK) != 0
    }

    /// Read the current XER overflow flag (OV) as a bool.
    pub fn xer_ov(&self) -> bool {
        (self.xer & XER_OV_MASK) != 0
    }

    /// Read the current XER summary-overflow flag (SO) as a bool.
    pub fn xer_so(&self) -> bool {
        (self.xer & XER_SO_MASK) != 0
    }

    fn set_xer_ov_so(&mut self, overflow: bool) {
        if overflow {
            self.xer |= XER_OV_MASK | XER_SO_MASK;
        } else {
            self.xer &= !XER_OV_MASK;
        }
    }

    /// Evaluate a `bc`/`bclr`/`bcctr` BO/BI selector and return
    /// whether the branch should be taken. Per ISA Book I §2.4.1
    /// Figure 21:
    ///
    /// ```text
    ///   BO[0] (mask 0x10) — ignore CR bit ("branch always" or
    ///                       "CTR-only" patterns)
    ///   BO[1] (mask 0x08) — CR-bit value to match (when BO[0]=0)
    ///   BO[2] (mask 0x04) — when 1, no CTR decrement
    ///   BO[3] (mask 0x02) — CTR sense (when CTR-decrement mode):
    ///                       0 = branch if CTR != 0 after decrement,
    ///                       1 = branch if CTR == 0
    ///   BO[4] (mask 0x01) — `t` hint bit (ignored)
    /// ```
    ///
    /// `bcctr` MUST have BO[2]=1 (no CTR decrement) per the spec
    /// — decrementing CTR while branching to it is undefined.
    /// The caller is responsible for surfacing that as an error.
    fn evaluate_branch_condition(&mut self, bo: u8, bi: u8) -> bool {
        let ignore_cr = (bo & 0x10) != 0;
        let cr_match_value = (bo & 0x08) != 0;
        let no_ctr_decrement = (bo & 0x04) != 0;

        let ctr_ok = if no_ctr_decrement {
            true
        } else {
            self.ctr = self.ctr.wrapping_sub(1);
            let dec_eq_zero = (bo & 0x02) != 0;
            let ctr_zero = self.ctr == 0;
            if dec_eq_zero {
                ctr_zero
            } else {
                !ctr_zero
            }
        };

        let cond_ok = if ignore_cr {
            true
        } else {
            self.cr_bit(bi) == cr_match_value
        };

        ctr_ok && cond_ok
    }

    /// Compute `a + b + carry_in`, returning the 32-bit result
    /// and the carry-out. Used by every addition / subtraction
    /// that needs to track XER.CA (addic, addc, adde, subfic,
    /// subfc, subfe). PowerPC subtraction is encoded as
    /// `~RA + RB + 1` (or `+ CA` for the extended forms), so the
    /// same helper drives both directions — the caller passes
    /// the bit-inverted operand and the appropriate carry-in.
    fn add_with_carry(a: u32, b: u32, carry_in: bool) -> (u32, bool) {
        let (s1, c1) = a.overflowing_add(b);
        let (s2, c2) = s1.overflowing_add(if carry_in { 1 } else { 0 });
        (s2, c1 || c2)
    }

    fn signed_add_overflow(a: u32, b: u32, carry_in: bool) -> bool {
        let sum = i64::from(a as i32) + i64::from(b as i32) + if carry_in { 1 } else { 0 };
        sum < i64::from(i32::MIN) || sum > i64::from(i32::MAX)
    }

    fn signed_sub_overflow(minuend: u32, subtrahend: u32, borrow: bool) -> bool {
        let difference =
            i64::from(minuend as i32) - i64::from(subtrahend as i32) - if borrow { 1 } else { 0 };
        difference < i64::from(i32::MIN) || difference > i64::from(i32::MAX)
    }

    fn trap_condition(to: u8, left: u32, right: u32) -> bool {
        let left_signed = left as i32;
        let right_signed = right as i32;
        ((to & 0x10) != 0 && left_signed < right_signed)
            || ((to & 0x08) != 0 && left_signed > right_signed)
            || ((to & 0x04) != 0 && left == right)
            || ((to & 0x02) != 0 && left < right)
            || ((to & 0x01) != 0 && left > right)
    }

    fn d_form_ea(&self, ra: u8, d: i16) -> u32 {
        let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
        base.wrapping_add(i32::from(d) as u32)
    }

    fn x_form_ea(&self, ra: u8, rb: u8) -> u32 {
        let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
        base.wrapping_add(self.gpr[rb as usize])
    }

    fn update_form_ea(&self, ra: u8, d: i16) -> u32 {
        self.gpr[ra as usize].wrapping_add(i32::from(d) as u32)
    }

    fn update_indexed_ea(&self, ra: u8, rb: u8) -> u32 {
        self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize])
    }

    fn alignment_exception(addr: u32, size: u8, access: PpcMemoryAccess) -> Option<PpcException> {
        debug_assert!(
            size.is_power_of_two(),
            "alignment size must be a power of two"
        );
        if size <= 1 || (addr & (u32::from(size) - 1)) == 0 {
            None
        } else {
            Some(PpcException::Alignment { addr, size, access })
        }
    }

    fn load_store_alignment_exception(&self, instr: PpcInstr) -> Option<PpcException> {
        match instr {
            PpcInstr::Lhz { ra, d, .. } | PpcInstr::Lha { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 2, PpcMemoryAccess::Load)
            }
            PpcInstr::Sth { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 2, PpcMemoryAccess::Store)
            }
            PpcInstr::Lwz { ra, d, .. }
            | PpcInstr::Lfs { ra, d, .. }
            | PpcInstr::Lmw { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Stw { ra, d, .. } | PpcInstr::Stfs { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Stmw { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Lfd { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 8, PpcMemoryAccess::Load)
            }
            PpcInstr::Stfd { ra, d, .. } => {
                Self::alignment_exception(self.d_form_ea(ra, d), 8, PpcMemoryAccess::Store)
            }
            PpcInstr::Lwzu { rt, ra, d } if ra != 0 && ra != rt => {
                Self::alignment_exception(self.update_form_ea(ra, d), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Lfsu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Lhzu { rt, ra, d } | PpcInstr::Lhau { rt, ra, d } if ra != 0 && ra != rt => {
                Self::alignment_exception(self.update_form_ea(ra, d), 2, PpcMemoryAccess::Load)
            }
            PpcInstr::Stwu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Stfsu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Sthu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 2, PpcMemoryAccess::Store)
            }
            PpcInstr::Lfdu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 8, PpcMemoryAccess::Load)
            }
            PpcInstr::Stfdu { ra, d, .. } if ra != 0 => {
                Self::alignment_exception(self.update_form_ea(ra, d), 8, PpcMemoryAccess::Store)
            }
            PpcInstr::Lwzx { ra, rb, .. }
            | PpcInstr::Lwarx { ra, rb, .. }
            | PpcInstr::Lwbrx { ra, rb, .. }
            | PpcInstr::Lfsx { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Lhzx { ra, rb, .. }
            | PpcInstr::Lhbrx { ra, rb, .. }
            | PpcInstr::Lhax { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 2, PpcMemoryAccess::Load)
            }
            PpcInstr::Stwx { ra, rb, .. }
            | PpcInstr::Stwcx { ra, rb, .. }
            | PpcInstr::Stwbrx { ra, rb, .. }
            | PpcInstr::Stfsx { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Sthx { ra, rb, .. } | PpcInstr::Sthbrx { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 2, PpcMemoryAccess::Store)
            }
            PpcInstr::Lfdx { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 8, PpcMemoryAccess::Load)
            }
            PpcInstr::Stfdx { ra, rb, .. } => {
                Self::alignment_exception(self.x_form_ea(ra, rb), 8, PpcMemoryAccess::Store)
            }
            PpcInstr::Lwzux { rt, ra, rb } if ra != 0 && ra != rt => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Stwux { ra, rb, .. } if ra != 0 => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Lfsux { ra, rb, .. } if ra != 0 => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 4, PpcMemoryAccess::Load)
            }
            PpcInstr::Lfdux { ra, rb, .. } if ra != 0 => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 8, PpcMemoryAccess::Load)
            }
            PpcInstr::Stfsux { ra, rb, .. } if ra != 0 => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 4, PpcMemoryAccess::Store)
            }
            PpcInstr::Stfdux { ra, rb, .. } if ra != 0 => {
                Self::alignment_exception(self.update_indexed_ea(ra, rb), 8, PpcMemoryAccess::Store)
            }
            _ => None,
        }
    }

    fn may_require_data_alignment_check(instr_word: u32) -> bool {
        let primary = ((instr_word >> 26) & 0x3f) as u8;
        matches!(
            primary,
            // Indexed integer/FP loads and stores share primary 31 with many
            // non-memory integer ops, so keep the detailed decoded match there.
            // D-form integer and floating-point loads/stores follow the
            // indexed integer/FP family at primary opcode 31.
            31..=55
        )
    }

    fn sign_extend_u32(value: u32, bits: u8) -> u32 {
        debug_assert!((1..=32).contains(&bits));
        let shift = 32 - u32::from(bits);
        (((value << shift) as i32) >> shift) as u32
    }

    fn step_fast_unobserved<M: PpcMemory + ?Sized>(
        &mut self,
        mem: &mut M,
        instr_word: u32,
    ) -> Option<PpcStepResult> {
        match instr_word {
            // Canonical `nop` (`ori r0,r0,0`) and `blr` are both
            // extremely common in CFM-heavy Mac PPC code. Handle the
            // exact encodings before the broader primary/XO dispatch.
            0x6000_0000 => {
                self.pc = self.pc.wrapping_add(4);
                return Some(PpcStepResult::Stepped);
            }
            0x4E80_0020 => {
                self.pc = self.lr & !0x3;
                return Some(PpcStepResult::Stepped);
            }
            _ => {}
        }

        let primary = (instr_word >> 26) & 0x3f;
        match primary {
            10 => {
                let bf = ((instr_word >> 23) & 0x7) as u8;
                let l = ((instr_word >> 21) & 0x1) != 0;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let ui = instr_word as u16;
                if l {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                self.set_cr_compare(bf, self.gpr[ra].cmp(&u32::from(ui)));
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            11 => {
                let bf = ((instr_word >> 23) & 0x7) as u8;
                let l = ((instr_word >> 21) & 0x1) != 0;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let si = instr_word as u16 as i16;
                if l {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                self.set_cr_compare(bf, (self.gpr[ra] as i32).cmp(&i32::from(si)));
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            14 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let si = instr_word as u16 as i16;
                let lhs = if ra == 0 { 0 } else { self.gpr[ra] };
                self.gpr[rt] = lhs.wrapping_add(i32::from(si) as u32);
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            15 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let si = instr_word as u16 as i16;
                let lhs = if ra == 0 { 0 } else { self.gpr[ra] };
                self.gpr[rt] = lhs.wrapping_add((i32::from(si) as u32) << 16);
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            16 => {
                let bo = ((instr_word >> 21) & 0x1f) as u8;
                let bi = ((instr_word >> 16) & 0x1f) as u8;
                let displacement = Self::sign_extend_u32(instr_word & 0xfffc, 16);
                let aa = (instr_word & 0x2) != 0;
                let lk = (instr_word & 0x1) != 0;
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    let target = if aa {
                        displacement
                    } else {
                        self.pc.wrapping_add(displacement)
                    };
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
                Some(PpcStepResult::Stepped)
            }
            18 => {
                let displacement = Self::sign_extend_u32(instr_word & 0x03ff_fffc, 26);
                let aa = (instr_word & 0x2) != 0;
                let lk = (instr_word & 0x1) != 0;
                let next_after = self.pc.wrapping_add(4);
                let target = if aa {
                    displacement
                } else {
                    self.pc.wrapping_add(displacement)
                };
                if lk {
                    self.lr = next_after;
                }
                self.pc = target;
                Some(PpcStepResult::Stepped)
            }
            19 if ((instr_word >> 1) & 0x3ff) == 16 => {
                let bo = ((instr_word >> 21) & 0x1f) as u8;
                let bi = ((instr_word >> 16) & 0x1f) as u8;
                let lk = (instr_word & 0x1) != 0;
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    let target = self.lr & !0x3;
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
                Some(PpcStepResult::Stepped)
            }
            19 if ((instr_word >> 1) & 0x3ff) == 528 => {
                let bo = ((instr_word >> 21) & 0x1f) as u8;
                let bi = ((instr_word >> 16) & 0x1f) as u8;
                let lk = (instr_word & 0x1) != 0;
                if (bo & 0x04) == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    let target = self.ctr & !0x3;
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
                Some(PpcStepResult::Stepped)
            }
            20 | 21 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let sh = ((instr_word >> 11) & 0x1f) as u8;
                let mb = ((instr_word >> 6) & 0x1f) as u8;
                let me = ((instr_word >> 1) & 0x1f) as u8;
                let rc = (instr_word & 0x1) != 0;
                let mask = Self::mask32(mb, me);
                let rotated = self.gpr[rs].rotate_left(u32::from(sh));
                let result = if primary == 20 {
                    (rotated & mask) | (self.gpr[ra] & !mask)
                } else {
                    rotated & mask
                };
                self.gpr[ra] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            24..=27 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let ui = instr_word as u16;
                let imm = if primary == 25 || primary == 27 {
                    u32::from(ui) << 16
                } else {
                    u32::from(ui)
                };
                self.gpr[ra] = if primary == 26 || primary == 27 {
                    self.gpr[rs] ^ imm
                } else {
                    self.gpr[rs] | imm
                };
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            31 => {
                let xo = (instr_word >> 1) & 0x3ff;
                let rs_or_rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let rb = ((instr_word >> 11) & 0x1f) as usize;
                let rc = (instr_word & 0x1) != 0;
                match xo {
                    0 => {
                        let bf = ((instr_word >> 23) & 0x7) as u8;
                        let l = ((instr_word >> 21) & 0x1) != 0;
                        if l {
                            return Some(Self::illegal_instruction_result(
                                instr_word,
                                PpcIllegalInstructionReason::InvalidForm,
                            ));
                        }
                        self.set_cr_compare(bf, (self.gpr[ra] as i32).cmp(&(self.gpr[rb] as i32)));
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    28 | 316 | 444 => {
                        let lhs = self.gpr[rs_or_rt];
                        let rhs = self.gpr[rb];
                        let result = match xo {
                            28 => lhs & rhs,
                            316 => lhs ^ rhs,
                            444 => lhs | rhs,
                            _ => unreachable!(),
                        };
                        self.gpr[ra] = result;
                        if rc {
                            self.update_cr0_from_signed(result);
                        }
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    23 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 4,
                                access: PpcMemoryAccess::Load,
                            }));
                        }
                        match mem.read_u32_be(addr) {
                            Some(value) => {
                                self.gpr[rs_or_rt] = value;
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: false,
                            }),
                        }
                    }
                    87 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        match mem.read_u8(addr) {
                            Some(value) => {
                                self.gpr[rs_or_rt] = u32::from(value);
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: false,
                            }),
                        }
                    }
                    32 => {
                        let bf = ((instr_word >> 23) & 0x7) as u8;
                        let l = ((instr_word >> 21) & 0x1) != 0;
                        if l {
                            return Some(Self::illegal_instruction_result(
                                instr_word,
                                PpcIllegalInstructionReason::InvalidForm,
                            ));
                        }
                        self.set_cr_compare(bf, self.gpr[ra].cmp(&self.gpr[rb]));
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    55 => {
                        if ra == 0 || ra == rs_or_rt {
                            return Some(Self::illegal_instruction_result(
                                instr_word,
                                PpcIllegalInstructionReason::InvalidForm,
                            ));
                        }
                        let addr = self.gpr[ra].wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 4,
                                access: PpcMemoryAccess::Load,
                            }));
                        }
                        match mem.read_u32_be(addr) {
                            Some(value) => {
                                self.gpr[rs_or_rt] = value;
                                self.gpr[ra] = addr;
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: false,
                            }),
                        }
                    }
                    151 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 4,
                                access: PpcMemoryAccess::Store,
                            }));
                        }
                        match mem.write_u32_be(addr, self.gpr[rs_or_rt]) {
                            Some(()) => {
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: true,
                            }),
                        }
                    }
                    183 => {
                        if ra == 0 {
                            return Some(Self::illegal_instruction_result(
                                instr_word,
                                PpcIllegalInstructionReason::InvalidForm,
                            ));
                        }
                        let addr = self.gpr[ra].wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 4,
                                access: PpcMemoryAccess::Store,
                            }));
                        }
                        match mem.write_u32_be(addr, self.gpr[rs_or_rt]) {
                            Some(()) => {
                                self.gpr[ra] = addr;
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: true,
                            }),
                        }
                    }
                    215 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        match mem.write_u8(addr, (self.gpr[rs_or_rt] & 0xff) as u8) {
                            Some(()) => {
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: true,
                            }),
                        }
                    }
                    266 | 778 => {
                        let overflow = Self::signed_add_overflow(self.gpr[ra], self.gpr[rb], false);
                        let result = self.gpr[ra].wrapping_add(self.gpr[rb]);
                        self.gpr[rs_or_rt] = result;
                        if xo == 778 {
                            self.set_xer_ov_so(overflow);
                        }
                        if rc {
                            self.update_cr0_from_signed(result);
                        }
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    279 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 2,
                                access: PpcMemoryAccess::Load,
                            }));
                        }
                        match mem.read_u16_be(addr) {
                            Some(value) => {
                                self.gpr[rs_or_rt] = u32::from(value);
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: false,
                            }),
                        }
                    }
                    339 => {
                        let spr = (((instr_word >> 11) & 0x1f) << 5) | ((instr_word >> 16) & 0x1f);
                        let value = match spr {
                            1 => self.xer,
                            8 => self.lr,
                            9 => self.ctr,
                            _ => {
                                return Some(PpcStepResult::Unimplemented(
                                    PpcDecodeError::UnsupportedSecondaryOpcode {
                                        primary: 31,
                                        secondary: 339,
                                    },
                                ));
                            }
                        };
                        self.gpr[rs_or_rt] = value;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    467 => {
                        let spr = (((instr_word >> 11) & 0x1f) << 5) | ((instr_word >> 16) & 0x1f);
                        let value = self.gpr[rs_or_rt];
                        match spr {
                            1 => self.xer = value,
                            8 => self.lr = value,
                            9 => self.ctr = value,
                            _ => {
                                return Some(PpcStepResult::Unimplemented(
                                    PpcDecodeError::UnsupportedSecondaryOpcode {
                                        primary: 31,
                                        secondary: 467,
                                    },
                                ));
                            }
                        }
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    407 => {
                        let base = if ra == 0 { 0 } else { self.gpr[ra] };
                        let addr = base.wrapping_add(self.gpr[rb]);
                        if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                            return Some(PpcStepResult::Exception(PpcException::Alignment {
                                addr,
                                size: 2,
                                access: PpcMemoryAccess::Store,
                            }));
                        }
                        match mem.write_u16_be(addr, (self.gpr[rs_or_rt] & 0xffff) as u16) {
                            Some(()) => {
                                self.pc = self.pc.wrapping_add(4);
                                Some(PpcStepResult::Stepped)
                            }
                            None => Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: true,
                            }),
                        }
                    }
                    792 => {
                        let n = self.gpr[rb] & 0x3f;
                        let signed = self.gpr[rs_or_rt] as i32;
                        let (result, ca) = if n == 0 {
                            (signed as u32, false)
                        } else if n >= 32 {
                            ((signed >> 31) as u32, signed < 0)
                        } else {
                            let bits_lost = self.gpr[rs_or_rt] & ((1u32 << n) - 1);
                            ((signed >> n) as u32, signed < 0 && bits_lost != 0)
                        };
                        self.gpr[ra] = result;
                        self.set_xer_ca(ca);
                        if rc {
                            self.update_cr0_from_signed(result);
                        }
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    _ => None,
                }
            }
            32 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                match mem.read_u32_be(addr) {
                    Some(value) => {
                        self.gpr[rt] = value;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            33 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 || ra == rt {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                match mem.read_u32_be(addr) {
                    Some(value) => {
                        self.gpr[rt] = value;
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            34 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                match mem.read_u8(addr) {
                    Some(value) => {
                        self.gpr[rt] = u32::from(value);
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            35 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 || ra == rt {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                match mem.read_u8(addr) {
                    Some(value) => {
                        self.gpr[rt] = u32::from(value);
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            36 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Store,
                    }));
                }
                match mem.write_u32_be(addr, self.gpr[rs]) {
                    Some(()) => {
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            37 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Store,
                    }));
                }
                match mem.write_u32_be(addr, self.gpr[rs]) {
                    Some(()) => {
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            38 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                match mem.write_u8(addr, (self.gpr[rs] & 0xff) as u8) {
                    Some(()) => {
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            39 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                match mem.write_u8(addr, (self.gpr[rs] & 0xff) as u8) {
                    Some(()) => {
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            40 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 2,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                match mem.read_u16_be(addr) {
                    Some(value) => {
                        self.gpr[rt] = u32::from(value);
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            41 | 43 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 || ra == rt {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 2,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                match mem.read_u16_be(addr) {
                    Some(value) => {
                        self.gpr[rt] = if primary == 43 {
                            (value as i16) as i32 as u32
                        } else {
                            u32::from(value)
                        };
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            42 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 2,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                match mem.read_u16_be(addr) {
                    Some(value) => {
                        self.gpr[rt] = (value as i16) as i32 as u32;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: false,
                    }),
                }
            }
            44 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 2,
                        access: PpcMemoryAccess::Store,
                    }));
                }
                match mem.write_u16_be(addr, (self.gpr[rs] & 0xffff) as u16) {
                    Some(()) => {
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            45 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x1) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 2,
                        access: PpcMemoryAccess::Store,
                    }));
                }
                match mem.write_u16_be(addr, (self.gpr[rs] & 0xffff) as u16) {
                    Some(()) => {
                        self.gpr[ra] = addr;
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    None => Some(PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    }),
                }
            }
            46 => {
                let rt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra >= rt {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let mut addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Load,
                    }));
                }
                for reg in rt..=31 {
                    match mem.read_u32_be(addr) {
                        Some(value) => {
                            self.gpr[reg] = value;
                            addr = addr.wrapping_add(4);
                        }
                        None => {
                            return Some(PpcStepResult::MemoryFault {
                                addr,
                                was_write: false,
                            });
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            47 => {
                let rs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let mut addr = base.wrapping_add(i32::from(d) as u32);
                if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                    return Some(PpcStepResult::Exception(PpcException::Alignment {
                        addr,
                        size: 4,
                        access: PpcMemoryAccess::Store,
                    }));
                }
                for reg in rs..=31 {
                    if mem.write_u32_be(addr, self.gpr[reg]).is_none() {
                        return Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        });
                    }
                    addr = addr.wrapping_add(4);
                }
                self.pc = self.pc.wrapping_add(4);
                Some(PpcStepResult::Stepped)
            }
            48 | 50 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let frt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if primary == 48 {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 4,
                            access: PpcMemoryAccess::Load,
                        }));
                    }
                    match mem.read_u32_be(addr) {
                        Some(bits32) => {
                            self.fpr[frt] = (f32::from_bits(bits32) as f64).to_bits();
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }),
                    }
                } else {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x7) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 8,
                            access: PpcMemoryAccess::Load,
                        }));
                    }
                    match mem.read_u64_be(addr) {
                        Some(bits) => {
                            self.fpr[frt] = bits;
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }),
                    }
                }
            }
            49 | 51 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let frt = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if primary == 49 {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 4,
                            access: PpcMemoryAccess::Load,
                        }));
                    }
                    match mem.read_u32_be(addr) {
                        Some(bits32) => {
                            self.fpr[frt] = (f32::from_bits(bits32) as f64).to_bits();
                            self.gpr[ra] = addr;
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }),
                    }
                } else {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x7) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 8,
                            access: PpcMemoryAccess::Load,
                        }));
                    }
                    match mem.read_u64_be(addr) {
                        Some(bits) => {
                            self.fpr[frt] = bits;
                            self.gpr[ra] = addr;
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }),
                    }
                }
            }
            52 | 54 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let frs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                let base = if ra == 0 { 0 } else { self.gpr[ra] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                if primary == 52 {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 4,
                            access: PpcMemoryAccess::Store,
                        }));
                    }
                    let double = f64::from_bits(self.fpr[frs]);
                    match mem.write_u32_be(addr, (double as f32).to_bits()) {
                        Some(()) => {
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        }),
                    }
                } else {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x7) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 8,
                            access: PpcMemoryAccess::Store,
                        }));
                    }
                    match mem.write_u64_be(addr, self.fpr[frs]) {
                        Some(()) => {
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        }),
                    }
                }
            }
            53 | 55 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let frs = ((instr_word >> 21) & 0x1f) as usize;
                let ra = ((instr_word >> 16) & 0x1f) as usize;
                let d = instr_word as u16 as i16;
                if ra == 0 {
                    return Some(Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    ));
                }
                let addr = self.gpr[ra].wrapping_add(i32::from(d) as u32);
                if primary == 53 {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x3) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 4,
                            access: PpcMemoryAccess::Store,
                        }));
                    }
                    let double = f64::from_bits(self.fpr[frs]);
                    match mem.write_u32_be(addr, (double as f32).to_bits()) {
                        Some(()) => {
                            self.gpr[ra] = addr;
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        }),
                    }
                } else {
                    if self.alignment_policy == PpcAlignmentPolicy::Trap && (addr & 0x7) != 0 {
                        return Some(PpcStepResult::Exception(PpcException::Alignment {
                            addr,
                            size: 8,
                            access: PpcMemoryAccess::Store,
                        }));
                    }
                    match mem.write_u64_be(addr, self.fpr[frs]) {
                        Some(()) => {
                            self.gpr[ra] = addr;
                            self.pc = self.pc.wrapping_add(4);
                            Some(PpcStepResult::Stepped)
                        }
                        None => Some(PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        }),
                    }
                }
            }
            59 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let xo = (instr_word >> 1) & 0x1f;
                let frt = ((instr_word >> 21) & 0x1f) as usize;
                let fra = ((instr_word >> 16) & 0x1f) as usize;
                let frb = ((instr_word >> 11) & 0x1f) as usize;
                let frc = ((instr_word >> 6) & 0x1f) as usize;
                let rc = (instr_word & 0x1) != 0;
                let result = match xo {
                    20 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        (a - b) as f32
                    }
                    21 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        (a + b) as f32
                    }
                    25 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let c = f64::from_bits(self.fpr[frc]);
                        (a * c) as f32
                    }
                    28 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        let c = f64::from_bits(self.fpr[frc]);
                        a.mul_add(c, -b) as f32
                    }
                    29 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        let c = f64::from_bits(self.fpr[frc]);
                        a.mul_add(c, b) as f32
                    }
                    30 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        let c = f64::from_bits(self.fpr[frc]);
                        (-a.mul_add(c, -b)) as f32
                    }
                    31 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        let c = f64::from_bits(self.fpr[frc]);
                        (-a.mul_add(c, b)) as f32
                    }
                    _ => return None,
                };
                self.finish_fp_result(frt as u8, (result as f64).to_bits(), rc);
                Some(PpcStepResult::Stepped)
            }
            63 => {
                if !self.msr_fp_available() {
                    return Some(PpcStepResult::Exception(
                        PpcException::FloatingPointUnavailable,
                    ));
                }
                let xo = (instr_word >> 1) & 0x3ff;
                let frt = ((instr_word >> 21) & 0x1f) as u8;
                let bf = ((instr_word >> 23) & 0x7) as u8;
                let fra = ((instr_word >> 16) & 0x1f) as usize;
                let frb = ((instr_word >> 11) & 0x1f) as usize;
                let rc = (instr_word & 0x1) != 0;
                match xo {
                    0 | 32 => {
                        let a = f64::from_bits(self.fpr[fra]);
                        let b = f64::from_bits(self.fpr[frb]);
                        let nibble: u8 = if a.is_nan() || b.is_nan() {
                            0b0001
                        } else if a < b {
                            0b1000
                        } else if a > b {
                            0b0100
                        } else {
                            0b0010
                        };
                        self.set_cr_field(bf, nibble);
                        self.set_fpscr_compare_result(nibble);
                        self.pc = self.pc.wrapping_add(4);
                        Some(PpcStepResult::Stepped)
                    }
                    72 => {
                        self.finish_fp_result(frt, self.fpr[frb], rc);
                        Some(PpcStepResult::Stepped)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn is_floating_point_instruction(instr: PpcInstr) -> bool {
        matches!(
            instr,
            PpcInstr::Lfs { .. }
                | PpcInstr::Lfsu { .. }
                | PpcInstr::Lfd { .. }
                | PpcInstr::Lfdu { .. }
                | PpcInstr::Stfs { .. }
                | PpcInstr::Stfsu { .. }
                | PpcInstr::Stfd { .. }
                | PpcInstr::Stfdu { .. }
                | PpcInstr::Lfsx { .. }
                | PpcInstr::Lfsux { .. }
                | PpcInstr::Lfdx { .. }
                | PpcInstr::Lfdux { .. }
                | PpcInstr::Stfsx { .. }
                | PpcInstr::Stfsux { .. }
                | PpcInstr::Stfdx { .. }
                | PpcInstr::Stfdux { .. }
                | PpcInstr::Fadd { .. }
                | PpcInstr::Fsub { .. }
                | PpcInstr::Fmul { .. }
                | PpcInstr::Fdiv { .. }
                | PpcInstr::Fneg { .. }
                | PpcInstr::Fmr { .. }
                | PpcInstr::Fabs { .. }
                | PpcInstr::Mffs { .. }
                | PpcInstr::Mcrfs { .. }
                | PpcInstr::Mtfsb1 { .. }
                | PpcInstr::Mtfsb0 { .. }
                | PpcInstr::Mtfsfi { .. }
                | PpcInstr::Mtfsf { .. }
                | PpcInstr::Fadds { .. }
                | PpcInstr::Fsubs { .. }
                | PpcInstr::Fmuls { .. }
                | PpcInstr::Fdivs { .. }
                | PpcInstr::Fcmpo { .. }
                | PpcInstr::Fcmpu { .. }
                | PpcInstr::Fmadd { .. }
                | PpcInstr::Fmsub { .. }
                | PpcInstr::Fnmadd { .. }
                | PpcInstr::Fnmsub { .. }
                | PpcInstr::Fmadds { .. }
                | PpcInstr::Fmsubs { .. }
                | PpcInstr::Fnmadds { .. }
                | PpcInstr::Fnmsubs { .. }
                | PpcInstr::Frsp { .. }
                | PpcInstr::Fctiw { .. }
                | PpcInstr::Fctiwz { .. }
                | PpcInstr::Fsqrt { .. }
                | PpcInstr::Fsqrts { .. }
                | PpcInstr::Fres { .. }
                | PpcInstr::Frsqrte { .. }
                | PpcInstr::Fnabs { .. }
                | PpcInstr::Fsel { .. }
        )
    }

    fn illegal_instruction_result(word: u32, reason: PpcIllegalInstructionReason) -> PpcStepResult {
        PpcStepResult::Exception(PpcException::IllegalInstruction { word, reason })
    }

    fn decode_error_illegal_instruction(word: u32, error: PpcDecodeError) -> Option<PpcException> {
        match error {
            PpcDecodeError::UnsupportedPrimaryOpcode(0) => Some(PpcException::IllegalInstruction {
                word,
                reason: PpcIllegalInstructionReason::ReservedOpcode,
            }),
            _ => None,
        }
    }

    fn decode_cached(&mut self, instr_word: u32) -> Result<PpcInstr, PpcDecodeError> {
        let index = Self::decode_cache_index(instr_word);
        if let Some((cached_word, decoded)) = self.decode_cache[index] {
            if cached_word == instr_word {
                return decoded;
            }
        }
        let decoded = decode(instr_word);
        self.decode_cache[index] = Some((instr_word, decoded));
        decoded
    }

    fn decode_cache_index(instr_word: u32) -> usize {
        let mixed = instr_word ^ (instr_word >> 12) ^ (instr_word >> 24);
        (mixed as usize) & PPC_DECODE_CACHE_INDEX_MASK
    }

    #[cfg(test)]
    fn decode_cache_entry_count(&self) -> usize {
        self.decode_cache
            .iter()
            .filter(|entry| entry.is_some())
            .count()
    }

    /// Generate a 32-bit mask covering bit positions `mb..me`
    /// in PowerPC's MSB=0 bit numbering (bit 0 = MSB,
    /// bit 31 = LSB). When `mb <= me` the mask is contiguous;
    /// when `mb > me` it wraps around (bits `mb..31` plus
    /// `0..me`). Used by `rlwinm` per ISA Book I §3.3.12.1.
    pub fn mask32(mb: u8, me: u8) -> u32 {
        let mb = mb & 0x1F;
        let me = me & 0x1F;
        let left = u32::MAX >> u32::from(mb);
        let right = u32::MAX << (31 - u32::from(me));
        if mb <= me {
            left & right
        } else {
            left | right
        }
    }

    /// Update CR field `bf` from the result of a fixed-point
    /// compare instruction (`cmp`/`cmpi`/`cmpl`/`cmpli`), per ISA
    /// Book I §3.3.9. The four nibble bits are LT/GT/EQ/SO with
    /// SO copied from XER bit 0 (MSB=0).
    fn set_cr_compare(&mut self, bf: u8, ordering: core::cmp::Ordering) {
        let so = ((self.xer >> 31) & 1) as u8;
        let mut nibble = match ordering {
            core::cmp::Ordering::Less => 0b1000,
            core::cmp::Ordering::Greater => 0b0100,
            core::cmp::Ordering::Equal => 0b0010,
        };
        nibble |= so;
        self.set_cr_field(bf, nibble);
    }

    /// Read one bit of the Condition Register, where `bit_index`
    /// follows ISA Book I MSB=0 numbering (bit 0 = high bit of
    /// `self.cr`). Used by `bc` to test CRBI.
    pub fn cr_bit(&self, bit_index: u8) -> bool {
        debug_assert!(bit_index < 32);
        ((self.cr >> (31 - bit_index)) & 1) != 0
    }

    /// Write one bit of the Condition Register at MSB=0
    /// `bit_index`. Used by the CR-logical mnemonics
    /// (`crand`/`cror`/`crxor`/...).
    pub fn set_cr_bit(&mut self, bit_index: u8, value: bool) {
        debug_assert!(bit_index < 32);
        let mask = 1u32 << (31 - bit_index);
        if value {
            self.cr |= mask;
        } else {
            self.cr &= !mask;
        }
    }

    /// Update CR0 from a signed 32-bit comparison of `result` to
    /// zero, per ISA Book I §3.1.4. The four nibble bits are:
    ///
    /// ```text
    ///   bit 0 (MSB) — LT  (result < 0  signed)
    ///   bit 1       — GT  (result > 0  signed)
    ///   bit 2       — EQ  (result == 0)
    ///   bit 3 (LSB) — SO  (copy of XER bit 0 / MSB=0)
    /// ```
    ///
    /// Used by record-form (`.`) integer arithmetic instructions
    /// (`addic.`, `or.`, `add.`, `subf.`, `mulli.`/`mulli` not having
    /// a record form …) — anything whose mnemonic ends in `.`.
    fn update_cr0_from_signed(&mut self, result: u32) {
        // XER.SO is at MSB=0 bit 0 → host bit 31 of self.xer.
        let so = ((self.xer >> 31) & 1) as u8;
        let signed = result as i32;
        let mut nibble = 0u8;
        if signed < 0 {
            nibble |= 0b1000;
        } else if signed > 0 {
            nibble |= 0b0100;
        } else {
            nibble |= 0b0010;
        }
        nibble |= so;
        self.set_cr_field(0, nibble);
    }

    fn update_cr0_from_store_conditional(&mut self, success: bool) {
        let so = ((self.xer >> 31) & 1) as u8;
        let eq = if success { 0b0010 } else { 0 };
        self.set_cr_field(0, eq | so);
    }

    fn update_cr1_from_fpscr(&mut self) {
        // Record-form floating-point instructions copy FPSCR bits
        // 0..3 (FX/FEX/VX/OX) into CR1 bits 4..7.
        self.set_cr_field(1, self.fpscr_field(0));
    }

    fn set_fpscr_fprf(&mut self, class_descriptor: bool, condition: u8) {
        self.set_fpscr_bit(15, class_descriptor);
        self.set_fpscr_field(4, condition);
    }

    fn set_fpscr_result_from_f64_bits(&mut self, bits: u64) {
        let sign = (bits >> 63) != 0;
        let exponent = (bits >> 52) & 0x07FF;
        let fraction = bits & 0x000F_FFFF_FFFF_FFFF;

        let (class_descriptor, condition) = if exponent == 0 && fraction == 0 {
            (sign, 0b0010)
        } else if exponent == 0 {
            (true, if sign { 0b1000 } else { 0b0100 })
        } else if exponent == 0x07FF && fraction == 0 {
            (false, if sign { 0b1001 } else { 0b0101 })
        } else if exponent == 0x07FF {
            (true, 0b0001)
        } else {
            (false, if sign { 0b1000 } else { 0b0100 })
        };

        self.set_fpscr_fprf(class_descriptor, condition);
    }

    fn set_fpscr_compare_result(&mut self, condition: u8) {
        self.set_fpscr_fprf(false, condition);
    }

    fn finish_fp_result(&mut self, frt: u8, bits: u64, rc: bool) {
        self.fpr[frt as usize] = bits;
        self.set_fpscr_result_from_f64_bits(bits);
        if rc {
            self.update_cr1_from_fpscr();
        }
        self.pc = self.pc.wrapping_add(4);
    }

    fn finish_fp_record(&mut self, rc: bool) {
        if rc {
            self.update_cr1_from_fpscr();
        }
        self.pc = self.pc.wrapping_add(4);
    }

    fn f64_to_i32_with_rounding_mode(v: f64, rn: u8) -> i32 {
        if v.is_nan() {
            return 0;
        }
        let rounded = match rn & 0x03 {
            0 => v.round_ties_even(),
            1 => v.trunc(),
            2 => v.ceil(),
            _ => v.floor(),
        };
        if rounded >= i32::MAX as f64 {
            i32::MAX
        } else if rounded <= i32::MIN as f64 {
            i32::MIN
        } else {
            rounded as i32
        }
    }
}

mod memory;
use memory::NullMemory;
pub use memory::{PpcMemory, PpcSectionMem, PpcSectionMemSpan};

mod decode;
pub use decode::{decode, PpcDecodeError, PpcInstr};

const XER_SO_MASK: u32 = 1 << 31;
const XER_OV_MASK: u32 = 1 << 30;
const XER_CA_MASK: u32 = 1 << 29;
/// MSR bit 18 (`FP`) in PowerPC's MSB=0 numbering.
pub const PPC_MSR_FP_AVAILABLE_BIT: u8 = 18;
/// Host-order mask for MSR bit 18 (`FP`).
pub const PPC_MSR_FP_AVAILABLE_MASK: u32 = 1 << (31 - PPC_MSR_FP_AVAILABLE_BIT);

/// What [`PpcCpu::run_with_imports`] should do when control
/// reaches a synthetic import-trap address.
///
/// The dispatcher is given the import index and a mutable
/// reference to the CPU; it inspects argument registers
/// (GPR3..GPR10 per the PowerPC linkage convention), performs
/// any side effect it needs (e.g. writing to memory through the
/// CPU's bus), and returns one of these actions:
///
/// - `Return(value)` — overwrite GPR3 with `value` (the standard
///   PowerPC return-register slot for a 32-bit integer / pointer
///   /  OSErr return). The run loop then sets `pc = lr` and
///   continues, skipping the trampoline `blr` entirely.
/// - `ReturnPreserve` — return through LR without touching GPR3.
///   This is the right action for procedure imports with no result.
/// - `ReturnPreserveWithExtraCycles(extra)` — return like
///   `ReturnPreserve`, and also charge `extra` guest cycles for a
///   host-proven polling loop that can be skipped without changing
///   architectural state.
/// - `ReturnWithExtraCycles(value, extra)` — return like
///   `Return(value)`, and also charge `extra` guest cycles.
/// - `CallNative` — enter a guest routine from an import handler
///   while preserving the import caller's final return PC and RTOC.
/// - `Yield(cycles)` — leave the CPU stopped at the import slot and end the
///   current execution slice after at most `cycles` more guest cycles. This
///   lets a blocking Toolbox routine wait for clocks or host input without
///   falsely returning to its guest caller.
/// - `RaiseException` — stop the run at the import slot with a
///   structured [`PpcException`], without counting the import as a
///   completed cycle.
/// - `Halt` — terminate the run with [`PpcRunResult::Halted`].
///   The natural fit for `ExitToShell`, `_Debugger`, and other
///   "abort the application" routines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcImportAction {
    /// Synthesize a return: r3 ← `value`, pc ← lr.
    Return(u32),
    /// Synthesize a procedure return: pc ← lr, leaving r3 as the
    /// guest set it before the import call.
    ReturnPreserve,
    /// Synthesize a procedure return and charge additional guest
    /// cycles for repeated polling work that the host skipped.
    ReturnPreserveWithExtraCycles(u64),
    /// Synthesize a value return and charge additional guest cycles
    /// for repeated polling work that the host skipped.
    ReturnWithExtraCycles(u32, u64),
    /// End the current execution slice without changing the PC or registers,
    /// consuming at most this many additional guest cycles. Use `u64::MAX`
    /// when the service can advance only after external input.
    Yield(u64),
    /// Enter native PowerPC guest code and return through
    /// `return_pc`. When the callback reaches `return_pc`, the run
    /// loop applies `return_gpr3`, restores `gpr2` to
    /// `restore_rtoc`, and resumes at `final_pc`.
    CallNative {
        entry: u32,
        rtoc: u32,
        return_pc: u32,
        final_pc: u32,
        restore_rtoc: u32,
        return_gpr3: PpcNativeReturnGpr3,
    },
    /// Stop the run loop with a structured exception at the current
    /// import-trap slot.
    RaiseException(PpcException),
    /// Stop the run loop. Surfaces as
    /// [`PpcRunResult::Halted`] with the trap PC.
    Halt,
}

/// Access category for architected memory exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcMemoryAccess {
    InstructionFetch,
    Load,
    Store,
}

/// Why an instruction was classified as architecturally illegal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcIllegalInstructionReason {
    /// The primary opcode is reserved/illegal in the user ISA.
    ReservedOpcode,
    /// The opcode is known, but its field combination is an invalid
    /// instruction form.
    InvalidForm,
}

/// Architected exception surfaced by the user-mode interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcException {
    /// A `tw`/`twi` instruction selected at least one true trap
    /// condition. `left` and `right` are the compared 32-bit
    /// operands after immediate sign extension, when applicable.
    ProgramTrap { to: u8, left: u32, right: u32 },
    /// A `sc` instruction requested a transition into supervisor
    /// state. User-mode hosts receive it as a stop condition.
    SystemCall { lev: u8 },
    /// A memory access was not naturally aligned for its operand
    /// size. Unmapped aligned accesses still surface separately as
    /// memory/fetch faults.
    Alignment {
        addr: u32,
        size: u8,
        access: PpcMemoryAccess,
    },
    /// The decoded instruction requires the floating-point facility,
    /// but MSR bit 18 (`FP`) is clear.
    FloatingPointUnavailable,
    /// A host import dispatcher chose to surface an import trap as an
    /// exception instead of returning, halting, or entering guest code.
    HostImportTrap { index: u32 },
    /// A reserved opcode or invalid instruction form was executed.
    IllegalInstruction {
        word: u32,
        reason: PpcIllegalInstructionReason,
    },
}

/// Result of stepping the PowerPC core by one instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcStepResult {
    /// Instruction executed normally; `pc` was advanced and any
    /// register effects applied.
    Stepped,
    /// The instruction at `pc` could not be decoded. The CPU
    /// state is unchanged (no PC advance, no register writes).
    Unimplemented(PpcDecodeError),
    /// A memory load or store hit an unmapped address. The CPU
    /// state is left at the faulting instruction's PC so the host
    /// can recover, log, or trap into a higher-level handler.
    MemoryFault { addr: u32, was_write: bool },
    /// An architected exception was raised. The CPU state is left
    /// at the exception instruction's PC for host inspection.
    Exception(PpcException),
}

/// Result of running a PowerPC instruction stream to a stop point.
///
/// The runner fetches words from `mem` at `pc`, decodes via
/// [`decode`], dispatches via [`PpcCpu::step`], and continues
/// until one of these terminating conditions trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcRunResult {
    /// Reached the requested cycle budget without hitting any
    /// other stop condition. The CPU is still in a valid
    /// runnable state — call `run` again with a fresh budget to
    /// continue. `cycles` is the number of instructions executed.
    CycleLimit { cycles: u64 },
    /// Hit a `bclr` / `bcctr` whose target was the configured
    /// halt address (typically 0). This is the host's signal
    /// that the guest has returned through the original entry
    /// point and is "done". `cycles` is the number of
    /// instructions executed up to and including the branch.
    Halted { pc: u32, cycles: u64 },
    /// Decoder rejected the instruction at `pc`. `cycles` is
    /// the number of instructions completed before the
    /// unimplemented one (the unimplemented instruction itself
    /// is NOT counted).
    Unimplemented {
        pc: u32,
        error: PpcDecodeError,
        cycles: u64,
    },
    /// A load or store at `pc` hit an unmapped address.
    MemoryFault {
        pc: u32,
        addr: u32,
        was_write: bool,
        cycles: u64,
    },
    /// An instruction raised an architected exception. `cycles` is
    /// the number of instructions completed before the exception
    /// instruction, which is not counted.
    Exception {
        pc: u32,
        exception: PpcException,
        cycles: u64,
    },
    /// Instruction fetch itself failed — the address pointed to
    /// by `pc` is not mapped in `mem`. Distinct from a load/store
    /// fault because no decode happened.
    FetchFault { pc: u32, cycles: u64 },
}

impl PpcCpu {
    /// Execute one instruction at the current `pc`. The caller
    /// supplies the instruction word in host byte order — when
    /// fetching from guest memory, the caller is responsible for
    /// the big-endian decode.
    ///
    /// The "RA = 0 means literal 0" rule for D-form arithmetic
    /// (§3.3.8) is handled here: `addi` / `addis` with `ra == 0`
    /// treat the operand as the constant zero rather than reading
    /// GPR0. `ori` does NOT have this special case — it always
    /// reads `gpr[rs]`.
    /// Step one instruction without a real memory bus. Memory
    /// loads and stores will surface as `MemoryFault` because the
    /// internal null memory rejects every access. Use [`Self::step`]
    /// for instruction streams that touch RAM.
    pub fn step_instruction(&mut self, instr_word: u32) -> PpcStepResult {
        self.step(&mut NullMemory, instr_word)
    }

    fn write_u8_observed<M: PpcMemory + ?Sized, O: PpcMemoryWriteObserver + ?Sized>(
        &self,
        mem: &mut M,
        observer: &mut O,
        addr: u32,
        value: u8,
    ) -> Option<()> {
        mem.write_u8(addr, value)?;
        observer.on_write(self.pc, self.lr, self.gpr[2], self.gpr[1], addr, value);
        Some(())
    }

    fn write_u16_be_observed<M: PpcMemory + ?Sized, O: PpcMemoryWriteObserver + ?Sized>(
        &self,
        mem: &mut M,
        observer: &mut O,
        addr: u32,
        value: u16,
    ) -> Option<()> {
        if !observer.observes_writes() {
            return mem.write_u16_be(addr, value);
        }
        let bytes = value.to_be_bytes();
        self.write_u8_observed(mem, observer, addr, bytes[0])?;
        self.write_u8_observed(mem, observer, addr.wrapping_add(1), bytes[1])?;
        Some(())
    }

    fn write_u32_be_observed<M: PpcMemory + ?Sized, O: PpcMemoryWriteObserver + ?Sized>(
        &self,
        mem: &mut M,
        observer: &mut O,
        addr: u32,
        value: u32,
    ) -> Option<()> {
        if !observer.observes_writes() {
            return mem.write_u32_be(addr, value);
        }
        let bytes = value.to_be_bytes();
        for (offset, byte) in bytes.into_iter().enumerate() {
            self.write_u8_observed(mem, observer, addr.wrapping_add(offset as u32), byte)?;
        }
        Some(())
    }

    fn write_u64_be_observed<M: PpcMemory + ?Sized, O: PpcMemoryWriteObserver + ?Sized>(
        &self,
        mem: &mut M,
        observer: &mut O,
        addr: u32,
        value: u64,
    ) -> Option<()> {
        if !observer.observes_writes() {
            return mem.write_u64_be(addr, value);
        }
        let bytes = value.to_be_bytes();
        for (offset, byte) in bytes.into_iter().enumerate() {
            self.write_u8_observed(mem, observer, addr.wrapping_add(offset as u32), byte)?;
        }
        Some(())
    }

    /// Run instructions in `mem` starting at `self.pc` until one
    /// of these stop conditions trips:
    ///
    /// - The CPU has executed `max_cycles` instructions without
    ///   any other event (`PpcRunResult::CycleLimit`).
    /// - The CPU branches to `halt_pc` (`PpcRunResult::Halted`).
    ///   Typically the host seeds the entry frame's saved LR
    ///   with `halt_pc=0`, so the program's outermost `blr` lands
    ///   here and naturally signals termination.
    /// - The decoder rejects an instruction
    ///   (`PpcRunResult::Unimplemented`).
    /// - A load/store hits unmapped memory
    ///   (`PpcRunResult::MemoryFault`).
    /// - The guest raises an architected exception
    ///   (`PpcRunResult::Exception`).
    /// - The instruction fetch itself fails
    ///   (`PpcRunResult::FetchFault`).
    ///
    /// On every result variant, `self.pc` is left pointing at the
    /// instruction the runner *would have executed next* — for
    /// `Stepped`/`CycleLimit`/`Halted` that's the next
    /// instruction; for `Unimplemented`/`*Fault`/`Exception` it's
    /// the faulting instruction itself, so the host can inspect it.
    pub fn run<M: PpcMemory + ?Sized>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
    ) -> PpcRunResult {
        let mut observer = PpcNoopFetchObserver;
        self.run_with_fetch_observer(mem, max_cycles, halt_pc, &mut observer)
    }

    /// Same as [`Self::run`], with an observer notified for every
    /// successful instruction fetch. Fetch faults and alignment
    /// exceptions are not reported because no instruction word was
    /// fetched.
    pub fn run_with_fetch_observer<M: PpcMemory + ?Sized, O: PpcFetchObserver + ?Sized>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        observer: &mut O,
    ) -> PpcRunResult {
        let mut write_observer = PpcNoopMemoryWriteObserver;
        self.run_with_fetch_and_write_observer(
            mem,
            max_cycles,
            halt_pc,
            observer,
            &mut write_observer,
        )
    }

    /// Same as [`Self::run_with_fetch_observer`], with an observer
    /// notified for every successful guest-store byte.
    pub fn run_with_fetch_and_write_observer<
        M: PpcMemory + ?Sized,
        O: PpcFetchObserver + ?Sized,
        W: PpcMemoryWriteObserver + ?Sized,
    >(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        observer: &mut O,
        write_observer: &mut W,
    ) -> PpcRunResult {
        let mut cycles = 0u64;
        while cycles < max_cycles {
            // Halt check happens BEFORE fetch so a guest that
            // returns via `blr` to halt_pc terminates cleanly
            // without trying to decode whatever happens to live
            // at that address.
            if self.pc == halt_pc {
                return PpcRunResult::Halted {
                    pc: self.pc,
                    cycles,
                };
            }
            let pc = self.pc;
            if let Some(exception) =
                Self::alignment_exception(pc, 4, PpcMemoryAccess::InstructionFetch)
            {
                return PpcRunResult::Exception {
                    pc,
                    exception,
                    cycles,
                };
            }
            let word = match mem.read_instruction_u32_be(pc) {
                Some(w) => w,
                None => {
                    return PpcRunResult::FetchFault { pc, cycles };
                }
            };
            observer.on_fetch_cpu(self, word);
            match self.step_with_write_observer(mem, word, write_observer) {
                PpcStepResult::Stepped => {
                    cycles = cycles.saturating_add(1);
                }
                PpcStepResult::Unimplemented(error) => {
                    return PpcRunResult::Unimplemented { pc, error, cycles };
                }
                PpcStepResult::MemoryFault { addr, was_write } => {
                    return PpcRunResult::MemoryFault {
                        pc,
                        addr,
                        was_write,
                        cycles,
                    };
                }
                PpcStepResult::Exception(exception) => {
                    return PpcRunResult::Exception {
                        pc,
                        exception,
                        cycles,
                    };
                }
            }
        }
        PpcRunResult::CycleLimit { cycles }
    }

    /// Run with a per-import dispatcher. Whenever the PC enters
    /// the synthetic import-trap region
    /// `[trap_base, trap_base + import_count * 4)`, `handler` is
    /// invoked with the import index, the CPU, and the memory
    /// bus. The dispatcher can read / write argument registers
    /// (GPR3..GPR10), follow pointers in arguments to populate
    /// output structures, allocate memory, etc. Its return
    /// [`PpcImportAction`] decides how the run loop continues:
    ///
    /// - [`PpcImportAction::Return`]: GPR3 is overwritten with the
    ///   supplied value, PC is set to LR, and execution resumes.
    /// - [`PpcImportAction::ReturnPreserve`]: PC is set to LR and
    ///   execution resumes with GPR3 unchanged.
    /// - [`PpcImportAction::ReturnPreserveWithExtraCycles`] and
    ///   [`PpcImportAction::ReturnWithExtraCycles`]: return as above
    ///   and charge extra guest cycles for host-skipped polling work.
    ///   Fully bypasses the trampoline `blr` — no memory fetch
    ///   for the trap word happens, no instruction is decoded.
    /// - [`PpcImportAction::CallNative`]: execution enters guest
    ///   PowerPC code at `entry` with `gpr2 = rtoc` and `lr =
    ///   return_pc`; when the guest reaches `return_pc`, the saved
    ///   caller RTOC is restored and execution resumes at `final_pc`.
    /// - [`PpcImportAction::RaiseException`]: the run terminates with
    ///   [`PpcRunResult::Exception`] at the import slot without
    ///   advancing PC or counting a completed cycle.
    /// - [`PpcImportAction::Halt`]: the run terminates with
    ///   [`PpcRunResult::Halted`] at the trap PC.
    ///
    /// `cycles` advances by 1 for each handled import call, the
    /// same as a normal stepped instruction, so the budget
    /// behaves the same way callers expect.
    pub fn run_with_imports<M: PpcMemory + ?Sized, F>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut handler = handler;
        self.run_with_imports_and_cycle_handler(
            mem,
            max_cycles,
            halt_pc,
            trap_base,
            import_count,
            move |_cycles, index, cpu, mem| handler(index, cpu, mem),
        )
    }

    /// Same as [`Self::run_with_imports`], but also supplies the number of
    /// elapsed cycles in the current run to the import handler.
    pub fn run_with_imports_and_cycle_handler<M: PpcMemory + ?Sized, F>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u64, u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        self.run_with_imports_unobserved(mem, max_cycles, halt_pc, trap_base, import_count, handler)
    }

    fn run_with_imports_unobserved<M: PpcMemory + ?Sized, F>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        mut handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u64, u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut cycles = 0u64;
        while cycles < max_cycles {
            if let Some(frame) = self.import_call_stack.last().copied() {
                if self.pc == frame.return_pc {
                    self.import_call_stack.pop();
                    match frame.return_gpr3 {
                        PpcNativeReturnGpr3::Preserve => {}
                        PpcNativeReturnGpr3::Mask(mask) => {
                            self.gpr[3] &= mask;
                        }
                        PpcNativeReturnGpr3::Set(value) => {
                            self.gpr[3] = value;
                        }
                        PpcNativeReturnGpr3::ZeroOrSet { zero, nonzero } => {
                            self.gpr[3] = if self.gpr[3] == 0 { zero } else { nonzero };
                        }
                        PpcNativeReturnGpr3::CrBit(bit_index) => {
                            self.gpr[3] = u32::from(self.cr_bit(bit_index));
                        }
                        PpcNativeReturnGpr3::XerCa => {
                            self.gpr[3] = u32::from(self.xer_ca());
                        }
                        PpcNativeReturnGpr3::XerOv => {
                            self.gpr[3] = u32::from(self.xer_ov());
                        }
                    }
                    self.gpr[2] = frame.restore_rtoc;
                    self.lr = frame.final_pc;
                    self.pc = frame.final_pc;
                    cycles = cycles.saturating_add(1);
                    continue;
                }
            }
            if self.pc == halt_pc {
                return PpcRunResult::Halted {
                    pc: self.pc,
                    cycles,
                };
            }
            let pc = self.pc;
            if import_count > 0 && pc >= trap_base {
                let off = pc.wrapping_sub(trap_base);
                if (off >> 2) < import_count && (off & 0x3) == 0 {
                    let index = off >> 2;
                    match handler(cycles, index, self, mem) {
                        PpcImportAction::Return(value) => {
                            self.gpr[3] = value;
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::ReturnPreserve => {
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles) => {
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1).saturating_add(extra_cycles);
                            continue;
                        }
                        PpcImportAction::ReturnWithExtraCycles(value, extra_cycles) => {
                            self.gpr[3] = value;
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1).saturating_add(extra_cycles);
                            continue;
                        }
                        PpcImportAction::Yield(yield_cycles) => {
                            return PpcRunResult::CycleLimit {
                                cycles: cycles.saturating_add(yield_cycles).min(max_cycles),
                            };
                        }
                        PpcImportAction::CallNative {
                            entry,
                            rtoc,
                            return_pc,
                            final_pc,
                            restore_rtoc,
                            return_gpr3,
                        } => {
                            self.import_call_stack.push(PpcImportReturnFrame {
                                return_pc,
                                final_pc,
                                restore_rtoc,
                                return_gpr3,
                            });
                            self.pc = entry;
                            self.lr = return_pc;
                            self.gpr[2] = rtoc;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::RaiseException(exception) => {
                            return PpcRunResult::Exception {
                                pc,
                                exception,
                                cycles,
                            };
                        }
                        PpcImportAction::Halt => {
                            return PpcRunResult::Halted { pc, cycles };
                        }
                    }
                }
            }
            if let Some(exception) =
                Self::alignment_exception(pc, 4, PpcMemoryAccess::InstructionFetch)
            {
                return PpcRunResult::Exception {
                    pc,
                    exception,
                    cycles,
                };
            }
            let word = match mem.read_instruction_u32_be(pc) {
                Some(w) => w,
                None => {
                    return PpcRunResult::FetchFault { pc, cycles };
                }
            };
            if import_count > 0 && (word & 0xFFFF_0000) == 0x8182_0000 {
                if let Some(result) = self.try_fast_cfm_import_stub(
                    mem,
                    pc,
                    word,
                    trap_base,
                    import_count,
                    &mut handler,
                    &mut cycles,
                    max_cycles,
                ) {
                    match result {
                        PpcFastImportStubResult::Continue => continue,
                        PpcFastImportStubResult::Stop(result) => return result,
                    }
                }
            }
            let step_result = self
                .step_fast_unobserved(mem, word)
                .unwrap_or_else(|| self.step(mem, word));
            match step_result {
                PpcStepResult::Stepped => {
                    cycles = cycles.saturating_add(1);
                }
                PpcStepResult::Unimplemented(error) => {
                    return PpcRunResult::Unimplemented { pc, error, cycles };
                }
                PpcStepResult::MemoryFault { addr, was_write } => {
                    return PpcRunResult::MemoryFault {
                        pc,
                        addr,
                        was_write,
                        cycles,
                    };
                }
                PpcStepResult::Exception(exception) => {
                    return PpcRunResult::Exception {
                        pc,
                        exception,
                        cycles,
                    };
                }
            }
        }
        PpcRunResult::CycleLimit { cycles }
    }

    #[allow(clippy::too_many_arguments)]
    fn try_fast_cfm_import_stub<M: PpcMemory + ?Sized, F>(
        &mut self,
        mem: &mut M,
        pc: u32,
        first_word: u32,
        trap_base: u32,
        import_count: u32,
        handler: &mut F,
        cycles: &mut u64,
        max_cycles: u64,
    ) -> Option<PpcFastImportStubResult>
    where
        F: FnMut(u64, u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let cache_index = ((pc >> 2) as usize) & PPC_CFM_IMPORT_STUB_CACHE_INDEX_MASK;
        match self.cfm_import_stub_cache[cache_index] {
            Some((cached_pc, cached_first_word))
                if cached_pc == pc && cached_first_word == first_word => {}
            _ => {
                if mem.read_u32_be(pc.wrapping_add(4))? != 0x9041_0014
                    || mem.read_u32_be(pc.wrapping_add(8))? != 0x800C_0000
                    || mem.read_u32_be(pc.wrapping_add(12))? != 0x804C_0004
                    || mem.read_u32_be(pc.wrapping_add(16))? != 0x7C09_03A6
                    || mem.read_u32_be(pc.wrapping_add(20))? != 0x4E80_0420
                {
                    return None;
                }
                self.cfm_import_stub_cache[cache_index] = Some((pc, first_word));
            }
        }

        let displacement = first_word as u16 as i16;
        let toc_slot = self.gpr[2].wrapping_add(i32::from(displacement) as u32);
        let Some(tvector) = mem.read_u32_be(toc_slot) else {
            return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                pc,
                addr: toc_slot,
                was_write: false,
                cycles: *cycles,
            }));
        };
        let entry = mem.read_u32_be(tvector);
        let rtoc = mem.read_u32_be(tvector.wrapping_add(4));
        let Some(entry) = entry else {
            let save_addr = self.gpr[1].wrapping_add(20);
            if mem.write_u32_be(save_addr, self.gpr[2]).is_none() {
                return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                    pc,
                    addr: save_addr,
                    was_write: true,
                    cycles: (*cycles).saturating_add(1),
                }));
            }
            *cycles = cycles.saturating_add(2);
            return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                pc,
                addr: tvector,
                was_write: false,
                cycles: *cycles,
            }));
        };
        let off = entry.wrapping_sub(trap_base);
        if entry < trap_base || (off & 0x3) != 0 || (off >> 2) >= import_count {
            return None;
        }
        let Some(rtoc) = rtoc else {
            let save_addr = self.gpr[1].wrapping_add(20);
            if mem.write_u32_be(save_addr, self.gpr[2]).is_none() {
                return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                    pc,
                    addr: save_addr,
                    was_write: true,
                    cycles: (*cycles).saturating_add(1),
                }));
            }
            *cycles = cycles.saturating_add(2);
            return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                pc,
                addr: tvector.wrapping_add(4),
                was_write: false,
                cycles: *cycles,
            }));
        };

        let save_addr = self.gpr[1].wrapping_add(20);
        if mem.write_u32_be(save_addr, self.gpr[2]).is_none() {
            return Some(PpcFastImportStubResult::Stop(PpcRunResult::MemoryFault {
                pc,
                addr: save_addr,
                was_write: true,
                cycles: (*cycles).saturating_add(1),
            }));
        }
        self.gpr[12] = tvector;
        self.gpr[0] = entry;
        self.gpr[2] = rtoc;
        self.ctr = entry;
        self.pc = entry;
        let index = off >> 2;
        match handler(cycles.saturating_add(6), index, self, mem) {
            PpcImportAction::Return(value) => {
                self.gpr[3] = value;
                self.pc = self.lr;
                *cycles = cycles.saturating_add(7);
                Some(PpcFastImportStubResult::Continue)
            }
            PpcImportAction::ReturnPreserve => {
                self.pc = self.lr;
                *cycles = cycles.saturating_add(7);
                Some(PpcFastImportStubResult::Continue)
            }
            PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles) => {
                self.pc = self.lr;
                *cycles = cycles.saturating_add(7).saturating_add(extra_cycles);
                Some(PpcFastImportStubResult::Continue)
            }
            PpcImportAction::ReturnWithExtraCycles(value, extra_cycles) => {
                self.gpr[3] = value;
                self.pc = self.lr;
                *cycles = cycles.saturating_add(7).saturating_add(extra_cycles);
                Some(PpcFastImportStubResult::Continue)
            }
            PpcImportAction::Yield(yield_cycles) => {
                Some(PpcFastImportStubResult::Stop(PpcRunResult::CycleLimit {
                    cycles: cycles
                        .saturating_add(7)
                        .saturating_add(yield_cycles)
                        .min(max_cycles),
                }))
            }
            PpcImportAction::CallNative {
                entry,
                rtoc,
                return_pc,
                final_pc,
                restore_rtoc,
                return_gpr3,
            } => {
                self.import_call_stack.push(PpcImportReturnFrame {
                    return_pc,
                    final_pc,
                    restore_rtoc,
                    return_gpr3,
                });
                self.pc = entry;
                self.lr = return_pc;
                self.gpr[2] = rtoc;
                *cycles = cycles.saturating_add(7);
                Some(PpcFastImportStubResult::Continue)
            }
            PpcImportAction::RaiseException(exception) => {
                Some(PpcFastImportStubResult::Stop(PpcRunResult::Exception {
                    pc: entry,
                    exception,
                    cycles: (*cycles).saturating_add(6),
                }))
            }
            PpcImportAction::Halt => Some(PpcFastImportStubResult::Stop(PpcRunResult::Halted {
                pc: entry,
                cycles: (*cycles).saturating_add(6),
            })),
        }
    }

    /// Same as [`Self::run_with_imports`], with an observer
    /// notified for every successful instruction fetch. Handled
    /// imports are not reported as fetched instructions because the
    /// dispatcher bypasses the synthetic trampoline word.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_imports_and_fetch_observer<
        M: PpcMemory + ?Sized,
        F,
        O: PpcFetchObserver + ?Sized,
    >(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        observer: &mut O,
        handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut handler = handler;
        self.run_with_imports_and_fetch_observer_and_cycle_handler(
            mem,
            max_cycles,
            halt_pc,
            trap_base,
            import_count,
            observer,
            move |_cycles, index, cpu, mem| handler(index, cpu, mem),
        )
    }

    /// Same as [`Self::run_with_imports_and_fetch_observer`], but also
    /// supplies elapsed cycles to the import handler.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_imports_and_fetch_observer_and_cycle_handler<
        M: PpcMemory + ?Sized,
        F,
        O: PpcFetchObserver + ?Sized,
    >(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        observer: &mut O,
        handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u64, u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut write_observer = PpcNoopMemoryWriteObserver;
        self.run_with_imports_and_observers_and_cycle_handler(
            mem,
            max_cycles,
            halt_pc,
            trap_base,
            import_count,
            observer,
            &mut write_observer,
            handler,
        )
    }

    /// Same as [`Self::run_with_imports_and_fetch_observer`], with
    /// an observer notified for every successful guest-store byte.
    /// Handled import dispatch itself is not reported as a memory
    /// write unless it enters native guest code; host-side writes made
    /// by the import handler go through the handler's memory reference.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_imports_and_observers<
        M: PpcMemory + ?Sized,
        F,
        O: PpcFetchObserver + ?Sized,
        W: PpcMemoryWriteObserver + ?Sized,
    >(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        observer: &mut O,
        write_observer: &mut W,
        handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut handler = handler;
        self.run_with_imports_and_observers_and_cycle_handler(
            mem,
            max_cycles,
            halt_pc,
            trap_base,
            import_count,
            observer,
            write_observer,
            move |_cycles, index, cpu, mem| handler(index, cpu, mem),
        )
    }

    /// Same as [`Self::run_with_imports_and_observers`], but also supplies
    /// elapsed cycles to the import handler.
    #[allow(clippy::too_many_arguments)]
    pub fn run_with_imports_and_observers_and_cycle_handler<
        M: PpcMemory + ?Sized,
        F,
        O: PpcFetchObserver + ?Sized,
        W: PpcMemoryWriteObserver + ?Sized,
    >(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        observer: &mut O,
        write_observer: &mut W,
        mut handler: F,
    ) -> PpcRunResult
    where
        F: FnMut(u64, u32, &mut PpcCpu, &mut M) -> PpcImportAction,
    {
        let mut cycles = 0u64;
        while cycles < max_cycles {
            if let Some(frame) = self.import_call_stack.last().copied() {
                if self.pc == frame.return_pc {
                    self.import_call_stack.pop();
                    match frame.return_gpr3 {
                        PpcNativeReturnGpr3::Preserve => {}
                        PpcNativeReturnGpr3::Mask(mask) => {
                            self.gpr[3] &= mask;
                        }
                        PpcNativeReturnGpr3::Set(value) => {
                            self.gpr[3] = value;
                        }
                        PpcNativeReturnGpr3::ZeroOrSet { zero, nonzero } => {
                            self.gpr[3] = if self.gpr[3] == 0 { zero } else { nonzero };
                        }
                        PpcNativeReturnGpr3::CrBit(bit_index) => {
                            self.gpr[3] = u32::from(self.cr_bit(bit_index));
                        }
                        PpcNativeReturnGpr3::XerCa => {
                            self.gpr[3] = u32::from(self.xer_ca());
                        }
                        PpcNativeReturnGpr3::XerOv => {
                            self.gpr[3] = u32::from(self.xer_ov());
                        }
                    }
                    self.gpr[2] = frame.restore_rtoc;
                    self.lr = frame.final_pc;
                    self.pc = frame.final_pc;
                    cycles = cycles.saturating_add(1);
                    continue;
                }
            }
            if self.pc == halt_pc {
                return PpcRunResult::Halted {
                    pc: self.pc,
                    cycles,
                };
            }
            let pc = self.pc;
            // Detect entry into the import-trap region. Each import
            // gets a 4-byte slot, so the index is the slot number.
            if import_count > 0 && pc >= trap_base {
                let off = pc.wrapping_sub(trap_base);
                if (off >> 2) < import_count && (off & 0x3) == 0 {
                    let index = off >> 2;
                    match handler(cycles, index, self, mem) {
                        PpcImportAction::Return(value) => {
                            self.gpr[3] = value;
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::ReturnPreserve => {
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles) => {
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1).saturating_add(extra_cycles);
                            continue;
                        }
                        PpcImportAction::ReturnWithExtraCycles(value, extra_cycles) => {
                            self.gpr[3] = value;
                            self.pc = self.lr;
                            cycles = cycles.saturating_add(1).saturating_add(extra_cycles);
                            continue;
                        }
                        PpcImportAction::Yield(yield_cycles) => {
                            return PpcRunResult::CycleLimit {
                                cycles: cycles.saturating_add(yield_cycles).min(max_cycles),
                            };
                        }
                        PpcImportAction::CallNative {
                            entry,
                            rtoc,
                            return_pc,
                            final_pc,
                            restore_rtoc,
                            return_gpr3,
                        } => {
                            self.import_call_stack.push(PpcImportReturnFrame {
                                return_pc,
                                final_pc,
                                restore_rtoc,
                                return_gpr3,
                            });
                            self.pc = entry;
                            self.lr = return_pc;
                            self.gpr[2] = rtoc;
                            cycles = cycles.saturating_add(1);
                            continue;
                        }
                        PpcImportAction::RaiseException(exception) => {
                            return PpcRunResult::Exception {
                                pc,
                                exception,
                                cycles,
                            };
                        }
                        PpcImportAction::Halt => {
                            return PpcRunResult::Halted { pc, cycles };
                        }
                    }
                }
            }
            if let Some(exception) =
                Self::alignment_exception(pc, 4, PpcMemoryAccess::InstructionFetch)
            {
                return PpcRunResult::Exception {
                    pc,
                    exception,
                    cycles,
                };
            }
            let word = match mem.read_instruction_u32_be(pc) {
                Some(w) => w,
                None => {
                    return PpcRunResult::FetchFault { pc, cycles };
                }
            };
            observer.on_fetch_cpu(self, word);
            match self.step_with_write_observer(mem, word, write_observer) {
                PpcStepResult::Stepped => {
                    cycles = cycles.saturating_add(1);
                }
                PpcStepResult::Unimplemented(error) => {
                    return PpcRunResult::Unimplemented { pc, error, cycles };
                }
                PpcStepResult::MemoryFault { addr, was_write } => {
                    return PpcRunResult::MemoryFault {
                        pc,
                        addr,
                        was_write,
                        cycles,
                    };
                }
                PpcStepResult::Exception(exception) => {
                    return PpcRunResult::Exception {
                        pc,
                        exception,
                        cycles,
                    };
                }
            }
        }
        PpcRunResult::CycleLimit { cycles }
    }

    /// Run with an import-call trace. Identical to [`Self::run`]
    /// except that whenever the PC enters
    /// `[trap_base, trap_base + import_count * 4)` — the synthetic
    /// import-trap region a host (e.g. systemless's PEF loader)
    /// has installed — the import index is pushed onto `trace`
    /// *before* the trap instruction (typically `blr`) is fetched
    /// and executed. The CPU then continues normally, returning
    /// through the `blr` to the caller's saved LR.
    ///
    /// Used by diagnostic tools (e.g. `play-inspect`) to surface
    /// the call sequence a guest performs against the import table,
    /// which is the data needed to design a real per-import
    /// dispatcher.
    pub fn run_with_import_trace<M: PpcMemory + ?Sized>(
        &mut self,
        mem: &mut M,
        max_cycles: u64,
        halt_pc: u32,
        trap_base: u32,
        import_count: u32,
        trace: &mut Vec<u32>,
    ) -> PpcRunResult {
        let mut cycles = 0u64;
        while cycles < max_cycles {
            if self.pc == halt_pc {
                return PpcRunResult::Halted {
                    pc: self.pc,
                    cycles,
                };
            }
            let pc = self.pc;
            // Detect entry into the import-trap region. Each import
            // gets a single 4-byte slot, so the index is simply the
            // 4-byte slot offset.
            if import_count > 0 && pc >= trap_base {
                let off = pc.wrapping_sub(trap_base);
                if (off >> 2) < import_count && (off & 0x3) == 0 {
                    trace.push(off >> 2);
                }
            }
            if let Some(exception) =
                Self::alignment_exception(pc, 4, PpcMemoryAccess::InstructionFetch)
            {
                return PpcRunResult::Exception {
                    pc,
                    exception,
                    cycles,
                };
            }
            let word = match mem.read_instruction_u32_be(pc) {
                Some(w) => w,
                None => {
                    return PpcRunResult::FetchFault { pc, cycles };
                }
            };
            match self.step(mem, word) {
                PpcStepResult::Stepped => {
                    cycles = cycles.saturating_add(1);
                }
                PpcStepResult::Unimplemented(error) => {
                    return PpcRunResult::Unimplemented { pc, error, cycles };
                }
                PpcStepResult::MemoryFault { addr, was_write } => {
                    return PpcRunResult::MemoryFault {
                        pc,
                        addr,
                        was_write,
                        cycles,
                    };
                }
                PpcStepResult::Exception(exception) => {
                    return PpcRunResult::Exception {
                        pc,
                        exception,
                        cycles,
                    };
                }
            }
        }
        PpcRunResult::CycleLimit { cycles }
    }

    /// Step one instruction with a memory-bus implementation in
    /// hand. This is the production form: the host fetches the
    /// instruction word from `mem` (handling the big-endian decode
    /// itself) and passes it in. Loads and stores route their
    /// data byte traffic through `mem`.
    pub fn step<M: PpcMemory + ?Sized>(&mut self, mem: &mut M, instr_word: u32) -> PpcStepResult {
        let mut write_observer = PpcNoopMemoryWriteObserver;
        self.step_with_write_observer(mem, instr_word, &mut write_observer)
    }

    /// Step one instruction and notify `write_observer` for each
    /// successful guest-store byte.
    pub fn step_with_write_observer<M: PpcMemory + ?Sized, W: PpcMemoryWriteObserver + ?Sized>(
        &mut self,
        mem: &mut M,
        instr_word: u32,
        write_observer: &mut W,
    ) -> PpcStepResult {
        let decoded = match self.decode_cached(instr_word) {
            Ok(d) => d,
            Err(e) => {
                if let Some(exception) = Self::decode_error_illegal_instruction(instr_word, e) {
                    return PpcStepResult::Exception(exception);
                }
                return PpcStepResult::Unimplemented(e);
            }
        };
        if !self.msr_fp_available() && Self::is_floating_point_instruction(decoded) {
            return PpcStepResult::Exception(PpcException::FloatingPointUnavailable);
        }
        if self.alignment_policy == PpcAlignmentPolicy::Trap
            && Self::may_require_data_alignment_check(instr_word)
        {
            if let Some(exception) = self.load_store_alignment_exception(decoded) {
                return PpcStepResult::Exception(exception);
            }
        }
        match decoded {
            PpcInstr::Twi { to, ra, si } => {
                let left = self.gpr[ra as usize];
                let right = i32::from(si) as u32;
                if Self::trap_condition(to, left, right) {
                    return PpcStepResult::Exception(PpcException::ProgramTrap { to, left, right });
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Tw { to, ra, rb } => {
                let left = self.gpr[ra as usize];
                let right = self.gpr[rb as usize];
                if Self::trap_condition(to, left, right) {
                    return PpcStepResult::Exception(PpcException::ProgramTrap { to, left, right });
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sc { lev } => {
                return PpcStepResult::Exception(PpcException::SystemCall { lev });
            }
            PpcInstr::Addi { rt, ra, si } => {
                let lhs = if ra == 0 { 0u32 } else { self.gpr[ra as usize] };
                self.gpr[rt as usize] = lhs.wrapping_add(i32::from(si) as u32);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Addis { rt, ra, si } => {
                let lhs = if ra == 0 { 0u32 } else { self.gpr[ra as usize] };
                let extended = (i32::from(si) as u32) << 16;
                self.gpr[rt as usize] = lhs.wrapping_add(extended);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Ori { ra, rs, ui } => {
                self.gpr[ra as usize] = self.gpr[rs as usize] | u32::from(ui);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Oris { ra, rs, ui } => {
                self.gpr[ra as usize] = self.gpr[rs as usize] | (u32::from(ui) << 16);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Xori { ra, rs, ui } => {
                self.gpr[ra as usize] = self.gpr[rs as usize] ^ u32::from(ui);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Xoris { ra, rs, ui } => {
                self.gpr[ra as usize] = self.gpr[rs as usize] ^ (u32::from(ui) << 16);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::AndiDot { ra, rs, ui } => {
                let result = self.gpr[rs as usize] & u32::from(ui);
                self.gpr[ra as usize] = result;
                // andi. always sets CR0 (no non-recording form).
                self.update_cr0_from_signed(result);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::AndisDot { ra, rs, ui } => {
                let result = self.gpr[rs as usize] & (u32::from(ui) << 16);
                self.gpr[ra as usize] = result;
                self.update_cr0_from_signed(result);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Slw { ra, rs, rb, rc } => {
                // Per §3.3.12.2: shift count = RB low 6 bits.
                // If bit 5 of the count is set (count >= 32),
                // the result is zero. Rust's `<<` on u32 panics
                // for n >= 32, so guard explicitly.
                let n = self.gpr[rb as usize] & 0x3F;
                let result = if n >= 32 {
                    0
                } else {
                    self.gpr[rs as usize] << n
                };
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Srw { ra, rs, rb, rc } => {
                let n = self.gpr[rb as usize] & 0x3F;
                let result = if n >= 32 {
                    0
                } else {
                    self.gpr[rs as usize] >> n
                };
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sraw { ra, rs, rb, rc } => {
                let n = self.gpr[rb as usize] & 0x3F;
                let signed = self.gpr[rs as usize] as i32;
                let (result, ca) = if n == 0 {
                    (signed as u32, false)
                } else if n >= 32 {
                    // 32 sign bits + CA = sign bit per §3.3.12.2.
                    let sign = signed >> 31;
                    (sign as u32, signed < 0)
                } else {
                    // Arithmetic right shift; CA = 1 iff signed
                    // negative AND any 1-bits shifted out.
                    let bits_lost = self.gpr[rs as usize] & ((1u32 << n) - 1);
                    let ca = signed < 0 && bits_lost != 0;
                    ((signed >> n) as u32, ca)
                };
                self.gpr[ra as usize] = result;
                self.set_xer_ca(ca);
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Srawi { ra, rs, sh, rc } => {
                // sh is the 5-bit immediate (always in 0..32).
                let signed = self.gpr[rs as usize] as i32;
                let (result, ca) = if sh == 0 {
                    (signed as u32, false)
                } else {
                    let bits_lost = self.gpr[rs as usize] & ((1u32 << sh) - 1);
                    let ca = signed < 0 && bits_lost != 0;
                    ((signed >> sh) as u32, ca)
                };
                self.gpr[ra as usize] = result;
                self.set_xer_ca(ca);
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Rlwinm {
                ra,
                rs,
                sh,
                mb,
                me,
                rc,
            } => {
                // Per ISA Book I §3.3.12.1:
                //   r ← ROTL32((RS), SH)
                //   m ← MASK(MB, ME)
                //   RA ← r & m
                let rotated = self.gpr[rs as usize].rotate_left(u32::from(sh));
                let mask = Self::mask32(mb, me);
                let result = rotated & mask;
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Rlwimi {
                ra,
                rs,
                sh,
                mb,
                me,
                rc,
            } => {
                // Mask Insert: blend the rotated RS bits into RA
                // under the mask, leaving bits outside the mask
                // untouched in RA.
                //   r ← ROTL32((RS), SH)
                //   m ← MASK(MB, ME)
                //   RA ← (r & m) | (RA & ~m)
                let rotated = self.gpr[rs as usize].rotate_left(u32::from(sh));
                let mask = Self::mask32(mb, me);
                let kept = self.gpr[ra as usize] & !mask;
                let inserted = rotated & mask;
                let result = kept | inserted;
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Rlwnm {
                ra,
                rs,
                rb,
                mb,
                me,
                rc,
            } => {
                // Same as rlwinm but the rotate amount is the
                // low 5 bits of RB rather than an immediate.
                let n = self.gpr[rb as usize] & 0x1F;
                let rotated = self.gpr[rs as usize].rotate_left(n);
                let mask = Self::mask32(mb, me);
                let result = rotated & mask;
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Or { ra, rs, rb, rc } => {
                let result = self.gpr[rs as usize] | self.gpr[rb as usize];
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::And { ra, rs, rb, rc } => {
                let result = self.gpr[rs as usize] & self.gpr[rb as usize];
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Xor { ra, rs, rb, rc } => {
                let result = self.gpr[rs as usize] ^ self.gpr[rb as usize];
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Nor { ra, rs, rb, rc } => {
                let result = !(self.gpr[rs as usize] | self.gpr[rb as usize]);
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Nand { ra, rs, rb, rc } => {
                let result = !(self.gpr[rs as usize] & self.gpr[rb as usize]);
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Andc { ra, rs, rb, rc } => {
                let result = self.gpr[rs as usize] & !self.gpr[rb as usize];
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Orc { ra, rs, rb, rc } => {
                let result = self.gpr[rs as usize] | !self.gpr[rb as usize];
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Eqv { ra, rs, rb, rc } => {
                let result = !(self.gpr[rs as usize] ^ self.gpr[rb as usize]);
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crand { bt, ba, bb } => {
                let v = self.cr_bit(ba) & self.cr_bit(bb);
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cror { bt, ba, bb } => {
                let v = self.cr_bit(ba) | self.cr_bit(bb);
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crxor { bt, ba, bb } => {
                let v = self.cr_bit(ba) ^ self.cr_bit(bb);
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crnand { bt, ba, bb } => {
                let v = !(self.cr_bit(ba) & self.cr_bit(bb));
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crnor { bt, ba, bb } => {
                let v = !(self.cr_bit(ba) | self.cr_bit(bb));
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Creqv { bt, ba, bb } => {
                let v = !(self.cr_bit(ba) ^ self.cr_bit(bb));
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crandc { bt, ba, bb } => {
                let v = self.cr_bit(ba) & !self.cr_bit(bb);
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Crorc { bt, ba, bb } => {
                let v = self.cr_bit(ba) | !self.cr_bit(bb);
                self.set_cr_bit(bt, v);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mcrf { bf, bfa } => {
                let value = self.cr_field(bfa);
                self.set_cr_field(bf, value);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Extsb { ra, rs, rc } => {
                // Sign-extend the low byte of RS to 32 bits.
                let result = (self.gpr[rs as usize] as i8) as i32 as u32;
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Extsh { ra, rs, rc } => {
                // Sign-extend the low halfword of RS to 32 bits.
                let result = (self.gpr[rs as usize] as i16) as i32 as u32;
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cntlzw { ra, rs, rc } => {
                // Count leading zeros in the 32-bit RS. Result
                // is 0..=32; Rust's `leading_zeros()` returns 32
                // for input zero, matching the spec's range.
                let result = self.gpr[rs as usize].leading_zeros();
                self.gpr[ra as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::B {
                displacement,
                aa,
                lk,
            } => {
                let next_after = self.pc.wrapping_add(4);
                let target = if aa {
                    displacement as u32
                } else {
                    self.pc.wrapping_add(displacement as u32)
                };
                if lk {
                    self.lr = next_after;
                }
                self.pc = target;
            }
            PpcInstr::Bclr { bo, bi, lk } => {
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    // Branch target is LR with low 2 bits forced
                    // to zero (word-aligned).
                    let target = self.lr & !0x3;
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        // LR updates regardless per §2.4.
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
            }
            PpcInstr::Bcctr { bo, bi, lk } => {
                // Per §2.4.1, bcctr requires BO[2]=1 (no CTR
                // decrement) — decrementing CTR while branching
                // to it is undefined. Surface that as a clean
                // error rather than producing nonsense.
                if (bo & 0x04) == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    let target = self.ctr & !0x3;
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
            }
            PpcInstr::Lbz { rt, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u8(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        };
                    }
                };
                self.gpr[rt as usize] = u32::from(value);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhz { rt, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u16_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        };
                    }
                };
                self.gpr[rt as usize] = u32::from(value);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stb { rs, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                // Per §3.3.2: stb writes the LOW 8 bits of RS,
                // discarding the upper 24.
                let value = (self.gpr[rs as usize] & 0xFF) as u8;
                if self
                    .write_u8_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lmw { rt, ra, d } => {
                // Per §3.3.5: if RA is in the loaded range,
                // including RA=0, the instruction is invalid.
                // Refuse rather than producing arbitrary state.
                if ra >= rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let mut ea = base.wrapping_add(i32::from(d) as u32);
                for r in rt..=31 {
                    let value = match mem.read_u32_be(ea) {
                        Some(v) => v,
                        None => {
                            // Pre-fault PC sticks at the lmw
                            // instruction itself, with the
                            // partially-completed loads visible
                            // in the GPR file (per spec
                            // "boundedly undefined").
                            return PpcStepResult::MemoryFault {
                                addr: ea,
                                was_write: false,
                            };
                        }
                    };
                    self.gpr[r as usize] = value;
                    ea = ea.wrapping_add(4);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lwzu { rt, ra, d } => {
                // "u"-form with RA=0 or RA==RT is invalid per spec.
                if ra == 0 || ra == rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = value;
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lbzu { rt, ra, d } => {
                if ra == 0 || ra == rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u8(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = u32::from(value);
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhzu { rt, ra, d } => {
                if ra == 0 || ra == rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u16_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = u32::from(value);
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lha { rt, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u16_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                // Sign-extend the loaded halfword to 32 bits.
                self.gpr[rt as usize] = (value as i16) as i32 as u32;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhau { rt, ra, d } => {
                if ra == 0 || ra == rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u16_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = (value as i16) as i32 as u32;
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stbu { rs, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = (self.gpr[rs as usize] & 0xFF) as u8;
                if self
                    .write_u8_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sthu { rs, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let value = (self.gpr[rs as usize] & 0xFFFF) as u16;
                if self
                    .write_u16_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfs { frt, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let bits32 = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                // Convert single → double, store as f64 bits.
                let single = f32::from_bits(bits32);
                self.fpr[frt as usize] = (single as f64).to_bits();
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfsu { frt, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let bits32 = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                let single = f32::from_bits(bits32);
                self.fpr[frt as usize] = (single as f64).to_bits();
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfd { frt, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let bits = match mem.read_u64_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.fpr[frt as usize] = bits;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfdu { frt, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let bits = match mem.read_u64_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.fpr[frt as usize] = bits;
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfs { frs, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let double = f64::from_bits(self.fpr[frs as usize]);
                let bits32 = (double as f32).to_bits();
                if self
                    .write_u32_be_observed(mem, write_observer, addr, bits32)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfsu { frs, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let double = f64::from_bits(self.fpr[frs as usize]);
                let bits32 = (double as f32).to_bits();
                if self
                    .write_u32_be_observed(mem, write_observer, addr, bits32)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Fadd { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, (a + b).to_bits(), rc);
            }
            PpcInstr::Fsub { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, (a - b).to_bits(), rc);
            }
            PpcInstr::Fmul { frt, fra, frc, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                self.finish_fp_result(frt, (a * c).to_bits(), rc);
            }
            PpcInstr::Fdiv { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                // IEEE-754 division — Rust's `/` produces
                // ±infinity / NaN naturally, matching the
                // FPU semantics for the no-trap default mode.
                self.finish_fp_result(frt, (a / b).to_bits(), rc);
            }
            PpcInstr::Fneg { frt, frb, rc } => {
                // Toggle the sign bit (bit 63 in MSB=0
                // numbering = high bit of the u64 pattern).
                let bits = self.fpr[frb as usize] ^ (1u64 << 63);
                self.finish_fp_result(frt, bits, rc);
            }
            PpcInstr::Fmr { frt, frb, rc } => {
                let bits = self.fpr[frb as usize];
                self.finish_fp_result(frt, bits, rc);
            }
            PpcInstr::Fabs { frt, frb, rc } => {
                // Clear the sign bit.
                let bits = self.fpr[frb as usize] & !(1u64 << 63);
                self.finish_fp_result(frt, bits, rc);
            }
            PpcInstr::Fnabs { frt, frb, rc } => {
                // Set the sign bit unconditionally.
                let bits = self.fpr[frb as usize] | (1u64 << 63);
                self.finish_fp_result(frt, bits, rc);
            }
            PpcInstr::Mffs { frt, rc } => {
                // Move FPSCR into FRT[32..63]; FRT[0..31] is
                // architecturally undefined per Book I §4.6.7.
                // Store the live FPSCR in the low half and zero
                // the high half for deterministic undefined bits.
                self.fpr[frt as usize] = u64::from(self.fpscr);
                self.finish_fp_record(rc);
            }
            PpcInstr::Mcrfs { bf, bfa } => {
                let value = self.fpscr_field(bfa);
                self.set_cr_field(bf, value);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mtfsb1 { bt, rc } => {
                self.set_fpscr_bit(bt, true);
                self.finish_fp_record(rc);
            }
            PpcInstr::Mtfsb0 { bt, rc } => {
                self.set_fpscr_bit(bt, false);
                self.finish_fp_record(rc);
            }
            PpcInstr::Mtfsfi { bf, u, rc } => {
                self.set_fpscr_field(bf, u);
                self.finish_fp_record(rc);
            }
            PpcInstr::Mtfsf { flm, frb, rc } => {
                let source = self.fpr[frb as usize] as u32;
                for field in 0u8..8 {
                    if (flm & (0x80u8 >> field)) != 0 {
                        let shift = 28 - (u32::from(field) * 4);
                        self.set_fpscr_field(field, ((source >> shift) & 0x0F) as u8);
                    }
                }
                self.finish_fp_record(rc);
            }
            PpcInstr::Fsel {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                // Per ISA Book I §4.6.5.1: pick FRC if FRA >= 0,
                // FRB otherwise. The check is on the IEEE-754
                // value with -0.0 treated as >= 0 (sign bit
                // alone doesn't pick the FALSE branch).
                let a = f64::from_bits(self.fpr[fra as usize]);
                let pick_frc = a >= 0.0;
                let bits = if pick_frc {
                    self.fpr[frc as usize]
                } else {
                    self.fpr[frb as usize]
                };
                self.finish_fp_result(frt, bits, rc);
            }
            PpcInstr::Fsqrt { frt, frb, rc } => {
                let v = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, v.sqrt().to_bits(), rc);
            }
            PpcInstr::Fsqrts { frt, frb, rc } => {
                let v = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, (v.sqrt() as f32 as f64).to_bits(), rc);
            }
            PpcInstr::Fres { frt, frb, rc } => {
                let v = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, ((1.0 / v) as f32 as f64).to_bits(), rc);
            }
            PpcInstr::Frsqrte { frt, frb, rc } => {
                let v = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, (1.0 / v.sqrt()).to_bits(), rc);
            }
            PpcInstr::Fadds { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                // Round to f32 precision then expand back to f64
                // for storage. Native `as f32 as f64` is the
                // canonical way to apply IEEE-754 round-to-single.
                let r32 = (a + b) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fsubs { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let r32 = (a - b) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fmuls { frt, fra, frc, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                let r32 = (a * c) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fdivs { frt, fra, frb, rc } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let r32 = (a / b) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fmadd {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                self.finish_fp_result(frt, a.mul_add(c, b).to_bits(), rc);
            }
            PpcInstr::Fmsub {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                self.finish_fp_result(frt, a.mul_add(c, -b).to_bits(), rc);
            }
            PpcInstr::Fnmadd {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                self.finish_fp_result(frt, (-a.mul_add(c, b)).to_bits(), rc);
            }
            PpcInstr::Fnmsub {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                self.finish_fp_result(frt, (-a.mul_add(c, -b)).to_bits(), rc);
            }
            PpcInstr::Fmadds {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                let r32 = a.mul_add(c, b) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fmsubs {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                let r32 = a.mul_add(c, -b) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fnmadds {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                let r32 = (-a.mul_add(c, b)) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Fnmsubs {
                frt,
                fra,
                frc,
                frb,
                rc,
            } => {
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let c = f64::from_bits(self.fpr[frc as usize]);
                let r32 = (-a.mul_add(c, -b)) as f32;
                self.finish_fp_result(frt, (r32 as f64).to_bits(), rc);
            }
            PpcInstr::Frsp { frt, frb, rc } => {
                let v = f64::from_bits(self.fpr[frb as usize]);
                self.finish_fp_result(frt, (v as f32 as f64).to_bits(), rc);
            }
            PpcInstr::Fctiw { frt, frb, rc } => {
                // Per ISA Book I §4.6.4.3: convert FRB to a
                // 32-bit signed integer; result lands in FRT
                // low 32 bits with high 32 "undefined". We zero
                // the high half for determinism.
                let v = f64::from_bits(self.fpr[frb as usize]);
                let i = Self::f64_to_i32_with_rounding_mode(v, self.fpscr_field(7));
                self.fpr[frt as usize] = u64::from(i as u32);
                self.finish_fp_record(rc);
            }
            PpcInstr::Fctiwz { frt, frb, rc } => {
                // fctiwz uses round-toward-zero regardless of
                // FPSCR.RN.
                let v = f64::from_bits(self.fpr[frb as usize]);
                let i = Self::f64_to_i32_with_rounding_mode(v, 1);
                self.fpr[frt as usize] = u64::from(i as u32);
                self.finish_fp_record(rc);
            }
            PpcInstr::Fcmpu { bf, fra, frb } | PpcInstr::Fcmpo { bf, fra, frb } => {
                // Per ISA Book I §4.6.6.2 / §4.6.7: write
                // LT/GT/EQ/UNO into CR field BF. Either operand
                // being NaN means "unordered" — set bit 3 (the
                // SO/UNO position). The fcmpu vs fcmpo
                // distinction is in FPSCR side effects on
                // signalling NaN, which we don't track —
                // both surface the same CR write.
                let a = f64::from_bits(self.fpr[fra as usize]);
                let b = f64::from_bits(self.fpr[frb as usize]);
                let nibble: u8 = if a.is_nan() || b.is_nan() {
                    0b0001 // UNO (the LSB-numbered bit 3 = SO/UNO)
                } else if a < b {
                    0b1000 // LT
                } else if a > b {
                    0b0100 // GT
                } else {
                    0b0010 // EQ
                };
                self.set_cr_field(bf, nibble);
                self.set_fpscr_compare_result(nibble);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfd { frs, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let bits = self.fpr[frs as usize];
                if self
                    .write_u64_be_observed(mem, write_observer, addr, bits)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfdu { frs, ra, d } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(i32::from(d) as u32);
                let bits = self.fpr[frs as usize];
                if self
                    .write_u64_be_observed(mem, write_observer, addr, bits)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfsx { frt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let bits32 = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                let single = f32::from_bits(bits32);
                self.fpr[frt as usize] = (single as f64).to_bits();
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfsux { frt, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let bits32 = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                let single = f32::from_bits(bits32);
                self.fpr[frt as usize] = (single as f64).to_bits();
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfdx { frt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let bits = match mem.read_u64_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.fpr[frt as usize] = bits;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lfdux { frt, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let bits = match mem.read_u64_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.fpr[frt as usize] = bits;
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfsx { frs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let double = f64::from_bits(self.fpr[frs as usize]);
                let bits32 = (double as f32).to_bits();
                if self
                    .write_u32_be_observed(mem, write_observer, addr, bits32)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfsux { frs, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let double = f64::from_bits(self.fpr[frs as usize]);
                let bits32 = (double as f32).to_bits();
                if self
                    .write_u32_be_observed(mem, write_observer, addr, bits32)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfdx { frs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let bits = self.fpr[frs as usize];
                if self
                    .write_u64_be_observed(mem, write_observer, addr, bits)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stfdux { frs, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let bits = self.fpr[frs as usize];
                if self
                    .write_u64_be_observed(mem, write_observer, addr, bits)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stmw { rs, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let mut ea = base.wrapping_add(i32::from(d) as u32);
                for r in rs..=31 {
                    let value = self.gpr[r as usize];
                    if self
                        .write_u32_be_observed(mem, write_observer, ea, value)
                        .is_none()
                    {
                        return PpcStepResult::MemoryFault {
                            addr: ea,
                            was_write: true,
                        };
                    }
                    ea = ea.wrapping_add(4);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sth { rs, ra, d } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = (self.gpr[rs as usize] & 0xFFFF) as u16;
                if self
                    .write_u16_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            // X-form indexed memory ops. Effective address is
            // `(RA|0) + RB` (RA=0 means literal zero, just like
            // the D-form variants).
            PpcInstr::Lwzx { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                match mem.read_u32_be(addr) {
                    Some(v) => self.gpr[rt as usize] = v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lwarx { rt, ra, rb } => {
                let addr = self.x_form_ea(ra, rb);
                let value = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = value;
                self.reservation_addr = Some(addr);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lwbrx { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                match mem.read_u32_be(addr) {
                    Some(v) => self.gpr[rt as usize] = v.swap_bytes(),
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lbzx { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                match mem.read_u8(addr) {
                    Some(v) => self.gpr[rt as usize] = u32::from(v),
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhbrx { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                match mem.read_u16_be(addr) {
                    Some(v) => self.gpr[rt as usize] = u32::from(v.swap_bytes()),
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhzx { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                match mem.read_u16_be(addr) {
                    Some(v) => self.gpr[rt as usize] = u32::from(v),
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stswi { rs, ra, nb } => {
                // Store String Word Immediate. Symmetric of Lswi.
                let n = if nb == 0 { 32u32 } else { u32::from(nb) };
                let ea = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let mut current = u32::from(rs);
                let mut i = 0u32;
                while i < n {
                    let shift = 24 - (i % 4) * 8;
                    let byte = ((self.gpr[(current & 0x1F) as usize] >> shift) & 0xFF) as u8;
                    if self
                        .write_u8_observed(mem, write_observer, ea.wrapping_add(i), byte)
                        .is_none()
                    {
                        return PpcStepResult::MemoryFault {
                            addr: ea.wrapping_add(i),
                            was_write: true,
                        };
                    }
                    i += 1;
                    if i.is_multiple_of(4) {
                        current = current.wrapping_add(1);
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stswx { rs, ra, rb } => {
                let n = self.xer & 0x7F;
                let ea = self.x_form_ea(ra, rb);
                let mut current = u32::from(rs);
                let mut i = 0u32;
                while i < n {
                    let shift = 24 - (i % 4) * 8;
                    let byte = ((self.gpr[(current & 0x1F) as usize] >> shift) & 0xFF) as u8;
                    if self
                        .write_u8_observed(mem, write_observer, ea.wrapping_add(i), byte)
                        .is_none()
                    {
                        return PpcStepResult::MemoryFault {
                            addr: ea.wrapping_add(i),
                            was_write: true,
                        };
                    }
                    i += 1;
                    if i.is_multiple_of(4) {
                        current = current.wrapping_add(1);
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lswi { rt, ra, nb } => {
                // Load String Word Immediate. NB == 0 means 32.
                // Pack NB bytes from EA into consecutive GPRs
                // starting at RT, big-endian within each word,
                // wrapping back to GPR0 after GPR31. Last word
                // is right-padded with zeros.
                let n = if nb == 0 { 32u32 } else { u32::from(nb) };
                let ea = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let mut current = u32::from(rt);
                let mut i = 0u32;
                while i < n {
                    if i.is_multiple_of(4) {
                        // New register: clear before packing.
                        self.gpr[(current & 0x1F) as usize] = 0;
                    }
                    let byte = match mem.read_u8(ea.wrapping_add(i)) {
                        Some(b) => b,
                        None => {
                            return PpcStepResult::MemoryFault {
                                addr: ea.wrapping_add(i),
                                was_write: false,
                            }
                        }
                    };
                    let shift = 24 - (i % 4) * 8;
                    self.gpr[(current & 0x1F) as usize] |= u32::from(byte) << shift;
                    i += 1;
                    if i.is_multiple_of(4) {
                        current = current.wrapping_add(1);
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lswx { rt, ra, rb } => {
                let n = self.xer & 0x7F;
                let ea = self.x_form_ea(ra, rb);
                let mut current = u32::from(rt);
                let mut i = 0u32;
                while i < n {
                    if i.is_multiple_of(4) {
                        self.gpr[(current & 0x1F) as usize] = 0;
                    }
                    let byte = match mem.read_u8(ea.wrapping_add(i)) {
                        Some(b) => b,
                        None => {
                            return PpcStepResult::MemoryFault {
                                addr: ea.wrapping_add(i),
                                was_write: false,
                            }
                        }
                    };
                    let shift = 24 - (i % 4) * 8;
                    self.gpr[(current & 0x1F) as usize] |= u32::from(byte) << shift;
                    i += 1;
                    if i.is_multiple_of(4) {
                        current = current.wrapping_add(1);
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stwcx { rs, ra, rb } => {
                let addr = self.x_form_ea(ra, rb);
                let success = self.reservation_addr == Some(addr);
                if success {
                    let value = self.gpr[rs as usize];
                    if self
                        .write_u32_be_observed(mem, write_observer, addr, value)
                        .is_none()
                    {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        };
                    }
                }
                self.reservation_addr = None;
                self.update_cr0_from_store_conditional(success);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stwx { rs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = self.gpr[rs as usize];
                if self
                    .write_u32_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stwbrx { rs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = self.gpr[rs as usize].swap_bytes();
                if self
                    .write_u32_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stbx { rs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = (self.gpr[rs as usize] & 0xFF) as u8;
                if self
                    .write_u8_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stbux { rs, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let value = (self.gpr[rs as usize] & 0xFF) as u8;
                if self
                    .write_u8_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sthx { rs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = (self.gpr[rs as usize] & 0xFFFF) as u16;
                if self
                    .write_u16_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sthbrx { rs, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = ((self.gpr[rs as usize] & 0xFFFF) as u16).swap_bytes();
                if self
                    .write_u16_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lhax { rt, ra, rb } => {
                let base = if ra == 0 { 0 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(self.gpr[rb as usize]);
                let value = match mem.read_u16_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                // Sign-extend halfword to 32 bits.
                self.gpr[rt as usize] = (value as i16) as i32 as u32;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lwzux { rt, ra, rb } => {
                if ra == 0 || ra == rt {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let value = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        }
                    }
                };
                self.gpr[rt as usize] = value;
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stwux { rs, ra, rb } => {
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let addr = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                let value = self.gpr[rs as usize];
                if self
                    .write_u32_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mtspr { spr, rs } => {
                let value = self.gpr[rs as usize];
                match spr {
                    1 => self.xer = value,
                    8 => self.lr = value,
                    9 => self.ctr = value,
                    _ => {
                        return PpcStepResult::Unimplemented(
                            PpcDecodeError::UnsupportedSecondaryOpcode {
                                primary: 31,
                                secondary: 467,
                            },
                        );
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mfcr { rt } => {
                self.gpr[rt as usize] = self.cr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mtcrf { fxm, rs } => {
                let value = self.gpr[rs as usize];
                // For each bit i of FXM that's 1, copy the
                // corresponding 4-bit CR field from RS into CR.
                let mut new_cr = self.cr;
                for i in 0..8u8 {
                    if (fxm >> (7 - i)) & 1 == 1 {
                        let shift = 28 - (i as u32) * 4;
                        let mask = 0x0F_u32 << shift;
                        new_cr = (new_cr & !mask) | (value & mask);
                    }
                }
                self.cr = new_cr;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Dcbz { ra, rb } => {
                let ea = self.x_form_ea(ra, rb);
                let block_start = ea & !31;
                for offset in 0..32u32 {
                    let addr = block_start.wrapping_add(offset);
                    if self
                        .write_u8_observed(mem, write_observer, addr, 0)
                        .is_none()
                    {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: true,
                        };
                    }
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Sync
            | PpcInstr::Isync
            | PpcInstr::Eieio
            | PpcInstr::Dcbst { .. }
            | PpcInstr::Dcbf { .. }
            | PpcInstr::Dcbt { .. }
            | PpcInstr::Dcbtst { .. }
            | PpcInstr::Icbi { .. } => {
                // Memory/cache barriers and prefetch hints are no-ops
                // in single-threaded user-mode emulation.
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mfspr { rt, spr } => {
                let value = match spr {
                    1 => self.xer,
                    8 => self.lr,
                    9 => self.ctr,
                    _ => {
                        return PpcStepResult::Unimplemented(
                            PpcDecodeError::UnsupportedSecondaryOpcode {
                                primary: 31,
                                secondary: 339,
                            },
                        );
                    }
                };
                self.gpr[rt as usize] = value;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Lwz { rt, ra, d } => {
                // RA=0 means literal 0 per ISA Book I §3.3.2.
                let base = if ra == 0 { 0u32 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = match mem.read_u32_be(addr) {
                    Some(v) => v,
                    None => {
                        return PpcStepResult::MemoryFault {
                            addr,
                            was_write: false,
                        };
                    }
                };
                self.gpr[rt as usize] = value;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stw { rs, ra, d } => {
                let base = if ra == 0 { 0u32 } else { self.gpr[ra as usize] };
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = self.gpr[rs as usize];
                if self
                    .write_u32_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Addic { rt, ra, si } => {
                let (result, ca) =
                    Self::add_with_carry(self.gpr[ra as usize], i32::from(si) as u32, false);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::AddicDot { rt, ra, si } => {
                let (result, ca) =
                    Self::add_with_carry(self.gpr[ra as usize], i32::from(si) as u32, false);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                self.update_cr0_from_signed(result);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subfic { rt, ra, si } => {
                // RT = ~RA + sext(SI) + 1 → use add_with_carry
                // with the inverted RA and carry_in=true.
                let (result, ca) =
                    Self::add_with_carry(!self.gpr[ra as usize], i32::from(si) as u32, true);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Addc { rt, ra, rb, oe, rc } => {
                let overflow =
                    Self::signed_add_overflow(self.gpr[ra as usize], self.gpr[rb as usize], false);
                let (result, ca) =
                    Self::add_with_carry(self.gpr[ra as usize], self.gpr[rb as usize], false);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Adde { rt, ra, rb, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow = Self::signed_add_overflow(
                    self.gpr[ra as usize],
                    self.gpr[rb as usize],
                    carry_in,
                );
                let (result, ca) =
                    Self::add_with_carry(self.gpr[ra as usize], self.gpr[rb as usize], carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subfc { rt, ra, rb, oe, rc } => {
                let overflow =
                    Self::signed_sub_overflow(self.gpr[rb as usize], self.gpr[ra as usize], false);
                let (result, ca) =
                    Self::add_with_carry(!self.gpr[ra as usize], self.gpr[rb as usize], true);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subfe { rt, ra, rb, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow = Self::signed_sub_overflow(
                    self.gpr[rb as usize],
                    self.gpr[ra as usize],
                    !carry_in,
                );
                let (result, ca) =
                    Self::add_with_carry(!self.gpr[ra as usize], self.gpr[rb as usize], carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Addze { rt, ra, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow = Self::signed_add_overflow(self.gpr[ra as usize], 0, carry_in);
                let (result, ca) = Self::add_with_carry(self.gpr[ra as usize], 0, carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Addme { rt, ra, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow =
                    Self::signed_add_overflow(self.gpr[ra as usize], 0xFFFF_FFFF, carry_in);
                let (result, ca) =
                    Self::add_with_carry(self.gpr[ra as usize], 0xFFFF_FFFF, carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subfze { rt, ra, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow = Self::signed_sub_overflow(0, self.gpr[ra as usize], !carry_in);
                let (result, ca) = Self::add_with_carry(!self.gpr[ra as usize], 0, carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subfme { rt, ra, oe, rc } => {
                let carry_in = self.xer_ca();
                let overflow =
                    Self::signed_sub_overflow(0xFFFF_FFFF, self.gpr[ra as usize], !carry_in);
                let (result, ca) =
                    Self::add_with_carry(!self.gpr[ra as usize], 0xFFFF_FFFF, carry_in);
                self.gpr[rt as usize] = result;
                self.set_xer_ca(ca);
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mulli { rt, ra, si } => {
                // Signed multiply, low 32 bits of result
                // (truncating wraps, no overflow flag).
                let lhs = self.gpr[ra as usize] as i32;
                let rhs = i32::from(si);
                self.gpr[rt as usize] = lhs.wrapping_mul(rhs) as u32;
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Add { rt, ra, rb, oe, rc } => {
                let overflow =
                    Self::signed_add_overflow(self.gpr[ra as usize], self.gpr[rb as usize], false);
                let result = self.gpr[ra as usize].wrapping_add(self.gpr[rb as usize]);
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cmpi { bf, l, ra, si } => {
                if l {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let lhs = self.gpr[ra as usize] as i32;
                let rhs = i32::from(si);
                self.set_cr_compare(bf, lhs.cmp(&rhs));
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cmpli { bf, l, ra, ui } => {
                if l {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let lhs = self.gpr[ra as usize];
                let rhs = u32::from(ui);
                self.set_cr_compare(bf, lhs.cmp(&rhs));
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cmp { bf, l, ra, rb } => {
                if l {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let lhs = self.gpr[ra as usize] as i32;
                let rhs = self.gpr[rb as usize] as i32;
                self.set_cr_compare(bf, lhs.cmp(&rhs));
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Cmpl { bf, l, ra, rb } => {
                if l {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let lhs = self.gpr[ra as usize];
                let rhs = self.gpr[rb as usize];
                self.set_cr_compare(bf, lhs.cmp(&rhs));
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Bc {
                bo,
                bi,
                displacement,
                aa,
                lk,
            } => {
                let take = self.evaluate_branch_condition(bo, bi);
                let next_after = self.pc.wrapping_add(4);
                if take {
                    let target = if aa {
                        displacement as u32
                    } else {
                        self.pc.wrapping_add(displacement as u32)
                    };
                    if lk {
                        self.lr = next_after;
                    }
                    self.pc = target;
                } else {
                    if lk {
                        // Per §2.4: LR is updated regardless of
                        // whether the branch is taken.
                        self.lr = next_after;
                    }
                    self.pc = next_after;
                }
            }
            PpcInstr::Neg { rt, ra, oe, rc } => {
                let overflow = self.gpr[ra as usize] == 0x8000_0000;
                let result = (!self.gpr[ra as usize]).wrapping_add(1);
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mullw { rt, ra, rb, oe, rc } => {
                // Signed wrapping multiply, low 32 bits.
                let lhs = self.gpr[ra as usize] as i32;
                let rhs = self.gpr[rb as usize] as i32;
                let product = i64::from(lhs) * i64::from(rhs);
                let overflow = product < i64::from(i32::MIN) || product > i64::from(i32::MAX);
                let result = product as u32;
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mulhw { rt, ra, rb, rc } => {
                // Upper 32 bits of the 64-bit signed product.
                let lhs = i64::from(self.gpr[ra as usize] as i32);
                let rhs = i64::from(self.gpr[rb as usize] as i32);
                let prod = lhs.wrapping_mul(rhs);
                let result = (prod >> 32) as u32;
                self.gpr[rt as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Mulhwu { rt, ra, rb, rc } => {
                // Upper 32 bits of the 64-bit unsigned product.
                let lhs = u64::from(self.gpr[ra as usize]);
                let rhs = u64::from(self.gpr[rb as usize]);
                let prod = lhs.wrapping_mul(rhs);
                let result = (prod >> 32) as u32;
                self.gpr[rt as usize] = result;
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Divw { rt, ra, rb, oe, rc } => {
                let lhs = self.gpr[ra as usize] as i32;
                let rhs = self.gpr[rb as usize] as i32;
                // Per §3.3.7, divide-by-zero and i32::MIN/-1
                // produce undefined RT — pick a safe value
                // (zero) rather than panicking on UB.
                let overflow = rhs == 0 || (lhs == i32::MIN && rhs == -1);
                let result = if overflow { 0u32 } else { (lhs / rhs) as u32 };
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Divwu { rt, ra, rb, oe, rc } => {
                let lhs = self.gpr[ra as usize];
                let rhs = self.gpr[rb as usize];
                // Per spec: divide-by-zero produces undefined RT.
                // We pick zero to avoid panicking on Rust's u32 / 0 UB.
                let overflow = rhs == 0;
                let result = if overflow { 0 } else { lhs / rhs };
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Subf { rt, ra, rb, oe, rc } => {
                // RT = ~RA + RB + 1 = RB - RA. Same OE handling
                // as `add`.
                let overflow =
                    Self::signed_sub_overflow(self.gpr[rb as usize], self.gpr[ra as usize], false);
                let result = self.gpr[rb as usize].wrapping_sub(self.gpr[ra as usize]);
                self.gpr[rt as usize] = result;
                if oe {
                    self.set_xer_ov_so(overflow);
                }
                if rc {
                    self.update_cr0_from_signed(result);
                }
                self.pc = self.pc.wrapping_add(4);
            }
            PpcInstr::Stwu { rs, ra, d } => {
                // The "u"-form is invalid when RA = 0 per the spec
                // (no base register to update).
                if ra == 0 {
                    return Self::illegal_instruction_result(
                        instr_word,
                        PpcIllegalInstructionReason::InvalidForm,
                    );
                }
                let base = self.gpr[ra as usize];
                let addr = base.wrapping_add(i32::from(d) as u32);
                let value = self.gpr[rs as usize];
                if self
                    .write_u32_be_observed(mem, write_observer, addr, value)
                    .is_none()
                {
                    return PpcStepResult::MemoryFault {
                        addr,
                        was_write: true,
                    };
                }
                // Then update RA (atomic with the store per spec).
                self.gpr[ra as usize] = addr;
                self.pc = self.pc.wrapping_add(4);
            }
        }
        PpcStepResult::Stepped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_keeps_large_host_caches_off_the_stack() {
        let size = std::mem::size_of::<PpcCpu>();
        assert!(size <= 1024, "PpcCpu grew to {size} stack bytes");
    }

    #[test]
    fn decode_cache_reuses_repeated_successful_instruction_words() {
        let mut cpu = PpcCpu::new();

        assert_eq!(cpu.step_instruction(0x6000_0000), PpcStepResult::Stepped);
        assert_eq!(cpu.pc, 4);
        assert_eq!(cpu.decode_cache_entry_count(), 1);
        assert_eq!(cpu.step_instruction(0x6000_0000), PpcStepResult::Stepped);
        assert_eq!(cpu.pc, 8);
        assert_eq!(cpu.decode_cache_entry_count(), 1);
    }

    #[test]
    fn decode_cache_reuses_repeated_decode_errors() {
        let mut cpu = PpcCpu::new();
        let unsupported_primary_one = 0x0400_0000;

        assert_eq!(
            cpu.step_instruction(unsupported_primary_one),
            PpcStepResult::Unimplemented(PpcDecodeError::UnsupportedPrimaryOpcode(1))
        );
        assert_eq!(cpu.decode_cache_entry_count(), 1);
        assert_eq!(
            cpu.step_instruction(unsupported_primary_one),
            PpcStepResult::Unimplemented(PpcDecodeError::UnsupportedPrimaryOpcode(1))
        );
        assert_eq!(cpu.decode_cache_entry_count(), 1);
    }

    #[test]
    fn stbux_decodes_and_updates_the_base_register() {
        let word = 0x7F2B_61EE;
        assert_eq!(
            crate::decode::decode(word),
            Ok(PpcInstr::Stbux {
                rs: 25,
                ra: 11,
                rb: 12,
            })
        );

        let mut cpu = PpcCpu::new();
        let mut mem = PpcSectionMem::new();
        mem.add_region(0x1000, vec![0; 0x100]);
        cpu.gpr[25] = 0x1234_56AB;
        cpu.gpr[11] = 0x1000;
        cpu.gpr[12] = 0x20;

        assert_eq!(cpu.step(&mut mem, word), PpcStepResult::Stepped);
        assert_eq!(mem.read_u8(0x1020), Some(0xAB));
        assert_eq!(cpu.gpr[11], 0x1020);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn stbux_rejects_ra_zero() {
        let word = 0x7F20_61EE;
        let mut cpu = PpcCpu::new();
        let mut mem = PpcSectionMem::new();

        assert_eq!(
            cpu.step(&mut mem, word),
            PpcStepResult::Exception(PpcException::IllegalInstruction {
                word,
                reason: PpcIllegalInstructionReason::InvalidForm,
            })
        );
        assert_eq!(cpu.pc, 0);
    }

    #[test]
    fn bcctr_null_target_branches_to_zero() {
        let mut cpu = PpcCpu::new();
        cpu.ctr = 0;

        assert_eq!(cpu.step_instruction(0x4E80_0420), PpcStepResult::Stepped);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.lr, 0);
        assert_eq!(cpu.gpr[3], 0);
    }
}
