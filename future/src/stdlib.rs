//! Guest storage for MPW's application startup contract, shared by every
//! StdCLib data import in a process. No host pointers or duplicate globals.
use crate::{
    memory::{GuestPtr, Memory, Permissions},
    Result,
};
const BASE: u32 = 0x7200_0000;
pub(crate) const PHASE: GuestPtr = GuestPtr(BASE + 64);
pub(crate) const EXIT_STATUS: GuestPtr = GuestPtr(BASE + 68);
pub(crate) const EXIT_TARGET: GuestPtr = GuestPtr(BASE + 80);

pub(crate) fn data(library: &[u8], symbol: &[u8]) -> Option<GuestPtr> {
    if library != b"StdCLib" {
        return None;
    }
    Some(match symbol {
        b"_IntEnv" => GuestPtr(BASE),
        b"__C_phase" => PHASE,
        b"_exit_status" => EXIT_STATUS,
        b"__target_for_exit" => EXIT_TARGET,
        b"__NubAt3" => GuestPtr(BASE + 72),
        _ => return None,
    })
}

pub(crate) fn install(memory: &mut Memory) -> Result<()> {
    let mut bytes = vec![0; 336];
    // MPW uses a packed environment record: SH signature, argc at +2,
    // argv at +6 and envp at +10. A standalone application has no command
    // arguments or environment. Both arrays contain a terminating null.
    // Remaining environment fields are reserved for future shell services.
    bytes[..2].copy_from_slice(b"SH");
    bytes[6..10].copy_from_slice(&(BASE + 56).to_be_bytes());
    bytes[10..14].copy_from_slice(&(BASE + 60).to_be_bytes());
    memory.map(GuestPtr(BASE), bytes, Permissions::RW)
}

/// Standalone application exit. stdio and atexit are not exported yet, so no
/// registered C-library cleanup can be silently lost. CFM termination remains
/// the shared process lifecycle's responsibility.
pub(crate) fn exit(memory: &mut Memory, status: u32) -> Result<()> {
    // MPW shell records contain additional exit hooks; accepting such records
    // without executing them would hide unsupported cleanup.
    for offset in [22, 36] {
        if memory.u32(GuestPtr(BASE + offset))? != 0 {
            return Err(crate::Error::Cpu(
                "MPW shell exit hooks are unsupported".into(),
            ));
        }
    }
    memory.validate_write(PHASE, 8)?;
    let mut values = [0; 8];
    values[..4].copy_from_slice(&5u32.to_be_bytes());
    values[4..].copy_from_slice(&status.to_be_bytes());
    memory.write(PHASE, &values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_publishes_status_but_rejects_unimplemented_shell_cleanup() {
        let mut memory = Memory::default();
        install(&mut memory).unwrap();
        exit(&mut memory, 37).unwrap();
        assert_eq!(memory.u32(PHASE).unwrap(), 5);
        assert_eq!(memory.u32(EXIT_STATUS).unwrap(), 37);
        memory
            .write(GuestPtr(BASE + 22), &123u32.to_be_bytes())
            .unwrap();
        assert!(exit(&mut memory, 99).is_err());
        assert_eq!(memory.u32(EXIT_STATUS).unwrap(), 37);
    }
}
