//! Shared scalar math import dispatch for the main and hot PowerPC paths.

use super::*;

pub(super) fn dispatch_math_import(
    target: &PpcImportDispatcherTarget,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> Option<PpcImportAction> {
    match target {
        PpcImportDispatcherTarget::MathCeil => {
            ppc_math_ceil(cpu);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathSqrt => {
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.sqrt().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathExp => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-18--10-19:
            // exp returns e raised to the power of x.
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.exp().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathSin => {
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.sin().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathCos => {
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.cos().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathAsin => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-34--10-35:
            // asin returns the arc sine in radians through the floating-point
            // result register used by the PowerPC calling convention.
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.asin().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathTan => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-32--10-33.
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.tan().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathAtan => {
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.atan().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathAtan2 => {
            let y = f64::from_bits(cpu.fpr[1]);
            let x = f64::from_bits(cpu.fpr[2]);
            cpu.fpr[1] = y.atan2(x).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathPow => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-14--10-16.
            let base = f64::from_bits(cpu.fpr[1]);
            let exponent = f64::from_bits(cpu.fpr[2]);
            cpu.fpr[1] = base.powf(exponent).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathFmod => {
            ppc_math_fmod(cpu);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathLog => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-22--10-23.
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.ln().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathLog10 => {
            // Inside Macintosh: PowerPC Numerics (1994), pp. 10-23–10-24:
            // `double_t log10(double_t x)` returns the common logarithm of x.
            let value = f64::from_bits(cpu.fpr[1]);
            cpu.fpr[1] = value.log10().to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MathDtox80 => {
            ppc_math_dtox80(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::X2Fix => {
            let value = f64::from_bits(cpu.fpr[1]);
            Some(PpcImportAction::Return(ppc_f64_to_fixed(value)))
        }
        PpcImportDispatcherTarget::FixRatio => Some(PpcImportAction::Return(ppc_fix_ratio(
            cpu.gpr[3] as u16 as i16,
            cpu.gpr[4] as u16 as i16,
        ) as u32)),
        PpcImportDispatcherTarget::FixMul => Some(PpcImportAction::Return(ppc_fix_mul(
            cpu.gpr[3] as i32,
            cpu.gpr[4] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FixDiv => Some(PpcImportAction::Return(ppc_fix_div(
            cpu.gpr[3] as i32,
            cpu.gpr[4] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::Long2Fix => Some(PpcImportAction::Return(ppc_long_to_fix(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::Fix2Long => Some(PpcImportAction::Return(ppc_fix_to_long(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FixRound => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fix_round(cpu.gpr[3] as i32),
        ))),
        PpcImportDispatcherTarget::Fix2Frac => Some(PpcImportAction::Return(ppc_fix_to_frac(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::Frac2Fix => Some(PpcImportAction::Return(ppc_frac_to_fix(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::Frac2X => {
            let frac = cpu.gpr[3] as i32;
            cpu.fpr[1] = (f64::from(frac) / 1_073_741_824.0).to_bits();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::X2Frac => {
            let value = f64::from_bits(cpu.fpr[1]);
            Some(PpcImportAction::Return(ppc_f64_to_frac(value)))
        }
        PpcImportDispatcherTarget::FracSin => Some(PpcImportAction::Return(ppc_frac_sin(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FracCos => Some(PpcImportAction::Return(ppc_frac_cos(
            cpu.gpr[3] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FracSqrt => {
            Some(PpcImportAction::Return(ppc_frac_sqrt(cpu.gpr[3])))
        }
        PpcImportDispatcherTarget::FracMul => Some(PpcImportAction::Return(ppc_frac_mul(
            cpu.gpr[3] as i32,
            cpu.gpr[4] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FracDiv => Some(PpcImportAction::Return(ppc_frac_div(
            cpu.gpr[3] as i32,
            cpu.gpr[4] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::FixATan2 => Some(PpcImportAction::Return(ppc_fix_atan2(
            cpu.gpr[3] as i32,
            cpu.gpr[4] as i32,
        ) as u32)),
        PpcImportDispatcherTarget::WideAdd => Some(PpcImportAction::Return(ppc_wide_add(
            memory, cpu.gpr[3], cpu.gpr[4],
        ))),
        PpcImportDispatcherTarget::WideSubtract => Some(PpcImportAction::Return(
            ppc_wide_subtract(memory, cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::WideNegate => {
            Some(PpcImportAction::Return(ppc_wide_negate(memory, cpu.gpr[3])))
        }
        PpcImportDispatcherTarget::WideShift => Some(PpcImportAction::Return(ppc_wide_shift(
            memory,
            cpu.gpr[3],
            cpu.gpr[4] as i32,
        ))),
        PpcImportDispatcherTarget::WideMultiply => Some(PpcImportAction::Return(
            ppc_wide_multiply(memory, cpu.gpr[3] as i32, cpu.gpr[4] as i32, cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::WideDivide => Some(PpcImportAction::Return(ppc_wide_divide(
            memory,
            cpu.gpr[3],
            cpu.gpr[4] as i32,
            cpu.gpr[5],
        ) as u32)),
        PpcImportDispatcherTarget::WideWideDivide => Some(PpcImportAction::Return(
            ppc_wide_wide_divide(memory, cpu.gpr[3], cpu.gpr[4] as i32, cpu.gpr[5]),
        )),
        PpcImportDispatcherTarget::WideCompare => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_wide_compare(memory, cpu.gpr[3], cpu.gpr[4]),
        ))),
        PpcImportDispatcherTarget::WideSquareRoot => Some(PpcImportAction::Return(
            ppc_wide_square_root(memory, cpu.gpr[3]),
        )),
        _ => None,
    }
}
