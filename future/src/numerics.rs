//! Shared software numeric services. Apple Numerics Manual, 2nd ed., ch. 24.
//! Initial SANE slice: default round-to-nearest extended arctangent only.
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
use astro_float_num::{BigFloat, Consts, RoundingMode, Sign};
#[derive(Default)]
pub struct Numerics {
    /// Sticky mathematical exceptions, retained for future SANE environment APIs.
    pub inexact: bool,
    pub underflow: bool,
    pub invalid: bool,
}
impl Numerics {
    pub fn atan_extended(&mut self, memory: &mut Memory, pointer: GuestPtr) -> Result<()> {
        let input: [u8; 10] = memory.read(pointer, 10)?.try_into().unwrap();
        memory.validate_write(pointer, 10)?;
        let result = self.atan(input)?;
        memory.write(pointer, &result)
    }
    fn atan(&mut self, input: [u8; 10]) -> Result<[u8; 10]> {
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
            return Ok(input);
        }
        if (exponent != 0 && mantissa >> 63 == 0) || (exponent == 0 && mantissa >> 63 != 0) {
            return Err(Error::Cpu("unsupported unnormal SANE operand".into()));
        }
        if exponent == 0x7fff {
            if mantissa != 0x8000_0000_0000_0000 {
                self.invalid |= mantissa & 0x4000_0000_0000_0000 == 0;
                return Ok(encode(exponent, mantissa | 0x4000_0000_0000_0000));
            }
            self.inexact = true;
            return Ok(encode(0x3fff, 0xc90f_daa2_2168_c235));
        }
        // atan(x) differs from x by less than half an extended ULP here.
        // Preserve even the smallest subnormal without narrowing its exponent.
        if exponent < 0x3fde {
            self.inexact = true;
            self.underflow |= exponent == 0;
            return Ok(input);
        }
        #[cfg(target_pointer_width = "64")]
        let words = [mantissa];
        #[cfg(target_pointer_width = "32")]
        let words = [mantissa as u32, (mantissa >> 32) as u32];
        let x = BigFloat::from_words(&words, sign, i32::from(exponent) - 16382);
        let mut constants =
            Consts::new().map_err(|e| Error::Cpu(format!("numeric constants: {e:?}")))?;
        let result = x.atan(64, RoundingMode::ToEven, &mut constants);
        let Some((words, bits, _, exp, _)) = result.as_raw_parts() else {
            return Err(Error::Cpu("software arctangent evaluation failed".into()));
        };
        if bits != 64 || !(-16381..=1).contains(&exp) {
            return Err(Error::Cpu(
                "unexpected software arctangent representation".into(),
            ));
        }
        #[cfg(target_pointer_width = "64")]
        let mantissa = words[0];
        #[cfg(target_pointer_width = "32")]
        let mantissa = u64::from(words[0]) | (u64::from(words[1]) << 32);
        self.inexact = true;
        Ok(encode((exp + 16382) as u16, mantissa))
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
    fn extended_arctangent_retains_low_significand_bits_and_special_values() {
        let mut n = Numerics::default();
        assert_eq!(
            n.atan(value(0x3fff, 0x8000_0000_0000_0000)).unwrap(),
            value(0x3ffe, 0xc90f_daa2_2168_c235)
        );
        assert_eq!(
            n.atan(value(0x3fff, 0x8000_0000_0000_0001)).unwrap(),
            value(0x3ffe, 0xc90f_daa2_2168_c236)
        );
        assert_eq!(
            n.atan(value(0xbfff, 0x8000_0000_0000_0000)).unwrap(),
            value(0xbffe, 0xc90f_daa2_2168_c235)
        );
        for sign in [0, 0x8000] {
            assert_eq!(n.atan(value(sign, 0)).unwrap(), value(sign, 0));
            assert_eq!(n.atan(value(sign, 1)).unwrap(), value(sign, 1));
            assert_eq!(
                n.atan(value(sign | 0x7fff, 0x8000_0000_0000_0000)).unwrap(),
                value(sign | 0x3fff, 0xc90f_daa2_2168_c235)
            );
        }
        assert!(n.underflow && n.inexact);
        assert_eq!(
            n.atan(value(0x7fff, 0x8000_0000_0000_0001)).unwrap(),
            value(0x7fff, 0xc000_0000_0000_0001)
        );
        assert!(n.invalid);
    }
}
