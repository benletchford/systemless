//! SANE general exponentiation; Apple Numerics Manual ch.8/table5-1/app.E.
use super::*;
fn encode(se: u16, m: u64) -> [u8; 10] {
    let mut b = [0; 10];
    b[..2].copy_from_slice(&se.to_be_bytes());
    b[2..].copy_from_slice(&m.to_be_bytes());
    b
}
fn parts(b: [u8; 10]) -> (u16, u64) {
    (
        u16::from_be_bytes(b[..2].try_into().unwrap()),
        u64::from_be_bytes(b[2..].try_into().unwrap()),
    )
}
fn integral(e: u16, m: u64) -> (bool, bool) {
    if m == 0 {
        return (true, false);
    }
    let k = i32::from(e & 0x7fff) - 16383;
    if k < 0 {
        return (false, false);
    }
    if k > 63 {
        return (true, false);
    }
    let shift = 63 - k as u32;
    let mask = if shift == 0 { 0 } else { (1u64 << shift) - 1 };
    (m & mask == 0, (m >> shift) & 1 != 0)
}
fn big(e: u16, m: u64) -> BigFloat {
    #[cfg(target_pointer_width = "64")]
    let words = [m];
    #[cfg(target_pointer_width = "32")]
    let words = [m as u32, (m >> 32) as u32];
    BigFloat::from_words(
        &words,
        if e & 0x8000 == 0 {
            Sign::Pos
        } else {
            Sign::Neg
        },
        i32::from(e & 0x7fff) - 16382,
    )
}
impl Numerics {
    pub fn power_extended(
        &mut self,
        memory: &mut Memory,
        source: GuestPtr,
        destination: GuestPtr,
    ) -> Result<()> {
        let a = memory.read(destination, 10)?.try_into().unwrap();
        let b = memory.read(source, 10)?.try_into().unwrap();
        memory.validate_write(destination, 10)?;
        let mut next = self.clone();
        let result = next.power(a, b)?;
        memory.write(destination, &result)?;
        *self = next;
        Ok(())
    }
    fn power(&mut self, a: [u8; 10], b: [u8; 10]) -> Result<[u8; 10]> {
        let (ae, am) = parts(canonical_extended(a));
        let (be, bm) = parts(canonical_extended(b));
        let an = ae & 0x7fff == 0x7fff && am != 1 << 63;
        let bn = be & 0x7fff == 0x7fff && bm != 1 << 63;
        if an || bn {
            self.invalid |= (an && am & (1 << 62) == 0) || (bn && bm & (1 << 62) == 0);
            let (e, m) = if an { (ae, am) } else { (be, bm) };
            return Ok(encode(e, m | 1 << 62));
        }
        let ai = ae & 0x7fff == 0x7fff;
        let bi = be & 0x7fff == 0x7fff;
        let az = !ai && am == 0;
        let bz = !bi && bm == 0;
        let (integer, odd) = if bi { (false, false) } else { integral(be, bm) };
        let one = (ae & 0x7fff, am) == (0x3fff, 1 << 63);
        if (bz && (az || ai)) || (one && bi) || (ae & 0x8000 != 0 && !integer) {
            self.invalid = true;
            return Ok(encode(0x7fff, 0xc025000000000000));
        }
        if bz {
            return Ok(encode(0x3fff, 1 << 63));
        }
        let sign = if ae & 0x8000 != 0 && odd { 0x8000 } else { 0 };
        if az {
            if be & 0x8000 != 0 {
                self.divide_by_zero = true;
                return Ok(encode(sign | 0x7fff, 1 << 63));
            }
            return Ok(encode(sign, 0));
        }
        if ai {
            return Ok(if be & 0x8000 == 0 {
                encode(sign | 0x7fff, 1 << 63)
            } else {
                encode(sign, 0)
            });
        }
        if bi {
            let large = (ae & 0x7fff, am) > (0x3fff, 1 << 63);
            return Ok(if large == (be & 0x8000 == 0) {
                encode(0x7fff, 1 << 63)
            } else {
                encode(0, 0)
            });
        }
        if one {
            return Ok(encode(sign | 0x3fff, 1 << 63));
        }
        let x = big(ae, am);
        let y = big(be, bm);
        let exact_power = if integer && be & 0x7fff <= 0x400e {
            let k = i32::from(be & 0x7fff) - 16383;
            Some((x.clone(), (bm >> (63 - k)) as usize))
        } else if !integer {
            let zeros = bm.trailing_zeros();
            let denominator = 63 - (i32::from(be & 0x7fff) - 16383) - zeros as i32;
            let numerator = bm >> zeros;
            if (1..=16).contains(&denominator) && numerator <= u64::from(u16::MAX) {
                let mut root = x.clone();
                let mut exact = true;
                for _ in 0..denominator {
                    let next = root.sqrt(128, RoundingMode::ToEven);
                    if next.mul_full_prec(&next).cmp(&root) != Some(0) {
                        exact = false;
                        break;
                    }
                    root = next;
                }
                exact.then_some((root, numerator as usize))
            } else {
                None
            }
        } else {
            None
        };
        let mut constants =
            Consts::new().map_err(|e| Error::Cpu(format!("power constants: {e:?}")))?;
        // Directed results enclose the mathematical value. Accept only when both
        // endpoints round to the same guest value, including gradual underflow.
        for precision in [128, 256, 512, 1024, 2048, 4096] {
            let (mut lo, mut hi) = if let Some((base, n)) = &exact_power {
                let down = base.powi(*n, precision, RoundingMode::Down);
                let up = base.powi(*n, precision, RoundingMode::Up);
                if be & 0x8000 == 0 {
                    (down, up)
                } else {
                    let one = BigFloat::from_u8(1, precision);
                    (
                        one.div(&up, precision, RoundingMode::Down),
                        one.div(&down, precision, RoundingMode::Up),
                    )
                }
            } else {
                let mut magnitude = x.clone();
                magnitude.set_sign(Sign::Pos);
                let lower = magnitude.ln(precision, RoundingMode::Down, &mut constants);
                let upper = magnitude.ln(precision, RoundingMode::Up, &mut constants);
                let (a, b) = if be & 0x8000 == 0 {
                    (lower, upper)
                } else {
                    (upper, lower)
                };
                let lower = a.mul(&y, precision, RoundingMode::Down);
                let upper = b.mul(&y, precision, RoundingMode::Up);
                let lo = lower.exp(precision, RoundingMode::Down, &mut constants);
                let hi = upper.exp(precision, RoundingMode::Up, &mut constants);
                (lo, hi)
            };
            if exact_power.is_none() && sign != 0 {
                lo.inv_sign();
                hi.inv_sign();
                std::mem::swap(&mut lo, &mut hi);
            }
            let mut low = Numerics::default();
            let mut high = Numerics::default();
            let a = low.pack_big(&lo)?;
            let b = high.pack_big(&hi)?;
            if a == b {
                low.inexact |= hi.inexact() || lo.inexact() || high.inexact;
                // Finite nonzero inputs cannot produce an exact mathematical zero.
                if parts(a).1 == 0 {
                    low.inexact = true;
                }
                low.underflow |= parts(a).1 < 1 << 63 && low.inexact;
                self.inexact |= low.inexact;
                self.underflow |= low.underflow;
                self.overflow |= low.overflow || high.overflow;
                return Ok(a);
            }
        }
        Err(Error::Cpu(
            "general power rounding unresolved at supported precision".into(),
        ))
    }
}
