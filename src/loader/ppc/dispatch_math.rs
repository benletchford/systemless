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
        _ => None,
    }
}
