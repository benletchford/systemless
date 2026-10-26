//! Classic PowerPC store-update compatibility at the native host boundary.

/// Select the equivalent non-updating integer store only for RA=0 update
/// forms. The original instruction in guest memory is never rewritten.
///
/// The 601 manual §3.5.3 (p. 3-47) specifies execution without updating r0.
/// The 604 manual §2.3.4.3.4 (p. 2-37) specifies an undefined GPR0 value for
/// these forms, rather than an illegal-instruction exception. We choose the
/// deterministic 601 result for this undefined register on the classic host.
/// Normal core execution supplies effective-address calculation, memory
/// faults, write observation, PC advancement, and time-base accounting.
pub(super) fn zero_base_store_instruction(word: u32) -> Option<u32> {
    if word & 0x001f_0000 != 0 {
        return None;
    }
    match word >> 26 {
        37 | 39 | 45 => Some(word - (1 << 26)), // stwu/stbu/sthu -> stw/stb/sth
        31 if word & 1 == 0 && matches!((word >> 1) & 0x3ff, 183 | 247 | 439) => {
            Some(word - (32 << 1)) // stwux/stbux/sthux -> stwx/stbx/sthx
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::{PpcCpu, PpcMemory, PpcMemoryWriteObserver, PpcStepResult};

    struct Memory(Vec<u8>);
    impl PpcMemory for Memory {
        fn read_u8(&mut self, addr: u32) -> Option<u8> {
            self.0.get(addr as usize).copied()
        }
        fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
            *self.0.get_mut(addr as usize)? = value;
            Some(())
        }
    }
    #[derive(Default)]
    struct Writes(Vec<(u32, u8)>);
    impl PpcMemoryWriteObserver for Writes {
        fn on_write(&mut self, _pc: u32, _lr: u32, _rtoc: u32, _sp: u32, addr: u32, value: u8) {
            self.0.push((addr, value));
        }
    }

    #[test]
    fn zero_base_stores_execute_all_integer_widths_without_changing_r0() {
        for (opcode, xo, width) in [
            (37, 0, 4),
            (39, 0, 1),
            (45, 0, 2),
            (31, 183, 4),
            (31, 247, 1),
            (31, 439, 2),
        ] {
            for source in [0, 5] {
                let word = (opcode << 26)
                    | (source << 21)
                    | if opcode == 31 {
                        (6 << 11) | (xo << 1)
                    } else {
                        32
                    };
                let mut cpu = PpcCpu::default();
                let mut memory = Memory(vec![0; 128]);
                cpu.pc = 0x1000;
                cpu.gpr[0] = 0x1122_3344;
                cpu.gpr[5] = 0x5566_7788;
                cpu.gpr[6] = 32;
                cpu.cr = 0xabcdef01;
                cpu.xer = 0x8000_0000;
                assert!(matches!(
                    cpu.step(&mut memory, word),
                    PpcStepResult::Exception(_)
                ));
                let before = cpu.gpr;
                let mut writes = Writes::default();
                assert_eq!(
                    cpu.step_with_write_observer(
                        &mut memory,
                        zero_base_store_instruction(word).unwrap(),
                        &mut writes
                    ),
                    PpcStepResult::Stepped
                );
                let value = before[source as usize].to_be_bytes();
                assert_eq!(&memory.0[32..32 + width], &value[4 - width..]);
                assert_eq!(writes.0.len(), width);
                assert_eq!(cpu.gpr, before);
                assert_eq!(cpu.pc, 0x1004);
                assert_eq!(cpu.time_base(), 1);
                assert_eq!(cpu.cr, 0xabcdef01);
                assert_eq!(cpu.xer, 0x8000_0000);
            }
        }
    }

    #[test]
    fn zero_base_store_faults_and_other_invalid_forms_remain_errors() {
        for word in [0, 0x8400_0020, 0x94a1_0020, (31 << 26) | (183 << 1) | 1] {
            assert_eq!(zero_base_store_instruction(word), None);
        }
        let mut cpu = PpcCpu::default();
        cpu.pc = 0x1000;
        cpu.gpr[0] = 123;
        let mut memory = Memory(vec![0; 128]);
        // Negative D-form address must sign-extend; it cannot become a
        // store relative to the nonzero contents of r0.
        let word = (45 << 26) | (5 << 21) | 0xfffe;
        assert_eq!(
            cpu.step(&mut memory, zero_base_store_instruction(word).unwrap()),
            PpcStepResult::MemoryFault {
                addr: 0xffff_fffe,
                was_write: true
            }
        );
        assert_eq!(cpu.pc, 0x1000);
        assert_eq!(cpu.time_base(), 0);
        assert_eq!(cpu.gpr[0], 123);
    }
}
