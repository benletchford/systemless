//! Synchronous virtual-volume information, IM Files pp. 2-144–2-146.
use super::Files;
use crate::{
    memory::{GuestPtr, Memory},
    Error, Result,
};
const NAME: &[u8] = b"Application Volume";
// The byte quota is real; 2 KiB allocation units keep 64 MiB representable by
// unsigned HFS words. Free blocks conservatively round available bytes down.
const BLOCK: usize = 2048;
impl Files {
    pub fn volume_info(
        &self,
        memory: &mut Memory,
        pointer: GuestPtr,
        hierarchical: bool,
    ) -> Result<i16> {
        if pointer.0 == 0 {
            return Err(Error::Cpu("nil volume information parameter block".into()));
        }
        let header = memory.read(pointer, 30)?.to_vec();
        let name = GuestPtr(u32::from_be_bytes(header[18..22].try_into().unwrap()));
        let reference = i16::from_be_bytes(header[22..24].try_into().unwrap());
        let index = i16::from_be_bytes(header[28..30].try_into().unwrap());
        let input = if index < 0 {
            Self::name_input(memory, name)?
        } else {
            Vec::new()
        };
        let selection = if self.name.is_empty() {
            Err(
                if index <= 0 && reference == 0 && (input.is_empty() || input[0] == b':') {
                    -50i16
                } else {
                    -35
                },
            )
        } else if index > 0 {
            if index == 1 {
                Ok(2)
            } else {
                Err(-35)
            }
        } else {
            if !input.is_empty() && input[0] != b':' {
                let volume = input.split(|&c| c == b':').next().unwrap();
                if volume.eq_ignore_ascii_case(NAME) {
                    Ok(2)
                } else {
                    Err(-35)
                }
            } else {
                self.classic_parent(reference)
            }
        };
        memory.validate_write(GuestPtr(pointer.0 + 12), 6)?;
        let directory = match selection {
            Ok(directory) => directory,
            Err(error) => {
                memory.write(GuestPtr(pointer.0 + 12), &[0; 4])?;
                memory.write(GuestPtr(pointer.0 + 16), &error.to_be_bytes())?;
                return Ok(error);
            }
        };
        let length = if hierarchical { 122 } else { 64 };
        let mut pb = memory.read(pointer, length)?.to_vec();
        memory.validate_write(pointer, length)?;
        let mut output_name = vec![NAME.len() as u8];
        output_name.extend(NAME);
        if name.0 != 0 {
            memory.validate_write(name, output_name.len())?;
        }
        pb[12..18].fill(0);
        pb[22..24].copy_from_slice(&(-1i16).to_be_bytes());
        pb[30..].fill(0);
        // The container has no physical bitmap extents, backup or Finder metadata.
        // Volume-level timestamps are not persisted by this catalog format; the
        // current profile reports epoch zero (1904), not fabricated host times.
        let valence = self.children(directory, false).len();
        pb[40..42].copy_from_slice(&(valence as u16).to_be_bytes());
        pb[46..48].copy_from_slice(&((super::io::LIMIT / BLOCK) as u16).to_be_bytes());
        pb[48..52].copy_from_slice(&(BLOCK as u32).to_be_bytes());
        pb[52..56].copy_from_slice(&(BLOCK as u32).to_be_bytes());
        pb[58..62].copy_from_slice(&self.next_node.to_be_bytes());
        let free = super::io::LIMIT.saturating_sub(self.space_used()) / BLOCK;
        pb[62..64].copy_from_slice(&(free as u16).to_be_bytes());
        if hierarchical {
            pb[64..66].copy_from_slice(&0x4244u16.to_be_bytes());
            // Stable virtual online-volume identities, not a physical disk driver.
            pb[66..68].copy_from_slice(&1i16.to_be_bytes());
            pb[68..70].copy_from_slice(&(-1i16).to_be_bytes());
            pb[78..82].copy_from_slice(&(self.revision as u32).to_be_bytes());
            pb[82..86].copy_from_slice(&((self.files.len() + 1) as u32).to_be_bytes());
            pb[86..90].copy_from_slice(&((self.directories.len() + 1) as u32).to_be_bytes());
        }
        // Snapshot all inputs and preflight every output before publication, like
        // the existing file/catalog queries. PB fields win if name output aliases.
        if name.0 != 0 {
            memory.write(name, &output_name)?;
        }
        memory.write(pointer, &pb)?;
        Ok(0)
    }
}
