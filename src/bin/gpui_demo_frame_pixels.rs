//! In-place guest RGBA to GPUI BGRA conversion, preserving alpha and allocation.
pub(crate) fn rgba_to_bgra(pixels: &mut [u8]) {
    let mut offset = 0;
    #[cfg(target_arch = "aarch64")]
    unsafe {
        use std::arch::aarch64::{vld1q_u8, vqtbl1q_u8, vst1q_u8};
        let order = vld1q_u8([2u8, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15].as_ptr());
        while pixels.len() - offset >= 16 {
            // AArch64 requires NEON. Both unaligned operations stay inside
            // this exclusive slice; each iteration converts four full pixels.
            let pointer = pixels.as_mut_ptr().add(offset);
            let rgba = vld1q_u8(pointer);
            vst1q_u8(pointer, vqtbl1q_u8(rgba, order));
            offset += 16;
        }
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use std::arch::x86_64::*;
        // SSE2 is part of the x86-64 baseline, without runtime feature probing.
        let preserved = _mm_set1_epi32(0xff00ff00u32 as i32);
        let red = _mm_set1_epi32(0xff);
        let blue = _mm_set1_epi32(0xff0000);
        while pixels.len() - offset >= 16 {
            let pointer = pixels.as_mut_ptr().add(offset).cast();
            let rgba = _mm_loadu_si128(pointer);
            let rb = _mm_or_si128(
                _mm_slli_epi32::<16>(_mm_and_si128(rgba, red)),
                _mm_srli_epi32::<16>(_mm_and_si128(rgba, blue)),
            );
            _mm_storeu_si128(pointer, _mm_or_si128(_mm_and_si128(rgba, preserved), rb));
            offset += 16;
        }
    }
    for pixel in pixels[offset..].chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::rgba_to_bgra;
    #[test]
    fn channel_conversion_preserves_alpha_partial_pixels_and_unaligned_slices() {
        for length in 0..=65 {
            for prefix in 0..16 {
                let mut source: Vec<u8> = (0..length + prefix + 7)
                    .map(|i| (i * 37 + 11) as u8)
                    .collect();
                let original = source.clone();
                rgba_to_bgra(&mut source[prefix..prefix + length]);
                for i in 0..length / 4 {
                    let start = prefix + i * 4;
                    assert_eq!(
                        &source[start..start + 4],
                        &[
                            original[start + 2],
                            original[start + 1],
                            original[start],
                            original[start + 3]
                        ]
                    );
                }
                assert_eq!(&source[..prefix], &original[..prefix]);
                assert_eq!(
                    &source[prefix + length / 4 * 4..],
                    &original[prefix + length / 4 * 4..]
                );
                rgba_to_bgra(&mut source[prefix..prefix + length]);
                assert_eq!(source, original);
            }
        }
    }
}
