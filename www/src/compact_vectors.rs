//! Cross-language reference vectors for the existing CompactPresentation format.
#[cfg(test)]
mod tests {
    use serde_json::json;
    use systemless::memory::CompactPresentation;

    #[test]
    fn gpu_vectors_match_the_native_compact_resolver() {
        let mut cases = Vec::new();
        for scale in 1..=4u32 {
            let tile = scale * scale;
            let detail = (0..tile * 2)
                .map(|i| {
                    let r = (i * 31 + scale * 3) & 255;
                    let g = if i & 1 == 0 { 0 } else { 255 };
                    let b = (255 - i * 7 % 256) & 255;
                    (r << 16) | (g << 8) | b
                })
                .collect::<Vec<_>>();
            let frame = CompactPresentation {
                width: 2,
                height: 2,
                scale,
                cells: vec![0x123456, 0x80000000, 0xabcdef, 0x80000000 | tile],
                detail,
            };
            let mut outputs = Vec::new();
            for output_scale in 1..=4 {
                let mut pixels = Vec::new();
                assert!(
                    frame.render_argb_resized((2 * output_scale, 2 * output_scale), &mut pixels)
                );
                outputs.push(json!({"scale": output_scale, "argb": pixels}));
            }
            cases.push(json!({"width": frame.width, "height": frame.height,
                "scale": scale, "cells": frame.cells, "detail": frame.detail, "outputs": outputs}));
        }
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/compact-native.json")).unwrap();
        assert_eq!(json!(cases), expected);
    }
}
