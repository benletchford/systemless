//! PowerPC instruction decoder.
//!
//! Maps a 32-bit instruction word to a [`PpcInstr`] variant. The
//! decoder is a pure function — no CPU state, no memory access —
//! so it can be exercised in isolation against synthetic
//! instruction words.
//!
//! Encodings cited from *PowerPC User Instruction Set Architecture,
//! Book I*, Version 2.01 (transcribed in
//! `systemless-inside-macintosh-md/PowerPC_User_ISA_Book_I_v2_01.md`).

/// One decoded PowerPC instruction. The decoder produces these
/// from raw 32-bit instruction words; the dispatcher consumes them.
///
/// Encodings cited from *PowerPC User Instruction Set Architecture,
/// Book I*, Version 2.01 (transcribed in
/// `systemless-inside-macintosh-md/PowerPC_User_ISA_Book_I_v2_01.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcInstr {
    /// `twi TO, RA, SI` — D-form, OPCD = 3. (§3.3.16)
    /// Trap Word Immediate. If any selected TO comparison between
    /// `RA` and the sign-extended immediate is true, the core
    /// raises a program trap exception.
    Twi { to: u8, ra: u8, si: i16 },
    /// `addi RT, RA, SI` — D-form, OPCD = 14. (§3.3.8)
    /// `RT = (RA|0) + sign_extend(SI)`.
    Addi { rt: u8, ra: u8, si: i16 },
    /// `addis RT, RA, SI` — D-form, OPCD = 15. (§3.3.8)
    /// `RT = (RA|0) + sign_extend(SI || 0x0000)`.
    Addis { rt: u8, ra: u8, si: i16 },
    /// `ori RA, RS, UI` — D-form, OPCD = 24. (§3.3.10)
    /// `RA = RS | zero_extend(UI)`. The all-zero variant
    /// (`ori 0,0,0` = encoding `0x60000000`) is the canonical
    /// PowerPC `nop`.
    Ori { ra: u8, rs: u8, ui: u16 },
    /// `oris RA, RS, UI` — D-form, OPCD = 25. (§3.3.10)
    /// `RA = RS | (UI << 16)`. The "shifted" pair of `ori` —
    /// used together they load arbitrary 32-bit constants
    /// (`lis ra, hi; ori ra, ra, lo`).
    Oris { ra: u8, rs: u8, ui: u16 },
    /// `xori RA, RS, UI` — D-form, OPCD = 26. (§3.3.10)
    /// `RA = RS ^ zero_extend(UI)`.
    Xori { ra: u8, rs: u8, ui: u16 },
    /// `xoris RA, RS, UI` — D-form, OPCD = 27. (§3.3.10)
    /// `RA = RS ^ (UI << 16)`.
    Xoris { ra: u8, rs: u8, ui: u16 },
    /// `andi. RA, RS, UI` — D-form, OPCD = 28. (§3.3.10)
    /// `RA = RS & zero_extend(UI)`. ALWAYS updates CR0
    /// (the trailing `.` is part of the mnemonic itself; there
    /// is no non-recording form).
    AndiDot { ra: u8, rs: u8, ui: u16 },
    /// `andis. RA, RS, UI` — D-form, OPCD = 29. (§3.3.10)
    /// `RA = RS & (UI << 16)`. ALWAYS updates CR0.
    AndisDot { ra: u8, rs: u8, ui: u16 },
    /// `slw RA, RS, RB` — X-form, OPCD = 31, XO = 24.
    /// (§3.3.12.2) Shift left word by `RB[26..31]`. Shift
    /// amounts ≥ 32 produce zero per the spec.
    Slw { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `srw RA, RS, RB` — X-form, OPCD = 31, XO = 536.
    /// (§3.3.12.2) Shift right word logical (zero-fill).
    Srw { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `sraw RA, RS, RB` — X-form, OPCD = 31, XO = 792.
    /// (§3.3.12.2) Shift right word algebraic (sign-fill).
    /// Updates the XER carry flag (CA) per §3.3.12.2: CA is
    /// set when the input is negative and any 1-bits are
    /// shifted out.
    Sraw { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `srawi RA, RS, SH` — X-form, OPCD = 31, XO = 824.
    /// (§3.3.12.2) Shift right algebraic by an immediate 5-bit
    /// count. Same XER.CA semantics as `sraw`.
    Srawi { ra: u8, rs: u8, sh: u8, rc: bool },
    /// `rlwimi RA, RS, SH, MB, ME` — M-form, OPCD = 20.
    /// (§3.3.12.1) Rotate Left Word Immediate then Mask Insert.
    /// `RA = (ROTL32(RS, SH) & MASK(MB, ME)) | (RA & ~MASK(MB, ME))`.
    /// Inserts a bitfield from RS into RA, leaving the bits
    /// outside the mask untouched. Used by compilers to update a
    /// single field within a packed structure word.
    Rlwimi {
        ra: u8,
        rs: u8,
        sh: u8,
        mb: u8,
        me: u8,
        rc: bool,
    },
    /// `rlwnm RA, RS, RB, MB, ME` — M-form, OPCD = 23.
    /// (§3.3.12.1) Rotate Left Word then AND with Mask.
    /// Same shape as `rlwinm` but the rotate amount comes from
    /// `RB[27..31]` (low 5 bits) instead of an immediate.
    Rlwnm {
        ra: u8,
        rs: u8,
        rb: u8,
        mb: u8,
        me: u8,
        rc: bool,
    },
    /// `rlwinm RA, RS, SH, MB, ME` — M-form, OPCD = 21. (§3.3.12.1)
    /// Rotate Left Word Immediate then AND with Mask.
    /// `RA = ROTL32(RS, SH) & MASK(MB, ME)`. The MSB=0 mask
    /// covers bits MB..ME inclusive when `mb <= me`; when
    /// `mb > me` it wraps around (bits MB..31 + 0..ME).
    /// `rc` selects CR0 update.
    ///
    /// rlwinm is one of the most-used PowerPC instructions
    /// because so many extended mnemonics decompose into it:
    ///   - `slwi rx, ry, n`   = `rlwinm rx, ry, n,    0,    31-n`
    ///   - `srwi rx, ry, n`   = `rlwinm rx, ry, 32-n, n,    31`
    ///   - `clrlwi rx, ry, n` = `rlwinm rx, ry, 0,    n,    31`
    ///   - `clrrwi rx, ry, n` = `rlwinm rx, ry, 0,    0,    31-n`
    ///   - `extlwi rx, ry, n, b` = `rlwinm rx, ry, b, 0, n-1`
    ///   - `rotlwi rx, ry, n` = `rlwinm rx, ry, n,    0,    31`
    Rlwinm {
        ra: u8,
        rs: u8,
        sh: u8,
        mb: u8,
        me: u8,
        rc: bool,
    },
    /// `or RA, RS, RB` — X-form, OPCD = 31, XO = 444. (§3.3.10)
    /// `RA = RS | RB`. If `rc`, also sets CR0 from the result.
    /// `mr RA, RS` (move register) is the extended mnemonic for
    /// `or RA, RS, RS`.
    Or { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `and RA, RS, RB` — X-form, OPCD = 31, XO = 28. (§3.3.10)
    /// `RA = RS & RB`. Note: there is no D-form `and`; the
    /// immediate variants are `andi.` / `andis.` (always Rc=1).
    And { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `xor RA, RS, RB` — X-form, OPCD = 31, XO = 316. (§3.3.10)
    /// `RA = RS ^ RB`.
    Xor { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `nor RA, RS, RB` — X-form, OPCD = 31, XO = 124. (§3.3.10)
    /// `RA = ~(RS | RB)`. The extended mnemonic `not RA, RS`
    /// expands to `nor RA, RS, RS` (one's complement).
    Nor { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `nand RA, RS, RB` — X-form, OPCD = 31, XO = 476.
    /// (§3.3.10) `RA = ~(RS & RB)`.
    Nand { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `andc RA, RS, RB` — X-form, OPCD = 31, XO = 60.
    /// (§3.3.10) `RA = RS & ~RB` ("AND with complement").
    Andc { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `orc RA, RS, RB` — X-form, OPCD = 31, XO = 412.
    /// (§3.3.10) `RA = RS | ~RB` ("OR with complement").
    Orc { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `eqv RA, RS, RB` — X-form, OPCD = 31, XO = 284.
    /// (§3.3.10) `RA = ~(RS ^ RB)` (equivalent / XNOR).
    Eqv { ra: u8, rs: u8, rb: u8, rc: bool },
    /// `crand BT, BA, BB` — XL-form, OPCD = 19, XO = 257.
    /// (§2.4.2) `CR[BT] = CR[BA] & CR[BB]`. BT/BA/BB are
    /// 5-bit indices into the 32-bit Condition Register
    /// (MSB=0; 0..3 = CR0, 4..7 = CR1, …).
    Crand { bt: u8, ba: u8, bb: u8 },
    /// `cror BT, BA, BB` — XL-form, OPCD = 19, XO = 449.
    /// (§2.4.2) `CR[BT] = CR[BA] | CR[BB]`. The extended
    /// mnemonic `crmove BT, BA` = `cror BT, BA, BA` (copy bit).
    Cror { bt: u8, ba: u8, bb: u8 },
    /// `crxor BT, BA, BB` — XL-form, OPCD = 19, XO = 193.
    /// (§2.4.2) `CR[BT] = CR[BA] ^ CR[BB]`. The extended
    /// mnemonic `crclr BT` = `crxor BT, BT, BT` (clear bit).
    Crxor { bt: u8, ba: u8, bb: u8 },
    /// `crnand BT, BA, BB` — XL-form, XO = 225. (§2.4.2)
    /// `CR[BT] = ~(CR[BA] & CR[BB])`.
    Crnand { bt: u8, ba: u8, bb: u8 },
    /// `crnor BT, BA, BB` — XL-form, XO = 33. (§2.4.2)
    /// `CR[BT] = ~(CR[BA] | CR[BB])`. The `crnot BT, BA`
    /// extended mnemonic expands to `crnor BT, BA, BA`.
    Crnor { bt: u8, ba: u8, bb: u8 },
    /// `creqv BT, BA, BB` — XL-form, XO = 289. (§2.4.2)
    /// `CR[BT] = ~(CR[BA] ^ CR[BB])`. The `crset BT`
    /// extended mnemonic expands to `creqv BT, BT, BT` (set bit).
    Creqv { bt: u8, ba: u8, bb: u8 },
    /// `crandc BT, BA, BB` — XL-form, XO = 129. (§2.4.2)
    /// `CR[BT] = CR[BA] & ~CR[BB]`.
    Crandc { bt: u8, ba: u8, bb: u8 },
    /// `crorc BT, BA, BB` — XL-form, XO = 417. (§2.4.2)
    /// `CR[BT] = CR[BA] | ~CR[BB]`.
    Crorc { bt: u8, ba: u8, bb: u8 },
    /// `mcrf BF, BFA` — XL-form, XO = 0. (§2.4.2)
    /// Move Condition Register Field. Copies the 4 bits of
    /// `CR field BFA` into `CR field BF`. BF and BFA are 3-bit
    /// CR-field selectors (0..7); the other CR fields are
    /// untouched.
    Mcrf { bf: u8, bfa: u8 },
    /// `extsb RA, RS` — X-form, OPCD = 31, XO = 954. (§3.3.11)
    /// Sign-extend the low 8 bits of RS into RA. The RB slot is
    /// reserved (must be zero per spec).
    Extsb { ra: u8, rs: u8, rc: bool },
    /// `extsh RA, RS` — X-form, OPCD = 31, XO = 922. (§3.3.11)
    /// Sign-extend the low 16 bits of RS into RA.
    Extsh { ra: u8, rs: u8, rc: bool },
    /// `cntlzw RA, RS` — X-form, OPCD = 31, XO = 26. (§3.3.11)
    /// Count leading zeros in the 32-bit RS, store count
    /// (0..=32) in RA.
    Cntlzw { ra: u8, rs: u8, rc: bool },
    /// `tw TO, RA, RB` — X-form, OPCD = 31, XO = 4. (§3.3.16)
    /// Trap Word. If any selected TO comparison between `RA` and
    /// `RB` is true, the core raises a program trap exception.
    Tw { to: u8, ra: u8, rb: u8 },
    /// `sc` — SC-form, OPCD = 17. (§2.3.4)
    /// System Call. User-mode hosts surface it as a structured
    /// exception instead of entering supervisor state.
    Sc { lev: u8 },
    /// `b target` / `bl target` — I-form, OPCD = 18. (§2.4)
    /// `displacement` is the signed 26-bit branch displacement
    /// already shifted into a `i32` (i.e. the value of
    /// `EXTS(LI || 0b00)`). `aa` selects absolute vs.
    /// PC-relative; `lk` requests link-register update.
    B {
        displacement: i32,
        aa: bool,
        lk: bool,
    },
    /// `bclr` / `blr` (extended mnemonic with BO=20) — XL-form,
    /// OPCD = 19, XO = 16. (§2.4)
    ///
    /// `bo` and `bi` select CTR/CR branch resolution; a value of
    /// 20 means "always branch" (the encoding produced by `blr`).
    Bclr { bo: u8, bi: u8, lk: bool },
    /// `mtspr SPR, RS` — XFX-form, OPCD = 31, XO = 467. (§3.3.13)
    /// Writes the contents of `RS` into the named SPR. The
    /// extended mnemonics `mtxer`, `mtlr`, `mtctr` decode to
    /// `mtspr 1`, `mtspr 8`, `mtspr 9`.
    Mtspr { spr: u16, rs: u8 },
    /// `mfspr RT, SPR` — XFX-form, OPCD = 31, XO = 339. (§3.3.13)
    /// Reads the named SPR into `RT`. Extended mnemonics
    /// `mfxer`, `mflr`, `mfctr`.
    Mfspr { rt: u8, spr: u16 },
    /// `mfcr RT` — XFX-form, OPCD = 31, XO = 19. (§3.3.13)
    /// Move From Condition Register: copies the entire 32-bit CR
    /// into RT.
    Mfcr { rt: u8 },
    /// `mtcrf FXM, RS` — XFX-form, OPCD = 31, XO = 144. (§3.3.13)
    /// Move To Condition Register Fields. The 8-bit `fxm` mask
    /// selects which of CR0..CR7 are updated from the
    /// corresponding 4-bit fields of RS. The extended mnemonic
    /// `mtcr Rx` expands to `mtcrf 0xFF, Rx`.
    Mtcrf { fxm: u8, rs: u8 },
    /// `sync` — X-form, OPCD = 31, XO = 598. Memory barrier.
    /// In single-threaded user-mode emulation this is a no-op
    /// (no out-of-order execution, no SMP).
    Sync,
    /// `isync` — XL-form, OPCD = 19, XO = 150. Instruction
    /// synchronisation barrier. No-op in this emulator.
    Isync,
    /// `eieio` — X-form, OPCD = 31, XO = 854. Enforce In-order
    /// Execution of I/O. No-op in this emulator (no I/O bus).
    Eieio,
    /// `dcbst RA, RB` - X-form, OPCD = 31, XO = 54.
    /// Data Cache Block Store. No-op in this user-mode emulator.
    Dcbst { ra: u8, rb: u8 },
    /// `dcbf RA, RB` - X-form, OPCD = 31, XO = 86.
    /// Data Cache Block Flush. No-op in this user-mode emulator.
    Dcbf { ra: u8, rb: u8 },
    /// `dcbt CT, RA, RB` - X-form, OPCD = 31, XO = 278.
    /// Data Cache Block Touch. Treated as a prefetch hint.
    Dcbt { ct: u8, ra: u8, rb: u8 },
    /// `dcbtst CT, RA, RB` - X-form, OPCD = 31, XO = 246.
    /// Data Cache Block Touch for Store. Treated as a prefetch hint.
    Dcbtst { ct: u8, ra: u8, rb: u8 },
    /// `icbi RA, RB` - X-form, OPCD = 31, XO = 982.
    /// Instruction Cache Block Invalidate. No-op without a host
    /// instruction cache for translated PPC code.
    Icbi { ra: u8, rb: u8 },
    /// `dcbz RA, RB` - X-form, OPCD = 31, XO = 1014.
    /// Data Cache Block Set to Zero. Zeros the 32-byte modeled
    /// data-cache block containing `(RA|0) + RB`.
    Dcbz { ra: u8, rb: u8 },
    /// `lwz RT, D(RA)` — D-form, OPCD = 32. (§3.3.2)
    /// `RT = mem32_be[(RA|0) + sign_extend(D)]`. Big-endian
    /// 32-bit load, zero-extended into the 32-bit GPR (no-op
    /// since GPRs are already 32 bits in the user model). The
    /// "RA = 0 means literal 0" rule applies.
    Lwz { rt: u8, ra: u8, d: i16 },
    /// `stw RS, D(RA)` — D-form, OPCD = 36. (§3.3.2)
    /// `mem32_be[(RA|0) + sign_extend(D)] = RS`. Big-endian
    /// 32-bit store. RA=0 special case applies.
    Stw { rs: u8, ra: u8, d: i16 },
    /// `stwu RS, D(RA)` — D-form, OPCD = 37. (§3.3.2)
    /// `mem32_be[RA + sign_extend(D)] = RS; RA = RA + sign_extend(D)`.
    /// "Store with update" — used to atomically push a stack
    /// frame in the prologue. The RA=0 case is invalid for the
    /// `u`-form per the spec ("if RA=0, the instruction is
    /// invalid").
    Stwu { rs: u8, ra: u8, d: i16 },
    /// `mulli RT, RA, SI` — D-form, OPCD = 7. (§3.3.7)
    /// `RT = (RA * sign_extend(SI))[31:0]`. Wrapping signed
    /// multiplication, low 32 bits of the result.
    Mulli { rt: u8, ra: u8, si: i16 },
    /// `addic RT, RA, SI` — D-form, OPCD = 12. (§3.3.5)
    /// `RT = RA + sign_extend(SI)`. Sets XER.CA from the carry-out.
    Addic { rt: u8, ra: u8, si: i16 },
    /// `addic. RT, RA, SI` — D-form, OPCD = 13. (§3.3.5)
    /// As `addic`, plus always updates CR0 from the result.
    AddicDot { rt: u8, ra: u8, si: i16 },
    /// `subfic RT, RA, SI` — D-form, OPCD = 8. (§3.3.6)
    /// `RT = ~RA + sign_extend(SI) + 1` = `SI - RA`. Sets CA.
    Subfic { rt: u8, ra: u8, si: i16 },
    /// `addc RT, RA, RB` — XO-form, OPCD = 31, XO = 10. (§3.3.5)
    /// `RT = RA + RB`. Sets XER.CA. Used as the low half of a
    /// multi-precision add (paired with `adde` for the high
    /// halves).
    Addc {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `adde RT, RA, RB` — XO-form, OPCD = 31, XO = 138.
    /// (§3.3.5) `RT = RA + RB + CA`. Sets CA from the new
    /// carry-out. The high-half partner of `addc`.
    Adde {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `subfc RT, RA, RB` — XO-form, OPCD = 31, XO = 8.
    /// (§3.3.6) `RT = ~RA + RB + 1` = `RB - RA`. Sets CA.
    Subfc {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `subfe RT, RA, RB` — XO-form, OPCD = 31, XO = 136.
    /// (§3.3.6) `RT = ~RA + RB + CA`. Sets CA. The high-half
    /// partner of `subfc`.
    Subfe {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `addze RT, RA` — XO-form, OPCD = 31, XO = 202. (§3.3.5)
    /// `RT = RA + 0 + CA`. Sets CA. Used to propagate a carry
    /// from a previous `addic`/`addc`/`adde` into a register
    /// without combining with another operand.
    Addze { rt: u8, ra: u8, oe: bool, rc: bool },
    /// `addme RT, RA` — XO-form, OPCD = 31, XO = 234. (§3.3.5)
    /// `RT = RA + 0xFFFFFFFF + CA` = `RA - 1 + CA`. Sets CA.
    Addme { rt: u8, ra: u8, oe: bool, rc: bool },
    /// `subfze RT, RA` — XO-form, OPCD = 31, XO = 200. (§3.3.6)
    /// `RT = ~RA + 0 + CA`. Sets CA.
    Subfze { rt: u8, ra: u8, oe: bool, rc: bool },
    /// `subfme RT, RA` — XO-form, OPCD = 31, XO = 232. (§3.3.6)
    /// `RT = ~RA + 0xFFFFFFFF + CA` = `-RA - 1 + CA - 1`. Sets CA.
    Subfme { rt: u8, ra: u8, oe: bool, rc: bool },
    /// `add RT, RA, RB` — XO-form, OPCD = 31, XO = 266. (§3.3.5)
    /// `RT = RA + RB`. The `oe` bit requests XER overflow
    /// tracking. The `rc` bit selects CR0 update from the signed
    /// result.
    Add {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `subf RT, RA, RB` — XO-form, OPCD = 31, XO = 40. (§3.3.6)
    /// `RT = ~RA + RB + 1` = `RB - RA`. Note the subtrahend
    /// (`RA`) is the FIRST source operand, not the second; the
    /// extended mnemonic `sub Rx,Ry,Rz` expands to
    /// `subf Rx,Rz,Ry` with the operand order swapped.
    Subf {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `neg RT, RA` — XO-form, OPCD = 31, XO = 104. (§3.3.6)
    /// `RT = -RA` (two's complement). The RB field is reserved
    /// (must be zero per spec).
    Neg { rt: u8, ra: u8, oe: bool, rc: bool },
    /// `mullw RT, RA, RB` — XO-form, OPCD = 31, XO = 235.
    /// (§3.3.7) Multiply low word: `RT = (RA * RB)[31:0]`
    /// (signed multiply, wrapping low 32 bits).
    Mullw {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `mulhw RT, RA, RB` — XO-form, OPCD = 31, XO = 75.
    /// (§3.3.7) Multiply high word, signed. Returns the upper
    /// 32 bits of the 64-bit signed product. No OE form.
    Mulhw { rt: u8, ra: u8, rb: u8, rc: bool },
    /// `mulhwu RT, RA, RB` — XO-form, OPCD = 31, XO = 11.
    /// (§3.3.7) Multiply high word, unsigned. Upper 32 bits
    /// of the 64-bit unsigned product. No OE form.
    Mulhwu { rt: u8, ra: u8, rb: u8, rc: bool },
    /// `divw RT, RA, RB` — XO-form, OPCD = 31, XO = 491.
    /// (§3.3.7) Signed integer divide. Per spec, both
    /// `<anything>/0` and `0x8000_0000/-1` produce an undefined
    /// result; this dispatcher writes the dividend pass-through
    /// for those cases (any value is spec-conformant).
    Divw {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `divwu RT, RA, RB` — XO-form, OPCD = 31, XO = 459.
    /// (§3.3.7) Unsigned integer divide. Divide-by-zero is
    /// undefined per spec; dispatcher returns the dividend.
    Divwu {
        rt: u8,
        ra: u8,
        rb: u8,
        oe: bool,
        rc: bool,
    },
    /// `cmpi BF, L, RA, SI` — D-form, OPCD = 11. (§3.3.9)
    /// Signed compare of `RA` against `sign_extend(SI)`; result
    /// goes into CR field `bf`. `l` selects 32-bit (=0) vs 64-bit
    /// (=1) compare; only the 32-bit case is implemented.
    Cmpi { bf: u8, l: bool, ra: u8, si: i16 },
    /// `cmpli BF, L, RA, UI` — D-form, OPCD = 10. (§3.3.9)
    /// Unsigned compare of `RA` against `zero_extend(UI)`.
    Cmpli { bf: u8, l: bool, ra: u8, ui: u16 },
    /// `cmp BF, L, RA, RB` — X-form, OPCD = 31, XO = 0. (§3.3.9)
    /// Signed register-to-register compare into CR field `bf`.
    Cmp { bf: u8, l: bool, ra: u8, rb: u8 },
    /// `cmpl BF, L, RA, RB` — X-form, OPCD = 31, XO = 32.
    /// (§3.3.9). Unsigned register-to-register compare.
    Cmpl { bf: u8, l: bool, ra: u8, rb: u8 },
    /// `bc BO, BI, target` — B-form, OPCD = 16. (§2.4)
    /// Conditional branch with optional CTR decrement and CR-bit
    /// test. `bo` selects branch resolution per ISA Book I §2.4.1
    /// Figure 21; `bi` selects the CR bit to test (0..31).
    /// `displacement` is the 16-bit signed BD field already
    /// shifted left 2 and sign-extended to `i32`.
    Bc {
        bo: u8,
        bi: u8,
        displacement: i32,
        aa: bool,
        lk: bool,
    },
    /// `bcctr` / `bctr` (extended mnemonic with BO=20) — XL-form,
    /// OPCD = 19, XO = 528. (§2.4) Branch to address held in CTR.
    /// Used for switch-statement dispatch and indirect calls
    /// (e.g. function pointers, virtual method tables). Today
    /// only the unconditional case (BO=20) dispatches.
    Bcctr { bo: u8, bi: u8, lk: bool },
    /// `lbz RT, D(RA)` — D-form, OPCD = 34. (§3.3.2)
    /// Load byte zero-extended.
    Lbz { rt: u8, ra: u8, d: i16 },
    /// `lhz RT, D(RA)` — D-form, OPCD = 40. (§3.3.2)
    /// Load halfword zero-extended (16-bit big-endian).
    Lhz { rt: u8, ra: u8, d: i16 },
    /// `stb RS, D(RA)` — D-form, OPCD = 38. (§3.3.2) Store byte.
    Stb { rs: u8, ra: u8, d: i16 },
    /// `sth RS, D(RA)` — D-form, OPCD = 44. (§3.3.2)
    /// Store halfword (16-bit big-endian).
    Sth { rs: u8, ra: u8, d: i16 },
    /// `lmw RT, D(RA)` — D-form, OPCD = 46. (§3.3.5)
    /// Load Multiple Word: loads consecutive 32-bit big-endian
    /// values from `EA = (RA|0) + sext(D)` into `GPR[RT..=31]`,
    /// one register per 4-byte slot. Used by compilers to
    /// restore the non-volatile register set in a function
    /// epilogue. Per spec, if RA falls within the loaded range
    /// the instruction is "invalid form" — surface as a clean
    /// `Unimplemented` rather than producing arbitrary state.
    Lmw { rt: u8, ra: u8, d: i16 },
    /// `stmw RS, D(RA)` — D-form, OPCD = 47. (§3.3.5)
    /// Store Multiple Word: stores `GPR[RS..=31]` to
    /// successive 4-byte big-endian slots starting at
    /// `EA = (RA|0) + sext(D)`. The prologue counterpart of
    /// `lmw`.
    Stmw { rs: u8, ra: u8, d: i16 },
    /// `lwzu RT, D(RA)` — D-form, OPCD = 33. (§3.3.2)
    /// Load Word and Zero with Update: as `lwz`, then
    /// `RA = EA`. RA=0 (or RA==RT) is invalid per spec.
    Lwzu { rt: u8, ra: u8, d: i16 },
    /// `lbzu RT, D(RA)` — D-form, OPCD = 35. (§3.3.2)
    /// Load Byte and Zero with Update.
    Lbzu { rt: u8, ra: u8, d: i16 },
    /// `lhzu RT, D(RA)` — D-form, OPCD = 41. (§3.3.2)
    /// Load Halfword and Zero with Update.
    Lhzu { rt: u8, ra: u8, d: i16 },
    /// `lha RT, D(RA)` — D-form, OPCD = 42. (§3.3.2)
    /// Load Halfword Algebraic — sign-extends the loaded
    /// 16-bit halfword to 32 bits (vs `lhz` which zero-extends).
    Lha { rt: u8, ra: u8, d: i16 },
    /// `lhau RT, D(RA)` — D-form, OPCD = 43. (§3.3.2)
    /// Load Halfword Algebraic with Update.
    Lhau { rt: u8, ra: u8, d: i16 },
    /// `lfs FRT, D(RA)` — D-form, OPCD = 48. (§4.6.2)
    /// Load Floating-Point Single. Reads a 32-bit IEEE-754 float
    /// from `EA = (RA|0) + sext(D)`, converts it to double
    /// precision, and stores in `FPR[FRT]`.
    Lfs { frt: u8, ra: u8, d: i16 },
    /// `lfsu FRT, D(RA)` — D-form, OPCD = 49. (§4.6.2)
    /// `lfs` with update. RA=0 is invalid for the update form.
    Lfsu { frt: u8, ra: u8, d: i16 },
    /// `lfd FRT, D(RA)` — D-form, OPCD = 50. (§4.6.2)
    /// Load Floating-Point Double. Reads a 64-bit IEEE-754 double
    /// from memory and stores its bit pattern in `FPR[FRT]`.
    Lfd { frt: u8, ra: u8, d: i16 },
    /// `lfdu FRT, D(RA)` — D-form, OPCD = 51. (§4.6.2)
    /// `lfd` with update. RA=0 is invalid for the update form.
    Lfdu { frt: u8, ra: u8, d: i16 },
    /// `stfs FRS, D(RA)` — D-form, OPCD = 52. (§4.6.3)
    /// Store Floating-Point Single. Reads the double in
    /// `FPR[FRS]`, narrows to 32-bit single precision, and writes
    /// to `EA = (RA|0) + sext(D)`.
    Stfs { frs: u8, ra: u8, d: i16 },
    /// `stfsu FRS, D(RA)` — D-form, OPCD = 53. (§4.6.3)
    /// `stfs` with update. RA=0 is invalid for the update form.
    Stfsu { frs: u8, ra: u8, d: i16 },
    /// `stfd FRS, D(RA)` — D-form, OPCD = 54. (§4.6.3)
    /// Store Floating-Point Double. Writes the 64-bit bit pattern
    /// of `FPR[FRS]` to `EA = (RA|0) + sext(D)`.
    Stfd { frs: u8, ra: u8, d: i16 },
    /// `stfdu FRS, D(RA)` — D-form, OPCD = 55. (§4.6.3)
    /// `stfd` with update. RA=0 is invalid for the update form.
    Stfdu { frs: u8, ra: u8, d: i16 },
    /// `lfsx FRT, RA, RB` — X-form, OPCD = 31, XO = 535. (§4.6.2)
    /// Indexed-form variant of `lfs`. EA = (RA|0) + RB.
    Lfsx { frt: u8, ra: u8, rb: u8 },
    /// `lfsux FRT, RA, RB` — X-form, OPCD = 31, XO = 567. (§4.6.2)
    /// `lfsx` with update — EA = RA + RB; RA must be != 0; RA := EA.
    Lfsux { frt: u8, ra: u8, rb: u8 },
    /// `lfdx FRT, RA, RB` — X-form, OPCD = 31, XO = 599. (§4.6.2)
    /// Indexed-form variant of `lfd`.
    Lfdx { frt: u8, ra: u8, rb: u8 },
    /// `lfdux FRT, RA, RB` — X-form, OPCD = 31, XO = 631. (§4.6.2)
    Lfdux { frt: u8, ra: u8, rb: u8 },
    /// `stfsx FRS, RA, RB` — X-form, OPCD = 31, XO = 663. (§4.6.3)
    Stfsx { frs: u8, ra: u8, rb: u8 },
    /// `stfsux FRS, RA, RB` — X-form, OPCD = 31, XO = 695. (§4.6.3)
    Stfsux { frs: u8, ra: u8, rb: u8 },
    /// `stfdx FRS, RA, RB` — X-form, OPCD = 31, XO = 727. (§4.6.3)
    Stfdx { frs: u8, ra: u8, rb: u8 },
    /// `stfdux FRS, RA, RB` — X-form, OPCD = 31, XO = 759. (§4.6.3)
    Stfdux { frs: u8, ra: u8, rb: u8 },
    /// `fadd FRT, FRA, FRB` — A-form, OPCD = 63, XO = 21.
    /// (§4.6.5.1) Double-precision add: `FRT = FRA + FRB`.
    Fadd { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fsub FRT, FRA, FRB` — A-form, OPCD = 63, XO = 20.
    /// `FRT = FRA - FRB`.
    Fsub { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fmul FRT, FRA, FRC` — A-form, OPCD = 63, XO = 25.
    /// `FRT = FRA * FRC`. Note FRC at the FRC slot (bits 21..25,
    /// host 6..10), not FRB.
    Fmul { frt: u8, fra: u8, frc: u8, rc: bool },
    /// `fdiv FRT, FRA, FRB` — A-form, OPCD = 63, XO = 18.
    /// `FRT = FRA / FRB`.
    Fdiv { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fneg FRT, FRB` — X-form, OPCD = 63, XO = 40.
    /// (§4.6.4.1) Sign flip — toggles the IEEE-754 sign bit.
    Fneg { frt: u8, frb: u8, rc: bool },
    /// `fmr FRT, FRB` — X-form, OPCD = 63, XO = 72. (§4.6.4.1)
    /// Floating-point move register: copies the 64-bit FPR bit
    /// pattern.
    Fmr { frt: u8, frb: u8, rc: bool },
    /// `fabs FRT, FRB` — X-form, OPCD = 63, XO = 264. (§4.6.4.1)
    /// Floating-point absolute value: clears the sign bit.
    Fabs { frt: u8, frb: u8, rc: bool },
    /// `mffs FRT` — X-form, OPCD = 63, XO = 583. (Book I §4.6.7)
    /// Move From FPSCR: copy the 32-bit FPSCR into the low 32
    /// bits of FRT; the high 32 bits of FRT are architecturally
    /// undefined. (FRA / FRB encoding fields are reserved.)
    Mffs { frt: u8, rc: bool },
    /// `mcrfs BF, BFA` — X-form, OPCD = 63, XO = 64.
    /// Copy FPSCR field BFA into CR field BF.
    Mcrfs { bf: u8, bfa: u8 },
    /// `mtfsb1 BT` — X-form, OPCD = 63, XO = 38.
    /// Set FPSCR bit BT to 1.
    Mtfsb1 { bt: u8, rc: bool },
    /// `mtfsb0 BT` — X-form, OPCD = 63, XO = 70.
    /// Clear FPSCR bit BT to 0.
    Mtfsb0 { bt: u8, rc: bool },
    /// `mtfsfi BF, U` — X-form, OPCD = 63, XO = 134.
    /// Copy a 4-bit immediate U into FPSCR field BF.
    Mtfsfi { bf: u8, u: u8, rc: bool },
    /// `mtfsf FLM, FRB` — XFL-form, OPCD = 63, XO = 711.
    /// Copy selected 4-bit fields from the low 32 bits of FRB
    /// into FPSCR. FLM bit 7 selects FPSCR field 0, bit 14
    /// selects field 7.
    Mtfsf { flm: u8, frb: u8, rc: bool },
    /// `fadds FRT, FRA, FRB` — A-form, OPCD = 59, XO = 21.
    /// (§4.6.5.1) Single-precision add. Result is rounded to
    /// f32 precision then stored back as a 64-bit FPR pattern
    /// (the IEEE-754 single value cast back to double).
    Fadds { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fsubs FRT, FRA, FRB` — A-form, OPCD = 59, XO = 20.
    Fsubs { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fmuls FRT, FRA, FRC` — A-form, OPCD = 59, XO = 25.
    Fmuls { frt: u8, fra: u8, frc: u8, rc: bool },
    /// `fdivs FRT, FRA, FRB` — A-form, OPCD = 59, XO = 18.
    Fdivs { frt: u8, fra: u8, frb: u8, rc: bool },
    /// `fcmpo BF, FRA, FRB` — X-form, OPCD = 63, XO = 32.
    /// (§4.6.6.2) Floating Compare Ordered. Same CR-field
    /// write as `fcmpu` — sets the LT/GT/EQ/UN bits of CR\[BF\]
    /// based on a < b / a > b / a == b / either is NaN. The
    /// difference from `fcmpu` is that `fcmpo` raises an FP
    /// invalid-operation exception when either operand is a
    /// signalling NaN; we don't track FPSCR exceptions yet, so
    /// in practice fcmpo and fcmpu behave identically for the
    /// CR side-effect that callers care about.
    Fcmpo { bf: u8, fra: u8, frb: u8 },
    /// `fcmpu BF, FRA, FRB` — X-form, OPCD = 63, XO = 0.
    /// (§4.6.7) Floating Compare Unordered. Compares two FPR
    /// values and writes LT/GT/EQ/UNORDERED into CR field BF.
    /// "Unordered" means at least one operand is NaN.
    Fcmpu { bf: u8, fra: u8, frb: u8 },
    /// `fmadd FRT, FRA, FRC, FRB` — A-form, OPCD = 63, XO = 29.
    /// (§4.6.6) Fused multiply-add: `FRT = (FRA × FRC) + FRB`
    /// with a single IEEE-754 rounding step.
    Fmadd {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fmsub FRT, FRA, FRC, FRB` — A-form, OPCD = 63, XO = 28.
    /// `FRT = (FRA × FRC) - FRB`.
    Fmsub {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fnmadd FRT, FRA, FRC, FRB` — A-form, OPCD = 63, XO = 31.
    /// `FRT = -((FRA × FRC) + FRB)`.
    Fnmadd {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fnmsub FRT, FRA, FRC, FRB` — A-form, OPCD = 63, XO = 30.
    /// `FRT = -((FRA × FRC) - FRB)`.
    Fnmsub {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fmadds FRT, FRA, FRC, FRB` — A-form, OPCD = 59, XO = 29.
    /// Single-precision fused multiply-add.
    Fmadds {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fmsubs FRT, FRA, FRC, FRB` — A-form, OPCD = 59, XO = 28.
    Fmsubs {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fnmadds FRT, FRA, FRC, FRB` — A-form, OPCD = 59, XO = 31.
    Fnmadds {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `fnmsubs FRT, FRA, FRC, FRB` — A-form, OPCD = 59, XO = 30.
    Fnmsubs {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `frsp FRT, FRB` — X-form, OPCD = 63, XO = 12.
    /// (§4.6.4.2) Round to Single Precision: round FRB's value
    /// to f32 precision and store as f64 in FRT.
    Frsp { frt: u8, frb: u8, rc: bool },
    /// `fctiw FRT, FRB` — X-form, OPCD = 63, XO = 14.
    /// (§4.6.4.3) Float Convert To Integer Word — round FRB to a
    /// signed 32-bit integer per FPSCR.RN and store in FRT's
    /// low 32 bits.
    Fctiw { frt: u8, frb: u8, rc: bool },
    /// `fctiwz FRT, FRB` — X-form, OPCD = 63, XO = 15.
    /// (§4.6.4.3) Same as `fctiw` but rounding mode is fixed to
    /// "round toward zero" regardless of FPSCR.RN.
    Fctiwz { frt: u8, frb: u8, rc: bool },
    /// `fsqrt FRT, FRB` — A-form, OPCD = 63, XO = 22.
    /// (§4.6.5.2) Floating square root, double precision.
    /// `FRT = sqrt(FRB)`. NaN inputs produce NaN; negative
    /// non-zero inputs produce NaN per IEEE-754.
    Fsqrt { frt: u8, frb: u8, rc: bool },
    /// `fsqrts FRT, FRB` — A-form, OPCD = 59, XO = 22.
    /// Single-precision square root.
    Fsqrts { frt: u8, frb: u8, rc: bool },
    /// `fres FRT, FRB` — A-form, OPCD = 59, XO = 24.
    /// Floating reciprocal estimate single. The architectural result
    /// is an implementation-dependent single-precision estimate of
    /// `1 / FRB`; this interpreter returns the correctly rounded
    /// single-precision reciprocal for deterministic game logic.
    Fres { frt: u8, frb: u8, rc: bool },
    /// `frsqrte FRT, FRB` — A-form, OPCD = 63, XO = 26.
    /// Floating reciprocal square-root estimate double. The
    /// architectural result is implementation-dependent; this
    /// interpreter returns the deterministic double-precision
    /// reciprocal square root.
    Frsqrte { frt: u8, frb: u8, rc: bool },
    /// `fnabs FRT, FRB` — X-form, OPCD = 63, XO = 136.
    /// (§4.6.4.1) Floating Negative Absolute Value: sets the
    /// sign bit unconditionally so the result is always
    /// negative-or-negative-zero.
    Fnabs { frt: u8, frb: u8, rc: bool },
    /// `fsel FRT, FRA, FRC, FRB` — A-form, OPCD = 63, XO = 23.
    /// (§4.6.5.1) Floating Select: `FRT = if FRA >= 0 then FRC
    /// else FRB`. Branchless conditional move on the sign of
    /// the first operand. Used heavily by graphics and DSP code
    /// for `max`/`min` and clamping idioms.
    Fsel {
        frt: u8,
        fra: u8,
        frc: u8,
        frb: u8,
        rc: bool,
    },
    /// `stbu RS, D(RA)` — D-form, OPCD = 39. (§3.3.2)
    /// Store Byte with Update.
    Stbu { rs: u8, ra: u8, d: i16 },
    /// `sthu RS, D(RA)` — D-form, OPCD = 45. (§3.3.2)
    /// Store Halfword with Update.
    Sthu { rs: u8, ra: u8, d: i16 },
    /// `lwzx RT, RA, RB` — X-form, OPCD = 31, XO = 23.
    /// (§3.3.2) Indexed load word zero-extended;
    /// `EA = (RA|0) + RB`.
    Lwzx { rt: u8, ra: u8, rb: u8 },
    /// `lwarx RT, RA, RB` - X-form, OPCD = 31, XO = 20.
    /// Load Word and Reserve Indexed. Loads the word at
    /// `(RA|0) + RB` and creates a reservation for a following
    /// `stwcx.`.
    Lwarx { rt: u8, ra: u8, rb: u8 },
    /// `lbzx RT, RA, RB` — X-form, OPCD = 31, XO = 87. (§3.3.2)
    Lbzx { rt: u8, ra: u8, rb: u8 },
    /// `lhzx RT, RA, RB` — X-form, OPCD = 31, XO = 279. (§3.3.2)
    Lhzx { rt: u8, ra: u8, rb: u8 },
    /// `lwbrx RT, RA, RB` - X-form, OPCD = 31, XO = 534.
    /// Indexed load word with byte-reversal.
    Lwbrx { rt: u8, ra: u8, rb: u8 },
    /// `lhbrx RT, RA, RB` - X-form, OPCD = 31, XO = 790.
    /// Indexed load halfword with byte-reversal.
    Lhbrx { rt: u8, ra: u8, rb: u8 },
    /// `lswi RT, RA, NB` — X-form, OPCD = 31, XO = 597.
    /// (§3.3.7 Load String Word Immediate) Loads `NB` bytes
    /// (where `NB == 0` means 32) starting at effective
    /// address `(RA|0)` into consecutive GPRs starting at RT,
    /// wrapping back to GPR0 after GPR31. Each register
    /// receives 4 bytes packed big-endian; the final register
    /// is right-padded with zeros if `NB` isn't a multiple of 4.
    /// Used by classic Mac PowerPC libraries (notably StdCLib's
    /// memcpy / `__memcpy`) for short bulk transfers.
    Lswi { rt: u8, ra: u8, nb: u8 },
    /// `lswx RT, RA, RB` - X-form, OPCD = 31, XO = 533.
    /// Load String Word Indexed. Like `lswi`, but the effective
    /// address is `(RA|0) + RB` and the byte count comes from
    /// XER[25..31].
    Lswx { rt: u8, ra: u8, rb: u8 },
    /// `stswi RS, RA, NB` — X-form, OPCD = 31, XO = 725.
    /// (§3.3.7 Store String Word Immediate) Symmetric of
    /// `lswi`: stores `NB` bytes from consecutive GPRs
    /// starting at RS into memory at `(RA|0)`. Same wrap-around
    /// and big-endian-within-word semantics.
    Stswi { rs: u8, ra: u8, nb: u8 },
    /// `stswx RS, RA, RB` - X-form, OPCD = 31, XO = 661.
    /// Store String Word Indexed. Like `stswi`, but the effective
    /// address is `(RA|0) + RB` and the byte count comes from
    /// XER[25..31].
    Stswx { rs: u8, ra: u8, rb: u8 },
    /// `stwx RS, RA, RB` — X-form, OPCD = 31, XO = 151. (§3.3.2)
    Stwx { rs: u8, ra: u8, rb: u8 },
    /// `stwcx. RS, RA, RB` - X-form, OPCD = 31, XO = 150.
    /// Store Word Conditional Indexed. The only valid form is the
    /// record form (`Rc=1`), which reports success through CR0 EQ.
    Stwcx { rs: u8, ra: u8, rb: u8 },
    /// `stbx RS, RA, RB` — X-form, OPCD = 31, XO = 215. (§3.3.2)
    Stbx { rs: u8, ra: u8, rb: u8 },
    /// `stbux RS, RA, RB` — X-form, OPCD = 31, XO = 247. (§3.3.2)
    /// Store Byte with Update Indexed. RA=0 is an invalid form.
    Stbux { rs: u8, ra: u8, rb: u8 },
    /// `sthx RS, RA, RB` — X-form, OPCD = 31, XO = 407. (§3.3.2)
    Sthx { rs: u8, ra: u8, rb: u8 },
    /// `stwbrx RS, RA, RB` - X-form, OPCD = 31, XO = 662.
    /// Indexed store word with byte-reversal.
    Stwbrx { rs: u8, ra: u8, rb: u8 },
    /// `sthbrx RS, RA, RB` - X-form, OPCD = 31, XO = 918.
    /// Indexed store halfword with byte-reversal.
    Sthbrx { rs: u8, ra: u8, rb: u8 },
    /// `lhax RT, RA, RB` — X-form, OPCD = 31, XO = 343.
    /// (§3.3.2) Load Halfword Algebraic Indexed: same as `lhax`'s
    /// D-form sister `lha` but the displacement comes from RB.
    /// Sign-extends the 16-bit halfword to 32 bits.
    Lhax { rt: u8, ra: u8, rb: u8 },
    /// `lwzux RT, RA, RB` — X-form, OPCD = 31, XO = 55. (§3.3.2)
    /// Load Word and Zero with Update Indexed. Same RA=0/RA=RT
    /// invalid-form rule as `lwzu`.
    Lwzux { rt: u8, ra: u8, rb: u8 },
    /// `stwux RS, RA, RB` — X-form, OPCD = 31, XO = 183.
    /// (§3.3.2) Store Word with Update Indexed. RA=0 invalid.
    Stwux { rs: u8, ra: u8, rb: u8 },
}

/// PowerPC instruction decode error. Returned by [`decode`] when
/// the instruction word cannot be mapped to a [`PpcInstr`] variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcDecodeError {
    /// The 6-bit primary opcode in bits 0..5 (MSB=0) is not yet
    /// implemented. The byte is preserved so the dispatcher can
    /// produce a useful "unimplemented opcode" diagnostic.
    UnsupportedPrimaryOpcode(u8),
    /// A primary opcode that has secondary tables (e.g. 19 for
    /// XL-form, 31 for X/XO-form integer arithmetic) was decoded,
    /// but the secondary opcode is not yet implemented. The
    /// `(primary, secondary)` pair is preserved for diagnostics.
    UnsupportedSecondaryOpcode { primary: u8, secondary: u16 },
}

/// Decode a single 32-bit PowerPC instruction word. Caller
/// supplies the raw word in host (native) byte order — the loader
/// is responsible for endian-swapping the on-disk big-endian form.
///
/// Decoded fields follow PowerPC's MSB=0 bit numbering as
/// transcribed in the local ISA reference. Unsupported primary
/// opcodes return [`PpcDecodeError::UnsupportedPrimaryOpcode`]
/// rather than guessing semantics.
pub fn decode(instr: u32) -> Result<PpcInstr, PpcDecodeError> {
    // OPCD lives in bits 0..5 (MSB=0) — i.e. the high 6 bits of
    // the host u32 in conventional LSB=0 numbering.
    let opcd = ((instr >> 26) & 0x3F) as u8;
    // RT/RS at bits 6..10 (MSB=0)  → host bits 21..25
    let rt = ((instr >> 21) & 0x1F) as u8;
    // RA at bits 11..15 (MSB=0)    → host bits 16..20
    let ra = ((instr >> 16) & 0x1F) as u8;
    // 16-bit immediate at bits 16..31 (MSB=0) is the low 16 host bits.
    let imm16 = (instr & 0xFFFF) as u16;

    // RB at bits 16..20 (MSB=0) → host bits 11..15
    let rb = ((instr >> 11) & 0x1F) as u8;

    // BF (3 bits at MSB=0 6..8 → host 23..25) and L (bit 10
    // MSB=0 → host 21) for compares.
    let bf = ((instr >> 23) & 0x07) as u8;
    let l = ((instr >> 21) & 1) != 0;

    match opcd {
        3 => Ok(PpcInstr::Twi {
            to: rt,
            ra,
            si: imm16 as i16,
        }),
        7 => Ok(PpcInstr::Mulli {
            rt,
            ra,
            si: imm16 as i16,
        }),
        8 => Ok(PpcInstr::Subfic {
            rt,
            ra,
            si: imm16 as i16,
        }),
        10 => Ok(PpcInstr::Cmpli {
            bf,
            l,
            ra,
            ui: imm16,
        }),
        11 => Ok(PpcInstr::Cmpi {
            bf,
            l,
            ra,
            si: imm16 as i16,
        }),
        12 => Ok(PpcInstr::Addic {
            rt,
            ra,
            si: imm16 as i16,
        }),
        13 => Ok(PpcInstr::AddicDot {
            rt,
            ra,
            si: imm16 as i16,
        }),
        14 => Ok(PpcInstr::Addi {
            rt,
            ra,
            si: imm16 as i16,
        }),
        15 => Ok(PpcInstr::Addis {
            rt,
            ra,
            si: imm16 as i16,
        }),
        // B-form `bc`. BO at MSB=0 6..10 → host 21..25, BI at
        // 11..15 → host 16..20, BD at 16..29 → host 2..15.
        16 => {
            let bo = ((instr >> 21) & 0x1F) as u8;
            let bi = ((instr >> 16) & 0x1F) as u8;
            let aa = (instr & 0b10) != 0;
            let lk = (instr & 0b01) != 0;
            // BD || 0b00 lives at host bits 2..15 → mask 0xFFFC.
            let raw = instr & 0x0000_FFFC;
            // Sign-extend bit 15 (MSB of the 16-bit signed value).
            let displacement = if (raw & 0x0000_8000) != 0 {
                (raw | 0xFFFF_0000) as i32
            } else {
                raw as i32
            };
            Ok(PpcInstr::Bc {
                bo,
                bi,
                displacement,
                aa,
                lk,
            })
        }
        17 => Ok(PpcInstr::Sc {
            lev: ((instr >> 5) & 0x7F) as u8,
        }),
        // I-form `b` / `bl` / `ba` / `bla`. LI is a 24-bit signed
        // value at MSB=0 bits 6..29; concatenated with 0b00 it
        // yields a 26-bit signed PC-relative or absolute target.
        18 => {
            // Host LSB=0 layout: AA at bit 1, LK at bit 0,
            // LI || 0b00 occupies bits 2..25. Sign-extend the
            // 26-bit (LI || 0b00) value to i32.
            let aa = (instr & 0b10) != 0;
            let lk = (instr & 0b01) != 0;
            // Mask off AA/LK then sign-extend bit 25.
            let raw = instr & 0x03FF_FFFC; // keep only LI || 0b00
            let displacement = if (raw & 0x0200_0000) != 0 {
                (raw | 0xFC00_0000) as i32
            } else {
                raw as i32
            };
            Ok(PpcInstr::B {
                displacement,
                aa,
                lk,
            })
        }
        // XL-form (OPCD=19) — secondary XO at MSB=0 bits 21..30
        // (host bits 1..10). bclr/bcctr share the BO/BI extraction
        // pattern; CR-logical ops (XO=33, 129, 193, …) land here too.
        19 => {
            let xo = ((instr >> 1) & 0x3FF) as u16;
            // BO at MSB=0 bits 6..10, BI at 11..15 — same as B-form.
            let bo = ((instr >> 21) & 0x1F) as u8;
            let bi = ((instr >> 16) & 0x1F) as u8;
            let lk = (instr & 1) != 0;
            // CR-logical ops use the same instruction-word
            // slots as bclr's BO/BI/RB but are interpreted as
            // BT/BA/BB (each a 5-bit single-CR-bit selector).
            // The dispatcher just renames the values.
            let bt = bo;
            let ba = bi;
            // BB at MSB=0 16..20 → host 11..15.
            let bb = ((instr >> 11) & 0x1F) as u8;
            match xo {
                // mcrf BF, BFA — its operand layout is BF in bits
                // 6..8 and BFA in bits 11..13 (3 bits each, MSB=0).
                // Reuse the bf field already extracted at the top
                // of decode().
                0 => {
                    let bfa = ((instr >> 18) & 0x7) as u8;
                    Ok(PpcInstr::Mcrf { bf, bfa })
                }
                16 => Ok(PpcInstr::Bclr { bo, bi, lk }),
                150 => Ok(PpcInstr::Isync),
                33 => Ok(PpcInstr::Crnor { bt, ba, bb }),
                129 => Ok(PpcInstr::Crandc { bt, ba, bb }),
                193 => Ok(PpcInstr::Crxor { bt, ba, bb }),
                225 => Ok(PpcInstr::Crnand { bt, ba, bb }),
                257 => Ok(PpcInstr::Crand { bt, ba, bb }),
                289 => Ok(PpcInstr::Creqv { bt, ba, bb }),
                417 => Ok(PpcInstr::Crorc { bt, ba, bb }),
                449 => Ok(PpcInstr::Cror { bt, ba, bb }),
                528 => Ok(PpcInstr::Bcctr { bo, bi, lk }),
                _ => Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                    primary: 19,
                    secondary: xo,
                }),
            }
        }
        24 => Ok(PpcInstr::Ori {
            ra,
            rs: rt,
            ui: imm16,
        }),
        25 => Ok(PpcInstr::Oris {
            ra,
            rs: rt,
            ui: imm16,
        }),
        26 => Ok(PpcInstr::Xori {
            ra,
            rs: rt,
            ui: imm16,
        }),
        27 => Ok(PpcInstr::Xoris {
            ra,
            rs: rt,
            ui: imm16,
        }),
        28 => Ok(PpcInstr::AndiDot {
            ra,
            rs: rt,
            ui: imm16,
        }),
        29 => Ok(PpcInstr::AndisDot {
            ra,
            rs: rt,
            ui: imm16,
        }),
        // M-form rlwimi / rlwinm / rlwnm — same field layout, only
        // the OPCD distinguishes them. SH (or RB) at MSB=0 16..20
        // → host 11..15; MB at 21..25 → host 6..10; ME at 26..30
        // → host 1..5; Rc at bit 31 → host bit 0.
        20 => {
            let sh = ((instr >> 11) & 0x1F) as u8;
            let mb = ((instr >> 6) & 0x1F) as u8;
            let me = ((instr >> 1) & 0x1F) as u8;
            let rc = (instr & 1) != 0;
            Ok(PpcInstr::Rlwimi {
                ra,
                rs: rt,
                sh,
                mb,
                me,
                rc,
            })
        }
        21 => {
            let sh = ((instr >> 11) & 0x1F) as u8;
            let mb = ((instr >> 6) & 0x1F) as u8;
            let me = ((instr >> 1) & 0x1F) as u8;
            let rc = (instr & 1) != 0;
            Ok(PpcInstr::Rlwinm {
                ra,
                rs: rt,
                sh,
                mb,
                me,
                rc,
            })
        }
        23 => {
            // rlwnm uses RB at the same instruction slot SH lives
            // in for the immediate variants.
            let rb = ((instr >> 11) & 0x1F) as u8;
            let mb = ((instr >> 6) & 0x1F) as u8;
            let me = ((instr >> 1) & 0x1F) as u8;
            let rc = (instr & 1) != 0;
            Ok(PpcInstr::Rlwnm {
                ra,
                rs: rt,
                rb,
                mb,
                me,
                rc,
            })
        }
        32 => Ok(PpcInstr::Lwz {
            rt,
            ra,
            d: imm16 as i16,
        }),
        33 => Ok(PpcInstr::Lwzu {
            rt,
            ra,
            d: imm16 as i16,
        }),
        34 => Ok(PpcInstr::Lbz {
            rt,
            ra,
            d: imm16 as i16,
        }),
        35 => Ok(PpcInstr::Lbzu {
            rt,
            ra,
            d: imm16 as i16,
        }),
        36 => Ok(PpcInstr::Stw {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        37 => Ok(PpcInstr::Stwu {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        38 => Ok(PpcInstr::Stb {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        39 => Ok(PpcInstr::Stbu {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        40 => Ok(PpcInstr::Lhz {
            rt,
            ra,
            d: imm16 as i16,
        }),
        41 => Ok(PpcInstr::Lhzu {
            rt,
            ra,
            d: imm16 as i16,
        }),
        42 => Ok(PpcInstr::Lha {
            rt,
            ra,
            d: imm16 as i16,
        }),
        43 => Ok(PpcInstr::Lhau {
            rt,
            ra,
            d: imm16 as i16,
        }),
        44 => Ok(PpcInstr::Sth {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        45 => Ok(PpcInstr::Sthu {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        46 => Ok(PpcInstr::Lmw {
            rt,
            ra,
            d: imm16 as i16,
        }),
        47 => Ok(PpcInstr::Stmw {
            rs: rt,
            ra,
            d: imm16 as i16,
        }),
        48 => Ok(PpcInstr::Lfs {
            frt: rt,
            ra,
            d: imm16 as i16,
        }),
        49 => Ok(PpcInstr::Lfsu {
            frt: rt,
            ra,
            d: imm16 as i16,
        }),
        50 => Ok(PpcInstr::Lfd {
            frt: rt,
            ra,
            d: imm16 as i16,
        }),
        51 => Ok(PpcInstr::Lfdu {
            frt: rt,
            ra,
            d: imm16 as i16,
        }),
        52 => Ok(PpcInstr::Stfs {
            frs: rt,
            ra,
            d: imm16 as i16,
        }),
        53 => Ok(PpcInstr::Stfsu {
            frs: rt,
            ra,
            d: imm16 as i16,
        }),
        54 => Ok(PpcInstr::Stfd {
            frs: rt,
            ra,
            d: imm16 as i16,
        }),
        55 => Ok(PpcInstr::Stfdu {
            frs: rt,
            ra,
            d: imm16 as i16,
        }),
        // Single-precision FP arithmetic (OPCD=59). Same A-form
        // shape as OPCD=63 — only the result-rounding precision
        // differs.
        59 => {
            let xo_5 = ((instr >> 1) & 0x1F) as u8;
            let rc = (instr & 1) != 0;
            let frt = rt;
            let fra = ra;
            let frb = rb;
            let frc = ((instr >> 6) & 0x1F) as u8;
            match xo_5 {
                18 => Ok(PpcInstr::Fdivs { frt, fra, frb, rc }),
                20 => Ok(PpcInstr::Fsubs { frt, fra, frb, rc }),
                21 => Ok(PpcInstr::Fadds { frt, fra, frb, rc }),
                22 => Ok(PpcInstr::Fsqrts { frt, frb, rc }),
                24 => Ok(PpcInstr::Fres { frt, frb, rc }),
                25 => Ok(PpcInstr::Fmuls { frt, fra, frc, rc }),
                28 => Ok(PpcInstr::Fmsubs {
                    frt,
                    fra,
                    frc,
                    frb,
                    rc,
                }),
                29 => Ok(PpcInstr::Fmadds {
                    frt,
                    fra,
                    frc,
                    frb,
                    rc,
                }),
                30 => Ok(PpcInstr::Fnmsubs {
                    frt,
                    fra,
                    frc,
                    frb,
                    rc,
                }),
                31 => Ok(PpcInstr::Fnmadds {
                    frt,
                    fra,
                    frc,
                    frb,
                    rc,
                }),
                _ => Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                    primary: 59,
                    secondary: u16::from(xo_5),
                }),
            }
        }
        // Floating-point arithmetic (OPCD=63). A-form
        // instructions have a 5-bit XO at bits 26..30; X-form
        // instructions for FP move / sign / compare have a
        // 10-bit XO. We dispatch on the 5-bit value first
        // (covering fadd / fsub / fmul / fdiv) and fall through
        // to the 10-bit XO for X-form.
        63 => {
            let xo_5 = ((instr >> 1) & 0x1F) as u8;
            let rc = (instr & 1) != 0;
            // FRT/FRA/FRB share slots with X-form RT/RA/RB.
            // FRC sits at the FRC slot bits 21..25 (host 6..10).
            let frt = rt;
            let fra = ra;
            let frb = rb;
            let frc = ((instr >> 6) & 0x1F) as u8;
            match xo_5 {
                18 => return Ok(PpcInstr::Fdiv { frt, fra, frb, rc }),
                20 => return Ok(PpcInstr::Fsub { frt, fra, frb, rc }),
                21 => return Ok(PpcInstr::Fadd { frt, fra, frb, rc }),
                22 => return Ok(PpcInstr::Fsqrt { frt, frb, rc }),
                23 => {
                    return Ok(PpcInstr::Fsel {
                        frt,
                        fra,
                        frc,
                        frb,
                        rc,
                    })
                }
                25 => return Ok(PpcInstr::Fmul { frt, fra, frc, rc }),
                26 => return Ok(PpcInstr::Frsqrte { frt, frb, rc }),
                28 => {
                    return Ok(PpcInstr::Fmsub {
                        frt,
                        fra,
                        frc,
                        frb,
                        rc,
                    })
                }
                29 => {
                    return Ok(PpcInstr::Fmadd {
                        frt,
                        fra,
                        frc,
                        frb,
                        rc,
                    })
                }
                30 => {
                    return Ok(PpcInstr::Fnmsub {
                        frt,
                        fra,
                        frc,
                        frb,
                        rc,
                    })
                }
                31 => {
                    return Ok(PpcInstr::Fnmadd {
                        frt,
                        fra,
                        frc,
                        frb,
                        rc,
                    })
                }
                _ => {}
            }
            // X-form 10-bit XO fall-through.
            let xo_10 = ((instr >> 1) & 0x3FF) as u16;
            match xo_10 {
                0 => {
                    // fcmpu — BF lives in the bf slot, FRA/FRB
                    // in the X-form RA/RB slots.
                    Ok(PpcInstr::Fcmpu { bf, fra, frb })
                }
                32 => Ok(PpcInstr::Fcmpo { bf, fra, frb }),
                12 => Ok(PpcInstr::Frsp { frt, frb, rc }),
                14 => Ok(PpcInstr::Fctiw { frt, frb, rc }),
                15 => Ok(PpcInstr::Fctiwz { frt, frb, rc }),
                38 => Ok(PpcInstr::Mtfsb1 { bt: frt, rc }),
                40 => Ok(PpcInstr::Fneg { frt, frb, rc }),
                64 => Ok(PpcInstr::Mcrfs {
                    bf,
                    bfa: ((instr >> 18) & 0x07) as u8,
                }),
                70 => Ok(PpcInstr::Mtfsb0 { bt: frt, rc }),
                72 => Ok(PpcInstr::Fmr { frt, frb, rc }),
                134 => Ok(PpcInstr::Mtfsfi {
                    bf,
                    u: ((instr >> 12) & 0x0F) as u8,
                    rc,
                }),
                136 => Ok(PpcInstr::Fnabs { frt, frb, rc }),
                264 => Ok(PpcInstr::Fabs { frt, frb, rc }),
                // Move From FPSCR — Power ISA Book I §4.6.7. Copies
                // the 32-bit FPSCR into the low 32 bits of FRT;
                // high 32 bits are undefined per the spec.
                583 => Ok(PpcInstr::Mffs { frt, rc }),
                711 => Ok(PpcInstr::Mtfsf {
                    flm: ((instr >> 17) & 0xFF) as u8,
                    frb,
                    rc,
                }),
                _ => Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                    primary: 63,
                    secondary: xo_10,
                }),
            }
        }
        // X-form / XO-form / XFX-form pool — secondary XO at
        // MSB=0 bits 21..30 (host bits 1..10), Rc at bit 31 (host
        // bit 0).
        //
        // Note that for XO-form the high bit of the 10-bit field
        // is actually the OE bit (MSB=0 bit 21); the XO proper is
        // 9 bits at bits 22..30. We dispatch on the 10-bit value
        // and let each XO-form arm cover both `oe=0` and `oe=1`
        // cells of the table.
        31 => {
            let xo = ((instr >> 1) & 0x3FF) as u16;
            let rc = (instr & 1) != 0;
            match xo {
                0 => Ok(PpcInstr::Cmp { bf, l, ra, rb }),
                4 => Ok(PpcInstr::Tw { to: rt, ra, rb }),
                20 if !rc => Ok(PpcInstr::Lwarx { rt, ra, rb }),
                23 => Ok(PpcInstr::Lwzx { rt, ra, rb }),
                24 => Ok(PpcInstr::Slw { ra, rs: rt, rb, rc }),
                26 => Ok(PpcInstr::Cntlzw { ra, rs: rt, rc }),
                28 => Ok(PpcInstr::And { ra, rs: rt, rb, rc }),
                32 => Ok(PpcInstr::Cmpl { bf, l, ra, rb }),
                87 => Ok(PpcInstr::Lbzx { rt, ra, rb }),
                60 => Ok(PpcInstr::Andc { ra, rs: rt, rb, rc }),
                124 => Ok(PpcInstr::Nor { ra, rs: rt, rb, rc }),
                284 => Ok(PpcInstr::Eqv { ra, rs: rt, rb, rc }),
                316 => Ok(PpcInstr::Xor { ra, rs: rt, rb, rc }),
                412 => Ok(PpcInstr::Orc { ra, rs: rt, rb, rc }),
                476 => Ok(PpcInstr::Nand { ra, rs: rt, rb, rc }),
                922 => Ok(PpcInstr::Extsh { ra, rs: rt, rc }),
                954 => Ok(PpcInstr::Extsb { ra, rs: rt, rc }),
                150 if rc => Ok(PpcInstr::Stwcx { rs: rt, ra, rb }),
                151 => Ok(PpcInstr::Stwx { rs: rt, ra, rb }),
                215 => Ok(PpcInstr::Stbx { rs: rt, ra, rb }),
                247 => Ok(PpcInstr::Stbux { rs: rt, ra, rb }),
                279 => Ok(PpcInstr::Lhzx { rt, ra, rb }),
                // lswi RT, RA, NB — NB is the 5-bit value at the
                // RB slot (MSB=0 bits 16..20 → host bits 11..15).
                // Reuse `rb` since the bit field is identical.
                597 => Ok(PpcInstr::Lswi { rt, ra, nb: rb }),
                725 => Ok(PpcInstr::Stswi { rs: rt, ra, nb: rb }),
                533 if !rc => Ok(PpcInstr::Lswx { rt, ra, rb }),
                661 if !rc => Ok(PpcInstr::Stswx { rs: rt, ra, rb }),
                407 => Ok(PpcInstr::Sthx { rs: rt, ra, rb }),
                343 => Ok(PpcInstr::Lhax { rt, ra, rb }),
                55 => Ok(PpcInstr::Lwzux { rt, ra, rb }),
                183 => Ok(PpcInstr::Stwux { rs: rt, ra, rb }),
                534 if !rc => Ok(PpcInstr::Lwbrx { rt, ra, rb }),
                662 if !rc => Ok(PpcInstr::Stwbrx { rs: rt, ra, rb }),
                790 if !rc => Ok(PpcInstr::Lhbrx { rt, ra, rb }),
                918 if !rc => Ok(PpcInstr::Sthbrx { rs: rt, ra, rb }),
                536 => Ok(PpcInstr::Srw { ra, rs: rt, rb, rc }),
                792 => Ok(PpcInstr::Sraw { ra, rs: rt, rb, rc }),
                // srawi: SH at the RB slot (MSB=0 16..20 → host 11..15).
                824 => Ok(PpcInstr::Srawi {
                    ra,
                    rs: rt,
                    sh: rb,
                    rc,
                }),
                444 => Ok(PpcInstr::Or { ra, rs: rt, rb, rc }),
                // XFX-form `mfspr` / `mtspr`. The 10-bit SPR
                // field is split: high 5 bits at instr bits
                // 11..15 (MSB=0 → host 11..15), low 5 bits at
                // instr bits 16..20 (host 16..20). The encoded
                // value swaps them to maintain compatibility
                // with the original POWER 5-bit SPR layout.
                339 | 467 => {
                    let high_5 = ((instr >> 11) & 0x1F) as u16;
                    let low_5 = ((instr >> 16) & 0x1F) as u16;
                    let spr = (high_5 << 5) | low_5;
                    if xo == 339 {
                        Ok(PpcInstr::Mfspr { rt, spr })
                    } else {
                        Ok(PpcInstr::Mtspr { spr, rs: rt })
                    }
                }
                // mfcr RT — XFX-form. RT at bits 6..10 already
                // extracted as `rt`.
                19 => Ok(PpcInstr::Mfcr { rt }),
                // mtcrf FXM, RS — XFX-form. RS at bits 6..10
                // (rt slot); FXM (8-bit mask) at bits 12..19,
                // i.e. host bits 12..19 (mask 0xFF after shift).
                144 => {
                    let fxm = ((instr >> 12) & 0xFF) as u8;
                    Ok(PpcInstr::Mtcrf { fxm, rs: rt })
                }
                54 if rt == 0 && !rc => Ok(PpcInstr::Dcbst { ra, rb }),
                86 if rt == 0 && !rc => Ok(PpcInstr::Dcbf { ra, rb }),
                246 if !rc => Ok(PpcInstr::Dcbtst { ct: rt, ra, rb }),
                278 if !rc => Ok(PpcInstr::Dcbt { ct: rt, ra, rb }),
                598 => Ok(PpcInstr::Sync),
                854 => Ok(PpcInstr::Eieio),
                982 if rt == 0 && !rc => Ok(PpcInstr::Icbi { ra, rb }),
                1014 if rt == 0 && !rc => Ok(PpcInstr::Dcbz { ra, rb }),
                // XO-form arithmetic: 9-bit XO with OE in bit 9 of
                // the 10-bit dispatch value. `add` is XO=266
                // (oe=0 → 266; oe=1 → 778); `subf` is XO=40
                // (oe=0 → 40; oe=1 → 552).
                266 | 778 => Ok(PpcInstr::Add {
                    rt,
                    ra,
                    rb,
                    oe: xo == 778,
                    rc,
                }),
                40 | 552 => Ok(PpcInstr::Subf {
                    rt,
                    ra,
                    rb,
                    oe: xo == 552,
                    rc,
                }),
                10 | 522 => Ok(PpcInstr::Addc {
                    rt,
                    ra,
                    rb,
                    oe: xo == 522,
                    rc,
                }),
                138 | 650 => Ok(PpcInstr::Adde {
                    rt,
                    ra,
                    rb,
                    oe: xo == 650,
                    rc,
                }),
                8 | 520 => Ok(PpcInstr::Subfc {
                    rt,
                    ra,
                    rb,
                    oe: xo == 520,
                    rc,
                }),
                136 | 648 => Ok(PpcInstr::Subfe {
                    rt,
                    ra,
                    rb,
                    oe: xo == 648,
                    rc,
                }),
                202 | 714 => Ok(PpcInstr::Addze {
                    rt,
                    ra,
                    oe: xo == 714,
                    rc,
                }),
                234 | 746 => Ok(PpcInstr::Addme {
                    rt,
                    ra,
                    oe: xo == 746,
                    rc,
                }),
                200 | 712 => Ok(PpcInstr::Subfze {
                    rt,
                    ra,
                    oe: xo == 712,
                    rc,
                }),
                232 | 744 => Ok(PpcInstr::Subfme {
                    rt,
                    ra,
                    oe: xo == 744,
                    rc,
                }),
                104 | 616 => Ok(PpcInstr::Neg {
                    rt,
                    ra,
                    oe: xo == 616,
                    rc,
                }),
                235 | 747 => Ok(PpcInstr::Mullw {
                    rt,
                    ra,
                    rb,
                    oe: xo == 747,
                    rc,
                }),
                75 => Ok(PpcInstr::Mulhw { rt, ra, rb, rc }),
                11 => Ok(PpcInstr::Mulhwu { rt, ra, rb, rc }),
                491 | 1003 => Ok(PpcInstr::Divw {
                    rt,
                    ra,
                    rb,
                    oe: xo == 1003,
                    rc,
                }),
                459 | 971 => Ok(PpcInstr::Divwu {
                    rt,
                    ra,
                    rb,
                    oe: xo == 971,
                    rc,
                }),
                // X-form floating-point indexed load/store. EA =
                // (RA|0) + RB for the non-update variants;
                // EA = RA + RB and RA := EA for the *ux variants.
                // PEM §4.6.2 / §4.6.3.
                535 => Ok(PpcInstr::Lfsx { frt: rt, ra, rb }),
                567 => Ok(PpcInstr::Lfsux { frt: rt, ra, rb }),
                599 => Ok(PpcInstr::Lfdx { frt: rt, ra, rb }),
                631 => Ok(PpcInstr::Lfdux { frt: rt, ra, rb }),
                663 => Ok(PpcInstr::Stfsx { frs: rt, ra, rb }),
                695 => Ok(PpcInstr::Stfsux { frs: rt, ra, rb }),
                727 => Ok(PpcInstr::Stfdx { frs: rt, ra, rb }),
                759 => Ok(PpcInstr::Stfdux { frs: rt, ra, rb }),
                _ => Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                    primary: 31,
                    secondary: xo,
                }),
            }
        }
        other => Err(PpcDecodeError::UnsupportedPrimaryOpcode(other)),
    }
}
