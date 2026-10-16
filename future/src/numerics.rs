//! Shared software numeric services. Apple Numerics Manual, 2nd ed., ch. 24.
//! SANE slices use default round-to-nearest 64-bit extended significands.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use astro_float_num::{BigFloat, Consts, RoundingMode, Sign};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Atan,
    Sin,
    Cos,
    Tan,
}
#[derive(Default)]
pub struct Numerics {
    /// Sticky mathematical exceptions, retained for future SANE environment APIs.
    pub inexact: bool,
    pub underflow: bool,
    pub invalid: bool,
    pub overflow: bool,
    pub divide_by_zero: bool,
}
impl Numerics {
    pub fn copy_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let mut bytes: [u8; 10] = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 10)?;
        let e = u16::from_be_bytes(bytes[..2].try_into().unwrap()) & 0x7fff;
        let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
        if (e != 0 && m >> 63 == 0) || (e == 0 && m >> 63 != 0) {
            return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
        }
        if e == 0x7fff && m != 1 << 63 {
            self.invalid |= m & (1 << 62) == 0;
            bytes[2] |= 0x40;
        }
        memory.write(destination, &bytes)
    }
    pub fn integer_to_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
        bits: u8,
    ) -> Result<()> {
        let value = match bits {
            16 => i32::from(memory.u16(source)? as i16),
            32 => memory.u32(source)? as i32,
            _ => return Err(Error::Cpu("unsupported SANE integer width".into())),
        };
        memory.validate_write(destination, 10)?;
        memory.write(destination, &Self::expand_integer(value))
    }
    fn expand_integer(value: i32) -> [u8; 10] {
        let mut bytes = [0; 10];
        if value == 0 {
            return bytes;
        }
        let magnitude = value.unsigned_abs();
        let top = 31 - magnitude.leading_zeros();
        let exponent = (16383 + top) as u16 | if value < 0 { 0x8000 } else { 0 };
        bytes[..2].copy_from_slice(&exponent.to_be_bytes());
        bytes[2..].copy_from_slice(&(u64::from(magnitude) << (63 - top)).to_be_bytes());
        bytes
    }
    pub fn double_to_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let input = u64::from_be_bytes(memory.read(source, 8)?.try_into().unwrap());
        memory.validate_write(destination, 10)?;
        let bytes = self.expand_double(input);
        memory.write(destination, &bytes)
    }
    fn expand_double(&mut self, input: u64) -> [u8; 10] {
        let sign = ((input >> 48) as u16) & 0x8000;
        let exponent = ((input >> 52) & 0x7ff) as u16;
        let fraction = input & 0x000f_ffff_ffff_ffff;
        let (e, m) = if exponent == 0x7ff {
            if fraction == 0 {
                (0x7fff, 1u64 << 63)
            } else {
                self.invalid |= fraction & (1 << 51) == 0;
                (0x7fff, (1u64 << 63) | (1u64 << 62) | (fraction << 11))
            }
        } else if exponent != 0 {
            (exponent + 15360, ((1u64 << 52) | fraction) << 11)
        } else if fraction == 0 {
            (0, 0)
        } else {
            let top = 63 - fraction.leading_zeros();
            ((top as i32 - 1074 + 16383) as u16, fraction << (63 - top))
        };
        let mut bytes = [0; 10];
        bytes[..2].copy_from_slice(&(sign | e).to_be_bytes());
        bytes[2..].copy_from_slice(&m.to_be_bytes());
        bytes
    }
    pub fn extended_to_double(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let bytes = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 8)?;
        let result = self.convert_double(bytes)?;
        memory.write(destination, &result.to_be_bytes())
    }
    fn convert_double(&mut self, bytes: [u8; 10]) -> Result<u64> {
        let se = u16::from_be_bytes(bytes[..2].try_into().unwrap());
        let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
        let e = se & 0x7fff;
        let sign = u64::from(se & 0x8000) << 48;
        if (e != 0 && m >> 63 == 0) || (e == 0 && m >> 63 != 0) {
            return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
        }
        if e == 0x7fff {
            if m == 1 << 63 {
                return Ok(sign | 0x7ff0_0000_0000_0000);
            }
            self.invalid |= m & (1 << 62) == 0;
            return Ok(sign | 0x7ff8_0000_0000_0000 | ((m & 0x7fff_ffff_ffff_ffff) >> 11));
        }
        if m == 0 {
            return Ok(sign);
        }
        let top = 63 - m.leading_zeros() as i32;
        let scale = i32::from(e.max(1)) - 16446;
        let mut exponent = top + scale;
        let tiny = exponent < -1022;
        let shift = if tiny { -scale - 1074 } else { top - 52 };
        let (mut result, lost) = if shift <= 0 {
            (m << (-shift as u32), false)
        } else if shift > 64 {
            (0, true)
        } else {
            let half = 1u64 << (shift - 1);
            let q = if shift == 64 { 0 } else { m >> shift };
            let rem = if shift == 64 {
                m
            } else {
                m & ((1u64 << shift) - 1)
            };
            (
                q + u64::from(rem > half || (rem == half && q & 1 != 0)),
                rem != 0,
            )
        };
        self.inexact |= lost;
        if !tiny && result == 1 << 53 {
            result >>= 1;
            exponent += 1;
        }
        if exponent > 1023 {
            self.inexact = true;
            self.overflow = true;
            return Ok(sign | 0x7ff0_0000_0000_0000);
        }
        let biased = if tiny {
            u64::from(result >= 1 << 52)
        } else {
            (exponent + 1023) as u64
        };
        self.underflow |= biased == 0 && lost;
        Ok(sign | (biased << 52) | (result & 0x000f_ffff_ffff_ffff))
    }
    pub fn divide_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let a = memory.read(destination, 10)?.try_into().unwrap();
        let b = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 10)?;
        let bytes = self.divide(a, b)?;
        memory.write(destination, &bytes)
    }
    fn divide(&mut self, a: [u8; 10], b: [u8; 10]) -> Result<[u8; 10]> {
        let decode = |v: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(v[..2].try_into().unwrap());
            let m = u64::from_be_bytes(v[2..].try_into().unwrap());
            if (e & 0x7fff != 0 && m >> 63 == 0) || (e & 0x7fff == 0 && m >> 63 != 0) {
                return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
            }
            Ok((e, m))
        };
        let (ae, am) = decode(a)?;
        let (be, bm) = decode(b)?;
        let sign = (ae ^ be) & 0x8000;
        let encode = |e: u16, m: u64| {
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&e.to_be_bytes());
            bytes[2..].copy_from_slice(&m.to_be_bytes());
            bytes
        };
        let an = ae & 0x7fff == 0x7fff && am != 1 << 63;
        let bn = be & 0x7fff == 0x7fff && bm != 1 << 63;
        if an || bn {
            self.invalid |= (an && am & (1 << 62) == 0) || (bn && bm & (1 << 62) == 0);
            let (e, m) = if an { (ae, am) } else { (be, bm) };
            return Ok(encode(e, m | 1 << 62));
        }
        let ai = ae & 0x7fff == 0x7fff;
        let bi = be & 0x7fff == 0x7fff;
        if (ai && bi) || (am == 0 && bm == 0) {
            self.invalid = true;
            return Ok(encode(0x7fff, 0xc004_0000_0000_0000));
        }
        if ai {
            return Ok(encode(sign | 0x7fff, 1 << 63));
        }
        if bi || am == 0 {
            return Ok(encode(sign, 0));
        }
        if bm == 0 {
            self.divide_by_zero = true;
            return Ok(encode(sign | 0x7fff, 1 << 63));
        }
        let ashift = am.leading_zeros();
        let bshift = bm.leading_zeros();
        let am = am << ashift;
        let bm = bm << bshift;
        let delta =
            i32::from((ae & 0x7fff).max(1)) - ashift as i32 - i32::from((be & 0x7fff).max(1))
                + bshift as i32;
        let mut exponent = delta - i32::from(am < bm);
        let tiny = exponent < -16382;
        let shift = if tiny {
            delta + 16445
        } else if am < bm {
            64
        } else {
            63
        };
        let (numerator, denominator) = if shift >= 0 {
            (u128::from(am) << shift, u128::from(bm))
        } else if shift == -1 {
            (u128::from(am), u128::from(bm) << 1)
        } else {
            self.inexact = true;
            self.underflow = true;
            return Ok(encode(sign, 0));
        };
        let quotient = numerator / denominator;
        let remainder = numerator % denominator;
        let mut result = quotient
            + u128::from(
                remainder * 2 > denominator || (remainder * 2 == denominator && quotient & 1 != 0),
            );
        self.inexact |= remainder != 0;
        if !tiny && result == 1u128 << 64 {
            result >>= 1;
            exponent += 1;
        }
        if exponent > 16383 {
            self.overflow = true;
            self.inexact = true;
            return Ok(encode(sign | 0x7fff, 1 << 63));
        }
        let biased = if tiny {
            u16::from(result >= 1u128 << 63)
        } else {
            (exponent + 16383) as u16
        };
        self.underflow |= biased == 0 && remainder != 0;
        Ok(encode(sign | biased, result as u64))
    }
    pub fn multiply_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let a = memory.read(destination, 10)?.try_into().unwrap();
        let b = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 10)?;
        let result = self.multiply(a, b)?;
        memory.write(destination, &result)
    }
    fn multiply(&mut self, a: [u8; 10], b: [u8; 10]) -> Result<[u8; 10]> {
        let decode = |bytes: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(bytes[..2].try_into().unwrap());
            let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
            if ((e & 0x7fff) != 0 && m >> 63 == 0) || ((e & 0x7fff) == 0 && m >> 63 != 0) {
                return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
            }
            Ok((e, m))
        };
        let (ae, am) = decode(a)?;
        let (be, bm) = decode(b)?;
        let sign = (ae ^ be) & 0x8000;
        let encode = |e: u16, m: u64| {
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&e.to_be_bytes());
            bytes[2..].copy_from_slice(&m.to_be_bytes());
            bytes
        };
        let nan = |e: u16, m: u64| e & 0x7fff == 0x7fff && m != 0x8000_0000_0000_0000;
        let an = nan(ae, am);
        let bn = nan(be, bm);
        if an || bn {
            self.invalid |=
                (an && am & 0x4000_0000_0000_0000 == 0) || (bn && bm & 0x4000_0000_0000_0000 == 0);
            let (e, m) = if an { (ae, am) } else { (be, bm) };
            return Ok(encode(e, m | 0x4000_0000_0000_0000));
        }
        if ae & 0x7fff == 0x7fff || be & 0x7fff == 0x7fff {
            if am == 0 || bm == 0 {
                self.invalid = true;
                return Ok(encode(0x7fff, 0xc008_0000_0000_0000));
            }
            return Ok(encode(sign | 0x7fff, 0x8000_0000_0000_0000));
        }
        if am == 0 || bm == 0 {
            return Ok(encode(sign, 0));
        }
        let product = u128::from(am) * u128::from(bm);
        let scale = i32::from((ae & 0x7fff).max(1)) + i32::from((be & 0x7fff).max(1)) - 32892;
        let top = 127 - product.leading_zeros() as i32;
        let mut exponent = top + scale;
        let tiny = exponent < -16382;
        let shift = if tiny { -scale - 16445 } else { top - 63 };
        let (mut mantissa, lost) = if shift <= 0 {
            (product << (-shift as u32), false)
        } else if shift > 128 {
            (0, true)
        } else {
            let half = 1u128 << (shift - 1);
            let quotient = if shift == 128 { 0 } else { product >> shift };
            let remainder = if shift == 128 {
                product
            } else {
                product & ((1u128 << shift) - 1)
            };
            (
                quotient + u128::from(remainder > half || (remainder == half && quotient & 1 != 0)),
                remainder != 0,
            )
        };
        self.inexact |= lost;
        if !tiny && mantissa == 1u128 << 64 {
            mantissa >>= 1;
            exponent += 1;
        }
        if exponent > 16383 {
            self.overflow = true;
            self.inexact = true;
            return Ok(encode(sign | 0x7fff, 0x8000_0000_0000_0000));
        }
        let biased = if tiny {
            u16::from(mantissa >= 1u128 << 63)
        } else {
            (exponent + 16383) as u16
        };
        self.underflow |= biased == 0 && lost;
        Ok(encode(sign | biased, mantissa as u64))
    }
    pub fn atan_extended(&mut self, memory: &mut Memory, pointer: GuestPtr) -> Result<()> {
        self.elementary(memory, pointer, Unary::Atan)
    }
    pub fn elementary(&mut self, memory: &mut Memory, pointer: GuestPtr, op: Unary) -> Result<()> {
        let input: [u8; 10] = memory.read(pointer, 10)?.try_into().unwrap();
        memory.validate_write(pointer, 10)?;
        let result = self.elementary_value(op, input)?;
        memory.write(pointer, &result)
    }
    fn elementary_value(&mut self, op: Unary, input: [u8; 10]) -> Result<[u8; 10]> {
        let se = u16::from_be_bytes(input[..2].try_into().unwrap());
        let mantissa = u64::from_be_bytes(input[2..].try_into().unwrap());
        let exponent = se & 0x7fff;
        let sign = if se & 0x8000 == 0 {
            Sign::Pos
        } else {
            Sign::Neg
        };
        let encode = |exponent: u16, mantissa: u64| {
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&((se & 0x8000) | exponent).to_be_bytes());
            bytes[2..].copy_from_slice(&mantissa.to_be_bytes());
            bytes
        };
        if exponent == 0 && mantissa == 0 {
            return Ok(if op == Unary::Cos {
                let mut one = [0; 10];
                one[..2].copy_from_slice(&0x3fffu16.to_be_bytes());
                one[2] = 0x80;
                one
            } else {
                input
            });
        }
        if (exponent != 0 && mantissa >> 63 == 0) || (exponent == 0 && mantissa >> 63 != 0) {
            return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
        }
        if exponent == 0x7fff {
            if mantissa != 0x8000_0000_0000_0000 {
                self.invalid |= mantissa & 0x4000_0000_0000_0000 == 0;
                return Ok(encode(exponent, mantissa | 0x4000_0000_0000_0000));
            }
            if op != Unary::Atan {
                self.invalid = true;
                let mut nan = [0; 10];
                nan[..2].copy_from_slice(&0x7fffu16.to_be_bytes());
                nan[2..].copy_from_slice(&0xc021_0000_0000_0000u64.to_be_bytes());
                return Ok(nan);
            }
            self.inexact = true;
            return Ok(encode(0x3fff, 0xc90f_daa2_2168_c235));
        }
        // The odd functions round to x here; cos rounds to one.
        // Preserve even the smallest subnormal without narrowing its exponent.
        if exponent < 0x3fde {
            self.inexact = true;
            self.underflow |= exponent == 0 && op != Unary::Cos;
            if op == Unary::Cos {
                let mut one = [0; 10];
                one[..2].copy_from_slice(&0x3fffu16.to_be_bytes());
                one[2] = 0x80;
                return Ok(one);
            }
            return Ok(input);
        }
        #[cfg(target_pointer_width = "64")]
        let words = [mantissa];
        #[cfg(target_pointer_width = "32")]
        let words = [mantissa as u32, (mantissa >> 32) as u32];
        let x = BigFloat::from_words(&words, sign, i32::from(exponent) - 16382);
        let mut constants =
            Consts::new().map_err(|e| Error::Cpu(format!("numeric constants: {e:?}")))?;
        let result = match op {
            Unary::Atan => x.atan(64, RoundingMode::ToEven, &mut constants),
            Unary::Sin => x.sin(64, RoundingMode::ToEven, &mut constants),
            Unary::Cos => x.cos(64, RoundingMode::ToEven, &mut constants),
            Unary::Tan => x.tan(64, RoundingMode::ToEven, &mut constants),
        };
        let Some((words, bits, result_sign, exp, _)) = result.as_raw_parts() else {
            return Err(Error::Cpu("software elementary evaluation failed".into()));
        };
        if bits != 64 || !(-16381..=16384).contains(&exp) {
            return Err(Error::Cpu(
                "unexpected software elementary representation".into(),
            ));
        }
        #[cfg(target_pointer_width = "64")]
        let mantissa = words[0];
        #[cfg(target_pointer_width = "32")]
        let mantissa = u64::from(words[0]) | (u64::from(words[1]) << 32);
        self.inexact = true;
        let mut bytes = [0; 10];
        let sign = if result_sign == Sign::Neg { 0x8000 } else { 0 };
        bytes[..2].copy_from_slice(&(sign | (exp + 16382) as u16).to_be_bytes());
        bytes[2..].copy_from_slice(&mantissa.to_be_bytes());
        Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn value(se: u16, m: u64) -> [u8; 10] {
        let mut b = [0; 10];
        b[..2].copy_from_slice(&se.to_be_bytes());
        b[2..].copy_from_slice(&m.to_be_bytes());
        b
    }
    #[test]
    fn trig_matches_independent_decimal_series_with_large_arguments_and_low_bits() {
        // Generated by tests/reference/trig_decimal.py at two precisions.
        for (op, e, m, re, rm) in [
            (
                Unary::Sin,
                0x3fff,
                0x8000000000000000,
                0x3ffe,
                0xd76aa47848677021,
            ),
            (
                Unary::Cos,
                0x3fff,
                0x8000000000000000,
                0x3ffe,
                0x8a51407da8345c92,
            ),
            (
                Unary::Tan,
                0x3fff,
                0x8000000000000000,
                0x3fff,
                0xc75922e5f71d2dc5,
            ),
            (
                Unary::Sin,
                0xc000,
                0x8000000000000000,
                0xbffe,
                0xe8c7b7568da22efd,
            ),
            (
                Unary::Cos,
                0xc000,
                0x8000000000000000,
                0xbffd,
                0xd51132ba9b902522,
            ),
            (
                Unary::Tan,
                0xc000,
                0x8000000000000000,
                0x4000,
                0x8bd7b1704a87c1db,
            ),
            (
                Unary::Sin,
                0x403f,
                0x8000000000000000,
                0x3ff9,
                0xc151a988c3c9e92f,
            ),
            (
                Unary::Cos,
                0x403f,
                0x8000000000000000,
                0xbffe,
                0xffedbfd1ff1a6a7a,
            ),
            (
                Unary::Tan,
                0x403f,
                0x8000000000000000,
                0xbff9,
                0xc15f72b97156463b,
            ),
            (
                Unary::Sin,
                0x43e7,
                0x8000000000000000,
                0xbffc,
                0xa305c570e443726d,
            ),
            (
                Unary::Cos,
                0x43e7,
                0x8000000000000000,
                0x3ffe,
                0xfcbc28b06440a8bc,
            ),
            (
                Unary::Tan,
                0x43e7,
                0x8000000000000000,
                0xbffc,
                0xa520eab060465f30,
            ),
            (
                Unary::Sin,
                0x3fff,
                0x8000000000000001,
                0x3ffe,
                0xd76aa47848677022,
            ),
            (
                Unary::Cos,
                0x3fff,
                0x8000000000000001,
                0x3ffe,
                0x8a51407da8345c90,
            ),
            (
                Unary::Tan,
                0x3fff,
                0x8000000000000001,
                0x3fff,
                0xc75922e5f71d2dc9,
            ),
        ] {
            assert_eq!(
                Numerics::default()
                    .elementary_value(op, value(e, m))
                    .unwrap(),
                value(re, rm),
                "{op:?} input {e:04x}:{m:016x}"
            );
        }
    }
    #[test]
    fn trig_zero_tiny_infinity_and_nan_cases_retain_real_status() {
        for op in [Unary::Sin, Unary::Cos, Unary::Tan] {
            let mut n = Numerics::default();
            let one = value(0x3fff, 1 << 63);
            assert_eq!(
                n.elementary_value(op, value(0x8000, 0)).unwrap(),
                if op == Unary::Cos {
                    one
                } else {
                    value(0x8000, 0)
                }
            );
            assert!(!n.inexact);
            assert_eq!(
                n.elementary_value(op, value(0, 1)).unwrap(),
                if op == Unary::Cos { one } else { value(0, 1) }
            );
            assert!(n.inexact);
            assert_eq!(n.underflow, op != Unary::Cos);
            assert_eq!(
                n.elementary_value(op, value(0x7fff, 1 << 63)).unwrap(),
                value(0x7fff, 0xc021_0000_0000_0000)
            );
            assert!(n.invalid);
        }
    }
    #[test]
    fn signed_integer_expansion_is_exact_at_both_width_boundaries() {
        assert_eq!(Numerics::expand_integer(0), value(0, 0));
        assert_eq!(Numerics::expand_integer(1), value(0x3fff, 1 << 63));
        assert_eq!(Numerics::expand_integer(-32768), value(0xc00e, 1 << 63));
        assert_eq!(Numerics::expand_integer(i32::MIN), value(0xc01e, 1 << 63));
        assert_eq!(
            Numerics::expand_integer(i32::MAX),
            value(0x401d, 0xffff_fffe_0000_0000)
        );
    }
    #[test]
    fn normal_division_agrees_with_independent_arbitrary_precision_rounding() {
        let mut seed = 0x9172_3aef_8124_5bcdu64;
        for i in 0..256 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let am = seed | (1 << 63);
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let bm = seed | (1 << 63);
            let ae = 0x3000 + (i % 3) * 0x1000;
            let be = 0x3000 + (i % 5) * 0x0800;
            let result = Numerics::default()
                .divide(value(ae, am), value(be, bm))
                .unwrap();
            #[cfg(target_pointer_width = "64")]
            let (aw, bw) = ([am], [bm]);
            #[cfg(target_pointer_width = "32")]
            let (aw, bw) = (
                [am as u32, (am >> 32) as u32],
                [bm as u32, (bm >> 32) as u32],
            );
            let a = BigFloat::from_words(&aw, Sign::Pos, i32::from(ae) - 16382);
            let b = BigFloat::from_words(&bw, Sign::Pos, i32::from(be) - 16382);
            let oracle = a.div(&b, 64, RoundingMode::ToEven);
            let (words, _, _, exponent, _) = oracle.as_raw_parts().unwrap();
            #[cfg(target_pointer_width = "64")]
            let mantissa = words[0];
            #[cfg(target_pointer_width = "32")]
            let mantissa = u64::from(words[0]) | (u64::from(words[1]) << 32);
            assert_eq!(result, value((exponent + 16382) as u16, mantissa));
        }
    }
    #[test]
    fn division_uses_exact_remainders_and_gradual_underflow() {
        let mut n = Numerics::default();
        let one = value(0x3fff, 1 << 63);
        let two = value(0x4000, 1 << 63);
        let three = value(0x4000, 0xc000_0000_0000_0000);
        assert_eq!(n.divide(one, two).unwrap(), value(0x3ffe, 1 << 63));
        assert!(!n.inexact);
        assert_eq!(
            n.divide(one, three).unwrap(),
            value(0x3ffd, 0xaaaa_aaaa_aaaa_aaab)
        );
        assert!(n.inexact);
        let mut n = Numerics::default();
        assert_eq!(n.divide(value(1, 1 << 63), two).unwrap(), value(0, 1 << 62));
        assert!(!n.underflow && !n.inexact);
        assert_eq!(n.divide(value(0, 1), two).unwrap(), value(0, 0));
        assert!(n.underflow && n.inexact);
        assert_eq!(n.divide(value(0, 3), two).unwrap(), value(0, 2));
        assert_eq!(n.divide(value(0, 1), value(0, 1)).unwrap(), one);
        assert_eq!(
            n.divide(value(0x7ffe, u64::MAX), value(0x3ffe, 1 << 63))
                .unwrap(),
            value(0x7fff, 1 << 63)
        );
        assert!(n.overflow);
    }
    #[test]
    fn division_special_values_keep_distinct_invalid_and_zero_divisor_status() {
        let mut n = Numerics::default();
        let one = value(0x3fff, 1 << 63);
        let zero = value(0x8000, 0);
        let infinity = value(0x7fff, 1 << 63);
        assert_eq!(n.divide(one, zero).unwrap(), value(0xffff, 1 << 63));
        assert!(n.divide_by_zero && !n.invalid && !n.inexact);
        assert_eq!(
            n.divide(infinity, infinity).unwrap(),
            value(0x7fff, 0xc004_0000_0000_0000)
        );
        assert!(n.invalid);
        assert_eq!(n.divide(zero, one).unwrap(), zero);
        assert_eq!(n.divide(one, infinity).unwrap(), value(0, 0));
    }
    #[test]
    fn expanding_binary64_is_exact_including_smallest_subnormal_and_signed_zero() {
        let mut n = Numerics::default();
        assert_eq!(n.expand_double(1), value(16383 - 1074, 1 << 63));
        assert_eq!(n.expand_double(1f64.to_bits()), value(0x3fff, 1 << 63));
        for bits in [
            0,
            1,
            0x000f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            1f64.to_bits(),
            0x7fef_ffff_ffff_ffff,
            0x7ff0_0000_0000_0000,
        ] {
            for sign in [0, 1u64 << 63] {
                let bits = bits | sign;
                let expanded = n.expand_double(bits);
                assert_eq!(n.convert_double(expanded).unwrap(), bits);
            }
        }
        assert!(!n.inexact && !n.underflow && !n.invalid && !n.overflow);
    }
    #[test]
    fn double_conversion_rounds_low_bits_subnormals_and_overflow() {
        let mut n = Numerics::default();
        assert_eq!(
            n.convert_double(value(0x3fff, 0x8000_0000_0000_0000))
                .unwrap(),
            1f64.to_bits()
        );
        assert!(!n.inexact);
        assert_eq!(
            n.convert_double(value(0x3fff, 0x8000_0000_0000_0400))
                .unwrap(),
            1f64.to_bits()
        );
        assert_eq!(
            n.convert_double(value(0x3fff, 0x8000_0000_0000_0c00))
                .unwrap(),
            1f64.to_bits() + 2
        );
        let mut n = Numerics::default();
        assert_eq!(n.convert_double(value(16383 - 1074, 1 << 63)).unwrap(), 1);
        assert!(!n.underflow && !n.inexact);
        assert_eq!(n.convert_double(value(16383 - 1075, 1 << 63)).unwrap(), 0);
        assert!(n.underflow && n.inexact);
        assert_eq!(
            n.convert_double(value(16383 + 1024, 1 << 63)).unwrap(),
            f64::INFINITY.to_bits()
        );
        assert!(n.overflow);
        assert_eq!(
            n.convert_double(value(0x8000, 0)).unwrap(),
            (-0f64).to_bits()
        );
    }
    #[test]
    fn multiplication_rounds_once_with_gradual_underflow_and_exact_low_bits() {
        let one = value(0x3fff, 1 << 63);
        let half = value(0x3ffe, 1 << 63);
        let mut n = Numerics::default();
        assert_eq!(n.multiply(one, one).unwrap(), one);
        assert!(!n.inexact);
        assert_eq!(
            n.multiply(
                value(0x3fff, (1 << 63) + 1),
                value(0x3fff, 0xc000_0000_0000_0000)
            )
            .unwrap(),
            value(0x3fff, 0xc000_0000_0000_0002)
        );
        assert!(n.inexact);
        let mut n = Numerics::default();
        assert_eq!(
            n.multiply(value(1, 1 << 63), half).unwrap(),
            value(0, 1 << 62)
        );
        assert!(!n.underflow && !n.inexact);
        assert_eq!(n.multiply(value(0, 1), half).unwrap(), value(0, 0));
        assert!(n.underflow && n.inexact);
        assert_eq!(n.multiply(value(0, 3), half).unwrap(), value(0, 2));
        let mut boundary = Numerics::default();
        assert_eq!(
            boundary
                .multiply(
                    value(0, 0x7fff_ffff_ffff_ffff),
                    value(0x3fff, 0x8000_0000_0000_0001)
                )
                .unwrap(),
            value(1, 0x8000_0000_0000_0000)
        );
        assert!(boundary.inexact && !boundary.underflow);
        assert_eq!(
            n.multiply(value(0x7ffe, u64::MAX), value(0x4000, 1 << 63))
                .unwrap(),
            value(0x7fff, 1 << 63)
        );
        assert!(n.overflow);
        assert_eq!(n.multiply(value(0x8000, 0), one).unwrap(), value(0x8000, 0));
        assert_eq!(
            n.multiply(value(0, 0), value(0x7fff, 1 << 63)).unwrap(),
            value(0x7fff, 0xc008_0000_0000_0000)
        );
        assert!(n.invalid);
    }
    #[test]
    fn extended_arctangent_retains_low_significand_bits_and_special_values() {
        let mut n = Numerics::default();
        assert_eq!(
            n.elementary_value(Unary::Atan, value(0x3fff, 0x8000_0000_0000_0000))
                .unwrap(),
            value(0x3ffe, 0xc90f_daa2_2168_c235)
        );
        assert_eq!(
            n.elementary_value(Unary::Atan, value(0x3fff, 0x8000_0000_0000_0001))
                .unwrap(),
            value(0x3ffe, 0xc90f_daa2_2168_c236)
        );
        assert_eq!(
            n.elementary_value(Unary::Atan, value(0xbfff, 0x8000_0000_0000_0000))
                .unwrap(),
            value(0xbffe, 0xc90f_daa2_2168_c235)
        );
        for sign in [0, 0x8000] {
            assert_eq!(
                n.elementary_value(Unary::Atan, value(sign, 0)).unwrap(),
                value(sign, 0)
            );
            assert_eq!(
                n.elementary_value(Unary::Atan, value(sign, 1)).unwrap(),
                value(sign, 1)
            );
            assert_eq!(
                n.elementary_value(Unary::Atan, value(sign | 0x7fff, 0x8000_0000_0000_0000))
                    .unwrap(),
                value(sign | 0x3fff, 0xc90f_daa2_2168_c235)
            );
        }
        assert!(n.underflow && n.inexact);
        assert_eq!(
            n.elementary_value(Unary::Atan, value(0x7fff, 0x8000_0000_0000_0001))
                .unwrap(),
            value(0x7fff, 0xc000_0000_0000_0001)
        );
        assert!(n.invalid);
    }
}
