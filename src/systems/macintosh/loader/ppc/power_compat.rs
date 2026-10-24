//! POWER instructions retained by the PowerPC 601.
//!
//! Code built for the 601 may use the POWER multiply, divide, difference-or-
//! zero, and absolute-value instructions and the MQ register. Later PowerPC
//! processors raise an illegal-instruction program exception for them, and
//! Mac OS emulates them in that exception handler so such code still runs.
//! The runtime does the same when the interpreter reports one of these
//! encodings as unimplemented. Semantics follow the PowerPC 601 RISC
//! Microprocessor User's Manual, chapter 10 (POWER instructions).

use super::*;

const XER_SO: u32 = 0x8000_0000;
const XER_OV: u32 = 0x4000_0000;
/// MQ is SPR 0 on the 601.
const SPR_MQ: u32 = 0;

/// Execute the POWER instruction at `pc` and step past it. Returns false,
/// leaving the CPU untouched, for any other encoding.
pub(crate) fn ppc_emulate_power_instruction(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    mq: &mut u32,
    pc: u32,
) -> bool {
    let Some(word) = memory.read_u32_be(pc) else {
        return false;
    };
    let primary = word >> 26;
    let rd = ((word >> 21) & 0x1F) as usize;
    let ra = ((word >> 16) & 0x1F) as usize;
    let rb = ((word >> 11) & 0x1F) as usize;
    let oe = word & 0x400 != 0;
    let rc = word & 1 != 0;
    let (a, b) = (cpu.gpr[ra] as i32, cpu.gpr[rb] as i32);

    // (result, overflow) for the arithmetic forms; None for MQ moves.
    let outcome: Option<(u32, bool)> = match (primary, (word >> 1) & 0x1FF) {
        // dozi rD,rA,SIMM: difference or zero against an immediate.
        (9, _) => {
            let immediate = i32::from(word as u16 as i16);
            Some(difference_or_zero(a, immediate))
        }
        // mul rD,rA,rB: high product word to rD, low word to MQ.
        (31, 107) => {
            let product = i64::from(a) * i64::from(b);
            *mq = product as u32;
            let overflow = product != i64::from(product as i32);
            Some(((product >> 32) as u32, overflow))
        }
        // div rD,rA,rB: (rA || MQ) / rB, remainder to MQ.
        (31, 331) => {
            let dividend = (i64::from(a) << 32) | i64::from(*mq);
            Some(divide(dividend, b, mq))
        }
        // divs rD,rA,rB: rA / rB, remainder to MQ.
        (31, 363) => Some(divide(i64::from(a), b, mq)),
        // doz rD,rA,rB.
        (31, 264) => Some(difference_or_zero(a, b)),
        // abs rD,rA and nabs rD,rA.
        (31, 360) => Some((a.wrapping_abs() as u32, a == i32::MIN)),
        (31, 488) => Some(((a.unsigned_abs() as i32).wrapping_neg() as u32, false)),
        _ => None,
    };
    if let Some((result, overflow)) = outcome {
        cpu.gpr[rd] = result;
        // dozi has no OE or Rc fields: those bits belong to its immediate.
        if primary == 31 && oe {
            cpu.xer = if overflow {
                cpu.xer | XER_OV | XER_SO
            } else {
                cpu.xer & !XER_OV
            };
        }
        if primary == 31 && rc {
            let low = if (word >> 1) & 0x1FF == 107 {
                *mq
            } else {
                result
            };
            set_cr0(cpu, low as i32);
        }
        cpu.pc = pc.wrapping_add(4);
        return true;
    }

    // mfspr rD,MQ and mtspr MQ,rS (10-bit XO 339 and 467, SPR 0).
    let spr = ((word >> 16) & 0x1F) | (((word >> 11) & 0x1F) << 5);
    match (primary, (word >> 1) & 0x3FF, spr) {
        (31, 339, SPR_MQ) => cpu.gpr[rd] = *mq,
        (31, 467, SPR_MQ) => *mq = cpu.gpr[rd],
        _ => return false,
    }
    cpu.pc = pc.wrapping_add(4);
    true
}

fn difference_or_zero(a: i32, b: i32) -> (u32, bool) {
    if a > b {
        (0, false)
    } else {
        let difference = i64::from(b) - i64::from(a);
        (difference as u32, difference > i64::from(i32::MAX))
    }
}

/// Signed 64-by-32 division: the quotient, with the remainder left in MQ.
/// Division by zero or a quotient that does not fit sets overflow.
fn divide(dividend: i64, divisor: i32, mq: &mut u32) -> (u32, bool) {
    if divisor == 0 {
        return (0, true);
    }
    let quotient = dividend.wrapping_div(i64::from(divisor));
    *mq = dividend.wrapping_rem(i64::from(divisor)) as u32;
    (quotient as u32, quotient != i64::from(quotient as i32))
}

fn set_cr0(cpu: &mut PpcCpu, value: i32) {
    let field = match value.cmp(&0) {
        std::cmp::Ordering::Less => 0x8,
        std::cmp::Ordering::Greater => 0x4,
        std::cmp::Ordering::Equal => 0x2,
    } | u32::from(cpu.xer & XER_SO != 0);
    cpu.cr = (cpu.cr & 0x0FFF_FFFF) | (field << 28);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(word: u32, cpu: &mut PpcCpu, mq: &mut u32) -> bool {
        let mut memory = PpcSectionMem::default();
        memory.add_region(0x1000, word.to_be_bytes().to_vec());
        ppc_emulate_power_instruction(cpu, &mut memory, mq, 0x1000)
    }

    fn xo(rd: u32, ra: u32, rb: u32, xo: u32) -> u32 {
        (31 << 26) | (rd << 21) | (ra << 16) | (rb << 11) | (xo << 1)
    }

    #[test]
    fn mul_splits_the_product_between_rd_and_mq() {
        let mut cpu = PpcCpu::default();
        let mut mq = 0;
        cpu.gpr[3] = 0x0001_0000;
        cpu.gpr[4] = 0x0003_0005;
        // 0x7CC320D6: mul r6,r3,r4, as in the Warcraft II demo.
        assert!(run(0x7CC3_20D6, &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6], 0x0000_0003);
        assert_eq!(mq, 0x0005_0000);
        assert_eq!(cpu.pc, 0x1004);
    }

    #[test]
    fn divide_forms_leave_the_remainder_in_mq() {
        let mut cpu = PpcCpu::default();
        let mut mq = 0;
        cpu.gpr[3] = (-17i32) as u32;
        cpu.gpr[4] = 5;
        assert!(run(xo(6, 3, 4, 363), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6] as i32, -3);
        assert_eq!(mq as i32, -2);

        cpu.gpr[3] = 1;
        mq = 0;
        assert!(run(xo(6, 3, 4, 331), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6], ((1u64 << 32) / 5) as u32);
        assert_eq!(mq, ((1u64 << 32) % 5) as u32);
    }

    #[test]
    fn doz_abs_and_mq_moves_follow_the_601_definitions() {
        let mut cpu = PpcCpu::default();
        let mut mq = 0;
        cpu.gpr[3] = 3;
        cpu.gpr[4] = 10;
        assert!(run(xo(6, 3, 4, 264), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6], 7);
        assert!(run(xo(6, 4, 3, 264), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6], 0);
        // dozi r6,r3,1 with r3 = 3: 3 > 1, so zero.
        assert!(run(
            (9 << 26) | (6 << 21) | (3 << 16) | 1,
            &mut cpu,
            &mut mq
        ));
        assert_eq!(cpu.gpr[6], 0);

        cpu.gpr[3] = (-9i32) as u32;
        assert!(run(xo(6, 3, 0, 360), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6], 9);
        assert!(run(xo(6, 3, 0, 488) | 1, &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[6] as i32, -9);
        assert_eq!(cpu.cr >> 28, 0x8);

        cpu.gpr[7] = 0x1234;
        // mtspr MQ,r7 then mfspr r8,MQ.
        assert!(run((31 << 26) | (7 << 21) | (467 << 1), &mut cpu, &mut mq));
        assert!(run((31 << 26) | (8 << 21) | (339 << 1), &mut cpu, &mut mq));
        assert_eq!(cpu.gpr[8], 0x1234);
        // mfspr r8,LR is not a POWER-only form.
        assert!(!run(
            (31 << 26) | (8 << 21) | (8 << 16) | (339 << 1),
            &mut cpu,
            &mut mq
        ));
    }
}
