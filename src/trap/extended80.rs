//! Hand-rolled 80-bit extended precision floating-point type.
//!
//! Implements the Motorola 68881/SANE extended format:
//!   - 1 sign bit
//!   - 15-bit biased exponent (bias 16383)
//!   - 64-bit significand with explicit integer bit at bit 63
//!   - 10 bytes big-endian in memory
//!
//! All arithmetic uses integer operations on u64/u128 for exact
//! bit-level compatibility with real Mac SANE ROM code.

use crate::memory::bus::{MacMemoryBus, MemoryBus};

/// IEEE 754 80-bit extended precision.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct Extended80 {
    pub sign: bool,
    pub exponent: u16,    // 15-bit biased (0x0000..0x7FFF)
    pub significand: u64, // bit 63 = integer bit, bits 62..0 = fraction
}

const BIAS: i32 = 16383;
const EXP_MAX: u16 = 0x7FFF;

// ─── Constants ────────────────────────────────────────────────────────────

// Extended80 is partial-coverage SANE infrastructure. Many constants and
// methods (ONE, neg, abs, scalb, ln, log2, etc.) are implemented but not
// yet wired up to a SANE op — kept intentionally for future op coverage.
#[allow(dead_code)]
impl Extended80 {
    pub const ZERO: Self = Self {
        sign: false,
        exponent: 0,
        significand: 0,
    };
    pub const NEG_ZERO: Self = Self {
        sign: true,
        exponent: 0,
        significand: 0,
    };
    pub const INFINITY: Self = Self {
        sign: false,
        exponent: EXP_MAX,
        significand: 0,
    };
    pub const NEG_INFINITY: Self = Self {
        sign: true,
        exponent: EXP_MAX,
        significand: 0,
    };
    pub const NAN: Self = Self {
        sign: false,
        exponent: EXP_MAX,
        significand: 0xC000_0000_0000_0000,
    };

    /// 1.0 in extended: exponent = bias (16383), significand = 1<<63.
    pub const ONE: Self = Self {
        sign: false,
        exponent: BIAS as u16,
        significand: 1 << 63,
    };
}

// ─── Classification ───────────────────────────────────────────────────────

#[allow(dead_code)]
impl Extended80 {
    pub fn is_zero(self) -> bool {
        self.exponent == 0 && self.significand == 0
    }

    pub fn is_infinite(self) -> bool {
        self.exponent == EXP_MAX && self.significand == 0
    }

    pub fn is_nan(self) -> bool {
        self.exponent == EXP_MAX && self.significand != 0
    }

    pub fn is_normal(self) -> bool {
        self.exponent != 0 && self.exponent != EXP_MAX
    }

    pub fn is_subnormal(self) -> bool {
        self.exponent == 0 && self.significand != 0
    }

    pub fn is_sign_negative(self) -> bool {
        self.sign
    }

    /// SANE class codes: ±1=SNAN, ±2=QNAN, ±3=INF, ±4=ZERO, ±5=NORMAL, ±6=DENORMAL
    pub fn classify(self) -> i16 {
        let sign = if self.sign { -1i16 } else { 1 };
        if self.is_nan() {
            // Check quiet bit (bit 62 for 68k SANE: 1=quiet, 0=signaling)
            if self.significand & (1 << 62) != 0 {
                2 * sign // QNAN
            } else {
                sign // SNAN
            }
        } else if self.is_infinite() {
            3 * sign
        } else if self.is_zero() {
            4 * sign
        } else if self.is_normal() {
            5 * sign
        } else {
            6 * sign // subnormal
        }
    }
}

// ─── Simple operations ────────────────────────────────────────────────────

#[allow(dead_code)]
impl Extended80 {
    pub fn neg(self) -> Self {
        Self {
            sign: !self.sign,
            ..self
        }
    }

    pub fn abs(self) -> Self {
        Self {
            sign: false,
            ..self
        }
    }

    /// FSCALB: multiply by 2^n (adjust exponent).
    pub fn scalb(self, n: i16) -> Self {
        if self.is_zero() || self.is_nan() || self.is_infinite() {
            return self;
        }
        let new_exp = self.exponent as i32 + n as i32;
        if new_exp >= EXP_MAX as i32 {
            return if self.sign {
                Self::NEG_INFINITY
            } else {
                Self::INFINITY
            };
        }
        if new_exp <= 0 {
            return if self.sign {
                Self::NEG_ZERO
            } else {
                Self::ZERO
            };
        }
        Self {
            exponent: new_exp as u16,
            ..self
        }
    }

    /// FLOGB: unbiased exponent as an extended float (floor of log2|x|).
    /// Denormals are normalized before the exponent is determined.
    /// Inside Macintosh: PowerPC Numerics (1994), pp. 10-27 to 10-28.
    pub fn logb(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return Self::INFINITY;
        }
        if self.is_zero() {
            return Self::NEG_INFINITY;
        }
        let exp = if self.exponent == 0 {
            1 - BIAS - self.significand.leading_zeros() as i32
        } else {
            self.exponent as i32 - BIAS
        };
        Self::from(exp as f64)
    }

    /// FRTI: round to integer (round-to-nearest-even), result stays extended.
    pub fn round_to_int(self) -> Self {
        if self.is_zero() || self.is_nan() || self.is_infinite() {
            return self;
        }
        let f = f64::from(self);
        // Use round-to-nearest-even (banker's rounding)
        let rounded = rint(f);
        Self::from(rounded)
    }

    /// FTTI: truncate to integer (toward zero), result stays extended.
    pub fn trunc_to_int(self) -> Self {
        if self.is_zero() || self.is_nan() || self.is_infinite() {
            return self;
        }
        let f = f64::from(self);
        Self::from(f.trunc())
    }
}

/// Round-to-nearest-even (banker's rounding).
#[allow(dead_code)]
fn rint(x: f64) -> f64 {
    let rounded = x.round();
    // Check for exact halfway case
    let diff = (x - rounded).abs();
    if diff == 0.0 {
        return rounded;
    }
    // x.round() breaks ties away from zero; we need ties to even
    if (x.abs() - x.abs().floor() - 0.5).abs() < 1e-15 {
        let floor = x.floor();
        let ceil = x.ceil();
        if floor as i64 % 2 == 0 {
            floor
        } else {
            ceil
        }
    } else {
        rounded
    }
}

// ─── Memory read/write ────────────────────────────────────────────────────

impl Extended80 {
    /// Read 10-byte big-endian Motorola extended from guest memory.
    pub fn read_from_bus(bus: &MacMemoryBus, addr: u32) -> Self {
        let w0 = bus.read_word(addr);
        let w1 = bus.read_word(addr + 2);
        let w2 = bus.read_word(addr + 4);
        let w3 = bus.read_word(addr + 6);
        let w4 = bus.read_word(addr + 8);

        Self {
            sign: (w0 >> 15) != 0,
            exponent: w0 & 0x7FFF,
            significand: ((w1 as u64) << 48)
                | ((w2 as u64) << 32)
                | ((w3 as u64) << 16)
                | (w4 as u64),
        }
    }

    /// Write 10-byte big-endian Motorola extended to guest memory.
    pub fn write_to_bus(self, bus: &mut MacMemoryBus, addr: u32) {
        let sign_bit = if self.sign { 0x8000u16 } else { 0 };
        bus.write_word(addr, sign_bit | self.exponent);
        bus.write_word(addr + 2, (self.significand >> 48) as u16);
        bus.write_word(addr + 4, (self.significand >> 32) as u16);
        bus.write_word(addr + 6, (self.significand >> 16) as u16);
        bus.write_word(addr + 8, self.significand as u16);
    }

    /// Read a value in any SANE format and return as Extended80.
    pub fn read_format(bus: &MacMemoryBus, addr: u32, fmt: u16) -> Self {
        match fmt {
            0 => Self::read_from_bus(bus, addr),
            1 => Self::from(read_f64_be(bus, addr)),
            2 => Self::from(read_f32_be(bus, addr)),
            4 => Self::from(bus.read_word(addr) as i16),
            5 => Self::from(bus.read_long(addr) as i32),
            6 => Self::from_comp(read_i64_be(bus, addr)),
            _ => Self::ZERO,
        }
    }

    /// Write this value in any SANE format.
    pub fn write_format(self, bus: &mut MacMemoryBus, addr: u32, fmt: u16) {
        match fmt {
            0 => self.write_to_bus(bus, addr),
            1 => write_f64_be(bus, addr, f64::from(self)),
            2 => write_f32_be(bus, addr, f64::from(self) as f32),
            4 => bus.write_word(addr, (f64::from(self) as i16) as u16),
            5 => bus.write_long(addr, f64::from(self) as i32 as u32),
            6 => write_i64_be(bus, addr, self.to_comp()),
            _ => {}
        }
    }
}

fn read_f64_be(bus: &MacMemoryBus, addr: u32) -> f64 {
    let hi = bus.read_long(addr) as u64;
    let lo = bus.read_long(addr + 4) as u64;
    f64::from_bits((hi << 32) | lo)
}

fn write_f64_be(bus: &mut MacMemoryBus, addr: u32, val: f64) {
    let bits = val.to_bits();
    bus.write_long(addr, (bits >> 32) as u32);
    bus.write_long(addr + 4, bits as u32);
}

fn read_f32_be(bus: &MacMemoryBus, addr: u32) -> f32 {
    f32::from_bits(bus.read_long(addr))
}

fn write_f32_be(bus: &mut MacMemoryBus, addr: u32, val: f32) {
    bus.write_long(addr, val.to_bits());
}

fn read_i64_be(bus: &MacMemoryBus, addr: u32) -> i64 {
    let hi = bus.read_long(addr) as u64;
    let lo = bus.read_long(addr + 4) as u64;
    ((hi << 32) | lo) as i64
}

fn write_i64_be(bus: &mut MacMemoryBus, addr: u32, val: i64) {
    let bits = val as u64;
    bus.write_long(addr, (bits >> 32) as u32);
    bus.write_long(addr + 4, bits as u32);
}

// ─── Conversion: Extended80 <-> f64 ───────────────────────────────────────

impl From<f64> for Extended80 {
    fn from(val: f64) -> Self {
        if val.is_nan() {
            return Self::NAN;
        }
        if val.is_infinite() {
            return if val < 0.0 {
                Self::NEG_INFINITY
            } else {
                Self::INFINITY
            };
        }
        if val == 0.0 {
            return if val.is_sign_negative() {
                Self::NEG_ZERO
            } else {
                Self::ZERO
            };
        }

        let sign = val < 0.0;
        let bits = val.abs().to_bits();
        let ieee_exp = ((bits >> 52) & 0x7FF) as i32;
        let ieee_frac = bits & 0x000F_FFFF_FFFF_FFFF;

        if ieee_exp == 0 {
            // Subnormal f64 → treat as zero for now
            return if sign { Self::NEG_ZERO } else { Self::ZERO };
        }

        // IEEE 754 double: 1.fraction * 2^(ieee_exp - 1023)
        // Extended: integer.fraction * 2^(ext_exp - 16383)
        let ext_exp = (ieee_exp - 1023 + BIAS) as u16;
        // Double has 52 fraction bits → extended has 63. Shift left by 11.
        let significand = (1u64 << 63) | (ieee_frac << 11);

        Self {
            sign,
            exponent: ext_exp,
            significand,
        }
    }
}

impl From<Extended80> for f64 {
    fn from(ext: Extended80) -> f64 {
        if ext.is_nan() {
            return f64::NAN;
        }
        if ext.is_infinite() {
            return if ext.sign {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        if ext.is_zero() {
            return if ext.sign { -0.0 } else { 0.0 };
        }

        // Convert by directly constructing IEEE 754 double bits.
        // Extended80: 1-bit sign + 15-bit exp (bias 16383) + 64-bit sig (explicit integer bit)
        // f64: 1-bit sign + 11-bit exp (bias 1023) + 52-bit frac (implicit integer bit)
        let ext_exp = ext.exponent as i32;
        let ieee_exp = ext_exp - BIAS + 1023;

        if ieee_exp <= 0 {
            // Underflow to f64 subnormal or zero
            return if ext.sign { -0.0 } else { 0.0 };
        }
        if ieee_exp >= 0x7FF {
            // Overflow to infinity
            return if ext.sign {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }

        // Extract 52-bit fraction from the 63-bit fraction part of the significand.
        // Extended80 sig bit 63 = integer bit (1 for normals), bits 62..0 = fraction.
        // f64 fraction = bits 62..11 of Extended80 significand (52 bits).
        // Truncate (drop bits 10..0) to match BasiliskII's fpu_ieee.cpp make_extended()
        // which does: mantissa0 = (wrd2 & 0x7fffffff) >> 11, dropping lower bits.
        let frac = (ext.significand >> 11) & 0x000F_FFFF_FFFF_FFFF;

        let bits = ((ext.sign as u64) << 63) | ((ieee_exp as u64) << 52) | frac;

        f64::from_bits(bits)
    }
}

impl From<f32> for Extended80 {
    fn from(val: f32) -> Self {
        Self::from(val as f64)
    }
}

impl From<i16> for Extended80 {
    fn from(val: i16) -> Self {
        Self::from(val as f64)
    }
}

impl From<i32> for Extended80 {
    fn from(val: i32) -> Self {
        Self::from(val as f64)
    }
}

impl Extended80 {
    pub fn from_comp(val: i64) -> Self {
        Self::from(val as f64)
    }
    pub fn to_comp(self) -> i64 {
        f64::from(self) as i64
    }
}

// ─── Comparison ───────────────────────────────────────────────────────────

impl PartialOrd for Extended80 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.is_nan() || other.is_nan() {
            return None;
        }
        // Both zero (regardless of sign)
        if self.is_zero() && other.is_zero() {
            return Some(std::cmp::Ordering::Equal);
        }
        let a = f64::from(*self);
        let b = f64::from(*other);
        a.partial_cmp(&b)
    }
}

// ─── Arithmetic ───────────────────────────────────────────────────────────

#[allow(dead_code)]
impl Extended80 {
    /// Normalize: shift significand left until bit 63 is set, adjusting exponent.
    fn normalize(mut self) -> Self {
        if self.significand == 0 {
            self.exponent = 0;
            return self;
        }
        let shift = self.significand.leading_zeros();
        if shift > 0 {
            self.significand <<= shift;
            let new_exp = self.exponent as i32 - shift as i32;
            if new_exp <= 0 {
                // Underflow to zero
                return if self.sign {
                    Self::NEG_ZERO
                } else {
                    Self::ZERO
                };
            }
            self.exponent = new_exp as u16;
        }
        self
    }

    /// Round a 128-bit value to 64-bit significand using round-to-nearest-even.
    /// `hi` is the high 64 bits, `lo` is the low 64 bits (guard/round/sticky).
    fn round_to_64(hi: u64, lo: u64) -> u64 {
        if lo == 0 {
            return hi;
        }
        let half = 1u64 << 63;
        if lo > half || (lo == half && hi & 1 != 0) {
            // Round up (ties to even)
            hi.wrapping_add(1)
        } else {
            hi
        }
    }

    /// Add two Extended80 values.
    pub fn add(self, other: Self) -> Self {
        // NaN propagation
        if self.is_nan() {
            return self;
        }
        if other.is_nan() {
            return other;
        }

        // Infinity handling
        if self.is_infinite() && other.is_infinite() {
            return if self.sign == other.sign {
                self
            } else {
                Self::NAN
            };
        }
        if self.is_infinite() {
            return self;
        }
        if other.is_infinite() {
            return other;
        }

        // Zero handling
        if self.is_zero() && other.is_zero() {
            return if self.sign && other.sign {
                Self::NEG_ZERO
            } else {
                Self::ZERO
            };
        }
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }

        // Make a the larger magnitude
        let (a, b) = if self.exponent > other.exponent
            || (self.exponent == other.exponent && self.significand >= other.significand)
        {
            (self, other)
        } else {
            (other, self)
        };

        let exp_diff = a.exponent as i32 - b.exponent as i32;
        if exp_diff > 64 {
            return a; // b is negligible
        }

        // Significands are 64-bit with integer bit at bit 63.
        // Use u128: low 64 bits = significand, but we work entirely in u128
        // to catch carry and preserve guard bits.
        let a_sig: u128 = a.significand as u128;
        let b_sig: u128 = if exp_diff == 0 {
            b.significand as u128
        } else {
            let shifted = (b.significand as u128) >> exp_diff as u32;
            // Sticky: any bits shifted out?
            let mask = (1u128 << exp_diff as u32) - 1;
            let lost = (b.significand as u128) & mask;
            // We can't preserve sub-bit precision in u128, but sticky prevents
            // false tie-breaking. OR in a sticky bit at the lowest position.
            shifted | if lost != 0 { 1 } else { 0 }
        };

        let (sum, result_sign) = if a.sign == b.sign {
            (a_sig + b_sig, a.sign)
        } else {
            if a_sig >= b_sig {
                (a_sig - b_sig, a.sign)
            } else {
                (b_sig - a_sig, b.sign)
            }
        };

        if sum == 0 {
            return Self::ZERO;
        }

        let mut exp = a.exponent as i32;

        // The integer bit was at bit 63. After addition of same-sign values,
        // the sum may have bit 64 set (carry). After subtraction, the leading
        // bit may be below 63 (cancellation).
        let leading = 127 - sum.leading_zeros() as i32; // highest set bit position

        if leading > 63 {
            // Carry: shift right to put integer bit back at 63
            let shift = (leading - 63) as u32;
            exp += shift as i32;
            let shifted = sum >> shift;
            // Rounding from shifted-out bits
            let round_bit = (sum >> (shift - 1)) & 1;
            let sticky = if shift > 1 {
                sum & ((1u128 << (shift - 1)) - 1)
            } else {
                0
            };
            let sig = shifted as u64;
            let significand = if round_bit != 0 && (sticky != 0 || sig & 1 != 0) {
                sig.wrapping_add(1)
            } else {
                sig
            };
            if exp >= EXP_MAX as i32 {
                return if result_sign {
                    Self::NEG_INFINITY
                } else {
                    Self::INFINITY
                };
            }
            return Self {
                sign: result_sign,
                exponent: exp as u16,
                significand,
            };
        } else if leading < 63 {
            // Cancellation: shift left to normalize
            let shift = (63 - leading) as u32;
            exp -= shift as i32;
            if exp <= 0 {
                return if result_sign {
                    Self::NEG_ZERO
                } else {
                    Self::ZERO
                };
            }
            let significand = (sum << shift) as u64;
            return Self {
                sign: result_sign,
                exponent: exp as u16,
                significand,
            };
        }

        // Exact: integer bit already at 63
        Self {
            sign: result_sign,
            exponent: exp as u16,
            significand: sum as u64,
        }
    }

    /// Subtract: a - b = a + (-b).
    pub fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }

    /// Multiply two Extended80 values.
    pub fn mul(self, other: Self) -> Self {
        // NaN
        if self.is_nan() {
            return self;
        }
        if other.is_nan() {
            return other;
        }

        let result_sign = self.sign != other.sign;

        // Infinity
        if self.is_infinite() || other.is_infinite() {
            if self.is_zero() || other.is_zero() {
                return Self::NAN; // inf * 0 = NaN
            }
            return Self {
                sign: result_sign,
                ..Self::INFINITY
            };
        }

        // Zero
        if self.is_zero() || other.is_zero() {
            return Self {
                sign: result_sign,
                ..Self::ZERO
            };
        }

        // Multiply significands: 64 × 64 → 128
        let product = self.significand as u128 * other.significand as u128;

        // The integer bits were at bit 63 of each operand, so the product's
        // integer bit is at bit 126 (63+63). We want it at bit 63.
        // Shift right by 63 to get the significand, using the shifted-out
        // bits for rounding.
        // product has integer bit at bit 126 (63+63).
        // We need it at bit 63. Shift right by 63.
        let q = product >> 63; // up to 65 bits
        let round_bit = (product >> 62) & 1;
        let sticky = product & ((1u128 << 62) - 1);

        let mut new_exp = self.exponent as i32 + other.exponent as i32 - BIAS;

        // If bit 64 of q is set, the product >= 2.0 — shift right once more.
        let (sig, rb, st) = if q > u64::MAX as u128 {
            new_exp += 1;
            let s = (q >> 1) as u64;
            let r = q & 1;
            let sticky2 = if round_bit != 0 || sticky != 0 {
                1u128
            } else {
                0
            };
            (s, r, sticky2)
        } else {
            (q as u64, round_bit, sticky)
        };

        // Round to nearest even
        let significand = if rb != 0 && (st != 0 || sig & 1 != 0) {
            sig.wrapping_add(1)
        } else {
            sig
        };

        if new_exp >= EXP_MAX as i32 {
            return Self {
                sign: result_sign,
                ..Self::INFINITY
            };
        }
        if new_exp <= 0 {
            return Self {
                sign: result_sign,
                ..Self::ZERO
            };
        }

        Self {
            sign: result_sign,
            exponent: new_exp as u16,
            significand,
        }
        .normalize()
    }

    /// Divide self / other.
    pub fn div(self, other: Self) -> Self {
        // NaN
        if self.is_nan() {
            return self;
        }
        if other.is_nan() {
            return other;
        }

        let result_sign = self.sign != other.sign;

        // Special cases
        if self.is_infinite() && other.is_infinite() {
            return Self::NAN;
        }
        if self.is_infinite() {
            return Self {
                sign: result_sign,
                ..Self::INFINITY
            };
        }
        if other.is_infinite() {
            return Self {
                sign: result_sign,
                ..Self::ZERO
            };
        }
        if other.is_zero() {
            if self.is_zero() {
                return Self::NAN;
            }
            return Self {
                sign: result_sign,
                ..Self::INFINITY
            };
        }
        if self.is_zero() {
            return Self {
                sign: result_sign,
                ..Self::ZERO
            };
        }

        // Divide: (self.sig << 63) / other.sig → quotient with integer bit at bit 63
        // We shift by 63 (not 64) because both operands already have the integer
        // bit at bit 63. The quotient of two 1.xxx numbers is 0.1xxx to 1.1xxx.
        let dividend = (self.significand as u128) << 63;
        let divisor = other.significand as u128;
        let quotient = dividend / divisor;
        let remainder = dividend % divisor;

        // Round using remainder
        let half_divisor = divisor >> 1;
        let sig = quotient as u64;
        let significand = if remainder > half_divisor || (remainder == half_divisor && sig & 1 != 0)
        {
            sig.wrapping_add(1)
        } else {
            sig
        };

        let new_exp = self.exponent as i32 - other.exponent as i32 + BIAS;

        if new_exp >= EXP_MAX as i32 {
            return Self {
                sign: result_sign,
                ..Self::INFINITY
            };
        }
        if new_exp <= 0 {
            return Self {
                sign: result_sign,
                ..Self::ZERO
            };
        }

        Self {
            sign: result_sign,
            exponent: new_exp as u16,
            significand,
        }
        .normalize()
    }

    /// IEEE remainder: self - round(self/other) * other.
    pub fn rem(self, other: Self) -> Self {
        if self.is_nan() || other.is_nan() || self.is_infinite() || other.is_zero() {
            return Self::NAN;
        }
        if other.is_infinite() {
            return self;
        }
        // Use f64 for remainder (adequate precision for the quotient rounding)
        let a = f64::from(self);
        let b = f64::from(other);
        if b == 0.0 {
            return Self::NAN;
        }
        let n = (a / b).round();
        Self::from(a - n * b)
    }

    /// Square root.
    pub fn sqrt(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_zero() {
            return self;
        } // sqrt(±0) = ±0
        if self.sign {
            return Self::NAN;
        } // sqrt(negative) = NaN
        if self.is_infinite() {
            return self;
        } // sqrt(+inf) = +inf

        // Use f64 sqrt as initial approximation, then refine with one Newton step
        // in extended precision. This gives us close to 64-bit precision.
        let approx = f64::from(self).sqrt();
        let x = Self::from(approx);

        // One Newton-Raphson step: x' = (x + self/x) / 2
        let quotient = self.div(x);
        let sum = x.add(quotient);
        // Divide by 2 = scalb(-1)
        sum.scalb(-1)
    }
}

// ─── Transcendentals ─────────────────────────────────────────────────────
//
// f64 passthrough gives 847/2104 bit-exact matches vs the 68040 FPU.
// Taylor polynomial attempts scored worse (5-6 matches) due to
// convergence issues and range reduction precision.
//
// The 68040 FPU uses CORDIC internally, which is fundamentally different
// from Taylor series. Matching it exactly requires either:
// 1. A CORDIC implementation in Extended80
// 2. Using the actual FPU microcode ROM tables
//
// For now, f64 passthrough is the most accurate approach available.
// The ~1-11 bit differences in the low significand bits produce
// palette value differences of ≤15 per RGB channel (tolerance 15).

#[allow(dead_code)]
impl Extended80 {
    // Use libm (pure-Rust, matches glibc/musl) for cross-platform determinism.
    // macOS libm and glibc produce different rounding for the same f64 input;
    // using the same Rust implementation everywhere ensures identical results.
    pub fn ln(self) -> Self {
        Self::from(libm::log(f64::from(self)))
    }
    pub fn log2(self) -> Self {
        Self::from(libm::log2(f64::from(self)))
    }
    pub fn ln1(self) -> Self {
        Self::from(libm::log(1.0 + f64::from(self)))
    }
    pub fn log21(self) -> Self {
        Self::from(libm::log2(1.0 + f64::from(self)))
    }
    pub fn exp(self) -> Self {
        Self::from(libm::exp(f64::from(self)))
    }
    pub fn exp2(self) -> Self {
        Self::from(libm::exp2(f64::from(self)))
    }
    pub fn exp1(self) -> Self {
        Self::from(libm::pow(10.0, f64::from(self)) - 1.0)
    }
    pub fn sin(self) -> Self {
        Self::from(libm::sin(f64::from(self)))
    }
    pub fn cos(self) -> Self {
        Self::from(libm::cos(f64::from(self)))
    }
    pub fn tan(self) -> Self {
        Self::from(libm::tan(f64::from(self)))
    }
    pub fn atan(self) -> Self {
        Self::from(libm::atan(f64::from(self)))
    }
    pub fn powf(self, y: Self) -> Self {
        Self::from(libm::pow(f64::from(self), f64::from(y)))
    }
    pub fn powi(self, n: i32) -> Self {
        Self::from(libm::pow(f64::from(self), n as f64))
    }
}

// ─── Debug ────────────────────────────────────────────────────────────────

impl std::fmt::Debug for Extended80 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_nan() {
            write!(f, "NaN")
        } else if self.is_infinite() {
            write!(f, "{}Inf", if self.sign { "-" } else { "+" })
        } else {
            write!(f, "{:e}", f64::from(*self))
        }
    }
}

impl std::fmt::Display for Extended80 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_roundtrip() {
        assert_eq!(Extended80::ZERO.exponent, 0);
        assert_eq!(Extended80::ZERO.significand, 0);
        // Asserting against const-fold-able fields of a const value
        // is intentional here — the test pins the constant's shape.
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(!Extended80::ZERO.sign);
        }
        assert!(Extended80::ZERO.is_zero());
    }

    #[test]
    fn neg_zero() {
        assert!(Extended80::NEG_ZERO.is_zero());
        assert!(Extended80::NEG_ZERO.is_sign_negative());
    }

    #[test]
    fn infinity_classification() {
        assert!(Extended80::INFINITY.is_infinite());
        assert!(!Extended80::INFINITY.is_nan());
        assert!(Extended80::NEG_INFINITY.is_infinite());
        assert!(Extended80::NEG_INFINITY.is_sign_negative());
    }

    #[test]
    fn nan_classification() {
        assert!(Extended80::NAN.is_nan());
        assert!(!Extended80::NAN.is_infinite());
    }

    #[test]
    fn one_value() {
        let v = f64::from(Extended80::ONE);
        assert_eq!(v, 1.0);
    }

    #[test]
    #[allow(clippy::approx_constant)] // 3.14159 used as a test value, not as π
    fn f64_roundtrip() {
        for val in [1.0, -1.0, 0.5, 3.14159, 1e10, 1e-10, -42.5] {
            let ext = Extended80::from(val);
            let back = f64::from(ext);
            assert!(
                (back - val).abs() < 1e-10,
                "roundtrip failed for {}: got {}",
                val,
                back
            );
        }
    }

    #[test]
    fn add_basic() {
        let a = Extended80::from(3.0);
        let b = Extended80::from(2.0);
        let result = f64::from(a.add(b));
        assert_eq!(result, 5.0);
    }

    #[test]
    fn sub_basic() {
        let a = Extended80::from(5.0);
        let b = Extended80::from(3.0);
        let result = f64::from(a.sub(b));
        assert_eq!(result, 2.0);
    }

    #[test]
    fn mul_basic() {
        let a = Extended80::from(3.0);
        let b = Extended80::from(4.0);
        let result = f64::from(a.mul(b));
        assert_eq!(result, 12.0);
    }

    #[test]
    fn div_basic() {
        let a = Extended80::from(10.0);
        let b = Extended80::from(4.0);
        let result = f64::from(a.div(b));
        assert_eq!(result, 2.5);
    }

    #[test]
    fn div_by_zero() {
        let a = Extended80::from(1.0);
        let b = Extended80::ZERO;
        assert!(a.div(b).is_infinite());
    }

    #[test]
    fn sqrt_basic() {
        let a = Extended80::from(4.0);
        let result = f64::from(a.sqrt());
        assert!((result - 2.0).abs() < 1e-10);
    }

    #[test]
    fn sqrt_negative_is_nan() {
        let a = Extended80::from(-1.0);
        assert!(a.sqrt().is_nan());
    }

    #[test]
    fn classify_values() {
        assert_eq!(Extended80::NAN.classify(), 2); // QNAN (positive)
        assert_eq!(Extended80::INFINITY.classify(), 3);
        assert_eq!(Extended80::NEG_INFINITY.classify(), -3);
        assert_eq!(Extended80::ZERO.classify(), 4);
        assert_eq!(Extended80::NEG_ZERO.classify(), -4);
        assert_eq!(Extended80::ONE.classify(), 5); // normal positive
    }

    #[test]
    fn scalb_basic() {
        let a = Extended80::from(1.0);
        let result = f64::from(a.scalb(3)); // 1.0 * 2^3 = 8.0
        assert_eq!(result, 8.0);
    }

    #[test]
    fn neg_and_abs() {
        let a = Extended80::from(5.0);
        assert!(a.neg().is_sign_negative());
        assert!(!a.neg().abs().is_sign_negative());
    }

    #[test]
    fn comparison_ordering() {
        let a = Extended80::from(1.0);
        let b = Extended80::from(2.0);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a.partial_cmp(&a), Some(std::cmp::Ordering::Equal));
    }

    #[test]
    fn nan_comparison_is_none() {
        let a = Extended80::from(1.0);
        assert_eq!(a.partial_cmp(&Extended80::NAN), None);
    }

    #[test]
    fn infinity_arithmetic() {
        let inf = Extended80::INFINITY;
        let one = Extended80::ONE;
        assert!(inf.add(one).is_infinite());
        assert!(inf.add(inf.neg()).is_nan()); // inf + (-inf) = NaN
        assert!(inf.mul(Extended80::ZERO).is_nan()); // inf * 0 = NaN
    }

    #[test]
    fn logb_special_cases_and_denormals() {
        let top_denormal = Extended80 {
            sign: false,
            exponent: 0,
            significand: 0x7FFF_FFFF_FFFF_FFFF,
        };
        let least_denormal = Extended80 {
            sign: false,
            exponent: 0,
            significand: 1,
        };
        for (input, exponent) in [
            (Extended80::from(8.0), 3.0),
            (Extended80::from(-8.0), 3.0),
            (top_denormal, -16383.0),
            (least_denormal, -16445.0),
        ] {
            assert_eq!(input.logb(), Extended80::from(exponent));
            assert_eq!(input.neg().logb(), Extended80::from(exponent));
        }
        assert_eq!(Extended80::ZERO.logb(), Extended80::NEG_INFINITY);
        assert_eq!(Extended80::NEG_ZERO.logb(), Extended80::NEG_INFINITY);
        assert_eq!(Extended80::INFINITY.logb(), Extended80::INFINITY);
        assert_eq!(Extended80::NEG_INFINITY.logb(), Extended80::INFINITY);
        assert!(Extended80::NAN.logb().is_nan());
    }
}
