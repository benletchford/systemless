//! PowerPC CFM argument decoding for C entry points.
//!
//! The classic Mac OS PowerPC calling convention assigns each argument to
//! consecutive words of a parameter area beginning 24 bytes above the caller's
//! stack pointer. The first eight words travel in r3-r10. Floating-point
//! arguments also travel in f1-f13 and still consume their parameter words
//! (one for `float`, two for `double`), so an integer following a float lands
//! in the next general register after the float's shadow slot. Values beyond
//! the registers are read from the parameter area.
//! Mac OS Runtime Architectures (1997), "PowerPC Runtime Conventions".

use super::super::PpcSectionMem;
use ppc::{PpcCpu, PpcMemory};

const GPR_ARGUMENTS: usize = 8;
const FPR_ARGUMENTS: usize = 13;
const LINKAGE_AREA_BYTES: u32 = 24;

pub struct PpcCallArgs<'a> {
    cpu: &'a PpcCpu,
    memory: &'a mut PpcSectionMem,
    word: usize,
    fpr: usize,
}

impl<'a> PpcCallArgs<'a> {
    pub fn new(cpu: &'a PpcCpu, memory: &'a mut PpcSectionMem) -> Self {
        Self {
            cpu,
            memory,
            word: 0,
            fpr: 0,
        }
    }

    pub fn with_first_word(cpu: &'a PpcCpu, memory: &'a mut PpcSectionMem, word: usize) -> Self {
        Self {
            cpu,
            memory,
            word,
            fpr: 0,
        }
    }

    pub fn memory(&mut self) -> &mut PpcSectionMem {
        self.memory
    }

    fn stack_word_address(&self, word: usize) -> u32 {
        self.cpu.gpr[1]
            .wrapping_add(LINKAGE_AREA_BYTES)
            .wrapping_add((word as u32).wrapping_mul(4))
    }

    fn word_at(&mut self, word: usize) -> u32 {
        if word < GPR_ARGUMENTS {
            self.cpu.gpr[3 + word]
        } else {
            let address = self.stack_word_address(word);
            self.memory.read_u32_be(address).unwrap_or(0)
        }
    }

    /// An integer, enumerant or pointer argument.
    pub fn u32(&mut self) -> u32 {
        let value = self.word_at(self.word);
        self.word += 1;
        value
    }

    pub fn i32(&mut self) -> i32 {
        self.u32() as i32
    }

    /// A `float` argument.
    pub fn f32(&mut self) -> f32 {
        let value = if self.fpr < FPR_ARGUMENTS {
            f64::from_bits(self.cpu.fpr[1 + self.fpr]) as f32
        } else {
            f32::from_bits(self.word_at(self.word))
        };
        self.fpr += 1;
        self.word += 1;
        value
    }

    /// A `double` argument.
    pub fn f64(&mut self) -> f64 {
        let value = if self.fpr < FPR_ARGUMENTS {
            f64::from_bits(self.cpu.fpr[1 + self.fpr])
        } else {
            let high = self.word_at(self.word);
            let low = self.word_at(self.word + 1);
            f64::from_bits((u64::from(high) << 32) | u64::from(low))
        };
        self.fpr += 1;
        self.word += 2;
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_shadow_general_registers_and_spill_to_the_parameter_area() {
        let mut cpu = PpcCpu::new();
        let mut memory = PpcSectionMem::new();
        // void f(int, float, int, double, int): words 0,1,2,3-4,5.
        cpu.gpr[3] = 7;
        cpu.gpr[5] = 9;
        cpu.gpr[8] = 11;
        cpu.fpr[1] = 1.5f64.to_bits();
        cpu.fpr[2] = (-2.25f64).to_bits();
        let mut args = PpcCallArgs::new(&cpu, &mut memory);
        assert_eq!(args.u32(), 7);
        assert_eq!(args.f32(), 1.5);
        assert_eq!(args.u32(), 9);
        assert_eq!(args.f64(), -2.25);
        assert_eq!(args.u32(), 11);
    }

    #[test]
    fn ninth_integer_argument_comes_from_the_caller_parameter_area() {
        let mut cpu = PpcCpu::new();
        let mut memory = PpcSectionMem::new();
        cpu.gpr[1] = 0x1000;
        memory.add_region(0x1000, vec![0; 64]);
        memory
            .write_u32_be(0x1000 + 24 + 8 * 4, 0xDEAD_BEEF)
            .unwrap();
        for index in 0..8 {
            cpu.gpr[3 + index] = index as u32;
        }
        let mut args = PpcCallArgs::new(&cpu, &mut memory);
        for index in 0..8 {
            assert_eq!(args.u32(), index);
        }
        assert_eq!(args.u32(), 0xDEAD_BEEF);
    }
}

#[cfg(test)]
#[test]
fn renderer_prefix_preserves_float_shadow_words_and_stack_spills() {
    let mut cpu = PpcCpu::new();
    let mut memory = PpcSectionMem::new();
    cpu.gpr[1] = 0x1000;
    memory.add_region(0x1000, vec![0; 128]);
    cpu.gpr[3] = 0xdead_beef;
    for word in 1..10 {
        if word < 8 {
            cpu.gpr[3 + word] = word as u32;
        }
        memory
            .write_u32_be(0x1000 + 24 + word as u32 * 4, word as u32)
            .unwrap();
    }
    cpu.fpr[1] = 0.5_f64.to_bits();
    let mut args = PpcCallArgs::with_first_word(&cpu, &mut memory, 1);
    assert_eq!(args.u32(), 1);
    assert_eq!(args.f32(), 0.5);
    for expected in 3..10 {
        assert_eq!(args.u32(), expected);
    }
}
