//! SANE (Standard Apple Numeric Environment) floating-point trap handlers.
//!
//! Implements Pack4 (FP68K, $A9EB) and Pack5 (Elems68K, $A9EC) for basic
//! floating-point arithmetic needed by games like Marathon.

use std::sync::OnceLock;

use crate::cpu::{CpuOps, Register};
use crate::memory::{MacMemoryBus, MemoryBus};
use crate::Result;

// Opt-in diagnostic for SANE Elems68K calls that produce NaN or +/-inf.
// Helps localize game-side bugs that pass invalid inputs (e.g. log(0) →
// -inf, sqrt(-1) → NaN). Set `SYSTEMLESS_TRACE_SANE_NAN=1` to enable.
// Cheap when unset.
static TRACE_SANE_NAN: OnceLock<bool> = OnceLock::new();
static TRACE_SANE: OnceLock<bool> = OnceLock::new();

#[inline]
fn trace_sane_nan_enabled() -> bool {
    *TRACE_SANE_NAN.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SANE_NAN").is_some())
}

#[inline]
fn trace_sane_enabled() -> bool {
    *TRACE_SANE.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SANE").is_some())
}

fn trace_sane_bytes(bus: &MacMemoryBus, addr: u32, len: u32) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(len as usize * 2);
    for i in 0..len {
        let _ = write!(out, "{:02X}", bus.read_byte(addr + i));
    }
    out
}

#[inline]
fn sane_op_name(op: u16) -> &'static str {
    match op {
        0x00 => "FLN",
        0x02 => "FLOG2",
        0x04 => "FLN1",
        0x06 => "FLOG21",
        0x08 => "FEXP",
        0x0A => "FEXP2",
        0x0C => "FEXP1",
        0x0E => "FEXP21",
        0x18 => "FSIN",
        0x1A => "FCOS",
        0x1C => "FTAN",
        0x1E => "FATAN",
        _ => "Felems",
    }
}

#[inline]
fn elems_op_name(opcode: u16) -> &'static str {
    match (opcode & 0x8000 != 0, opcode & 0x00FF) {
        (true, 0x10) => "FXPWRI",
        (true, 0x12) => "FXPWRY",
        (_, op) => sane_op_name(op),
    }
}

/// SANE memory-load reporter. Fires when a SANE handler reads a NaN or
/// +/-inf value from guest memory — meaning the value was created somewhere
/// we don't trace (earlier SANE ops, game arithmetic, or uninit memory).
#[inline]
fn report_loaded_nan_if_enabled(
    pc: u32,
    caller: Option<u32>,
    addr: u32,
    fmt_label: &str,
    role: &str,
    value: f64,
) {
    if !trace_sane_nan_enabled() || value.is_finite() {
        return;
    }
    let caller_str = caller
        .map(|c| format!(" caller=${:08X}", c))
        .unwrap_or_default();
    eprintln!(
        "[SANE-NAN] LOAD {} ${:08X} fmt={} = {} ({}) pc=${:08X}{}",
        role,
        addr,
        fmt_label,
        value,
        if value.is_nan() { "NaN" } else { "inf" },
        pc,
        caller_str,
    );
}

/// Shared NaN/inf reporter. Logs an `op_name(inputs) → result` line when
/// the result is non-finite, flagging "newly produced" (input finite →
/// output non-finite) vs "propagated" cases.
#[inline]
fn report_nan_if_enabled(
    pc: u32,
    caller: Option<u32>,
    op_name: &str,
    x: f64,
    y: Option<f64>,
    result: f64,
) {
    if !trace_sane_nan_enabled() || result.is_finite() {
        return;
    }
    let inputs_finite = x.is_finite() && y.map(|v| v.is_finite()).unwrap_or(true);
    let tag = if inputs_finite { "NEW" } else { "PROP" };
    let kind = if result.is_nan() { "NaN" } else { "inf" };
    let caller_str = caller
        .map(|c| format!(" caller=${:08X}", c))
        .unwrap_or_default();
    match y {
        Some(v) => eprintln!(
            "[SANE-NAN] {} {}({}, {}) → {} ({}) pc=${:08X}{}",
            tag, op_name, x, v, result, kind, pc, caller_str,
        ),
        None => eprintln!(
            "[SANE-NAN] {} {}({}) → {} ({}) pc=${:08X}{}",
            tag, op_name, x, result, kind, pc, caller_str,
        ),
    }
}

impl super::TrapDispatcher {
    pub(crate) fn dispatch_sane<C: CpuOps>(
        &mut self,
        is_tool: bool,
        trap_num: u16,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Option<Result<()>> {
        let result = match (is_tool, trap_num) {
            // Pack4 / FP68K ($A9EB)
            // SANE floating-point arithmetic package.
            // Apple Numerics Manual Second Edition 1988
            // Pack4 / FP68K ($A9EB): Full SANE floating-point engine; see sub-operations below
            (true, 0x1EB) => {
                let sp = cpu.read_reg(Register::A7);
                let opcode = bus.read_word(sp);
                self.handle_fp68k(opcode, sp, cpu, bus)
            }

            // Pack5 / Elems68K ($A9EC)
            // SANE elementary functions package (sin, cos, exp, ln, etc).
            // Apple Numerics Manual Second Edition 1988
            // Pack5 / Elems68K ($A9EC): Full transcendental functions; see sub-operations below
            (true, 0x1EC) => {
                let sp = cpu.read_reg(Register::A7);
                let opcode = bus.read_word(sp);
                self.handle_elems68k(opcode, sp, cpu, bus)
            }

            _ => return None,
        };

        Some(result)
    }

    /// Handle FP68K (Pack4) operations.
    ///
    /// Stack layout for two-address ops:
    ///   SP+0: opcode (2), SP+2: dst_ptr (4), SP+6: src_ptr (4) = 10 bytes
    /// Stack layout for one-address ops:
    ///   SP+0: opcode (2), SP+2: dst_ptr (4) = 6 bytes
    fn handle_fp68k<C: CpuOps>(
        &mut self,
        opcode: u16,
        sp: u32,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Result<()> {
        // SANE opcode format:
        // Bits 14-11: operand format (0=ext, 1=dbl, 2=sgl, 3=?, 4=int16, 5=lint32, 6=comp)
        // Bits 10-0: operation code (but really only ~6 bits used)
        let op = opcode & 0x00FF; // operation (lower 8 bits seems safer)
        let fmt = (opcode >> 11) & 0x07; // operand format (bits 13-11)

        // Determine if this is a two-address or one-address operation
        // Two-address ops have: SP+0: opcode(2), SP+2: dst_ptr(4), SP+6: src_ptr(4) = 10 bytes
        // One-address ops have: SP+0: opcode(2), SP+2: dst_ptr(4) = 6 bytes
        let is_two_addr = matches!(
            op,
            0x00 | 0x02 | 0x04 | 0x06 | // add, sub, mul, div
            0x08 | 0x0A | 0x0C |         // cmp, cpx, rem
            0x0E | 0x10 |                 // z2x, x2z (conversions)
            0x18 |                         // scalb (scale binary, two-address)
            0x1C // class (classify, two-address: src=ext, dst=int16)
        );
        // PC reflects the post-trap address (m68k step advances pc += 2
        // past the A-line word). Subtract 2 to get the actual trap call
        // site for diagnostics.
        let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
        // When the trap was called via auto-pop (e.g. through a jump-table
        // trampoline), surface the original JSR-er PC too.
        let trap_caller = self.current_trap_caller;
        let trace_sane = trace_sane_enabled();
        let trace_sane_nan = trace_sane_nan_enabled();

        use super::extended80::Extended80;

        if is_two_addr {
            // Two-address: SP+0: opcode(2), SP+2: dst_ptr(4), SP+6: src_ptr(4)
            let dst_ptr = bus.read_long(sp + 2);
            let src_ptr = bus.read_long(sp + 6);
            cpu.write_reg(Register::A7, sp + 10);

            let src_ext = Extended80::read_format(bus, src_ptr, fmt);
            let dst_ext = Extended80::read_from_bus(bus, dst_ptr);
            let src_f = f64::from(src_ext);
            let dst_f = f64::from(dst_ext);
            if trace_sane {
                eprintln!(
                    "[SANE] FP68K pc=${:08X} caller={} opcode=0x{:04X} op=0x{:02X} fmt={} two=1 sp=${:08X} dst=${:08X} src=${:08X} dst={} src={} dst_raw={} src_raw={}",
                    trap_pc,
                    trap_caller
                        .map(|c| format!("${:08X}", c))
                        .unwrap_or_else(|| "-".to_string()),
                    opcode,
                    op,
                    fmt,
                    sp,
                    dst_ptr,
                    src_ptr,
                    dst_f,
                    src_f,
                    trace_sane_bytes(bus, dst_ptr, 10),
                    trace_sane_bytes(bus, src_ptr, if fmt == 0 { 10 } else { 8 }),
                );
            }
            // Report any non-finite SANE input load.
            if trace_sane_nan {
                let src_fmt = format!("{}", fmt);
                report_loaded_nan_if_enabled(
                    trap_pc,
                    trap_caller,
                    src_ptr,
                    &src_fmt,
                    "src",
                    src_f,
                );
                report_loaded_nan_if_enabled(trap_pc, trap_caller, dst_ptr, "ext", "dst", dst_f);
            }

            // Helper for the FP68K result + NaN/inf report.
            let fp_result = |op_name: &str, r: f64| {
                if trace_sane_nan {
                    report_nan_if_enabled(trap_pc, trap_caller, op_name, dst_f, Some(src_f), r);
                }
                Some(Extended80::from(r))
            };
            let result = match op {
                0x00 => fp_result("FADD", dst_f + src_f),
                0x02 => fp_result("FSUB", dst_f - src_f),
                0x04 => fp_result("FMUL", dst_f * src_f),
                0x06 => fp_result("FDIV", dst_f / src_f),
                0x08 | 0x0A => {
                    // MC68000 FP68K comparisons return the relation for
                    // DST compared with SRC in the CCR bits.
                    let ccr = if dst_f.is_nan() || src_f.is_nan() {
                        0x02 // V: unordered
                    } else if dst_f == src_f {
                        0x04 // Z
                    } else if dst_f < src_f {
                        0x19 // X + N + C
                    } else {
                        0x00
                    };
                    cpu.set_ccr(ccr);
                    return Ok(());
                }
                0x0C => {
                    // FREM: IEEE remainder in f64
                    let rem = dst_f - (dst_f / src_f).round() * src_f;
                    Some(Extended80::from(rem))
                }
                0x0E => {
                    // FZ2X: convert src format to extended dst
                    src_ext.write_to_bus(bus, dst_ptr);
                    return Ok(());
                }
                0x10 => {
                    // FX2Z: convert extended src to dst format
                    let ext_val = Extended80::read_from_bus(bus, src_ptr);
                    ext_val.write_format(bus, dst_ptr, fmt);
                    return Ok(());
                }
                0x18 => {
                    // FSCALB: dst = dst * 2^src (src is always int16)
                    let n = bus.read_word(src_ptr) as i16;
                    Some(Extended80::from(libm::scalbn(dst_f, n as i32)))
                }
                0x1C => {
                    // FCLASS: classify src, write int16 to dst
                    bus.write_word(dst_ptr, src_ext.classify() as u16);
                    return Ok(());
                }
                _ => None,
            };

            if let Some(r) = result {
                if trace_sane {
                    eprintln!(
                        "[SANE] FP68K result pc=${:08X} opcode=0x{:04X} dst=${:08X} result={}",
                        trap_pc,
                        opcode,
                        dst_ptr,
                        f64::from(r)
                    );
                }
                r.write_to_bus(bus, dst_ptr);
            }
        } else {
            // One-address: SP+0: opcode(2), SP+2: dst_ptr(4)
            let dst_ptr = bus.read_long(sp + 2);
            cpu.write_reg(Register::A7, sp + 6);

            let dst_ext = Extended80::read_from_bus(bus, dst_ptr);
            let dst_f = f64::from(dst_ext);
            if trace_sane {
                eprintln!(
                    "[SANE] FP68K pc=${:08X} caller={} opcode=0x{:04X} op=0x{:02X} fmt={} two=0 sp=${:08X} dst=${:08X} dst={} dst_raw={}",
                    trap_pc,
                    trap_caller
                        .map(|c| format!("${:08X}", c))
                        .unwrap_or_else(|| "-".to_string()),
                    opcode,
                    op,
                    fmt,
                    sp,
                    dst_ptr,
                    dst_f,
                    trace_sane_bytes(bus, dst_ptr, 10),
                );
            }
            // Report any non-finite SANE input load.
            if trace_sane_nan {
                report_loaded_nan_if_enabled(trap_pc, trap_caller, dst_ptr, "ext", "dst", dst_f);
            }

            // Same NaN/inf reporter for one-address FP68K ops.
            let fp1_result = |op_name: &str, r: f64| {
                if trace_sane_nan {
                    report_nan_if_enabled(trap_pc, trap_caller, op_name, dst_f, None, r);
                }
                Some(Extended80::from(r))
            };
            let result = match op {
                0x0D => fp1_result("FNEG", -dst_f),
                0x0F => fp1_result("FABS", dst_f.abs()),
                0x12 => fp1_result("FSQRT", libm::sqrt(dst_f)),
                0x14 => fp1_result("FRTI", libm::rint(dst_f)),
                0x16 => fp1_result("FTTI", libm::trunc(dst_f)),
                0x1A => fp1_result("FLOGB", libm::ilogb(dst_f) as f64),
                0x01 | 0x17 => {
                    bus.write_word(dst_ptr, 0);
                    return Ok(());
                } // FGETENV
                0x03 | 0x09 | 0x19 | 0x1E => {
                    return Ok(());
                } // FSETENV/FSETXCP/FPROCEXIT
                0x05 => {
                    bus.write_word(dst_ptr, 0);
                    return Ok(());
                } // FGETERR
                0x0B => {
                    bus.write_word(dst_ptr, 0);
                    return Ok(());
                } // FTESTXCP
                _ => None,
            };

            if let Some(r) = result {
                if trace_sane {
                    eprintln!(
                        "[SANE] FP68K result pc=${:08X} opcode=0x{:04X} dst=${:08X} result={}",
                        trap_pc,
                        opcode,
                        dst_ptr,
                        f64::from(r)
                    );
                }
                r.write_to_bus(bus, dst_ptr);
            }
        }

        Ok(())
    }

    /// Handle Elems68K (Pack5) transcendental operations.
    ///
    /// Opcode format is the same as FP68K: format bits in 13-11, operation in lower bits.
    /// When format != 0 (i.e. source is not extended), it's a two-address operation
    /// with src in the specified format and dst as extended.
    ///
    /// Two-address: SP+0: opcode(2), SP+2: dst_ptr(4), SP+6: src_ptr(4) = 10 bytes
    /// One-address: SP+0: opcode(2), SP+2: dst_ptr(4) = 6 bytes
    fn handle_elems68k<C: CpuOps>(
        &mut self,
        opcode: u16,
        sp: u32,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
    ) -> Result<()> {
        let op = opcode & 0x00FF;
        let fmt = (opcode >> 11) & 0x07;

        // Same trap-PC capture as in handle_fp68k.
        let trap_pc = cpu.read_reg(Register::PC).wrapping_sub(2);
        let trap_caller = self.current_trap_caller;
        let trace_sane = trace_sane_enabled();
        let trace_sane_nan = trace_sane_nan_enabled();
        if trace_sane {
            eprintln!(
                "[SANE] Elems68K opcode=0x{:04X} op=0x{:02X} fmt={} SP=${:08X}",
                opcode, op, fmt, sp
            );
        }

        use super::extended80::Extended80;

        // The high bit marks Elems68K's two-address power/financial forms.
        // Abuse uses opcode 0x8012; treating that as one-address leaves the
        // source pointer on the stack and corrupts the caller's return sequence.
        let is_two_addr_elem = (opcode & 0x8000) != 0;

        if is_two_addr_elem {
            let dst_ptr = bus.read_long(sp + 2);
            let src_ptr = bus.read_long(sp + 6);
            cpu.write_reg(Register::A7, sp + 10);

            let src_val = Extended80::read_format(bus, src_ptr, fmt);
            let dst_val = Extended80::read_from_bus(bus, dst_ptr);
            if trace_sane {
                eprintln!(
                    "[SANE] Elems68K pc=${:08X} caller={} opcode=0x{:04X} op={} fmt={} two=1 sp=${:08X} dst=${:08X} src=${:08X} dst={} src={} dst_raw={} src_raw={}",
                    trap_pc,
                    trap_caller
                        .map(|c| format!("${:08X}", c))
                        .unwrap_or_else(|| "-".to_string()),
                    opcode,
                    elems_op_name(opcode),
                    fmt,
                    sp,
                    dst_ptr,
                    src_ptr,
                    f64::from(dst_val),
                    f64::from(src_val),
                    trace_sane_bytes(bus, dst_ptr, 10),
                    trace_sane_bytes(bus, src_ptr, if fmt == 0 { 10 } else { 8 }),
                );
            }
            // Report any non-finite SANE input load.
            if trace_sane_nan {
                let src_fmt = format!("{}", fmt);
                report_loaded_nan_if_enabled(
                    trap_pc,
                    trap_caller,
                    src_ptr,
                    &src_fmt,
                    "src",
                    f64::from(src_val),
                );
                report_loaded_nan_if_enabled(
                    trap_pc,
                    trap_caller,
                    dst_ptr,
                    "ext",
                    "dst",
                    f64::from(dst_val),
                );
            }

            let result = match op {
                0x10 if (opcode & 0x8000) != 0 => {
                    // FXPWRI: dst^(int)src in f64. The high-bit Pack5 form
                    // is the two-address power entry; one-address 0x0010 is
                    // unrelated/unsupported here.
                    let base = f64::from(dst_val);
                    let i = f64::from(src_val) as i32;
                    let r = libm::pow(base, i as f64);
                    if trace_sane_nan {
                        report_nan_if_enabled(
                            trap_pc,
                            trap_caller,
                            "FXPWRI",
                            base,
                            Some(i as f64),
                            r,
                        );
                    }
                    Extended80::from(r)
                }
                0x12 if (opcode & 0x8000) != 0 => {
                    // Basilisk/System 7.5's Pack5 path consumes this two-address
                    // call but leaves DST unchanged. Abuse relies on that observed
                    // behavior when building its palette fade tables.
                    cpu.set_ccr(0x08);
                    dst_val
                }
                _ => Self::apply_elems_op(trace_sane_nan, trap_pc, trap_caller, op, src_val),
            };

            if trace_sane {
                eprintln!(
                    "[SANE] Elems68K result pc=${:08X} opcode=0x{:04X} op={} dst=${:08X} result={}",
                    trap_pc,
                    opcode,
                    elems_op_name(opcode),
                    dst_ptr,
                    f64::from(result)
                );
            }
            result.write_to_bus(bus, dst_ptr);
            return Ok(());
        }

        let dst_ptr = bus.read_long(sp + 2);
        cpu.write_reg(Register::A7, sp + 6);

        let val = Extended80::read_from_bus(bus, dst_ptr);
        if trace_sane {
            eprintln!(
                "[SANE] Elems68K pc=${:08X} caller={} opcode=0x{:04X} op={} fmt={} two=0 sp=${:08X} dst=${:08X} val={} raw={}",
                trap_pc,
                trap_caller
                    .map(|c| format!("${:08X}", c))
                    .unwrap_or_else(|| "-".to_string()),
                opcode,
                elems_op_name(opcode),
                fmt,
                sp,
                dst_ptr,
                f64::from(val),
                trace_sane_bytes(bus, dst_ptr, 10),
            );
        }
        // Report any non-finite SANE input load.
        if trace_sane_nan {
            report_loaded_nan_if_enabled(
                trap_pc,
                trap_caller,
                dst_ptr,
                "ext",
                "dst",
                f64::from(val),
            );
        }
        let result = Self::apply_elems_op(trace_sane_nan, trap_pc, trap_caller, op, val);

        if trace_sane {
            eprintln!(
                "[SANE] Elems68K result pc=${:08X} opcode=0x{:04X} op={} dst=${:08X} result={}",
                trap_pc,
                opcode,
                elems_op_name(opcode),
                dst_ptr,
                f64::from(result)
            );
        }
        result.write_to_bus(bus, dst_ptr);
        Ok(())
    }

    /// Evaluate an Elems68K operation using f64 + libm.
    /// This matches the libm-bridge's ext80_elems_op pipeline exactly:
    /// Extended80 → f64 → libm function → f64 → Extended80
    fn apply_elems_op(
        trace_sane_nan: bool,
        pc: u32,
        caller: Option<u32>,
        op: u16,
        val: super::extended80::Extended80,
    ) -> super::extended80::Extended80 {
        use super::extended80::Extended80;
        let x = f64::from(val);
        let result = match op {
            0x00 => libm::log(x),
            0x02 => libm::log2(x),
            0x04 => libm::log(1.0 + x),
            0x06 => libm::log2(1.0 + x),
            0x08 => libm::exp(x),
            0x0A => libm::exp2(x),
            0x0C => libm::exp(x) - 1.0,
            0x0E => libm::exp2(x) - 1.0,
            0x18 => libm::sin(x),
            0x1A => libm::cos(x),
            0x1C => libm::tan(x),
            0x1E => libm::atan(x),
            _ => return val,
        };
        // Warn when result is NaN/inf via the shared reporter.
        if trace_sane_nan {
            report_nan_if_enabled(pc, caller, sane_op_name(op), x, None, result);
        }
        Extended80::from(result)
    }

    /// Read a floating-point value from guest memory in the specified SANE format.
    /// Format: 0=extended(10), 1=double(8), 2=single(4), 4=integer(2), 5=longint(4), 6=comp(8)
    /// Kept as infrastructure for future SANE op coverage.
    #[allow(dead_code)]
    fn read_fp_value(&self, bus: &MacMemoryBus, addr: u32, fmt: u16) -> f64 {
        match fmt {
            0 => self.read_fp_extended(bus, addr),
            1 => self.read_fp_double(bus, addr),
            2 => self.read_fp_single(bus, addr),
            4 => bus.read_word(addr) as i16 as f64,
            5 => bus.read_long(addr) as i32 as f64,
            6 => self.read_fp_comp(bus, addr),
            _ => {
                eprintln!("[SANE] Unknown format {} at ${:08X}", fmt, addr);
                0.0
            }
        }
    }

    /// Write a floating-point value to guest memory in the specified SANE format.
    /// Kept as infrastructure (paired with read_fp_value).
    #[allow(dead_code)]
    fn write_fp_value(&self, bus: &mut MacMemoryBus, addr: u32, fmt: u16, val: f64) {
        match fmt {
            0 => self.write_fp_extended(bus, addr, val),
            1 => self.write_fp_double(bus, addr, val),
            2 => self.write_fp_single(bus, addr, val),
            4 => bus.write_word(addr, (val as i16) as u16),
            5 => bus.write_long(addr, (val as i32) as u32),
            6 => self.write_fp_comp(bus, addr, val),
            _ => eprintln!("[SANE] Cannot write unknown format {}", fmt),
        }
    }

    /// Read 80-bit extended precision from guest memory (big-endian Motorola format).
    ///
    /// Format: sign(1) + exponent(15) + integer_bit(1) + fraction(63) = 80 bits = 10 bytes
    fn read_fp_extended(&self, bus: &MacMemoryBus, addr: u32) -> f64 {
        // Read 10 bytes in big-endian order
        let w0 = bus.read_word(addr);
        let w1 = bus.read_word(addr + 2);
        let w2 = bus.read_word(addr + 4);
        let w3 = bus.read_word(addr + 6);
        let w4 = bus.read_word(addr + 8);

        let sign = (w0 >> 15) & 1;
        let exponent = w0 & 0x7FFF;
        let significand: u64 =
            ((w1 as u64) << 48) | ((w2 as u64) << 32) | ((w3 as u64) << 16) | (w4 as u64);

        // Handle special cases
        if exponent == 0 && significand == 0 {
            return if sign == 1 { -0.0 } else { 0.0 };
        }
        if exponent == 0x7FFF {
            if significand == 0 {
                return if sign == 1 {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                };
            }
            return f64::NAN;
        }

        // Convert: value = (-1)^sign * 2^(exponent - 16383) * significand/2^63
        // The integer bit is explicit in extended format (bit 63 of significand)
        let exp = exponent as i32 - 16383;
        let frac = significand as f64 / (1u64 << 63) as f64;
        let val = frac * (2.0f64).powi(exp);

        if sign == 1 {
            -val
        } else {
            val
        }
    }

    /// Write f64 as 80-bit extended precision to guest memory (big-endian Motorola format).
    fn write_fp_extended(&self, bus: &mut MacMemoryBus, addr: u32, val: f64) {
        if val.is_nan() {
            // NaN
            bus.write_word(addr, 0x7FFF);
            bus.write_word(addr + 2, 0xFFFF);
            bus.write_word(addr + 4, 0xFFFF);
            bus.write_word(addr + 6, 0xFFFF);
            bus.write_word(addr + 8, 0xFFFF);
            return;
        }
        if val.is_infinite() {
            let sign_bit = if val < 0.0 { 0x8000u16 } else { 0u16 };
            bus.write_word(addr, sign_bit | 0x7FFF);
            bus.write_word(addr + 2, 0x0000);
            bus.write_word(addr + 4, 0x0000);
            bus.write_word(addr + 6, 0x0000);
            bus.write_word(addr + 8, 0x0000);
            return;
        }
        if val == 0.0 {
            let sign_bit = if val.is_sign_negative() {
                0x8000u16
            } else {
                0u16
            };
            bus.write_word(addr, sign_bit);
            bus.write_word(addr + 2, 0);
            bus.write_word(addr + 4, 0);
            bus.write_word(addr + 6, 0);
            bus.write_word(addr + 8, 0);
            return;
        }

        let sign = if val < 0.0 { 1u16 } else { 0u16 };
        let abs_val = val.abs();

        // Decompose: abs_val = frac * 2^exp where 1.0 <= frac < 2.0
        // frexp returns (frac, exp) where 0.5 <= frac < 1.0, so adjust
        let bits = abs_val.to_bits();
        let ieee_exp = ((bits >> 52) & 0x7FF) as i32;
        let ieee_frac = bits & 0x000FFFFFFFFFFFFF;

        if ieee_exp == 0 {
            // Subnormal double - just write zero for simplicity
            bus.write_word(addr, sign << 15);
            bus.write_word(addr + 2, 0);
            bus.write_word(addr + 4, 0);
            bus.write_word(addr + 6, 0);
            bus.write_word(addr + 8, 0);
            return;
        }

        // IEEE 754 double: 1.fraction * 2^(ieee_exp - 1023)
        // Extended:         integer.fraction * 2^(ext_exp - 16383)
        let ext_exp = (ieee_exp - 1023 + 16383) as u16;

        // Build 64-bit significand with explicit integer bit
        // Double has 52 fraction bits, extended has 63 fraction bits
        let significand: u64 = (1u64 << 63) | (ieee_frac << 11);

        let w0 = (sign << 15) | ext_exp;
        let w1 = (significand >> 48) as u16;
        let w2 = (significand >> 32) as u16;
        let w3 = (significand >> 16) as u16;
        let w4 = significand as u16;

        bus.write_word(addr, w0);
        bus.write_word(addr + 2, w1);
        bus.write_word(addr + 4, w2);
        bus.write_word(addr + 6, w3);
        bus.write_word(addr + 8, w4);
    }

    /// Read IEEE 754 double (64-bit) from guest memory (big-endian).
    fn read_fp_double(&self, bus: &MacMemoryBus, addr: u32) -> f64 {
        let hi = bus.read_long(addr) as u64;
        let lo = bus.read_long(addr + 4) as u64;
        f64::from_bits((hi << 32) | lo)
    }

    /// Write IEEE 754 double (64-bit) to guest memory (big-endian).
    fn write_fp_double(&self, bus: &mut MacMemoryBus, addr: u32, val: f64) {
        let bits = val.to_bits();
        bus.write_long(addr, (bits >> 32) as u32);
        bus.write_long(addr + 4, bits as u32);
    }

    /// Read IEEE 754 single (32-bit) from guest memory (big-endian).
    fn read_fp_single(&self, bus: &MacMemoryBus, addr: u32) -> f64 {
        let bits = bus.read_long(addr);
        f32::from_bits(bits) as f64
    }

    /// Write IEEE 754 single (32-bit) to guest memory (big-endian).
    fn write_fp_single(&self, bus: &mut MacMemoryBus, addr: u32, val: f64) {
        let bits = (val as f32).to_bits();
        bus.write_long(addr, bits);
    }

    /// Read SANE Comp (64-bit signed integer) from guest memory.
    fn read_fp_comp(&self, bus: &MacMemoryBus, addr: u32) -> f64 {
        let hi = bus.read_long(addr) as u64;
        let lo = bus.read_long(addr + 4) as u64;
        let val = ((hi << 32) | lo) as i64;
        val as f64
    }

    /// Write SANE Comp (64-bit signed integer) to guest memory.
    fn write_fp_comp(&self, bus: &mut MacMemoryBus, addr: u32, val: f64) {
        let ival = val as i64;
        bus.write_long(addr, (ival >> 32) as u32);
        bus.write_long(addr + 4, ival as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{setup, MockCpu, TEST_SP};
    use crate::cpu::{CpuOps, Register};
    use crate::memory::MemoryBus;

    const DST_ADDR: u32 = 0x300000;
    const SRC_ADDR: u32 = 0x300100;
    const NON_STACK_REGS: [Register; 15] = [
        Register::D0,
        Register::D1,
        Register::D2,
        Register::D3,
        Register::D4,
        Register::D5,
        Register::D6,
        Register::D7,
        Register::A0,
        Register::A1,
        Register::A2,
        Register::A3,
        Register::A4,
        Register::A5,
        Register::A6,
    ];

    fn seed_non_stack_registers(cpu: &mut MockCpu) -> Vec<(Register, u32)> {
        NON_STACK_REGS
            .iter()
            .enumerate()
            .map(|(idx, reg)| {
                let value = 0x1111_0000u32.wrapping_add((idx as u32) * 0x101);
                cpu.write_reg(*reg, value);
                (*reg, value)
            })
            .collect()
    }

    fn assert_non_stack_registers_unchanged(cpu: &MockCpu, expected: &[(Register, u32)]) {
        for (reg, value) in expected {
            assert_eq!(
                cpu.read_reg(*reg),
                *value,
                "register {:?} changed across SANE call",
                reg
            );
        }
    }

    /// Helper: set up a two-address FP68K operation on the stack and dispatch it.
    /// Stack layout: SP+0: opcode(2), SP+2: dst_ptr(4), SP+6: src_ptr(4)
    fn run_fp68k_two_addr(
        opcode: u16,
        dst_val: f64,
        src_val: f64,
    ) -> (f64, u32, crate::trap::test_helpers::MockCpu) {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, dst_val);
        disp.write_fp_extended(&mut bus, SRC_ADDR, src_val);

        bus.write_word(sp, opcode);
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        let result = disp.read_fp_extended(&bus, DST_ADDR);
        let new_sp = cpu.read_reg(Register::A7);
        (result, new_sp, cpu)
    }

    /// Helper: set up a one-address FP68K operation on the stack and dispatch it.
    /// Stack layout: SP+0: opcode(2), SP+2: dst_ptr(4)
    fn run_fp68k_one_addr(opcode: u16, dst_val: f64) -> (f64, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, dst_val);

        bus.write_word(sp, opcode);
        bus.write_long(sp + 2, DST_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        let result = disp.read_fp_extended(&bus, DST_ADDR);
        let new_sp = cpu.read_reg(Register::A7);
        (result, new_sp)
    }

    /// Helper: set up a one-address Elems68K operation and dispatch it.
    fn run_elems_one_addr(opcode: u16, dst_val: f64) -> (f64, u32) {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, dst_val);

        bus.write_word(sp, opcode);
        bus.write_long(sp + 2, DST_ADDR);

        disp.dispatch_sane(true, 0x1EC, &mut cpu, &mut bus);

        let result = disp.read_fp_extended(&bus, DST_ADDR);
        let new_sp = cpu.read_reg(Register::A7);
        (result, new_sp)
    }

    // -----------------------------------------------------------------------
    // FP68K (Pack4, trap 0x1EB) tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_fadd() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x0000, 3.0, 2.0);
        assert!((result - 5.0).abs() < 1e-10, "expected 5.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn pack4_two_address_call_consumes_opword_and_two_pointers() {
        // IM:II (1985) p. II-406: _FP68K/_Pack4 macros push a 2-byte opword.
        // Two-address calls in this runtime then consume dst/src pointers.
        let (result, new_sp, _) = run_fp68k_two_addr(0x0000, 3.0, 2.0);
        assert!((result - 5.0).abs() < 1e-10, "expected 5.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn pack4_non_remainder_ops_preserve_non_stack_registers() {
        // IM:II (1985) p. II-406 notes Pack4 preserves MC68000 registers
        // across calls (with remainder op as a documented exception).
        let (mut disp, mut cpu, mut bus) = setup();
        let saved = seed_non_stack_registers(&mut cpu);
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 3.0);
        disp.write_fp_extended(&mut bus, SRC_ADDR, 2.0);
        bus.write_word(sp, 0x0000); // FADD
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        assert_non_stack_registers_unchanged(&cpu, &saved);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn test_fsub() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x0002, 5.0, 3.0);
        assert!((result - 2.0).abs() < 1e-10, "expected 2.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn test_fmul() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x0004, 3.0, 4.0);
        assert!(
            (result - 12.0).abs() < 1e-10,
            "expected 12.0, got {}",
            result
        );
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn test_fdiv() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x0006, 10.0, 4.0);
        assert!((result - 2.5).abs() < 1e-10, "expected 2.5, got {}", result);
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn test_fdiv_by_zero() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x0006, 1.0, 0.0);
        assert!(result.is_infinite(), "expected infinity, got {}", result);
        assert!(result > 0.0, "expected positive infinity");
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn test_fcmp_equal() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 5.0);
        disp.write_fp_extended(&mut bus, SRC_ADDR, 5.0);

        bus.write_word(sp, 0x0008); // FCMP opcode
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        assert_eq!(
            cpu.ccr, 0x04,
            "expected Z flag (0x04), got 0x{:02X}",
            cpu.ccr
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn test_fcmp_destination_less_than_source() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 3.0);
        disp.write_fp_extended(&mut bus, SRC_ADDR, 5.0);

        bus.write_word(sp, 0x0008);
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        assert_eq!(
            cpu.ccr, 0x19,
            "expected X+N+C flags (0x19), got 0x{:02X}",
            cpu.ccr
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn test_fcmp_destination_greater_than_source() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 7.0);
        disp.write_fp_extended(&mut bus, SRC_ADDR, 5.0);

        bus.write_word(sp, 0x0008);
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        assert_eq!(
            cpu.ccr, 0x00,
            "expected no flags (0x00), got 0x{:02X}",
            cpu.ccr
        );
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
    }

    #[test]
    fn test_frem() {
        let (result, new_sp, _) = run_fp68k_two_addr(0x000C, 7.0, 3.0);
        assert!((result - 1.0).abs() < 1e-10, "expected 1.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 10);
    }

    #[test]
    fn test_fneg() {
        let (result, new_sp) = run_fp68k_one_addr(0x000D, 3.0);
        assert!(
            (result - (-3.0)).abs() < 1e-10,
            "expected -3.0, got {}",
            result
        );
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fabs() {
        let (result, new_sp) = run_fp68k_one_addr(0x000F, -5.0);
        assert!((result - 5.0).abs() < 1e-10, "expected 5.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fsqrt() {
        let (result, new_sp) = run_fp68k_one_addr(0x0012, 9.0);
        assert!((result - 3.0).abs() < 1e-10, "expected 3.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_frti() {
        let (result, new_sp) = run_fp68k_one_addr(0x0014, 3.7);
        assert!((result - 4.0).abs() < 1e-10, "expected 4.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_ftti() {
        let (result, new_sp) = run_fp68k_one_addr(0x0016, 3.7);
        assert!((result - 3.0).abs() < 1e-10, "expected 3.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_flogb() {
        let (result, new_sp) = run_fp68k_one_addr(0x001A, 8.0);
        assert!((result - 3.0).abs() < 1e-10, "expected 3.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fgetenv() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // Pre-fill dst with non-zero to verify it gets cleared
        bus.write_word(DST_ADDR, 0xFFFF);

        bus.write_word(sp, 0x0001); // FGETENV opcode
        bus.write_long(sp + 2, DST_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        let env_word = bus.read_word(DST_ADDR);
        assert_eq!(env_word, 0, "expected environment word 0, got {}", env_word);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn test_fsetenv() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 0.0);

        bus.write_word(sp, 0x001E); // FSETENV opcode
        bus.write_long(sp + 2, DST_ADDR);

        disp.dispatch_sane(true, 0x1EB, &mut cpu, &mut bus);

        // FSETENV is a no-op in the emulator; just verify SP advanced
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    // -----------------------------------------------------------------------
    // Elems68K (Pack5, trap 0x1EC) tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_fln() {
        let e = std::f64::consts::E;
        let (result, new_sp) = run_elems_one_addr(0x0000, e);
        assert!((result - 1.0).abs() < 1e-10, "expected 1.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn pack5_one_address_call_consumes_opword_and_pointer() {
        // IM:II (1985) p. II-407: _Elems68K/_Pack5 macros push a 2-byte opword.
        // One-address calls in this runtime then consume one destination pointer.
        let (result, new_sp) = run_elems_one_addr(0x0018, 0.0); // FSIN
        assert!(result.abs() < 1e-10, "expected 0.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn pack5_high_bit_8012_consumes_two_addresses_and_preserves_destination() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, 2.0);
        disp.write_fp_extended(&mut bus, SRC_ADDR, 3.0);

        bus.write_word(sp, 0x8012);
        bus.write_long(sp + 2, DST_ADDR);
        bus.write_long(sp + 6, SRC_ADDR);

        disp.dispatch_sane(true, 0x1EC, &mut cpu, &mut bus);

        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert!((result - 2.0).abs() < 1e-10, "expected 2.0, got {}", result);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 10);
        assert_eq!(cpu.ccr, 0x08);
    }

    #[test]
    fn pack5_one_address_0012_is_unimplemented_noop() {
        let (result, new_sp) = run_elems_one_addr(0x0012, 81.0);
        assert!(
            (result - 81.0).abs() < 1e-10,
            "expected 81.0, got {}",
            result
        );
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn pack5_ops_preserve_non_stack_registers() {
        // IM:II (1985) p. II-407 notes Pack5 preserves MC68000 registers
        // across calls while updating floating-point condition codes.
        let (mut disp, mut cpu, mut bus) = setup();
        let saved = seed_non_stack_registers(&mut cpu);
        let sp = TEST_SP;

        disp.write_fp_extended(&mut bus, DST_ADDR, std::f64::consts::E);
        bus.write_word(sp, 0x0000); // FLN
        bus.write_long(sp + 2, DST_ADDR);

        disp.dispatch_sane(true, 0x1EC, &mut cpu, &mut bus);

        assert_non_stack_registers_unchanged(&cpu, &saved);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
    }

    #[test]
    fn test_flog2() {
        let (result, new_sp) = run_elems_one_addr(0x0002, 8.0);
        assert!((result - 3.0).abs() < 1e-10, "expected 3.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fexp() {
        let (result, new_sp) = run_elems_one_addr(0x0008, 1.0);
        assert!(
            (result - std::f64::consts::E).abs() < 1e-6,
            "expected e (~2.71828), got {}",
            result
        );
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fexp2() {
        let (result, new_sp) = run_elems_one_addr(0x000A, 3.0);
        assert!((result - 8.0).abs() < 1e-10, "expected 8.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fsin() {
        // FSINX ($0018): sin(0) = 0
        // Apple Numerics Manual Second Edition 1988, p. 11017
        let (result, new_sp) = run_elems_one_addr(0x0018, 0.0);
        assert!(result.abs() < 1e-10, "expected 0.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fcos() {
        // FCOSX ($001A): cos(0) = 1
        // Apple Numerics Manual Second Edition 1988, p. 11018
        let (result, new_sp) = run_elems_one_addr(0x001A, 0.0);
        assert!((result - 1.0).abs() < 1e-10, "expected 1.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_ftan() {
        let (result, new_sp) = run_elems_one_addr(0x001C, 0.0);
        assert!(result.abs() < 1e-10, "expected 0.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fatan() {
        let (result, new_sp) = run_elems_one_addr(0x001E, 1.0);
        let expected = std::f64::consts::FRAC_PI_4;
        assert!(
            (result - expected).abs() < 1e-6,
            "expected pi/4 (~0.7854), got {}",
            result
        );
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fexp1() {
        let (result, new_sp) = run_elems_one_addr(0x000C, 1.0);
        let expected = std::f64::consts::E - 1.0;
        assert!(
            (result - expected).abs() < 1e-10,
            "expected {}, got {}",
            expected,
            result
        );
        assert_eq!(new_sp, TEST_SP + 6);
    }

    #[test]
    fn test_fexp21() {
        let (result, new_sp) = run_elems_one_addr(0x000E, 3.0);
        assert!((result - 7.0).abs() < 1e-10, "expected 7.0, got {}", result);
        assert_eq!(new_sp, TEST_SP + 6);
    }

    // -----------------------------------------------------------------------
    // Extended round-trip tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_read_extended_positive() {
        let (disp, _, mut bus) = setup();
        disp.write_fp_extended(&mut bus, DST_ADDR, 42.5);
        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert!(
            (result - 42.5).abs() < 1e-10,
            "expected 42.5, got {}",
            result
        );
    }

    #[test]
    fn test_write_read_extended_negative() {
        let (disp, _, mut bus) = setup();
        disp.write_fp_extended(&mut bus, DST_ADDR, -100.0);
        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert!(
            (result - (-100.0)).abs() < 1e-10,
            "expected -100.0, got {}",
            result
        );
    }

    #[test]
    fn test_extended_zero() {
        let (disp, _, mut bus) = setup();
        disp.write_fp_extended(&mut bus, DST_ADDR, 0.0);
        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert_eq!(result, 0.0, "expected 0.0, got {}", result);
    }

    #[test]
    fn test_extended_nan() {
        let (disp, _, mut bus) = setup();
        disp.write_fp_extended(&mut bus, DST_ADDR, f64::NAN);
        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert!(result.is_nan(), "expected NaN, got {}", result);
    }

    #[test]
    fn test_extended_infinity() {
        let (disp, _, mut bus) = setup();
        disp.write_fp_extended(&mut bus, DST_ADDR, f64::INFINITY);
        let result = disp.read_fp_extended(&bus, DST_ADDR);
        assert!(result.is_infinite(), "expected infinity, got {}", result);
        assert!(result > 0.0, "expected positive infinity");
    }

    // -----------------------------------------------------------------------
    // Format conversion round-trip tests
    // -----------------------------------------------------------------------

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159… used as a test bit-pattern, not as π
    fn test_write_read_double() {
        let (disp, _, mut bus) = setup();
        let val = 3.141592653589793;
        disp.write_fp_double(&mut bus, DST_ADDR, val);
        let result = disp.read_fp_double(&bus, DST_ADDR);
        assert!(
            (result - val).abs() < 1e-15,
            "expected {}, got {}",
            val,
            result
        );
    }

    #[test]
    fn test_write_read_single() {
        let (disp, _, mut bus) = setup();
        let val = 2.5;
        disp.write_fp_single(&mut bus, DST_ADDR, val);
        let result = disp.read_fp_single(&bus, DST_ADDR);
        assert!(
            (result - val).abs() < 1e-6,
            "expected {}, got {}",
            val,
            result
        );
    }

    #[test]
    fn test_write_read_comp() {
        let (disp, _, mut bus) = setup();
        let val = 123456789.0;
        disp.write_fp_comp(&mut bus, DST_ADDR, val);
        let result = disp.read_fp_comp(&bus, DST_ADDR);
        assert!(
            (result - val).abs() < 1e-10,
            "expected {}, got {}",
            val,
            result
        );
    }
}
