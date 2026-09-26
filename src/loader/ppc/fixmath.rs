//! PowerPC Fixed-point and Wide integer math operations.

use super::*;

pub(crate) fn ppc_fix_ratio(numerator: i16, denominator: i16) -> i32 {
    // Inside Macintosh Volume I (1985), p. I-467: FixRatio returns the
    // truncated signed 16.16 quotient and uses asymmetric saturation when the
    // denominator is zero.
    if denominator == 0 {
        return if numerator < 0 {
            i32::MIN + 1
        } else {
            i32::MAX
        };
    }
    ((i64::from(numerator) << 16) / i64::from(denominator)) as i32
}

pub(crate) fn ppc_fix_mul(left: i32, right: i32) -> i32 {
    // Inside Macintosh Volume I (1985), p. I-467: FixMul rounds its signed
    // 32.32 intermediate to the nearest representable 16.16 value.
    ((i64::from(left) * i64::from(right) + 0x8000) >> 16) as i32
}

pub(crate) fn ppc_fix_div(numerator: i32, denominator: i32) -> i32 {
    // Operating System Utilities (1994), pp. 3-39--3-40: FixDiv divides two
    // signed 16.16 values and returns a signed 16.16 quotient. Saturate when
    // the quotient is not representable, including division by zero.
    if denominator == 0 {
        return if numerator < 0 { i32::MIN } else { i32::MAX };
    }
    ((i64::from(numerator) << 16) / i64::from(denominator))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn ppc_long_to_fix(value: i32) -> i32 {
    // Operating System Utilities (1994), p. 3-43: inputs outside the signed
    // 16-bit integer range saturate to the extrema of Fixed.
    if value > 0x7fff {
        i32::MAX
    } else if value < -0x8000 {
        i32::MIN
    } else {
        value << 16
    }
}

pub(crate) fn ppc_fix_to_long(value: i32) -> i32 {
    // Operating System Utilities (1994), p. 3-44: round to the nearest
    // integer, with exact halves rounded away from zero.
    let value = i64::from(value);
    if value >= 0 {
        ((value + 0x8000) >> 16) as i32
    } else {
        -(((-value + 0x8000) >> 16) as i32)
    }
}

pub(crate) fn ppc_fix_round(value: i32) -> i16 {
    // Inside Macintosh Volume I (1985), p. I-467: round to nearest integer,
    // with exact halves rounded away from zero.
    let value = i64::from(value);
    let magnitude = (value.abs() + 0x8000) >> 16;
    let rounded = if value < 0 { -magnitude } else { magnitude };
    rounded as i16
}

const PPC_FIXMATH_PI_FIXED: f64 = 205_888.0;

fn ppc_fixed_radians(value: i32) -> f64 {
    // Inside Macintosh Volume IV (1986), p. IV-64: FracSin and FracCos use
    // P = 3.1416015625 (Fixed $00032440) rather than IEEE pi for reduction.
    f64::from(value) * std::f64::consts::PI / PPC_FIXMATH_PI_FIXED
}

fn ppc_radians_to_fixed(value: f64) -> i32 {
    // Inside Macintosh Volume IV (1986), p. IV-65: FixATan2 uses the same
    // $0000C910 approximation to pi/4, hence $00032440 for pi.
    (value * PPC_FIXMATH_PI_FIXED / std::f64::consts::PI)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub(crate) fn ppc_fix_to_frac(value: i32) -> i32 {
    // Operating System Utilities (1994), p. 3-44: shift left 14 bits with saturation.
    (i64::from(value) << 14).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn ppc_frac_to_fix(value: i32) -> i32 {
    // Operating System Utilities (1994), p. 3-44: shift right 14 bits with nearest rounding.
    ((i64::from(value) + (1 << 13)) >> 14).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn ppc_f64_to_frac(value: f64) -> u32 {
    // Operating System Utilities (1994), p. 3-46: convert float to Fract with saturation.
    (value * 1_073_741_824.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32 as u32
}

pub(crate) fn ppc_frac_sin(value: i32) -> i32 {
    // Inside Macintosh Volume IV (1986), p. IV-64: sine of Fixed radians returned as Fract.
    let sin_val = ppc_fixed_radians(value).sin();
    (sin_val * 1_073_741_824.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub(crate) fn ppc_frac_cos(value: i32) -> i32 {
    // Inside Macintosh Volume IV (1986), p. IV-64: cosine of Fixed radians returned as Fract.
    let cos_val = ppc_fixed_radians(value).cos();
    (cos_val * 1_073_741_824.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub(crate) fn ppc_frac_sqrt(value: u32) -> u32 {
    // Operating System Utilities (1994), p. 3-41: unsigned Fract 0..4-2^-30 square root.
    let val = (value as f64) / 1_073_741_824.0;
    let sqrt_val = val.sqrt();
    (sqrt_val * 1_073_741_824.0)
        .round()
        .clamp(0.0, 2_147_483_648.0) as u32
}

pub(crate) fn ppc_frac_mul(x: i32, y: i32) -> i32 {
    // Inside Macintosh Volume IV (1986), p. IV-63: add half a unit in
    // magnitude, then chop toward zero.
    let product = i64::from(x) * i64::from(y);
    let magnitude = (product.abs() + (1i64 << 29)) >> 30;
    let rounded = if product < 0 { -magnitude } else { magnitude };
    rounded.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn ppc_frac_div(numerator: i32, denominator: i32) -> i32 {
    // Inside Macintosh Volume I (1985), p. I-468: signed 2.30 quotient, saturating on divide-by-zero.
    if denominator == 0 {
        return if numerator >= 0 { i32::MAX } else { i32::MIN };
    }
    ((i64::from(numerator) << 30) / i64::from(denominator))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

pub(crate) fn ppc_fix_atan2(x: i32, y: i32) -> i32 {
    // Inside Macintosh Volume IV (1986), p. IV-65: arctangent of y/x in radians.
    ppc_radians_to_fixed(f64::from(y).atan2(f64::from(x)))
}

pub(crate) fn ppc_read_wide(memory: &mut PpcSectionMem, address: u32) -> Option<i64> {
    let high = memory.read_u32_be(address)? as i32;
    let low = memory.read_u32_be(address.checked_add(4)?)?;
    Some((i64::from(high) << 32) | i64::from(low))
}

pub(crate) fn ppc_write_wide(memory: &mut PpcSectionMem, address: u32, value: i64) -> Option<()> {
    memory.write_u32_be(address, (value >> 32) as u32)?;
    memory.write_u32_be(address.checked_add(4)?, value as u32)?;
    Some(())
}

pub(crate) fn ppc_wide_add(memory: &mut PpcSectionMem, target: u32, source: u32) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-49: add source to
    // target in place and return the target pointer.
    if let (Some(target_value), Some(source_value)) =
        (ppc_read_wide(memory, target), ppc_read_wide(memory, source))
    {
        let _ = ppc_write_wide(memory, target, target_value.wrapping_add(source_value));
    }
    target
}

pub(crate) fn ppc_wide_subtract(memory: &mut PpcSectionMem, target: u32, source: u32) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-50: subtract source
    // from target in place and return the target pointer.
    if let (Some(target_value), Some(source_value)) =
        (ppc_read_wide(memory, target), ppc_read_wide(memory, source))
    {
        let _ = ppc_write_wide(memory, target, target_value.wrapping_sub(source_value));
    }
    target
}

pub(crate) fn ppc_wide_negate(memory: &mut PpcSectionMem, target: u32) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-50: replace target
    // with its two's-complement negative and return the target pointer.
    if let Some(value) = ppc_read_wide(memory, target) {
        let _ = ppc_write_wide(memory, target, value.wrapping_neg());
    }
    target
}

fn ppc_round_wide_quotient(dividend: i128, divisor: i128) -> i128 {
    let quotient = dividend / divisor;
    let remainder = dividend % divisor;
    let doubled_remainder = remainder.abs() * 2;
    let divisor_magnitude = divisor.abs();
    if doubled_remainder > divisor_magnitude
        || (doubled_remainder == divisor_magnitude && (dividend < 0) == (divisor < 0))
    {
        quotient
            + if (dividend < 0) == (divisor < 0) {
                1
            } else {
                -1
            }
    } else {
        quotient
    }
}

pub(crate) fn ppc_wide_shift(memory: &mut PpcSectionMem, target: u32, shift: i32) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-51: positive shifts
    // move right with rounding; negative shifts move left.
    if let Some(value) = ppc_read_wide(memory, target) {
        let shifted = if shift > 0 {
            if shift >= 64 {
                0
            } else {
                ppc_round_wide_quotient(i128::from(value), 1i128 << shift) as i64
            }
        } else if shift < 0 {
            let magnitude = shift.unsigned_abs();
            if magnitude >= 64 {
                0
            } else {
                value.wrapping_shl(magnitude)
            }
        } else {
            value
        };
        let _ = ppc_write_wide(memory, target, shifted);
    }
    target
}

pub(crate) fn ppc_wide_bit_shift(memory: &mut PpcSectionMem, target: u32, shift: i32) -> u32 {
    // FixMath.h: WideBitShift(wide *target, SInt32 shift) updates target and
    // returns its pointer. Positive counts shift right without WideShift's
    // rounding; negative counts shift left. CarbonCore masks counts to six
    // bits (observed with native WideBitShift for shifts 64 and 65).
    if let Some(value) = ppc_read_wide(memory, target) {
        let count = shift.unsigned_abs() & 63;
        let shifted = if shift >= 0 {
            value >> count
        } else {
            value.wrapping_shl(count)
        };
        let _ = ppc_write_wide(memory, target, shifted);
    }
    target
}

pub(crate) fn ppc_wide_multiply(
    memory: &mut PpcSectionMem,
    multiplicand: i32,
    multiplier: i32,
    target: u32,
) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-51: store the exact
    // signed 32-by-32-bit product in target and return the target pointer.
    let product = i64::from(multiplicand) * i64::from(multiplier);
    let _ = ppc_write_wide(memory, target, product);
    target
}

pub(crate) fn ppc_wide_divide(
    memory: &mut PpcSectionMem,
    dividend_ptr: u32,
    divisor: i32,
    remainder_ptr: u32,
) -> i32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-52: return a
    // signed-long quotient, optionally store the remainder, round when the
    // remainder pointer is nil or -1, and pin overflow to an infinity.
    let Some(dividend) = ppc_read_wide(memory, dividend_ptr) else {
        return 0;
    };
    let no_remainder = remainder_ptr == 0 || remainder_ptr == u32::MAX;
    let (quotient, remainder, quotient_is_negative) = if divisor == 0 {
        (
            if dividend < 0 { i128::MIN } else { i128::MAX },
            i32::MIN,
            dividend < 0,
        )
    } else {
        let dividend = i128::from(dividend);
        let divisor = i128::from(divisor);
        let quotient = if no_remainder {
            ppc_round_wide_quotient(dividend, divisor)
        } else {
            dividend / divisor
        };
        (
            quotient,
            (dividend % divisor) as i32,
            (dividend < 0) != (divisor < 0),
        )
    };
    let overflow = quotient < i128::from(i32::MIN) || quotient > i128::from(i32::MAX);
    if overflow {
        if remainder_ptr != 0 && remainder_ptr != u32::MAX {
            let _ = memory.write_u32_be(remainder_ptr, i32::MIN as u32);
        }
        if remainder_ptr == u32::MAX || quotient_is_negative {
            i32::MIN
        } else {
            i32::MAX
        }
    } else {
        if remainder_ptr != 0 && remainder_ptr != u32::MAX {
            let _ = memory.write_u32_be(remainder_ptr, remainder as u32);
        }
        quotient as i32
    }
}

pub(crate) fn ppc_wide_wide_divide(
    memory: &mut PpcSectionMem,
    dividend_ptr: u32,
    divisor: i32,
    remainder_ptr: u32,
) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-52: replace the
    // dividend with its wide quotient, optionally store the remainder, and
    // round when the remainder pointer is nil or -1.
    let Some(dividend) = ppc_read_wide(memory, dividend_ptr) else {
        return dividend_ptr;
    };
    let no_remainder = remainder_ptr == 0 || remainder_ptr == u32::MAX;
    let (quotient, remainder) = if divisor == 0 {
        (if dividend < 0 { i64::MIN } else { i64::MAX }, i32::MIN)
    } else {
        let dividend = i128::from(dividend);
        let divisor = i128::from(divisor);
        let quotient = if no_remainder {
            ppc_round_wide_quotient(dividend, divisor)
        } else {
            dividend / divisor
        };
        (quotient as i64, (dividend % divisor) as i32)
    };
    let _ = ppc_write_wide(memory, dividend_ptr, quotient);
    if remainder_ptr != 0 && remainder_ptr != u32::MAX {
        let _ = memory.write_u32_be(remainder_ptr, remainder as u32);
    }
    dividend_ptr
}

pub(crate) fn ppc_wide_square_root(memory: &mut PpcSectionMem, source: u32) -> u32 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-53: interpret the
    // complete source as an unsigned wide value and return its square root.
    let Some(high) = memory.read_u32_be(source) else {
        return 0;
    };
    let Some(low) = source
        .checked_add(4)
        .and_then(|address| memory.read_u32_be(address))
    else {
        return 0;
    };
    ((u64::from(high) << 32) | u64::from(low)).isqrt() as u32
}

pub(crate) fn ppc_wide_compare(memory: &mut PpcSectionMem, target: u32, source: u32) -> i16 {
    // QuickDraw GX Environment and Utilities (1994), p. 8-54: return 1, -1,
    // or 0 according to the ordering of the two wide values.
    match (ppc_read_wide(memory, target), ppc_read_wide(memory, source)) {
        (Some(target), Some(source)) => match target.cmp(&source) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
        _ => 0,
    }
}
