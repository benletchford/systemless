//! Shared software numeric services. Apple Numerics Manual, 2nd ed., ch. 24.
//! SANE slices use default round-to-nearest 64-bit extended significands.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use astro_float_num::{BigFloat, Consts, RoundingMode, Sign};
mod power;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Atan,
    Sin,
    Cos,
    Tan,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Get,
    Set,
    Entry,
    Exit,
}
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Less,
    Equal,
    Greater,
    Unordered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Extended,
    Double,
    Single,
    Integer,
    Long,
}
impl SourceFormat {
    pub fn bytes(self) -> usize {
        match self {
            Self::Extended => 10,
            Self::Double => 8,
            Self::Single | Self::Long => 4,
            Self::Integer => 2,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
}
#[derive(Default, Clone)]
pub struct Numerics {
    /// Sticky mathematical exceptions, retained for future SANE environment APIs.
    pub inexact: bool,
    pub underflow: bool,
    pub invalid: bool,
    pub overflow: bool,
    pub divide_by_zero: bool,
}
// Apple Numerics Manual, table E-5: the explicit integer bit is ignored for
// infinity and NaN. Canonicalize only local arithmetic operands; raw sign
// operations deliberately retain every nonsign bit in guest memory.
fn canonical_special(mut bytes: [u8; 10]) -> [u8; 10] {
    if u16::from_be_bytes(bytes[..2].try_into().unwrap()) & 0x7fff == 0x7fff {
        bytes[2] |= 0x80;
    }
    bytes
}
// Apple Numerics Manual, table E-5: every finite exponent permits either
// explicit integer bit. Normalize local arithmetic operands without rounding;
// exponent zero is a valid normal exponent and the floor for denormals.
fn canonical_extended(bytes: [u8; 10]) -> [u8; 10] {
    let mut bytes = canonical_special(bytes);
    let se = u16::from_be_bytes(bytes[..2].try_into().unwrap());
    let mut exponent = se & 0x7fff;
    if exponent == 0x7fff {
        return bytes;
    }
    let mut mantissa = u64::from_be_bytes(bytes[2..].try_into().unwrap());
    if mantissa == 0 {
        exponent = 0;
    } else {
        let shift = mantissa.leading_zeros().min(u32::from(exponent));
        mantissa <<= shift;
        exponent -= shift as u16;
    }
    bytes[..2].copy_from_slice(&((se & 0x8000) | exponent).to_be_bytes());
    bytes[2..].copy_from_slice(&mantissa.to_be_bytes());
    bytes
}
impl Numerics {
    /// Nonarithmetic SANE sign manipulation: preserve payload and exception state,
    /// including signaling NaNs (Apple Numerics Manual, ch. 20, p. 150).
    pub fn sign_extended(
        &mut self,
        memory: &mut Memory,
        pointer: GuestPtr,
        absolute: bool,
    ) -> Result<()> {
        let mut bytes: [u8; 10] = memory.read(pointer, 10)?.try_into().unwrap();
        memory.validate_write(pointer, 10)?;
        if absolute {
            bytes[0] &= 0x7f;
        } else {
            bytes[0] ^= 0x80;
        }
        memory.write(pointer, &bytes)
    }

    pub fn round_integral_extended(
        &mut self,
        memory: &mut Memory,
        pointer: GuestPtr,
        truncate: bool,
    ) -> Result<()> {
        let input = memory.read(pointer, 10)?.try_into().unwrap();
        memory.validate_write(pointer, 10)?;
        let result = self.round_integral(input, truncate)?;
        memory.write(pointer, &result)
    }
    fn round_integral(&mut self, input: [u8; 10], truncate: bool) -> Result<[u8; 10]> {
        let input = canonical_extended(input);
        let se = u16::from_be_bytes(input[..2].try_into().unwrap());
        let e = se & 0x7fff;
        let m = u64::from_be_bytes(input[2..].try_into().unwrap());
        if e == 0x7fff {
            if m == 1 << 63 {
                return Ok(input);
            }
            self.invalid |= m & (1 << 62) == 0;
            let mut bytes = input;
            bytes[2] |= 0x40;
            return Ok(bytes);
        }
        if m == 0 {
            return Ok(input);
        }
        let exponent = i32::from(e) - 16383;
        if exponent >= 63 {
            return Ok(input);
        }
        let encode = |e: u16, m: u64| {
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&((se & 0x8000) | e).to_be_bytes());
            bytes[2..].copy_from_slice(&m.to_be_bytes());
            bytes
        };
        if exponent < 0 {
            self.inexact = true;
            return Ok(if !truncate && exponent == -1 && m > 1 << 63 {
                encode(0x3fff, 1 << 63)
            } else {
                encode(0, 0)
            });
        }
        let shift = (63 - exponent) as u32;
        let remainder = m & ((1u64 << shift) - 1);
        if remainder == 0 {
            return Ok(input);
        }
        self.inexact = true;
        let mut integer = u128::from(m >> shift);
        let half = 1u64 << (shift - 1);
        if !truncate && (remainder > half || (remainder == half && integer & 1 != 0)) {
            integer += 1;
        }
        let mut significand = integer << shift;
        let mut biased = e;
        if significand == 1u128 << 64 {
            significand >>= 1;
            biased += 1;
        }
        Ok(encode(biased, significand as u64))
    }
    pub fn binary(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
        format: SourceFormat,
        op: Binary,
    ) -> Result<()> {
        let a: [u8; 10] = memory.read(destination, 10)?.try_into().unwrap();
        let source = memory.read(source, format.bytes())?.to_vec();
        memory.validate_write(destination, 10)?;
        let mut next = self.clone();
        let b = next.expand_source(&source, format);
        let result = match op {
            Binary::Add => next.add(a, b, false)?,
            Binary::Subtract => next.add(a, b, true)?,
            Binary::Multiply => next.multiply(a, b)?,
            Binary::Divide => next.divide(a, b)?,
        };
        memory.write(destination, &result)?;
        *self = next;
        Ok(())
    }
    fn expand_source(&mut self, source: &[u8], format: SourceFormat) -> [u8; 10] {
        match format {
            SourceFormat::Extended => source.try_into().unwrap(),
            SourceFormat::Double => {
                self.expand_double(u64::from_be_bytes(source.try_into().unwrap()))
            }
            SourceFormat::Single => {
                self.expand_single(u32::from_be_bytes(source.try_into().unwrap()))
            }
            SourceFormat::Integer => {
                Self::expand_integer(i32::from(i16::from_be_bytes(source.try_into().unwrap())))
            }
            SourceFormat::Long => {
                Self::expand_integer(i32::from_be_bytes(source.try_into().unwrap()))
            }
        }
    }
    fn expand_single(&mut self, input: u32) -> [u8; 10] {
        let sign = ((input >> 16) as u16) & 0x8000;
        let exponent = (input >> 23) & 0xff;
        let fraction = input & 0x007f_ffff;
        let (e, m) = if exponent == 0xff {
            if fraction == 0 {
                (0x7fff, 1u64 << 63)
            } else {
                self.invalid |= fraction & (1 << 22) == 0;
                (
                    0x7fff,
                    (1u64 << 63) | (1u64 << 62) | (u64::from(fraction) << 40),
                )
            }
        } else if exponent != 0 {
            (
                (exponent + 16256) as u16,
                (u64::from(fraction) | (1 << 23)) << 40,
            )
        } else if fraction == 0 {
            (0, 0)
        } else {
            let top = 31 - fraction.leading_zeros();
            ((top + 16234) as u16, u64::from(fraction) << (63 - top))
        };
        let mut bytes = [0; 10];
        bytes[..2].copy_from_slice(&(sign | e).to_be_bytes());
        bytes[2..].copy_from_slice(&m.to_be_bytes());
        bytes
    }
    pub fn square_root_extended(&mut self, memory: &mut Memory, pointer: GuestPtr) -> Result<()> {
        let bytes = memory.read(pointer, 10)?.try_into().unwrap();
        memory.validate_write(pointer, 10)?;
        let result = self.square_root(bytes)?;
        memory.write(pointer, &result)
    }
    fn square_root(&mut self, input: [u8; 10]) -> Result<[u8; 10]> {
        let input = canonical_extended(input);
        let se = u16::from_be_bytes(input[..2].try_into().unwrap());
        let exponent = se & 0x7fff;
        let mantissa = u64::from_be_bytes(input[2..].try_into().unwrap());
        if exponent == 0x7fff && mantissa != 1 << 63 {
            self.invalid |= mantissa & (1 << 62) == 0;
            let mut result = input;
            result[2] |= 0x40;
            return Ok(result);
        }
        if exponent == 0 && mantissa == 0 {
            return Ok(input);
        }
        if se & 0x8000 != 0 {
            self.invalid = true;
            let mut result = [0; 10];
            result[..2].copy_from_slice(&0x7fffu16.to_be_bytes());
            result[2..].copy_from_slice(&0xc001_0000_0000_0000u64.to_be_bytes());
            return Ok(result);
        }
        if exponent == 0x7fff {
            return Ok(input);
        }
        #[cfg(target_pointer_width = "64")]
        let words = [mantissa];
        #[cfg(target_pointer_width = "32")]
        let words = [mantissa as u32, (mantissa >> 32) as u32];
        let value = BigFloat::from_words(&words, Sign::Pos, i32::from(exponent) - 16382);
        let result = value.sqrt(64, RoundingMode::ToEven);
        let Some((words, bits, _, exp, inexact)) = result.as_raw_parts() else {
            return Err(Error::Cpu("software square root failed".into()));
        };
        if bits != 64 || !(-8222..=8193).contains(&exp) {
            return Err(Error::Cpu(
                "invalid extended square root representation".into(),
            ));
        }
        #[cfg(target_pointer_width = "64")]
        let significand = words[0];
        #[cfg(target_pointer_width = "32")]
        let significand = u64::from(words[0]) | (u64::from(words[1]) << 32);
        self.inexact |= inexact;
        let mut bytes = [0; 10];
        bytes[..2].copy_from_slice(&((exp + 16382) as u16).to_be_bytes());
        bytes[2..].copy_from_slice(&significand.to_be_bytes());
        Ok(bytes)
    }
    pub fn add_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
        subtract: bool,
    ) -> Result<()> {
        let a = memory.read(destination, 10)?.try_into().unwrap();
        let b = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 10)?;
        let result = self.add(a, b, subtract)?;
        memory.write(destination, &result)
    }
    fn add(&mut self, a: [u8; 10], b: [u8; 10], subtract: bool) -> Result<[u8; 10]> {
        let (a, b) = (canonical_extended(a), canonical_extended(b));
        let decode = |bytes: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(bytes[..2].try_into().unwrap());
            let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
            Ok((e, m))
        };
        let (ae, am) = decode(a)?;
        let (be, bm) = decode(b)?;
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
        let effective_be = be ^ if subtract { 0x8000 } else { 0 };
        let ai = ae & 0x7fff == 0x7fff;
        let bi = be & 0x7fff == 0x7fff;
        if ai || bi {
            if ai && bi && (ae ^ effective_be) & 0x8000 != 0 {
                self.invalid = true;
                return Ok(encode(0x7fff, 0xc002_0000_0000_0000));
            }
            return Ok(encode(if ai { ae } else { effective_be }, 1 << 63));
        }
        if am == 0 && bm == 0 {
            return Ok(encode(ae & effective_be & 0x8000, 0));
        }
        // Preserve exact finite identities before the full-precision library
        // operation: astro-float-num 0.3.7 applies precision zero when one
        // operand is zero, discarding the nonzero value in that special branch.
        if am == 0 {
            return Ok(encode(effective_be, bm));
        }
        if bm == 0 {
            return Ok(encode(ae, am));
        }
        #[cfg(target_pointer_width = "64")]
        let (aw, bw) = ([am], [bm]);
        #[cfg(target_pointer_width = "32")]
        let (aw, bw) = (
            [am as u32, (am >> 32) as u32],
            [bm as u32, (bm >> 32) as u32],
        );
        let av = BigFloat::from_words(
            &aw,
            if ae & 0x8000 == 0 {
                Sign::Pos
            } else {
                Sign::Neg
            },
            i32::from(ae & 0x7fff) - 16382,
        );
        let bv = BigFloat::from_words(
            &bw,
            if be & 0x8000 == 0 {
                Sign::Pos
            } else {
                Sign::Neg
            },
            i32::from(be & 0x7fff) - 16382,
        );
        let exact = if subtract {
            av.sub_full_prec(&bv)
        } else {
            av.add_full_prec(&bv)
        };
        if exact.is_zero() {
            return Ok(encode(0, 0));
        }
        self.pack_big(&exact)
    }
    fn pack_big(&mut self, value: &BigFloat) -> Result<[u8; 10]> {
        let encode = |se: u16, m: u64| {
            let mut bytes = [0; 10];
            bytes[..2].copy_from_slice(&se.to_be_bytes());
            bytes[2..].copy_from_slice(&m.to_be_bytes());
            bytes
        };
        let sign = if value.sign() == Some(Sign::Neg) {
            0x8000
        } else {
            0
        };
        if value.is_inf() {
            self.overflow = true;
            self.inexact = true;
            return Ok(encode(sign | 0x7fff, 1 << 63));
        }
        if value.is_zero() {
            return Ok(encode(sign, 0));
        }
        let Some((words, _, sign, exponent, _)) = value.as_raw_parts() else {
            return Err(Error::Cpu("SANE result representation failed".into()));
        };
        if exponent > 16384 {
            self.overflow = true;
            self.inexact = true;
            return Ok(encode(
                if sign == Sign::Neg { 0xffff } else { 0x7fff },
                1 << 63,
            ));
        }
        let word_bits = std::mem::size_of_val(&words[0]) * 8;
        let bits = words.len() * word_bits;
        if bits > 33024 {
            return Err(Error::Cpu(
                "SANE result exceeded bounded significand storage".into(),
            ));
        }
        let tiny = exponent < -16382;
        let shift = if tiny {
            bits as i32 - exponent - 16446
        } else {
            bits as i32 - 64
        };
        if shift < 0 {
            return Err(Error::Cpu("invalid exact addition representation".into()));
        }
        let shift = shift as usize;
        let bit = |i: usize| i < bits && words[i / word_bits] & (1 << (i % word_bits)) != 0;
        let mut significand = 0u128;
        for i in 0..64 {
            if bit(shift + i) {
                significand |= 1u128 << i;
            }
        }
        let lost = (0..shift.min(bits)).any(bit);
        let guard = shift != 0 && bit(shift - 1);
        let sticky = shift > 1 && (0..(shift - 1).min(bits)).any(bit);
        if guard && (sticky || significand & 1 != 0) {
            significand += 1;
        }
        self.inexact |= lost;
        let sign = if sign == Sign::Neg { 0x8000 } else { 0 };
        let mut biased = if tiny { 0 } else { (exponent + 16382) as u16 };
        if !tiny && significand == 1u128 << 64 {
            significand >>= 1;
            biased += 1;
        }
        if biased >= 0x7fff {
            self.overflow = true;
            self.inexact = true;
            return Ok(encode(sign | 0x7fff, 1 << 63));
        }
        self.underflow |= significand < 1u128 << 63 && lost;
        Ok(encode(sign | biased, significand as u64))
    }
    pub fn compare(
        &mut self,
        memory: &Memory,
        source: GuestPtr,
        destination: GuestPtr,
        format: SourceFormat,
        signaling: bool,
    ) -> Result<Relation> {
        let a = memory.read(destination, 10)?.try_into().unwrap();
        let source = memory.read(source, format.bytes())?;
        let mut next = self.clone();
        let b = next.expand_source(source, format);
        let relation = next.compare_values(a, b, signaling)?;
        *self = next;
        Ok(relation)
    }
    fn compare_values(&mut self, a: [u8; 10], b: [u8; 10], signaling: bool) -> Result<Relation> {
        let (a, b) = (canonical_extended(a), canonical_extended(b));
        let decode = |bytes: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(bytes[..2].try_into().unwrap());
            let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
            Ok((e, m))
        };
        let (ae, am) = decode(a)?;
        let (be, bm) = decode(b)?;
        let an = ae & 0x7fff == 0x7fff && am != 1 << 63;
        let bn = be & 0x7fff == 0x7fff && bm != 1 << 63;
        if an || bn {
            self.invalid |= signaling || (an && am & (1 << 62) == 0) || (bn && bm & (1 << 62) == 0);
            return Ok(Relation::Unordered);
        }
        if am == 0 && bm == 0 {
            return Ok(Relation::Equal);
        }
        let order = if (ae ^ be) & 0x8000 != 0 {
            if ae & 0x8000 != 0 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        } else {
            let order = (ae & 0x7fff, am).cmp(&(be & 0x7fff, bm));
            if ae & 0x8000 != 0 {
                order.reverse()
            } else {
                order
            }
        };
        Ok(match order {
            std::cmp::Ordering::Less => Relation::Less,
            std::cmp::Ordering::Equal => Relation::Equal,
            std::cmp::Ordering::Greater => Relation::Greater,
        })
    }
    pub fn environment_word(&self) -> u16 {
        (u16::from(self.invalid) << 8)
            | (u16::from(self.underflow) << 9)
            | (u16::from(self.overflow) << 10)
            | (u16::from(self.divide_by_zero) << 11)
            | (u16::from(self.inexact) << 12)
    }
    fn restore_flags(&mut self, word: u16) {
        self.invalid = word & 0x0100 != 0;
        self.underflow = word & 0x0200 != 0;
        self.overflow = word & 0x0400 != 0;
        self.divide_by_zero = word & 0x0800 != 0;
        self.inexact = word & 0x1000 != 0;
    }
    pub fn environment(
        &mut self,
        memory: &mut Memory,
        pointer: GuestPtr,
        op: Environment,
    ) -> Result<()> {
        match op {
            Environment::Get | Environment::Entry => {
                memory.validate_write(pointer, 2)?;
                let saved = self.environment_word();
                memory.write(pointer, &saved.to_be_bytes())?;
                if op == Environment::Entry {
                    self.restore_flags(0);
                }
            }
            Environment::Set | Environment::Exit => {
                let saved = memory.u16(pointer)?;
                if saved & !0x1f00 != 0 {
                    return Err(Error::Cpu("SANE alternate rounding/precision, halts or reserved environment bits are unsupported".into()));
                }
                let word = if op == Environment::Exit {
                    saved | self.environment_word()
                } else {
                    saved
                };
                self.restore_flags(word);
            }
        }
        Ok(())
    }

    pub fn classify_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let bytes: [u8; 10] = memory.read(source, 10)?.try_into().unwrap();
        let bytes = canonical_special(bytes);
        memory.validate_write(destination, 2)?;
        let se = u16::from_be_bytes(bytes[..2].try_into().unwrap());
        let e = se & 0x7fff;
        let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
        let class: i16 = if e == 0x7fff {
            if m == 1 << 63 {
                3
            } else if m & (1 << 62) != 0 {
                2
            } else {
                1
            }
        } else if m == 0 {
            4
        } else if m >> 63 == 0 {
            6
        } else {
            5
        };
        let result = if se & 0x8000 != 0 { -class } else { class };
        memory.write(destination, &result.to_be_bytes())
    }
    pub fn copy_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let bytes: [u8; 10] = memory.read(source, 10)?.try_into().unwrap();
        let mut bytes = canonical_extended(bytes);
        memory.validate_write(destination, 10)?;
        let e = u16::from_be_bytes(bytes[..2].try_into().unwrap()) & 0x7fff;
        let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
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
    pub fn single_to_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let input = u32::from_be_bytes(memory.read(source, 4)?.try_into().unwrap());
        memory.validate_write(destination, 10)?;
        let bytes = self.expand_single(input);
        memory.write(destination, &bytes)
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
    /// Convert using nearest-even and the MC68000 software-SANE integer profile.
    /// Apple Numerics Manual pp.22,197: invalid conversions return the most
    /// negative destination integer, unlike sign-preserving 68881 results.
    pub fn extended_to_integer(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
        bits: u8,
    ) -> Result<()> {
        if ![16, 32].contains(&bits) {
            return Err(Error::Cpu("unsupported SANE integer width".into()));
        }
        let input = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, usize::from(bits / 8))?;
        let mut next = self.clone();
        let rounded = next.round_integral(input, false)?;
        let se = u16::from_be_bytes(rounded[..2].try_into().unwrap());
        let m = u64::from_be_bytes(rounded[2..].try_into().unwrap());
        let exponent = i32::from(se & 0x7fff) - 16383;
        let limit = 1u64 << (bits - 1);
        let negative = se & 0x8000 != 0;
        let magnitude = if m == 0 {
            Some(0)
        } else if exponent >= i32::from(bits) {
            None
        } else {
            m.checked_shr((63 - exponent) as u32)
        };
        let invalid = se & 0x7fff == 0x7fff
            || magnitude.is_none_or(|n| n > limit || (n == limit && !negative));
        let result = if invalid {
            next.invalid = true;
            // Invalid takes precedence over an intermediate rounding's inexact;
            // already-sticky caller state remains set.
            next.inexact = self.inexact;
            -(limit as i64)
        } else {
            let magnitude = magnitude.unwrap();
            if negative {
                -(magnitude as i64)
            } else {
                magnitude as i64
            }
        };
        let bytes = (result as i32).to_be_bytes();
        memory.write(destination, &bytes[4 - usize::from(bits / 8)..])?;
        *self = next;
        Ok(())
    }
    pub fn extended_to_single(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let bytes = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 4)?;
        let result = self.convert_binary(bytes, 23, 8)? as u32;
        memory.write(destination, &result.to_be_bytes())
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
        self.convert_binary(bytes, 52, 11)
    }
    fn convert_binary(
        &mut self,
        bytes: [u8; 10],
        fraction_bits: u32,
        exponent_bits: u32,
    ) -> Result<u64> {
        let bias = (1i32 << (exponent_bits - 1)) - 1;
        let infinity = ((1u64 << exponent_bits) - 1) << fraction_bits;
        let fraction_mask = (1u64 << fraction_bits) - 1;
        let bytes = canonical_extended(bytes);
        let se = u16::from_be_bytes(bytes[..2].try_into().unwrap());
        let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
        let e = se & 0x7fff;
        let sign = u64::from(se & 0x8000) << (fraction_bits + exponent_bits - 15);
        if e == 0x7fff {
            if m == 1 << 63 {
                return Ok(sign | infinity);
            }
            self.invalid |= m & (1 << 62) == 0;
            return Ok(sign
                | (infinity | (1u64 << (fraction_bits - 1)))
                | ((m & 0x7fff_ffff_ffff_ffff) >> (63 - fraction_bits)));
        }
        if m == 0 {
            return Ok(sign);
        }
        let top = 63 - m.leading_zeros() as i32;
        let scale = i32::from(e) - 16446;
        let mut exponent = top + scale;
        let tiny = exponent < 1 - bias;
        let shift = if tiny {
            -scale - (bias + fraction_bits as i32 - 1)
        } else {
            top - fraction_bits as i32
        };
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
        if !tiny && result == 1 << (fraction_bits + 1) {
            result >>= 1;
            exponent += 1;
        }
        if exponent > bias {
            self.inexact = true;
            self.overflow = true;
            return Ok(sign | infinity);
        }
        let biased = if tiny {
            u64::from(result >= 1 << fraction_bits)
        } else {
            (exponent + bias) as u64
        };
        self.underflow |= biased == 0 && lost;
        Ok(sign | (biased << fraction_bits) | (result & fraction_mask))
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
        let (a, b) = (canonical_extended(a), canonical_extended(b));
        let decode = |v: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(v[..2].try_into().unwrap());
            let m = u64::from_be_bytes(v[2..].try_into().unwrap());
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
        let delta = i32::from(ae & 0x7fff) - ashift as i32 - i32::from(be & 0x7fff) + bshift as i32;
        let mut exponent = delta - i32::from(am < bm);
        let tiny = exponent < -16383;
        let shift = if tiny {
            delta + 16446
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
        let biased = if tiny { 0 } else { (exponent + 16383) as u16 };
        self.underflow |= result < 1u128 << 63 && remainder != 0;
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
        let (a, b) = (canonical_extended(a), canonical_extended(b));
        let decode = |bytes: [u8; 10]| -> Result<(u16, u64)> {
            let e = u16::from_be_bytes(bytes[..2].try_into().unwrap());
            let m = u64::from_be_bytes(bytes[2..].try_into().unwrap());
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
        let scale = i32::from(ae & 0x7fff) + i32::from(be & 0x7fff) - 32892;
        let top = 127 - product.leading_zeros() as i32;
        let mut exponent = top + scale;
        let tiny = exponent < -16383;
        let shift = if tiny { -scale - 16446 } else { top - 63 };
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
        let biased = if tiny { 0 } else { (exponent + 16383) as u16 };
        self.underflow |= mantissa < 1u128 << 63 && lost;
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
        let input = canonical_extended(input);
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
            self.underflow |= mantissa >> 63 == 0 && op != Unary::Cos;
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
        if bits != 64 || !(-16382..=16384).contains(&exp) {
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
    fn both_special_integer_bits_have_identical_numeric_meaning_and_flags() {
        let one = value(0x3fff, 1 << 63);
        for sign in [0, 0x8000] {
            for fraction in [0, 1, 0x12345, 1 << 62, (1 << 62) | 0x12345] {
                let raw = value(sign | 0x7fff, fraction);
                let canonical = value(sign | 0x7fff, fraction | 1 << 63);
                for other in [value(0, 0), one, value(0xbfff, 1 << 63), raw, canonical] {
                    for reverse in [false, true] {
                        let (a, b, ca, cb) = if reverse {
                            (other, raw, other, canonical)
                        } else {
                            (raw, other, canonical, other)
                        };
                        for operation in 0..6 {
                            let mut n = Numerics::default();
                            let mut c = Numerics::default();
                            if operation < 4 {
                                let run = |n: &mut Numerics, a, b| match operation {
                                    0 => n.add(a, b, false),
                                    1 => n.add(a, b, true),
                                    2 => n.multiply(a, b),
                                    _ => n.divide(a, b),
                                };
                                assert_eq!(
                                    run(&mut n, a, b).unwrap(),
                                    run(&mut c, ca, cb).unwrap()
                                );
                            } else {
                                assert_eq!(
                                    n.compare_values(a, b, operation == 5).unwrap(),
                                    c.compare_values(ca, cb, operation == 5).unwrap()
                                );
                            }
                            assert_eq!(n.environment_word(), c.environment_word());
                        }
                    }
                }
                for operation in 0..8 {
                    let mut n = Numerics::default();
                    let mut c = Numerics::default();
                    let run = |n: &mut Numerics, input| match operation {
                        0 => n.round_integral(input, false),
                        1 => n.round_integral(input, true),
                        2 => n.square_root(input),
                        3 => n.elementary_value(Unary::Atan, input),
                        4 => n.elementary_value(Unary::Sin, input),
                        5 => n.elementary_value(Unary::Cos, input),
                        6 => n.elementary_value(Unary::Tan, input),
                        _ => n.convert_double(input).map(|v| {
                            let mut b = [0; 10];
                            b[2..].copy_from_slice(&v.to_be_bytes());
                            b
                        }),
                    };
                    assert_eq!(run(&mut n, raw).unwrap(), run(&mut c, canonical).unwrap());
                    assert_eq!(n.environment_word(), c.environment_word());
                }
            }
        }
        let mut n = Numerics::default();
        let infinity = value(0x7fff, 0);
        assert_eq!(
            n.compare_values(infinity, one, false).unwrap(),
            Relation::Greater
        );
        assert_eq!(
            n.compare_values(value(0xffff, 0), one, false).unwrap(),
            Relation::Less
        );
        assert_eq!(
            n.compare_values(infinity, value(0x7fff, 1 << 63), false)
                .unwrap(),
            Relation::Equal
        );
        assert_eq!(n.convert_double(infinity).unwrap(), f64::INFINITY.to_bits());
        assert_eq!(n.environment_word(), 0);
    }
    #[test]
    fn special_classification_and_copy_preserve_sources_and_accept_both_integer_bits() {
        use crate::memory::Permissions;
        let mut memory = Memory::default();
        memory
            .map(GuestPtr(0x1000), vec![0; 32], Permissions::RW)
            .unwrap();
        for sign in [0, 0x8000] {
            for integer in [0, 1 << 63] {
                for (fraction, class, invalid) in [(0, 3, false), (1, 1, true), (1 << 62, 2, false)]
                {
                    let input = value(sign | 0x7fff, integer | fraction);
                    memory.write(GuestPtr(0x1000), &input).unwrap();
                    let mut n = Numerics::default();
                    n.classify_extended(&mut memory, GuestPtr(0x1000), GuestPtr(0x1010))
                        .unwrap();
                    assert_eq!(
                        memory.u16(GuestPtr(0x1010)).unwrap() as i16,
                        if sign == 0 { class } else { -class }
                    );
                    assert_eq!(n.environment_word(), 0);
                    n.copy_extended(&mut memory, GuestPtr(0x1000), GuestPtr(0x1010))
                        .unwrap();
                    assert_eq!(memory.read(GuestPtr(0x1000), 10).unwrap(), input);
                    let mut expected = value(sign | 0x7fff, (1 << 63) | fraction);
                    if invalid {
                        expected[2] |= 0x40;
                    }
                    assert_eq!(memory.read(GuestPtr(0x1010), 10).unwrap(), expected);
                    assert_eq!(n.invalid, invalid);
                }
            }
        }
    }
    #[test]
    fn integral_rounding_uses_even_ties_and_truncation_preserves_signed_zero() {
        let mut n = Numerics::default();
        assert_eq!(
            n.round_integral(value(0xbfff, 0xe000_0000_0000_0000), true)
                .unwrap(),
            value(0xbfff, 1 << 63)
        );
        assert_eq!(
            n.round_integral(value(0xbfff, 0xe000_0000_0000_0000), false)
                .unwrap(),
            value(0xc000, 1 << 63)
        );
        assert_eq!(
            n.round_integral(value(0xbffe, 1 << 63), false).unwrap(),
            value(0x8000, 0)
        );
        assert_eq!(
            n.round_integral(value(0x4000, 0xa000_0000_0000_0000), false)
                .unwrap(),
            value(0x4000, 1 << 63)
        );
        assert_eq!(
            n.round_integral(value(0x4000, 0xe000_0000_0000_0000), false)
                .unwrap(),
            value(0x4001, 1 << 63)
        );
        let mut n = Numerics::default();
        assert_eq!(
            n.round_integral(value(0x403e, u64::MAX), true).unwrap(),
            value(0x403e, u64::MAX)
        );
        assert!(!n.inexact);
        assert_eq!(n.round_integral(value(0, 1), true).unwrap(), value(0, 0));
        assert!(n.inexact && !n.underflow);
    }
    #[test]
    fn square_root_retains_extended_range_exactness_signed_zero_and_invalid_status() {
        let mut n = Numerics::default();
        assert_eq!(
            n.square_root(value(0x4001, 1 << 63)).unwrap(),
            value(0x4000, 1 << 63)
        );
        assert!(!n.inexact);
        assert_eq!(
            n.square_root(value(1, 1 << 63)).unwrap(),
            value(0x2000, 1 << 63)
        );
        assert!(!n.inexact);
        assert_eq!(
            n.square_root(value(0x4000, 1 << 63)).unwrap(),
            value(0x3fff, 0xb504_f333_f9de_6484)
        );
        assert!(n.inexact);
        assert_eq!(
            n.square_root(value(0, 1)).unwrap(),
            value(0x1fe0, 1 << 63) // sqrt(2^-16446) = 2^-8223
        );
        assert_eq!(n.square_root(value(0x8000, 0)).unwrap(), value(0x8000, 0));
        assert_eq!(
            n.square_root(value(0xbfff, 1 << 63)).unwrap(),
            value(0x7fff, 0xc001_0000_0000_0000)
        );
        assert!(n.invalid);
    }
    #[test]
    fn finite_addition_zero_operands_preserve_every_value_and_exception_state() {
        for (exponent, mantissa) in [
            (0x4006, 0xf000_0000_0000_0000), // exact 240 from original EV coordinates
            (0x3fff, 1 << 63),
            (0x7ffe, u64::MAX),
            (0, 1),
            (0, 1 << 63),
        ] {
            for sign in [0, 0x8000] {
                let x = value(exponent | sign, mantissa);
                let negative_x = value((exponent | sign) ^ 0x8000, mantissa);
                for zero_sign in [0, 0x8000] {
                    let zero = value(zero_sign, 0);
                    let mut n = Numerics::default();
                    assert_eq!(n.add(x, zero, false).unwrap(), x, "x + zero");
                    assert_eq!(n.add(zero, x, false).unwrap(), x, "zero + x");
                    assert_eq!(n.add(x, zero, true).unwrap(), x, "x - zero");
                    assert_eq!(n.add(zero, x, true).unwrap(), negative_x, "zero - x");
                    assert_eq!(n.environment_word(), 0);
                }
            }
        }
    }
    #[test]
    fn addition_and_subtraction_keep_cancellation_and_single_rounding_exact() {
        let one = value(0x3fff, 1 << 63);
        let mut n = Numerics::default();
        assert_eq!(n.add(one, one, false).unwrap(), value(0x4000, 1 << 63));
        assert_eq!(
            n.add(one, value(0x3ffe, u64::MAX), true).unwrap(),
            value(0x3fbf, 1 << 63)
        );
        assert_eq!(
            n.add(value(1, 1 << 63), value(0, 0x7fff_ffff_ffff_ffff), true)
                .unwrap(),
            value(0, (1 << 63) + 1)
        );
        assert!(!n.inexact);
        assert_eq!(n.add(one, value(0x3fbf, 1 << 63), false).unwrap(), one);
        assert!(n.inexact);
        assert_eq!(
            n.add(one, value(0x3fc0, 0xc000_0000_0000_0000), false)
                .unwrap(),
            value(0x3fff, 0x8000_0000_0000_0002)
        );
        let mut n = Numerics::default();
        let max = value(0x7ffe, u64::MAX);
        assert_eq!(n.add(max, value(0, 1), false).unwrap(), max);
        assert!(n.inexact && !n.overflow);
        assert_eq!(n.add(max, max, false).unwrap(), value(0x7fff, 1 << 63));
        assert!(n.overflow);
        assert_eq!(
            n.add(value(0x8000, 0), value(0x8000, 0), false).unwrap(),
            value(0x8000, 0)
        );
        assert_eq!(n.add(one, one, true).unwrap(), value(0, 0));
        assert_eq!(
            n.add(value(0x7fff, 1 << 63), value(0x7fff, 1 << 63), true)
                .unwrap(),
            value(0x7fff, 0xc002_0000_0000_0000)
        );
        assert!(n.invalid);
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
        assert_eq!(n.divide(value(1, 1 << 63), two).unwrap(), value(0, 1 << 63));
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
            value(0, 1 << 63)
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
            value(0, 0x8000_0000_0000_0000)
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
    #[test]
    fn finite_encodings_follow_table_e5_and_normalize_without_rounding() {
        for sign in [0, 0x8000] {
            for (raw_e, raw_m, e, m) in [
                (0x4000u16, 1u64 << 62, 0x3fff, 1u64 << 63),
                (1, 1, 0, 2),
                (0, 1, 0, 1),
                (0, 1 << 63, 0, 1 << 63),
                (0x2000, 0, 0, 0),
                (0x7ffe, 1, 0x7fbf, 1 << 63),
                (0x1ff9, 0x1006_49b8_b82a_1010, 0x1ff6, 0x8032_4dc5_c150_8080),
            ] {
                let raw = value(sign | raw_e, raw_m);
                let canonical = value(sign | e, m);
                assert_eq!(canonical_extended(raw), canonical);
                for other in [value(0x3fff, 1 << 63), value(0, 1), value(0, 1 << 63)] {
                    for op in 0..6 {
                        let mut n = Numerics::default();
                        let mut c = Numerics::default();
                        let run = |n: &mut Numerics, a, b| match op {
                            0 => n.add(a, b, false),
                            1 => n.add(a, b, true),
                            2 => n.multiply(a, b),
                            3 => n.divide(a, b),
                            4 => n.square_root(a),
                            _ => n.round_integral(a, false),
                        };
                        assert_eq!(
                            run(&mut n, raw, other).unwrap(),
                            run(&mut c, canonical, other).unwrap()
                        );
                        assert_eq!(n.environment_word(), c.environment_word());
                    }
                }
            }
        }
    }
    #[test]
    fn exponent_zero_has_its_own_value_and_normal_boundary() {
        let mut n = Numerics::default();
        let smallest = value(0, 1);
        let normal = value(0, 1 << 63);
        assert_eq!(
            n.add(normal, value(0, 0x7fff_ffff_ffff_ffff), true)
                .unwrap(),
            smallest
        );
        assert_eq!(
            n.multiply(normal, value(0x3ffe, 1 << 63)).unwrap(),
            value(0, 1 << 62)
        );
        assert_eq!(
            n.divide(normal, value(0x4000, 1 << 63)).unwrap(),
            value(0, 1 << 62)
        );
        assert_eq!(n.square_root(smallest).unwrap(), value(0x1fe0, 1 << 63));
        assert_eq!(n.environment_word(), 0);
        let mut normal_n = Numerics::default();
        normal_n.elementary_value(Unary::Atan, normal).unwrap();
        assert!(normal_n.inexact && !normal_n.underflow);
    }
}
