//! Routed GPU readback commits. Bit 15 identifies untouched background pixels.
use super::{PpcFrontBuffer, PpcSectionMem};

/// Preflight every painted span before publishing any. Untouched pixels must
/// not notify guest write observers: even an identical store clears text detail.
pub(crate) fn commit_pixels(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    pixels: &[u16],
) -> bool {
    commit_pixels_impl(memory, front, pixels, false)
}

pub(crate) fn commit_indexed_pixels(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    pixels: &[u16],
) -> bool {
    if front.depth != 8 || pixels.iter().any(|p| p & 0x8000 == 0 && *p > 255) {
        return false;
    }
    commit_pixels_impl(memory, front, pixels, true)
}

fn commit_pixels_impl(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    pixels: &[u16],
    indexed: bool,
) -> bool {
    let bytes_per_pixel = match front.depth {
        8 if indexed => 1,
        16 if !indexed => 2,
        _ => return false,
    };
    let Some(row_len) = front.width.checked_mul(bytes_per_pixel) else {
        return false;
    };
    if row_len > front.row_bytes
        || pixels.len() as u64 != u64::from(front.width) * u64::from(front.height)
    {
        return false;
    }
    let mut bytes = Vec::new();
    let mut writes = Vec::new();
    for y in 0..front.height {
        let row =
            &pixels[y as usize * front.width as usize..(y as usize + 1) * front.width as usize];
        let mut x = 0usize;
        while x < row.len() {
            if row[x] & 0x8000 != 0 {
                x += 1;
                continue;
            }
            let first = x;
            let start = bytes.len();
            while x < row.len() && row[x] & 0x8000 == 0 {
                if indexed {
                    bytes.push(row[x] as u8);
                } else {
                    bytes.extend(row[x].to_be_bytes());
                }
                x += 1;
            }
            let Some(address) = y
                .checked_mul(front.row_bytes)
                .and_then(|offset| front.base_addr.checked_add(offset))
                .and_then(|row| row.checked_add(first as u32 * bytes_per_pixel))
            else {
                return false;
            };
            let len = (bytes.len() - start) as u32;
            if !memory.is_writable_range(address, len) {
                return false;
            }
            writes.push((address, start..bytes.len()));
        }
    }
    for (address, range) in writes {
        memory
            .write_bytes(address, &bytes[range])
            .expect("preflighted GPU target stays writable");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppc::PpcMemory;

    #[test]
    fn gpu_indexed_commits_preserve_indices_padding_and_reject_invalid_cells() {
        let front = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 6,
            width: 4,
            height: 2,
            depth: 8,
        };
        let mut memory = PpcSectionMem::new();
        memory.add_region(front.base_addr, vec![200; 12]);
        let token = PpcMemory::instruction_cache_token(&mut memory, front.base_addr).unwrap();
        let mut pixels = vec![0x8000; 8];
        assert!(commit_indexed_pixels(&mut memory, front, &pixels));
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, front.base_addr),
            Some(token)
        );
        pixels[1] = 30;
        pixels[5] = 256;
        assert!(!commit_indexed_pixels(&mut memory, front, &pixels));
        assert_eq!(memory.read_u8(0x1001), Some(200));
        pixels[5] = 200;
        assert!(commit_indexed_pixels(&mut memory, front, &pixels));
        assert_eq!(memory.read_u8(0x1001), Some(30));
        assert_eq!(memory.read_u8(0x1007), Some(200));
        assert_eq!(memory.read_u8(0x1000), Some(200));
        assert_eq!(memory.read_u8(0x1004), Some(200));
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, front.base_addr),
            Some(token)
        );
        memory.add_readonly_region(0x1007, vec![200]);
        pixels[1] = 42;
        assert!(!commit_indexed_pixels(&mut memory, front, &pixels));
        assert_eq!(memory.read_u8(0x1001), Some(30));
    }

    #[test]
    fn gpu_painted_spans_preserve_raw_background_padding_and_code_observers() {
        let front = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 10,
            width: 4,
            height: 2,
            depth: 16,
        };
        let mut memory = PpcSectionMem::new();
        memory.add_region(front.base_addr, vec![0xab; 20]);
        let token = PpcMemory::instruction_cache_token(&mut memory, front.base_addr).unwrap();
        let mut pixels = vec![0x8000; 8];
        assert!(commit_pixels(&mut memory, front, &pixels));
        assert_eq!(
            PpcMemory::instruction_cache_token(&mut memory, front.base_addr),
            Some(token)
        );
        pixels[1] = 0x1234;
        assert!(commit_pixels(&mut memory, front, &pixels));
        assert_eq!(memory.read_u16_be(0x1002), Some(0x1234));
        assert_eq!(memory.read_u16_be(0x1000), Some(0xabab));
        assert_eq!(memory.read_u16_be(0x1008), Some(0xabab));
        assert_ne!(
            PpcMemory::instruction_cache_token(&mut memory, front.base_addr),
            Some(token)
        );
    }

    #[test]
    fn gpu_span_preflight_rejects_readonly_and_invalid_results_atomically() {
        let front = PpcFrontBuffer {
            base_addr: 0x1000,
            row_bytes: 10,
            width: 4,
            height: 2,
            depth: 16,
        };
        let mut memory = PpcSectionMem::new();
        memory.add_region(front.base_addr, vec![0xab; 20]);
        memory.add_readonly_region(0x100a, vec![0xab; 2]);
        let mut pixels = vec![0x8000; 8];
        pixels[0] = 0x1234;
        pixels[4] = 0x4567;
        assert!(!commit_pixels(&mut memory, front, &pixels));
        assert_eq!(memory.read_u16_be(0x1000), Some(0xabab));
        assert_eq!(memory.read_u16_be(0x100a), Some(0xabab));
        assert!(!commit_pixels(&mut memory, front, &pixels[..7]));
        assert_eq!(memory.read_u16_be(0x1000), Some(0xabab));
    }
}
