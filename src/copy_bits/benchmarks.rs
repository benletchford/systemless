//! Opt-in cost decomposition before selecting a parallel graphics kernel.
//! No timing threshold runs in ordinary correctness tests.

use super::*;
use std::hint::black_box;
use std::time::Instant;

fn median_ns(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "run explicitly in an optimized profile on an otherwise idle host"]
fn measure_indexed_shrink_full_operation() {
    const SOURCE: u32 = 0x0010_0000;
    const DESTINATION: u32 = 0x0100_0000;
    for (width, height) in [
        (64usize, 64usize),
        (128, 128),
        (256, 256),
        (512, 256),
        (512, 512),
        (640, 480),
        (2048, 2048),
    ] {
        let destination_width = width / 2;
        let plan =
            Indexed8HorizontalShrink::new(width, destination_width, 0..destination_width).unwrap();
        assert_eq!(plan.source_range(), 0..width);
        // Alternate disjoint index ranges so every measured operation changes
        // destination pixels, including when presentation tracking is enabled.
        let sources: [Vec<u8>; 2] = std::array::from_fn(|phase| {
            (0..width * height)
                .map(|index| ((index * 31 + index / width) % 128 + phase * 128) as u8)
                .collect()
        });
        for participants in [1, 2, 4] {
            super::parallel::with_participants(participants, || {
                for presentation in [false, true] {
                    let mut bus = MacMemoryBus::new(32 * 1024 * 1024);
                    bus.set_addressing_32_bit(true);
                    if presentation {
                        bus.enable_outline_presentation(
                            (
                                DESTINATION,
                                destination_width as u32,
                                destination_width as u16,
                                height as u16,
                                8,
                            ),
                            std::array::from_fn(|index| [index as u8; 3]),
                            2,
                        );
                    }
                    let mut full = Vec::new();
                    let mut kernel = Vec::new();
                    let mut first_operation_ns = 0;
                    for iteration in 0..35 {
                        let source = &sources[iteration % 2];
                        bus.write_bytes(SOURCE, source);
                        let copy = RowCopy {
                            mode: 0,
                            source: BytePixmap {
                                base: SOURCE,
                                row_bytes: width as u32,
                                depth: 8,
                                bounds: [0, 0, height as i32, width as i32],
                            },
                            destination: BytePixmap {
                                base: DESTINATION,
                                row_bytes: destination_width as u32,
                                depth: 8,
                                bounds: [0, 0, height as i32, destination_width as i32],
                            },
                            source_rect: [0, 0, height as i32, width as i32],
                            destination_rect: [0, 0, height as i32, destination_width as i32],
                            clip: [0, 0, height as i32, destination_width as i32],
                            palette: None,
                        };
                        let selection =
                            Indexed8ScalingSelection::from_adapter_facts(true, true, true, true, 0);
                        let start = Instant::now();
                        assert_eq!(
                            black_box(copy.execute_with_indexed8_scaling(&mut bus, selection)),
                            RowCopyOutcome::Completed
                        );
                        let full_ns = start.elapsed().as_nanos();
                        if iteration == 0 {
                            first_operation_ns = full_ns;
                        }
                        let start = Instant::now();
                        drop(black_box(
                            plan.reduce_rows(black_box(source), height).unwrap(),
                        ));
                        let kernel_ns = start.elapsed().as_nanos();
                        if iteration >= 4 {
                            full.push(full_ns);
                            kernel.push(kernel_ns);
                        }
                    }
                    let expected = plan.reduce_rows(&sources[0], height).unwrap();
                    let mut actual = vec![0; expected.len()];
                    bus.read_bytes_into(DESTINATION, &mut actual);
                    assert_eq!(actual, expected);
                    println!("{{\"kernel\":\"indexed8_horizontal_shrink\",\"width\":{width},\"height\":{height},\"presentation\":{presentation},\"participants\":{participants},\"samples\":31,\"first_operation_ns\":{first_operation_ns},\"full_median_ns\":{},\"pure_median_ns\":{}}}", median_ns(full), median_ns(kernel));
                }
            });
        }
    }
}
