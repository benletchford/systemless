use super::*;

pub(super) fn ppc_decimal_read(memory: &mut PpcSectionMem, decimal: u32) -> Option<f64> {
    let negative = memory.read_u8(decimal)? != 0;
    let exponent = memory.read_u16_be(decimal + 2)? as i16;
    let length = usize::from(memory.read_u8(decimal + 4)?).min(36);
    let digits = (0..length)
        .map(|offset| memory.read_u8(decimal + 5 + offset as u32))
        .collect::<Option<Vec<_>>>()?;
    let digits = std::str::from_utf8(&digits).ok()?;
    let mut value = match digits {
        "NAN" => f64::NAN,
        "INF" => f64::INFINITY,
        _ => digits.parse::<f64>().ok()? * 10f64.powi(i32::from(exponent)),
    };
    if negative {
        value = -value;
    }
    Some(value)
}

pub(super) fn ppc_decimal_write(
    memory: &mut PpcSectionMem,
    decimal: u32,
    value: f64,
    digits: usize,
) -> bool {
    if decimal == 0 || !ppc_memory_can_write_bytes(memory, decimal, 42) {
        return false;
    }
    let negative = value.is_sign_negative();
    let (significand, exponent) = if value.is_nan() {
        (b"NAN".to_vec(), 0i16)
    } else if value.is_infinite() {
        (b"INF".to_vec(), 0)
    } else if value == 0.0 {
        (b"0".to_vec(), 0)
    } else {
        let digits = digits.clamp(1, 36);
        let scientific = format!("{:.*e}", digits - 1, value.abs());
        let (mantissa, exponent_text) = scientific.split_once('e').unwrap_or((&scientific, "0"));
        let significand = mantissa
            .bytes()
            .filter(|byte| *byte != b'.')
            .collect::<Vec<_>>();
        let scientific_exponent = exponent_text.parse::<i32>().unwrap_or(0);
        let decimal_exponent = scientific_exponent - (significand.len() as i32 - 1);
        (
            significand,
            decimal_exponent.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        )
    };
    let _ = ppc_zero_guest_bytes(memory, decimal, 42);
    let _ = memory.write_u8(decimal, u8::from(negative));
    let _ = memory.write_u16_be(decimal + 2, exponent as u16);
    let _ = memory.write_u8(decimal + 4, significand.len().min(36) as u8);
    memory
        .write_bytes(decimal + 5, &significand[..significand.len().min(36)])
        .is_some()
}

fn ppc_math64_read_gprs(cpu: &PpcCpu, high_register: usize) -> u64 {
    (u64::from(cpu.gpr[high_register]) << 32) | u64::from(cpu.gpr[high_register + 1])
}

fn ppc_math64_write_result(memory: &mut PpcSectionMem, result: u32, value: u64) {
    let _ = memory.write_u32_be(result, (value >> 32) as u32);
    let _ = memory.write_u32_be(result.wrapping_add(4), value as u32);
}

fn ppc_math64_signed_shift_right(value: i64, shift: u32) -> i64 {
    let shift = shift & 0x7f;
    if shift < 64 {
        value >> shift
    } else if value < 0 {
        -1
    } else {
        0
    }
}

fn ppc_math64_shift_left(value: u64, shift: u32) -> u64 {
    let shift = shift & 0x7f;
    if shift < 64 {
        value.wrapping_shl(shift)
    } else {
        0
    }
}

fn ppc_math64_shift_right(value: u64, shift: u32) -> u64 {
    let shift = shift & 0x7f;
    if shift < 64 {
        value >> shift
    } else {
        0
    }
}

fn ppc_math64_signed_to_long_double(value: i64) -> (f64, f64) {
    let head = value as f64;
    let tail = (i128::from(value) - head as i128) as f64;
    (head, tail)
}

fn ppc_math64_unsigned_to_long_double(value: u64) -> (f64, f64) {
    let head = value as f64;
    let tail = (i128::from(value) - head as i128) as f64;
    (head, tail)
}

fn ppc_math64_long_double_to_i128(head: f64, tail: f64) -> Option<i128> {
    if !head.is_finite() || !tail.is_finite() || head.abs() > 2f64.powi(65) {
        return None;
    }
    let head_integer = head.trunc();
    let tail_integer = tail.trunc();
    let fraction = (head - head_integer) + (tail - tail_integer);
    (head_integer as i128)
        .checked_add(tail_integer as i128)?
        .checked_add(fraction.trunc() as i128)
}

fn ppc_math64_long_double_to_signed(head: f64, tail: f64) -> i64 {
    ppc_math64_long_double_to_i128(head, tail)
        .map(|value| value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
        .unwrap_or_else(|| {
            if head.is_nan() || tail.is_nan() {
                0
            } else if head.is_sign_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        })
}

fn ppc_math64_long_double_to_unsigned(head: f64, tail: f64) -> u64 {
    ppc_math64_long_double_to_i128(head, tail)
        .map(|value| value.clamp(0, i128::from(u64::MAX)) as u64)
        .unwrap_or_else(|| {
            if head.is_nan() || tail.is_nan() || head.is_sign_negative() {
                0
            } else {
                u64::MAX
            }
        })
}

pub(super) fn ppc_dispatch_math64(
    operation: PpcMath64Operation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    // Universal Interfaces 3.4 Math64.h specifies the arithmetic, Boolean,
    // shift, divide-by-zero, and conversion behavior used here. The PowerPC
    // aggregate ABI passes an 8-byte result address in r3 and starts its
    // explicit arguments in r4; scalar results use r3 directly.
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-41--1-50.
    let left = ppc_math64_read_gprs(cpu, 4);
    let right = ppc_math64_read_gprs(cpu, 6);
    let unary = left;
    let value = match operation {
        PpcMath64Operation::S64Max => i64::MAX as u64,
        PpcMath64Operation::S64Min => i64::MIN as u64,
        PpcMath64Operation::S64Add => left.wrapping_add(right),
        PpcMath64Operation::S64Subtract => left.wrapping_sub(right),
        PpcMath64Operation::S64Negate => (left as i64).wrapping_neg() as u64,
        PpcMath64Operation::S64Absolute => (left as i64).wrapping_abs() as u64,
        PpcMath64Operation::S64Multiply => (left as i64).wrapping_mul(right as i64) as u64,
        PpcMath64Operation::S64BitwiseAnd | PpcMath64Operation::U64BitwiseAnd => left & right,
        PpcMath64Operation::S64BitwiseOr | PpcMath64Operation::U64BitwiseOr => left | right,
        PpcMath64Operation::S64BitwiseEor | PpcMath64Operation::U64BitwiseEor => left ^ right,
        PpcMath64Operation::S64BitwiseNot | PpcMath64Operation::U64BitwiseNot => !unary,
        PpcMath64Operation::S64ShiftRight => {
            ppc_math64_signed_shift_right(left as i64, cpu.gpr[6]) as u64
        }
        PpcMath64Operation::S64ShiftLeft | PpcMath64Operation::U64ShiftLeft => {
            ppc_math64_shift_left(left, cpu.gpr[6])
        }
        PpcMath64Operation::U64ShiftRight => ppc_math64_shift_right(left, cpu.gpr[6]),
        PpcMath64Operation::S64Set => i64::from(cpu.gpr[4] as i32) as u64,
        PpcMath64Operation::S64SetU | PpcMath64Operation::U64SetU => u64::from(cpu.gpr[4]),
        PpcMath64Operation::U64Set => (cpu.gpr[4] as i32 as i64) as u64,
        PpcMath64Operation::U64Max => u64::MAX,
        PpcMath64Operation::U64Add => left.wrapping_add(right),
        PpcMath64Operation::U64Subtract => left.wrapping_sub(right),
        PpcMath64Operation::U64Multiply => left.wrapping_mul(right),
        PpcMath64Operation::UInt64ToSInt64 | PpcMath64Operation::SInt64ToUInt64 => unary,
        PpcMath64Operation::S64Divide => {
            let dividend = left as i64;
            let divisor = right as i64;
            let (quotient, remainder) = if divisor == 0 {
                (if dividend < 0 { i64::MIN } else { i64::MAX }, dividend)
            } else if dividend == i64::MIN && divisor == -1 {
                (i64::MIN, 0)
            } else {
                (dividend / divisor, dividend % divisor)
            };
            if cpu.gpr[8] != 0 {
                ppc_math64_write_result(memory, cpu.gpr[8], remainder as u64);
            }
            quotient as u64
        }
        PpcMath64Operation::U64Divide => {
            let (quotient, remainder) = if right == 0 {
                (u64::MAX, left)
            } else {
                (left / right, left % right)
            };
            if cpu.gpr[8] != 0 {
                ppc_math64_write_result(memory, cpu.gpr[8], remainder);
            }
            quotient
        }
        PpcMath64Operation::LongDoubleToSInt64 => {
            ppc_math64_long_double_to_signed(f64::from_bits(cpu.fpr[1]), f64::from_bits(cpu.fpr[2]))
                as u64
        }
        PpcMath64Operation::LongDoubleToUInt64 => ppc_math64_long_double_to_unsigned(
            f64::from_bits(cpu.fpr[1]),
            f64::from_bits(cpu.fpr[2]),
        ),
        PpcMath64Operation::SInt64ToLongDouble => {
            let (head, tail) =
                ppc_math64_signed_to_long_double(ppc_math64_read_gprs(cpu, 3) as i64);
            cpu.fpr[1] = head.to_bits();
            cpu.fpr[2] = tail.to_bits();
            return PpcImportAction::ReturnPreserve;
        }
        PpcMath64Operation::UInt64ToLongDouble => {
            let (head, tail) = ppc_math64_unsigned_to_long_double(ppc_math64_read_gprs(cpu, 3));
            cpu.fpr[1] = head.to_bits();
            cpu.fpr[2] = tail.to_bits();
            return PpcImportAction::ReturnPreserve;
        }
        PpcMath64Operation::S32Set | PpcMath64Operation::U32SetU => {
            return PpcImportAction::Return(cpu.gpr[4]);
        }
        PpcMath64Operation::S64And | PpcMath64Operation::U64And => {
            return PpcImportAction::Return(u32::from(
                ppc_math64_read_gprs(cpu, 3) != 0 && ppc_math64_read_gprs(cpu, 5) != 0,
            ));
        }
        PpcMath64Operation::S64Or | PpcMath64Operation::U64Or => {
            return PpcImportAction::Return(u32::from(
                ppc_math64_read_gprs(cpu, 3) != 0 || ppc_math64_read_gprs(cpu, 5) != 0,
            ));
        }
        PpcMath64Operation::S64Eor | PpcMath64Operation::U64Eor => {
            return PpcImportAction::Return(u32::from(
                (ppc_math64_read_gprs(cpu, 3) != 0) ^ (ppc_math64_read_gprs(cpu, 5) != 0),
            ));
        }
        PpcMath64Operation::S64Not | PpcMath64Operation::U64Not => {
            return PpcImportAction::Return(u32::from(ppc_math64_read_gprs(cpu, 3) == 0));
        }
        PpcMath64Operation::S64Compare => {
            return PpcImportAction::Return(
                match (ppc_math64_read_gprs(cpu, 3) as i64)
                    .cmp(&(ppc_math64_read_gprs(cpu, 5) as i64))
                {
                    std::cmp::Ordering::Less => -1i32 as u32,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                },
            );
        }
        PpcMath64Operation::U64Compare => {
            return PpcImportAction::Return(
                match ppc_math64_read_gprs(cpu, 3).cmp(&ppc_math64_read_gprs(cpu, 5)) {
                    std::cmp::Ordering::Less => -1i32 as u32,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                },
            );
        }
    };
    ppc_math64_write_result(memory, cpu.gpr[3], value);
    PpcImportAction::ReturnPreserve
}

pub(super) fn ppc_dispatch_math_compatibility(
    operation: PpcMathCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    match operation {
        PpcMathCompatibilityOperation::LdToX80 => {
            // PowerPC Numerics (1994), Appendix E: long double is a
            // double-double pair. Convert its head and tail to the 80-bit
            // interchange format used by classic Mac OS numeric APIs.
            let source = cpu.gpr[3];
            let destination = cpu.gpr[4];
            if ppc_memory_can_read_bytes(memory, source, 16)
                && ppc_memory_can_write_bytes(memory, destination, 10)
            {
                if let (Some(head), Some(tail)) = (
                    memory.read_u64_be(source),
                    memory.read_u64_be(source + 8),
                ) {
                    let extended = Extended80::from(f64::from_bits(head))
                        .add(Extended80::from(f64::from_bits(tail)));
                    let _ = memory.write_u16_be(
                        destination,
                        (u16::from(extended.sign) << 15) | extended.exponent,
                    );
                    let _ = memory.write_u64_be(destination + 2, extended.significand);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Floor => {
            // floor returns the nearest integer not greater than its argument,
            // preserving signed zero, NaN, and infinities.
            // PowerPC Numerics (1994), pp. 9-7--9-8.
            cpu.fpr[1] = f64::from_bits(cpu.fpr[1]).floor().to_bits();
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Modf => {
            let value = f64::from_bits(cpu.fpr[1]);
            let integer = if value.is_nan() { value } else { value.trunc() };
            let fractional = if value.is_infinite() {
                0.0f64.copysign(value)
            } else {
                value - integer
            };
            let pointer = cpu.gpr[5];
            let bits = integer.to_bits();
            let _ = memory.write_u32_be(pointer, (bits >> 32) as u32);
            let _ = memory.write_u32_be(pointer + 4, bits as u32);
            cpu.fpr[1] = fractional.to_bits();
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Dec2Num => {
            cpu.fpr[1] = ppc_decimal_read(memory, cpu.gpr[3])
                .unwrap_or(f64::NAN)
                .to_bits();
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Num2Dec => {
            let digits = memory
                .read_u16_be(cpu.gpr[3] + 2)
                .map(|value| value as i16)
                .unwrap_or(16)
                .unsigned_abs() as usize;
            let _ = ppc_decimal_write(memory, cpu.gpr[6], f64::from_bits(cpu.fpr[1]), digits);
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Dec2Str => {
            let style = memory.read_u8(cpu.gpr[3]).unwrap_or(0);
            let digits = memory
                .read_u16_be(cpu.gpr[3] + 2)
                .map(|value| value as i16)
                .unwrap_or(6);
            let value = ppc_decimal_read(memory, cpu.gpr[4]).unwrap_or(f64::NAN);
            let text = if style == 1 {
                format!("{:.*}", usize::from(digits.max(0) as u16).min(36), value)
            } else {
                format!(
                    " {:.*e}",
                    usize::from(digits.max(1) as u16).saturating_sub(1).min(35),
                    value
                )
            };
            let _ = memory.write_bytes(cpu.gpr[5], text.as_bytes());
            let _ = memory.write_u8(cpu.gpr[5] + text.len() as u32, 0);
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::Str2Dec => {
            let bytes = ppc_std_c_string(memory, cpu.gpr[3], 4096);
            let start = memory.read_u16_be(cpu.gpr[4]).unwrap_or(0) as usize;
            let suffix = bytes.get(start..).unwrap_or_default();
            let text = String::from_utf8_lossy(suffix);
            let token_len = text
                .bytes()
                .take_while(|byte| {
                    byte.is_ascii_digit() || matches!(*byte, b'+' | b'-' | b'.' | b'e' | b'E')
                })
                .count();
            let value = text
                .get(..token_len)
                .and_then(|text| text.parse::<f64>().ok());
            if let Some(value) = value {
                let _ = ppc_decimal_write(memory, cpu.gpr[5], value, 16);
                let _ = memory.write_u16_be(cpu.gpr[4], start.saturating_add(token_len) as u16);
                let _ = memory.write_u16_be(cpu.gpr[6], 1);
            } else {
                let _ = ppc_decimal_write(memory, cpu.gpr[5], f64::NAN, 3);
                let _ = memory.write_u16_be(cpu.gpr[6], 0);
            }
            PpcImportAction::ReturnPreserve
        }
        PpcMathCompatibilityOperation::FeClearExcept => {
            cpu.fpscr &= 0x0000_00ff;
            PpcImportAction::Return(0)
        }
        PpcMathCompatibilityOperation::FeTestExcept => PpcImportAction::Return(0),
    }
}
