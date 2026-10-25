//! MathLib's fp.h functions beyond the basic transcendental set.
//!
//! PowerPC Numerics (1994), Appendix C (fp.h). Doubles arrive in f1 and f2
//! and also occupy two GPR slots each under the PowerPC CFM calling
//! convention, so a pointer or integer after one double is in r5, and after
//! two doubles in r7. Results return in f1, or in r3 for the `long` forms.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcFpFunction {
    Unary(fn(f64) -> f64),
    Binary(fn(f64, f64) -> f64),
    Frexp,
    Remquo,
    Scalb,
    Logb,
    RintToLong,
    RoundToLong,
}

impl PpcFpFunction {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "acosh" => Self::Unary(libm::acosh),
            "asinh" => Self::Unary(libm::asinh),
            "atanh" => Self::Unary(libm::atanh),
            "cosh" => Self::Unary(libm::cosh),
            "sinh" => Self::Unary(libm::sinh),
            "tanh" => Self::Unary(libm::tanh),
            "erf" => Self::Unary(libm::erf),
            "erfc" => Self::Unary(libm::erfc),
            "exp2" => Self::Unary(libm::exp2),
            "expm1" => Self::Unary(libm::expm1),
            "log1p" => Self::Unary(libm::log1p),
            "log2" => Self::Unary(libm::log2),
            // fp.h gamma is the gamma function itself, not its logarithm.
            "gamma" => Self::Unary(libm::tgamma),
            "lgamma" => Self::Unary(libm::lgamma),
            // nearbyint rounds in the current (round-to-nearest) mode
            // without raising inexact, which rint's result matches.
            "nearbyint" => Self::Unary(libm::rint),
            "trunc" => Self::Unary(libm::trunc),
            "copysign" => Self::Binary(libm::copysign),
            "fdim" => Self::Binary(libm::fdim),
            "fmax" => Self::Binary(libm::fmax),
            "fmin" => Self::Binary(libm::fmin),
            "remainder" => Self::Binary(libm::remainder),
            "nextafterd" => Self::Binary(libm::nextafter),
            "frexp" => Self::Frexp,
            "remquo" => Self::Remquo,
            "scalb" => Self::Scalb,
            "logb" => Self::Logb,
            "rinttol" => Self::RintToLong,
            "roundtol" => Self::RoundToLong,
            _ => return None,
        })
    }
}

fn logb(x: f64) -> f64 {
    if x.is_nan() {
        x
    } else if x == 0.0 {
        f64::NEG_INFINITY
    } else if x.is_infinite() {
        f64::INFINITY
    } else {
        f64::from(libm::ilogb(x))
    }
}

/// Convert to `long`, saturating out-of-range values as the PowerPC
/// fctiw instruction does.
fn to_long(x: f64) -> u32 {
    if x.is_nan() {
        i32::MIN as u32
    } else {
        (x.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32) as u32
    }
}

pub(crate) fn ppc_dispatch_fp(
    function: PpcFpFunction,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    let x = f64::from_bits(cpu.fpr[1]);
    let y = f64::from_bits(cpu.fpr[2]);
    let result = match function {
        PpcFpFunction::Unary(f) => f(x),
        PpcFpFunction::Binary(f) => f(x, y),
        PpcFpFunction::Frexp => {
            // double frexp(double x, int *exponent).
            let (mantissa, exponent) = libm::frexp(x);
            if cpu.gpr[5] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[5], exponent as u32);
            }
            mantissa
        }
        PpcFpFunction::Remquo => {
            // double remquo(double x, double y, int *quo).
            let (remainder, quotient) = libm::remquo(x, y);
            if cpu.gpr[7] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[7], quotient as u32);
            }
            remainder
        }
        // double scalb(double x, long n).
        PpcFpFunction::Scalb => libm::scalbn(x, cpu.gpr[5] as i32),
        PpcFpFunction::Logb => logb(x),
        PpcFpFunction::RintToLong => return PpcImportAction::Return(to_long(libm::rint(x))),
        PpcFpFunction::RoundToLong => return PpcImportAction::Return(to_long(libm::round(x))),
    };
    cpu.fpr[1] = result.to_bits();
    PpcImportAction::ReturnPreserve
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, x: f64, y: f64, cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> f64 {
        cpu.fpr[1] = x.to_bits();
        cpu.fpr[2] = y.to_bits();
        let action = ppc_dispatch_fp(PpcFpFunction::from_name(name).unwrap(), cpu, memory);
        assert!(matches!(action, PpcImportAction::ReturnPreserve), "{name}");
        f64::from_bits(cpu.fpr[1])
    }

    #[test]
    fn fp_functions_follow_their_c99_definitions() {
        let mut cpu = PpcCpu::default();
        let mut memory = PpcSectionMem::default();
        memory.add_region(0x1000, vec![0; 16]);
        assert!((call("gamma", 5.0, 0.0, &mut cpu, &mut memory) - 24.0).abs() < 1e-9);
        assert!((call("log2", 8.0, 0.0, &mut cpu, &mut memory) - 3.0).abs() < 1e-12);
        assert_eq!(call("copysign", 2.0, -0.0, &mut cpu, &mut memory), -2.0);
        assert_eq!(call("fdim", 2.0, 5.0, &mut cpu, &mut memory), 0.0);
        assert_eq!(call("trunc", -2.7, 0.0, &mut cpu, &mut memory), -2.0);
        assert_eq!(call("logb", 10.0, 0.0, &mut cpu, &mut memory), 3.0);
        assert_eq!(
            call("logb", 0.0, 0.0, &mut cpu, &mut memory),
            f64::NEG_INFINITY
        );

        cpu.gpr[5] = 0x1000;
        assert_eq!(call("frexp", 12.0, 0.0, &mut cpu, &mut memory), 0.75);
        assert_eq!(memory.read_u32_be(0x1000), Some(4));
        cpu.gpr[5] = 3;
        assert_eq!(call("scalb", 1.5, 0.0, &mut cpu, &mut memory), 12.0);
        cpu.gpr[7] = 0x1004;
        assert_eq!(call("remquo", 7.0, 2.0, &mut cpu, &mut memory), -1.0);
        assert_eq!(memory.read_u32_be(0x1004), Some(4));

        cpu.fpr[1] = 2.5f64.to_bits();
        let action = ppc_dispatch_fp(
            PpcFpFunction::from_name("roundtol").unwrap(),
            &mut cpu,
            &mut memory,
        );
        assert!(matches!(action, PpcImportAction::Return(3)));
        let action = ppc_dispatch_fp(
            PpcFpFunction::from_name("rinttol").unwrap(),
            &mut cpu,
            &mut memory,
        );
        assert!(matches!(action, PpcImportAction::Return(2)));
    }

    #[test]
    fn fp_functions_resolve_from_mathlib_and_carbonlib() {
        for library in ["MathLib", "CarbonLib"] {
            assert!(matches!(
                dispatcher_target_for_import(library, "lgamma"),
                PpcImportDispatcherTarget::MathFp(_)
            ));
        }
        assert_eq!(
            dispatcher_target_for_import("MathLib", "sin"),
            PpcImportDispatcherTarget::MathSin
        );
    }
}
